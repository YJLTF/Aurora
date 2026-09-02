<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { api } from "../api";
import { compareVersion } from "../types";
import type { NpmCheck, Settings } from "../types";
import { timeAgo } from "../utils";

const props = defineProps<{
  settings: Settings;
  /** 上次会话持久化的检查结果，首次扫描后恢复 */
  initialChecks: NpmCheck[];
}>();

const emit = defineEmits<{
  (e: "openSettings"): void;
  (e: "openUrl", url: string): void;
  (e: "notify", text: string, kind: "ok" | "err" | "info"): void;
  (e: "stats", total: number, updates: number): void;
  (e: "saveChecks", checks: NpmCheck[]): void;
}>();

interface NpmRow {
  name: string;
  version: string;
  dir: string;
}

interface NpmViewRow extends NpmRow {
  status: NStatus;
}

type NStatus = "idle" | "update" | "uptodate" | "error";

const rows = ref<NpmRow[]>([]);
const checks = ref<Record<string, NpmCheck>>({});
const busy = reactive({ scanning: false, checking: false });
const scanned = ref(false);
const scanError = ref("");
const root = ref("");

// 面板随视图常驻，手动全局目录变更时自动重扫
watch(
  () => props.settings.npmGlobalRoot,
  () => {
    void scan();
  },
  { immediate: true },
);

/** 探测全局目录（npm root -g 或手动指定）并扫描包清单 */
async function scan() {
  if (busy.scanning) return;
  busy.scanning = true;
  scanError.value = "";
  try {
    root.value = await api.npmDetectRoot(props.settings.npmGlobalRoot);
    rows.value = await api.scanNpm(root.value);
    scanned.value = true;
    // 检查结果跨扫描保留：首次扫描恢复上次会话的结果，之后按新本地版本重算
    if (!Object.keys(checks.value).length && props.initialChecks.length) {
      applyChecks(props.initialChecks);
    }
    recomputeHasUpdate();
    emitStats();
  } catch (e) {
    scanError.value = String(e);
    rows.value = [];
    scanned.value = false;
    emitStats();
  } finally {
    busy.scanning = false;
  }
}

function applyChecks(list: NpmCheck[]) {
  const m: Record<string, NpmCheck> = {};
  for (const c of list) m[c.name.toLowerCase()] = { ...c };
  checks.value = m;
}

/** 本地版本可能因重新扫描而变化，可更新标记按最新数据重算 */
function recomputeHasUpdate() {
  for (const r of rows.value) {
    const c = checks.value[r.name.toLowerCase()];
    if (c && c.latestVersion && r.version) {
      c.hasUpdate = compareVersion(c.latestVersion, r.version) > 0;
    }
  }
}

/** 批量到 registry 查 dist-tags.latest */
async function check() {
  if (busy.checking || busy.scanning || !rows.value.length) return;
  busy.checking = true;
  try {
    const items = rows.value.map((r) => ({
      name: r.name,
      localVersion: r.version,
    }));
    const list = await api.checkNpmUpdates(items, props.settings);
    const now = Date.now();
    for (const c of list) c.checkedAt = now;
    applyChecks(list);
    // 交回父级写入配置持久化
    emit("saveChecks", list);
    emitStats();
    const upd = updateCount.value;
    if (list.some((c) => c.error)) {
      emit("notify", "部分包检查失败，详见列表", "info");
    } else {
      emit(
        "notify",
        upd > 0 ? `${upd} 个包有新版本` : "全部包均为最新",
        upd > 0 ? "ok" : "info",
      );
    }
  } catch (e) {
    emit("notify", `检查更新失败: ${e}`, "err");
  } finally {
    busy.checking = false;
  }
}

/** 供顶栏按钮调用（扫描 / 检查全部） */
defineExpose({ scan, check, busy });

function statusOf(r: NpmRow): NStatus {
  const c = checks.value[r.name.toLowerCase()];
  if (!c) return "idle";
  if (c.error) return "error";
  return c.hasUpdate ? "update" : "uptodate";
}

const statusLabel: Record<NStatus, string> = {
  idle: "未检查",
  update: "可更新",
  uptodate: "已最新",
  error: "检查失败",
};

/** 状态随 checks/rows 变化统一预计算，避免模板里每格重复求值 */
const viewRows = computed<NpmViewRow[]>(() =>
  rows.value.map((r) => ({ ...r, status: statusOf(r) })),
);

const updateCount = computed(
  () => viewRows.value.filter((r) => r.status === "update").length,
);

function latestOf(r: NpmRow): string {
  return checks.value[r.name.toLowerCase()]?.latestVersion || "—";
}

function checkedAtOf(r: NpmRow): number {
  return checks.value[r.name.toLowerCase()]?.checkedAt ?? 0;
}

function upgradeCmd(r: NpmRow): string {
  return `npm install -g ${r.name}@latest`;
}

async function copyUpgrade(r: NpmRow) {
  try {
    await navigator.clipboard.writeText(upgradeCmd(r));
    emit("notify", `已复制：${upgradeCmd(r)}`, "ok");
  } catch {
    emit("notify", "复制失败，请手动输入升级命令", "err");
  }
}

function npmPageUrl(name: string): string {
  return `https://www.npmjs.com/package/${name}`;
}

function emitStats() {
  emit("stats", rows.value.length, updateCount.value);
}
</script>

<template>
  <div class="npm-wrap">
    <div v-if="scanError" class="vrow-empty">
      <p>{{ scanError }}</p>
      <div class="vops">
        <button class="btn primary" @click="scan()">重试</button>
        <button class="btn ghost" @click="emit('openSettings')">去设置</button>
      </div>
    </div>
    <div v-else-if="!scanned && busy.scanning" class="vrow-empty">
      正在扫描 npm 全局目录…
    </div>
    <div v-else-if="scanned && !rows.length" class="vrow-empty">
      全局目录下没有已安装的包
    </div>

    <div v-else class="npm-list vc-list">
      <div
        v-for="r in viewRows"
        :key="r.name"
        class="vrow"
        :data-status="r.status"
      >
        <div class="vmain">
          <div class="vid" :title="r.name">
            {{ r.name }}
            <span class="pill" :class="r.status">{{ statusLabel[r.status] }}</span>
          </div>
          <div class="vsub" :title="r.dir">{{ r.dir }}</div>
        </div>
        <div>
          <span class="vlabel">本地版本</span>
          <div class="ver">{{ r.version }}</div>
        </div>
        <div>
          <span class="vlabel">最新版本</span>
          <div
            class="ver"
            :class="{ hot: r.status === 'update', ok: r.status === 'uptodate' }"
            :title="checkedAtOf(r) ? `${timeAgo(checkedAtOf(r))}检查` : ''"
          >
            {{ latestOf(r) }}
          </div>
        </div>
        <div class="vops">
          <button
            v-if="r.status === 'update'"
            class="btn primary sm"
            :title="`复制升级命令 ${upgradeCmd(r)}`"
            @click="copyUpgrade(r)"
          >
            复制命令
          </button>
          <button
            class="btn ghost sm"
            title="在浏览器打开 npm 包页面"
            @click="emit('openUrl', npmPageUrl(r.name))"
          >
            主页
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
