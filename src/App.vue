<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "./api";
import { dlStore } from "./download";
import type {
  Asset,
  Config,
  SelfUpdateInfo,
  Settings,
  VsixCheck,
} from "./types";
import { containsVersion, stemOf, withVersionSuffix } from "./utils";
import RadarPanel from "./components/RadarPanel.vue";
import VscodePanel from "./components/VscodePanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import SelfUpdateDialog from "./components/SelfUpdateDialog.vue";

const config = ref<Config>({
  settings: {
    downloadDir: "",
    githubApiBase: "https://api.github.com",
    downloadProxy: "",
    githubToken: "",
    vscodeDir: "",
    autoCheckSelf: true,
  },
  vscodeChecks: [],
  items: [],
});
const ready = ref(false);
const settingsOpen = ref(false);

/** 主视图切换：软件雷达 / VSCode 插件 */
const view = ref<"radar" | "vscode">("radar");
/** 软件雷达面板：顶栏按钮直调其 openAdd/checkAll，下载完成经 handleDone 转发 */
const radarPanel = ref<InstanceType<typeof RadarPanel> | null>(null);
const radarStats = ref({ total: 0, updates: 0 });
/** VSCode 面板：顶栏按钮直调其 scan/check */
const vsPanel = ref<InstanceType<typeof VscodePanel> | null>(null);
const vsStats = ref({ total: 0, updates: 0 });

/** Aurora 自身更新 */
const SELF_DL_ID = "aurora-self-update";
const appVersion = ref("");
const selfInfo = ref<SelfUpdateInfo | null>(null);
const selfDialogOpen = ref(false);
const selfChecking = ref(false);
/** 自更新安装包传输中（跟随共享下载队列） */
const selfDownloading = computed(() => dlStore.active.has(SELF_DL_ID));

interface Toast {
  id: number;
  text: string;
  kind: "ok" | "err" | "info";
}
const toasts = ref<Toast[]>([]);
let toastSeq = 1;

function toast(text: string, kind: Toast["kind"] = "info") {
  const id = toastSeq++;
  toasts.value.push({ id, text, kind });
  setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }, 4200);
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
function persist() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    api.save(config.value).catch((e) => toast(`保存配置失败: ${e}`, "err"));
  }, 250);
}

onMounted(async () => {
  try {
    config.value = await api.load();
  } catch (e) {
    toast(`读取配置失败: ${e}`, "err");
  }
  ready.value = true;
  try {
    const info = await api.appInfo();
    appVersion.value = info.version;
  } catch {
    appVersion.value = "";
  }
  dlStore.setNotify(toast);
  // 启动时静默检查自身更新（可在设置中关闭）
  if (config.value.settings.autoCheckSelf && api.isTauri) void checkSelf(true);
  // 全局唯一订阅下载进度：状态入共享队列，完成提示与雷达「已下载识别」在这里收口
  await api.onProgress((p) => {
    dlStore.handle(p);
    if (p.status === "done") {
      toast(
        p.itemId === SELF_DL_ID
          ? `${p.fileName} 下载完成，运行安装包即可完成升级`
          : `${p.fileName} 下载完成`,
        "ok",
      );
      if (p.itemId !== SELF_DL_ID) radarPanel.value?.handleDone(p);
    }
  });
});

/** 检查 Aurora 自身更新；silent 为启动时的静默检查 */
async function checkSelf(silent = false) {
  if (selfChecking.value) return;
  selfChecking.value = true;
  try {
    const info = await api.checkSelfUpdate(config.value.settings);
    selfInfo.value = info;
    if (!info.error) {
      if (info.hasUpdate) {
        toast(
          silent
            ? `Aurora 有新版本 v${info.latestVersion}，点击左下角版本号查看`
            : `发现新版本 v${info.latestVersion}`,
          "ok",
        );
      } else if (!silent) {
        toast(`Aurora 已是最新（v${info.currentVersion}）`);
      }
    } else if (!silent) {
      toast(`检查更新失败: ${info.error}`, "err");
    }
  } catch (e) {
    if (!silent) toast(`检查更新失败: ${e}`, "err");
  } finally {
    selfChecking.value = false;
  }
}

function openSelfDialog() {
  selfDialogOpen.value = true;
  // 从未检查过时进入弹窗自动检查
  if (!selfInfo.value) void checkSelf(true);
}

async function startSelfDownload(asset: Asset) {
  if (!config.value.settings.downloadDir.trim()) {
    toast("请先在设置里填写下载目录", "err");
    settingsOpen.value = true;
    return;
  }
  const ver = selfInfo.value?.latestVersion ?? "";
  let name = asset.name || `Aurora_${ver || "latest"}.exe`;
  // 文件名不含版本号时补上，便于与下载目录里的历史安装包区分
  if (ver && !containsVersion(stemOf(name), ver)) name = withVersionSuffix(name, ver);
  await dlStore.queue({
    itemId: SELF_DL_ID,
    url: asset.url,
    fileName: name,
    destDir: config.value.settings.downloadDir,
    proxyPrefix: config.value.settings.downloadProxy,
  });
}

/** VSCode 插件检查结果写入配置，跨会话/切视图恢复 */
function saveVscodeChecks(list: VsixCheck[]) {
  config.value.vscodeChecks = list;
  persist();
}

function saveSettings(s: Settings) {
  config.value.settings = s;
  settingsOpen.value = false;
  persist();
  toast("设置已保存", "ok");
}

function openUrl(url: string) {
  if (!url) return;
  if (!api.isTauri) {
    window.open(url, "_blank");
    return;
  }
  api.openUrl(url).catch((e) => toast(`打开链接失败: ${e}`, "err"));
}

function openLocal(path: string, reveal = false) {
  if (!path) return;
  api.open(path, reveal).catch((e) => toast(`打开路径失败: ${e}`, "err"));
}
</script>

<template>
  <div class="shell">
    <header class="topbar">
      <div class="brand">
        <svg class="radar" viewBox="0 0 48 48" width="32" height="32" aria-hidden="true">
          <defs>
            <linearGradient id="sweepGrad" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stop-color="#f7ae45" stop-opacity="0.85" />
              <stop offset="1" stop-color="#f7ae45" stop-opacity="0.05" />
            </linearGradient>
          </defs>
          <circle cx="24" cy="24" r="20" fill="none" stroke="#4d5b8e" stroke-width="1.6" />
          <circle cx="24" cy="24" r="12" fill="none" stroke="#4d5b8e" stroke-width="1.2" />
          <g class="sweep">
            <path d="M24 24 L24 4 A20 20 0 0 1 41.3 14 Z" fill="url(#sweepGrad)" />
          </g>
          <circle cx="24" cy="24" r="2.6" fill="#62cbe8" />
          <circle cx="35" cy="31" r="2.4" fill="#f7ae45" />
        </svg>
        <div class="brand-text">
          <h1>Aurora</h1>
          <span>软件更新雷达</span>
        </div>
      </div>
      <div class="top-actions">
        <div class="seg view-seg">
          <button :class="{ on: view === 'radar' }" @click="view = 'radar'">
            软件雷达
          </button>
          <button :class="{ on: view === 'vscode' }" @click="view = 'vscode'">
            VSCode 插件
          </button>
        </div>
        <button class="btn ghost" @click="settingsOpen = true">设置</button>
        <button
          v-if="view === 'radar'"
          class="btn ghost"
          @click="radarPanel?.openAdd()"
        >
          ＋ 添加软件
        </button>
        <button
          v-else
          class="btn ghost"
          :disabled="vsPanel?.busy.scanning || !config.settings.vscodeDir.trim()"
          @click="vsPanel?.scan()"
        >
          <span v-if="vsPanel?.busy.scanning" class="spin" aria-hidden="true"></span>
          扫描
        </button>
        <button
          v-if="view === 'radar'"
          class="btn primary"
          :disabled="radarPanel?.busy.running || !config.items.length"
          @click="radarPanel?.checkAll()"
        >
          <span v-if="radarPanel?.busy.running" class="spin light" aria-hidden="true"></span>
          {{
            radarPanel?.busy.running
              ? `检查中 ${radarPanel?.busy.done}/${radarPanel?.busy.total}`
              : "检查全部"
          }}
        </button>
        <button
          v-else
          class="btn primary"
          :disabled="vsPanel?.busy.checking || vsPanel?.busy.scanning || !vsStats.total"
          @click="vsPanel?.check()"
        >
          <span v-if="vsPanel?.busy.checking" class="spin light" aria-hidden="true"></span>
          {{ vsPanel?.busy.checking ? "检查中…" : "检查全部" }}
        </button>
      </div>
    </header>

    <RadarPanel
      v-show="view === 'radar'"
      ref="radarPanel"
      :ready="ready"
      :items="config.items"
      :settings="config.settings"
      @open-settings="settingsOpen = true"
      @open-path="openLocal"
      @open-url="openUrl"
      @notify="toast"
      @persist="persist"
      @stats="(t: number, u: number) => (radarStats = { total: t, updates: u })"
    />

    <VscodePanel
      v-show="view === 'vscode'"
      ref="vsPanel"
      :settings="config.settings"
      :initial-checks="config.vscodeChecks"
      @open-settings="settingsOpen = true"
      @open-path="openLocal"
      @notify="toast"
      @stats="(t: number, u: number) => (vsStats = { total: t, updates: u })"
      @save-checks="saveVscodeChecks"
    />

    <footer class="statusbar">
      <template v-if="view === 'radar'">
        <span>
          {{ radarStats.total }} 项受监控 ·
          <b :class="{ amber: radarStats.updates > 0 }">{{ radarStats.updates }}</b> 项可更新
        </span>
      </template>
      <template v-else>
        <span>
          {{ vsStats.total }} 个插件 ·
          <b :class="{ amber: vsStats.updates > 0 }">{{ vsStats.updates }}</b> 个可更新
        </span>
      </template>
      <span class="grow"></span>
      <button
        v-if="view === 'radar' && config.settings.downloadDir"
        class="linklike"
        title="打开下载目录"
        @click="openLocal(config.settings.downloadDir)"
      >
        下载目录：{{ config.settings.downloadDir }}
      </button>
      <button
        v-else-if="view === 'vscode' && config.settings.vscodeDir"
        class="linklike"
        title="打开插件备份目录"
        @click="openLocal(config.settings.vscodeDir)"
      >
        备份目录：{{ config.settings.vscodeDir }}
      </button>
      <button
        class="linklike selfver"
        title="检查 Aurora 更新"
        @click="openSelfDialog"
      >
        <span v-if="selfInfo?.hasUpdate && !selfInfo?.error" class="dot" aria-hidden="true"></span>
        Aurora v{{ appVersion || "…" }}
      </button>
      <span v-if="!api.isTauri" class="mocktag">浏览器预览 · 模拟数据</span>
    </footer>

    <SettingsPanel
      v-if="settingsOpen"
      :settings="config.settings"
      :app-version="appVersion"
      @save="saveSettings"
      @close="settingsOpen = false"
      @open-dir="openLocal(config.settings.downloadDir)"
      @open-vscode-dir="openLocal(config.settings.vscodeDir)"
      @check-update="openSelfDialog"
    />
    <SelfUpdateDialog
      v-if="selfDialogOpen && selfInfo"
      :info="selfInfo"
      :checking="selfChecking"
      :downloading="selfDownloading"
      :dl="dlStore.downloads[SELF_DL_ID] ?? null"
      @check="checkSelf()"
      @download="startSelfDownload"
      @open-url="openUrl"
      @pause="dlStore.pause(SELF_DL_ID)"
      @resume="dlStore.resume(SELF_DL_ID)"
      @cancel="dlStore.cancel(SELF_DL_ID)"
      @close="selfDialogOpen = false"
    />

    <div class="toasts" aria-live="polite">
      <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
        {{ t.text }}
      </div>
    </div>
  </div>
</template>
