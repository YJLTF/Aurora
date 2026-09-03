<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { api } from "../api";
import { dlStore } from "../download";
import { compareVersion } from "../types";
import type { DownloadProgress, Settings, VsixCheck } from "../types";
import { joinPath, timeAgo } from "../utils";
import DlProgress from "./DlProgress.vue";

const props = defineProps<{
  settings: Settings;
  /** 上次会话持久化的检查结果，首次扫描后恢复 */
  initialChecks: VsixCheck[];
}>();

const emit = defineEmits<{
  (e: "openSettings"): void;
  (e: "openPath", path: string, reveal?: boolean): void;
  (e: "notify", text: string, kind: "ok" | "err" | "info"): void;
  (e: "stats", total: number, updates: number): void;
  (e: "saveChecks", checks: VsixCheck[]): void;
}>();

interface VsRow {
  id: string;
  version: string;
  target: string;
  fileName: string;
  dir: string;
}

interface VsViewRow extends VsRow {
  status: VStatus;
}

type VStatus = "idle" | "update" | "uptodate" | "error";

const rows = ref<VsRow[]>([]);
const installed = ref<Record<string, string>>({});
const checks = ref<Record<string, VsixCheck>>({});
const busy = reactive({ scanning: false, checking: false });
const scanned = ref(false);
const scanError = ref("");

const dir = computed(() => props.settings.vscodeDir.trim());

// 面板随视图常驻（v-show），配置异步就绪或目录变更时（重新）扫描
watch(
  () => props.settings.vscodeDir,
  (d) => {
    if (d.trim()) void scan();
  },
  { immediate: true },
);

/** 扫描备份目录并读取本机已安装版本；同名插件保留最高版本 */
async function scan() {
  if (busy.scanning || !dir.value) return;
  busy.scanning = true;
  scanError.value = "";
  try {
    const files = await api.listVsix(dir.value);
    const map = new Map<string, VsRow>();
    for (const f of files) {
      const key = f.id.toLowerCase();
      if (!map.has(key)) map.set(key, { ...f });
    }
    rows.value = [...map.values()];
    scanned.value = true;
    // 检查结果跨扫描保留：首次扫描恢复上次会话的结果，之后按新本地版本重算
    if (!Object.keys(checks.value).length && props.initialChecks.length) {
      applyChecks(props.initialChecks);
    }
    recomputeHasUpdate();
    try {
      installed.value = await api.readInstalledExtensions();
    } catch {
      installed.value = {};
    }
    emitStats();
  } catch (e) {
    scanError.value = String(e);
  } finally {
    busy.scanning = false;
  }
}

function applyChecks(list: VsixCheck[]) {
  const m: Record<string, VsixCheck> = {};
  for (const c of list) m[c.id.toLowerCase()] = { ...c };
  checks.value = m;
}

/** 本地版本可能因重新扫描而变化，可更新标记按最新数据重算 */
function recomputeHasUpdate() {
  for (const r of rows.value) {
    const c = checks.value[r.id.toLowerCase()];
    if (c && c.latestVersion && r.version) {
      c.hasUpdate = compareVersion(c.latestVersion, r.version) > 0;
    }
  }
}

/** 批量到 Marketplace 查最新版本 */
async function check() {
  if (busy.checking || !rows.value.length) return;
  busy.checking = true;
  try {
    const items = rows.value.map((r) => ({
      id: r.id,
      target: r.target,
      localVersion: r.version,
    }));
    const list = await api.checkVscodeUpdates(items);
    const now = Date.now();
    for (const c of list) c.checkedAt = now;
    applyChecks(list);
    // 交回父级写入配置持久化
    emit("saveChecks", list);
    emitStats();
    const upd = updateCount.value;
    if (list.some((c) => c.error)) {
      emit("notify", "部分插件检查失败，详见列表", "info");
    } else {
      emit(
        "notify",
        upd > 0 ? `${upd} 个插件有新版本` : "全部插件均为最新",
        upd > 0 ? "ok" : "info",
      );
    }
  } catch (e) {
    emit("notify", `检查更新失败: ${e}`, "err");
  } finally {
    busy.checking = false;
  }
}

/** 供顶栏按钮调用（扫描 / 检查全部），handleDone 由 App 的下载完成事件转发 */
defineExpose({ scan, check, busy, handleDone });

function statusOf(r: VsRow): VStatus {
  const c = checks.value[r.id.toLowerCase()];
  if (!c) return "idle";
  if (c.error) return "error";
  return c.hasUpdate ? "update" : "uptodate";
}

const statusLabel: Record<VStatus, string> = {
  idle: "未检查",
  update: "可更新",
  uptodate: "已最新",
  error: "检查失败",
};

/** 状态随 checks/rows 变化统一预计算，避免模板里每格重复求值 */
const viewRows = computed<VsViewRow[]>(() =>
  rows.value.map((r) => ({ ...r, status: statusOf(r) })),
);

const updateCount = computed(
  () => viewRows.value.filter((r) => r.status === "update").length,
);

function latestOf(r: VsRow): string {
  return checks.value[r.id.toLowerCase()]?.latestVersion || "—";
}

function checkedAtOf(r: VsRow): number {
  return checks.value[r.id.toLowerCase()]?.checkedAt ?? 0;
}

function vsixKey(id: string): string {
  return `vsix:${id}`;
}

function downloadOf(r: VsRow) {
  return dlStore.downloads[vsixKey(r.id)] ?? null;
}

/** 下载新版 vsix；进行中的同一条由共享队列去重，暂停/失败可重新入队续传 */
function download(r: VsRow) {
  const c = checks.value[r.id.toLowerCase()];
  if (!c?.downloadUrl) return;
  dlStore.queue({
    itemId: vsixKey(r.id),
    url: c.downloadUrl,
    fileName: `${r.id}-${c.latestVersion}${r.target ? `-${r.target}` : ""}.vsix`,
    destDir: r.dir,
    proxyPrefix: props.settings.downloadProxy,
    preferIpv4: true,
  });
}

function emitStats() {
  emit("stats", rows.value.length, updateCount.value);
}

/** 下载完成转发入口（App 的全局进度订阅调用）：重扫备份目录，备份版本与行状态即随新文件刷新 */
function handleDone(p: DownloadProgress) {
  if (!p.itemId.startsWith("vsix:")) return;
  void rescanAfterDone();
}

/** 扫描进行中收到完成事件时记一笔，扫完补扫一次，保证最后落盘的文件不漏 */
let rescanPending = false;
async function rescanAfterDone() {
  if (busy.scanning) {
    rescanPending = true;
    return;
  }
  await scan();
  if (rescanPending && !busy.scanning) {
    rescanPending = false;
    void scan();
  }
}
</script>

<template>
  <div class="vscode-wrap">
    <div v-if="scanError" class="err">{{ scanError }}</div>

    <div v-if="!dir" class="vrow-empty">
      <p>尚未设置 VSCode 备份目录</p>
      <button class="btn primary" @click="emit('openSettings')">去设置</button>
    </div>
    <div v-else-if="scanned && !rows.length" class="vrow-empty">
      该目录下没有找到 .vsix 文件
    </div>

    <div v-else class="vc-list">
      <div
        v-for="r in viewRows"
        :key="r.id"
        class="vrow"
        :data-status="r.status"
      >
        <div class="vmain">
          <div class="vid" :title="r.id">
            {{ r.id }}
            <span class="pill" :class="r.status">{{ statusLabel[r.status] }}</span>
          </div>
          <div class="vsub" :title="r.dir">{{ r.fileName }}</div>
        </div>
        <div>
          <span class="vlabel">备份版本</span>
          <div class="ver">{{ r.version }}</div>
        </div>
        <div>
          <span class="vlabel">已装版本</span>
          <div class="ver">{{ installed[r.id.toLowerCase()] || "—" }}</div>
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
            v-if="r.status === 'update' && !downloadOf(r)"
            class="btn primary sm"
            title="下载新版 vsix 到备份目录"
            @click="download(r)"
          >
            下载
          </button>
          <button
            class="btn ghost sm"
            title="在资源管理器中定位本地文件"
            @click="emit('openPath', joinPath(r.dir, r.fileName), true)"
          >
            定位
          </button>
        </div>
        <DlProgress
          v-if="downloadOf(r)"
          class="dl-inline"
          :dl="downloadOf(r)!"
          @pause="dlStore.pause(vsixKey(r.id))"
          @resume="dlStore.resume(vsixKey(r.id))"
          @cancel="dlStore.cancel(vsixKey(r.id))"
        />
      </div>
    </div>
  </div>
</template>
