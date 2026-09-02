<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { api } from "../api";
import type { DownloadProgress, Settings, VsixCheck } from "../types";
import { joinPath } from "../utils";

const props = defineProps<{ settings: Settings }>();

const emit = defineEmits<{
  (e: "openSettings"): void;
  (e: "openPath", path: string, reveal?: boolean): void;
  (e: "notify", text: string, kind: "ok" | "err" | "info"): void;
  (e: "stats", total: number, updates: number): void;
}>();

interface VsRow {
  id: string;
  version: string;
  target: string;
  fileName: string;
  dir: string;
}

type VStatus = "idle" | "update" | "uptodate" | "error";

const rows = ref<VsRow[]>([]);
const installed = ref<Record<string, string>>({});
const checks = ref<Record<string, VsixCheck>>({});
const busy = reactive({ scanning: false, checking: false });
const scanned = ref(false);
const scanError = ref("");
const downloads = ref<Record<string, DownloadProgress>>({});

const dir = computed(() => props.settings.vscodeDir.trim());

// 独立订阅下载进度；完成/失败提示由 App 的全局监听统一弹出
void api.onProgress((p) => {
  if (p.status === "progressing") downloads.value[p.itemId] = p;
  else delete downloads.value[p.itemId];
});

onMounted(() => {
  void scan();
});

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
    checks.value = {};
    scanned.value = true;
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
    const list = await api.checkVscodeUpdates(items, props.settings);
    const m: Record<string, VsixCheck> = {};
    for (const c of list) m[c.id.toLowerCase()] = c;
    checks.value = m;
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

/** 供顶栏按钮调用（扫描 / 检查全部） */
defineExpose({ scan, check, busy });

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

const updateCount = computed(
  () => rows.value.filter((r) => statusOf(r) === "update").length,
);

function latestOf(r: VsRow): string {
  return checks.value[r.id.toLowerCase()]?.latestVersion || "—";
}

function downloadOf(r: VsRow): DownloadProgress | null {
  return downloads.value[`vsix:${r.id}`] ?? null;
}

async function download(r: VsRow) {
  const c = checks.value[r.id.toLowerCase()];
  if (!c?.downloadUrl || downloadOf(r)) return;
  const name = `${r.id}-${c.latestVersion}${r.target ? `-${r.target}` : ""}.vsix`;
  try {
    await api.download({
      itemId: `vsix:${r.id}`,
      url: c.downloadUrl,
      fileName: name,
      destDir: r.dir,
      proxyPrefix: props.settings.downloadProxy,
      preferIpv4: true,
    });
  } catch (e) {
    if (!String(e).includes("取消")) emit("notify", `${r.id} 下载失败：${e}`, "err");
  }
}

function emitStats() {
  emit("stats", rows.value.length, updateCount.value);
}

function pctOf(p: DownloadProgress): number {
  if (!p.total) return 0;
  return Math.min(100, Math.round((p.received / p.total) * 100));
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
        v-for="r in rows"
        :key="r.id + r.dir"
        class="vrow"
        :data-status="statusOf(r)"
      >
        <div class="vmain">
          <div class="vid" :title="r.id">
            {{ r.id }}
            <span class="pill" :class="statusOf(r)">{{ statusLabel[statusOf(r)] }}</span>
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
            :class="{ hot: statusOf(r) === 'update', ok: statusOf(r) === 'uptodate' }"
          >
            {{ latestOf(r) }}
          </div>
        </div>
        <div class="vops">
          <button
            v-if="statusOf(r) === 'update' && !downloadOf(r)"
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
        <div v-if="downloadOf(r)" class="dl dl-inline" role="status">
          <div class="bar" :class="{ indet: !downloadOf(r)!.total }">
            <div class="fill" :style="{ width: pctOf(downloadOf(r)!) + '%' }"></div>
          </div>
          <span class="dl-text">{{
            downloadOf(r)!.total
              ? `${downloadOf(r)!.fileName} · ${pctOf(downloadOf(r)!)}%`
              : `${downloadOf(r)!.fileName} · ${(downloadOf(r)!.received / 1048576).toFixed(1)} MB`
          }}</span>
          <button class="mini danger" @click="api.cancel(`vsix:${r.id}`)">取消</button>
        </div>
      </div>
    </div>
  </div>
</template>
