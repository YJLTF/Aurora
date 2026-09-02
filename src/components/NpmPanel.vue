<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { api } from "../api";
import { compareVersion } from "../types";
import type { NpmCheck, NpmUpgradeProgress, Settings } from "../types";
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
  (e: "rootChange", root: string): void;
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

/** 进行中的升级（包名 → 状态），后端 npm install -g 全局串行 */
type UpgStatus = NpmUpgradeProgress["status"];
interface UpgState {
  status: UpgStatus;
  output: string;
  error: string;
  /** 本地记录的启动时间，用于展示已用时 */
  startedAt: number;
}
const upgrades = ref<Record<string, UpgState>>({});
const anyUpgrading = computed(() =>
  Object.values(upgrades.value).some(
    (u) => u.status === "preparing" || u.status === "progressing",
  ),
);

/** 升级进行中每秒跳一次，驱动「已用时」刷新 */
const nowTick = ref(0);
let upgTimer: ReturnType<typeof setInterval> | undefined;
watch(anyUpgrading, (on) => {
  clearInterval(upgTimer);
  if (on) upgTimer = setInterval(() => (nowTick.value += 1), 1000);
});

/** 安装中显示已用时秒数（npm 管道模式日志稀疏，安静期也给进度感） */
function elapsedSuffix(u: UpgState): string {
  void nowTick.value;
  if (u.status !== "preparing" && u.status !== "progressing") return "";
  return ` ${Math.max(1, Math.round((Date.now() - u.startedAt) / 1000))}s`;
}

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
    emit("rootChange", root.value);
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
    emit("rootChange", "");
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

/** 供顶栏按钮调用（扫描 / 检查全部 / 升级进度回流），文末 defineExpose */
function statusOf(r: NpmRow): NStatus {  const c = checks.value[r.name.toLowerCase()];
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
    await api.copyText(upgradeCmd(r));
    emit("notify", `已复制：${upgradeCmd(r)}`, "ok");
  } catch {
    emit("notify", "复制失败，请手动输入升级命令", "err");
  }
}

function npmPageUrl(name: string): string {
  return `https://www.npmjs.com/package/${name}`;
}

function upgradeOf(r: NpmRow): UpgState | null {
  return upgrades.value[r.name] ?? null;
}

const upgLabel: Record<UpgStatus, string> = {
  preparing: "正在准备…",
  progressing: "正在安装…",
  done: "升级完成",
  error: "升级失败",
  cancelled: "已取消",
};

/** 执行更新：后端 npm install -g，进度经 onUpgrade 事件回流（全局串行） */
function upgrade(r: NpmRow) {
  if (anyUpgrading.value) return;
  upgrades.value[r.name] = {
    status: "preparing",
    output: "",
    error: "",
    startedAt: Date.now(),
  };
  api.npmUpgrade(r.name, props.settings.npmGlobalRoot).catch((e) => {
    // 命令本身被拒（如包名校验失败），事件流不会再来终态
    const cur = upgrades.value[r.name];
    if (cur && cur.status !== "done") {
      upgrades.value[r.name] = { ...cur, status: "error", error: String(e) };
    }
  });
}

function cancelUpgrade(r: NpmRow) {
  api.npmCancelUpgrade(r.name).catch(() => {});
}

function dismissUpgrade(r: NpmRow) {
  delete upgrades.value[r.name];
}

/** 升级进度回流（App.vue 全局订阅后转发，面板可能已重扫过） */
function handleUpgrade(p: NpmUpgradeProgress) {
  const cur = upgrades.value[p.name];
  if (p.status === "preparing") {
    upgrades.value[p.name] = {
      status: p.status,
      output: "",
      error: "",
      startedAt: cur?.startedAt ?? Date.now(),
    };
    return;
  }
  if (p.status === "progressing") {
    if (cur)
      upgrades.value[p.name] = { ...cur, status: p.status, output: p.output };
    return;
  }
  // 终态（后端终态事件不带 output，保留此前最后一行输出供查看）
  const output = p.output || cur?.output || "";
  if (p.status === "done") {
    upgrades.value[p.name] = { status: "done", output, error: "", startedAt: cur?.startedAt ?? 0 };
    const row = rows.value.find((r) => r.name === p.name);
    if (row && p.localVersion) row.version = p.localVersion;
    const c = checks.value[p.name.toLowerCase()];
    if (c && row) {
      c.localVersion = row.version;
      c.hasUpdate = c.latestVersion
        ? compareVersion(c.latestVersion, row.version) > 0
        : c.hasUpdate;
      emit(
        "saveChecks",
        rows.value
          .map((r) => checks.value[r.name.toLowerCase()])
          .filter((x): x is NpmCheck => !!x)
          .map((x) => ({ ...x })),
      );
    }
    if (p.localVersion) {
      emit("notify", `${p.name} 已升级到 ${p.localVersion}`, "ok");
    } else {
      emit("notify", `${p.name} 升级完成，重新扫描可刷新版本`, "ok");
    }
    recomputeHasUpdate();
    emitStats();
    setTimeout(() => {
      if (upgrades.value[p.name]?.status === "done") delete upgrades.value[p.name];
    }, 2500);
  } else if (p.status === "cancelled") {
    upgrades.value[p.name] = {
      status: "cancelled",
      output,
      error: "",
      startedAt: cur?.startedAt ?? 0,
    };
    emit("notify", `${p.name} 升级已取消`, "info");
    setTimeout(() => {
      if (upgrades.value[p.name]?.status === "cancelled") delete upgrades.value[p.name];
    }, 2000);
  } else {
    upgrades.value[p.name] = {
      status: "error",
      output,
      error: p.error || "升级失败",
      startedAt: cur?.startedAt ?? 0,
    };
    emit("notify", `${p.name} 升级失败：${p.error || "详见输出"}`, "err");
  }
}

function emitStats() {
  emit("stats", rows.value.length, updateCount.value);
}

/** 供顶栏与 App 调用（扫描 / 检查全部 / 升级进度回流） */
defineExpose({ scan, check, busy, handleUpgrade });
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
          <template v-if="r.status === 'update' && !upgradeOf(r)">
            <button
              class="btn primary sm"
              :title="anyUpgrading ? '已有升级任务进行中' : `执行 ${upgradeCmd(r)}`"
              :disabled="anyUpgrading"
              @click="upgrade(r)"
            >
              执行更新
            </button>
            <button
              class="btn ghost sm"
              :title="`复制升级命令 ${upgradeCmd(r)}`"
              @click="copyUpgrade(r)"
            >
              复制命令
            </button>
          </template>
          <button
            class="btn ghost sm"
            title="在浏览器打开 npm 包页面"
            @click="emit('openUrl', npmPageUrl(r.name))"
          >
            主页
          </button>
        </div>
        <div v-if="upgradeOf(r)" class="dl-inline upg">
          <span
            class="upg-label"
            :class="{ err: upgradeOf(r)!.status === 'error' }"
          >{{ upgLabel[upgradeOf(r)!.status] }}{{ elapsedSuffix(upgradeOf(r)!) }}</span>
          <span class="upg-output" :title="upgradeOf(r)!.error || upgradeOf(r)!.output">
            {{ upgradeOf(r)!.output || upgradeOf(r)!.error || "…" }}
          </span>
          <button
            v-if="upgradeOf(r)!.status === 'preparing' || upgradeOf(r)!.status === 'progressing'"
            class="btn ghost sm"
            @click="cancelUpgrade(r)"
          >
            取消
          </button>
          <button
            v-else-if="upgradeOf(r)!.status === 'error'"
            class="btn ghost sm"
            @click="dismissUpgrade(r)"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
