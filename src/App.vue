<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { api } from "./api";
import { compareVersion } from "./types";
import type {
  Asset,
  Config,
  DownloadProgress,
  ItemStatus,
  SelfUpdateInfo,
  Settings,
  SoftItem,
  VsixCheck,
} from "./types";
import { containsVersion, joinPath, matchDownloaded, slugify } from "./utils";
import SoftRow from "./components/SoftRow.vue";
import ItemEditor from "./components/ItemEditor.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import AssetPicker from "./components/AssetPicker.vue";
import SelfUpdateDialog from "./components/SelfUpdateDialog.vue";
import VscodePanel from "./components/VscodePanel.vue";

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
const checkingIds = reactive(new Set<string>());
const downloadingIds = reactive(new Set<string>());
const downloads = ref<Record<string, DownloadProgress>>({});
const donePaths = ref<Record<string, string>>({});
const downloadedFiles = ref<string[]>([]);
const checkAllRunning = ref(false);
const checkAllDone = ref(0);
const filter = ref<"all" | "update" | "untracked" | "error" | "uptodate">("all");
const editorItem = ref<SoftItem | null>(null);
const editorOpen = ref(false);
const settingsOpen = ref(false);
const pickerItem = ref<SoftItem | null>(null);

/** 主视图切换：软件雷达 / VSCode 插件 */
const view = ref<"radar" | "vscode">("radar");
const vsStats = ref({ total: 0, updates: 0 });
/** VSCode 面板实例：顶栏的 扫描/检查全部 直接调面板的 scan/check */
const vsPanel = ref<InstanceType<typeof VscodePanel> | null>(null);

/** Aurora 自身更新 */
const SELF_DL_ID = "aurora-self-update";
const appVersion = ref("");
const selfInfo = ref<SelfUpdateInfo | null>(null);
const selfDialogOpen = ref(false);
const selfChecking = ref(false);
const selfDownloading = ref(false);

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

function statusOf(item: SoftItem): ItemStatus {
  if (checkingIds.has(item.id)) return "checking";
  if (item.lastError) return "error";
  if (!item.latestVersion) return "idle";
  if (!item.installedVersion) return "untracked";
  return compareVersion(item.latestVersion, item.installedVersion) > 0
    ? "update"
    : "uptodate";
}

onMounted(async () => {
  try {
    config.value = await api.load();
  } catch (e) {
    toast(`读取配置失败: ${e}`, "err");
  }
  ready.value = true;
  void refreshDownloaded();
  try {
    const info = await api.appInfo();
    appVersion.value = info.version;
  } catch {
    appVersion.value = "";
  }
  // 启动时静默检查自身更新（可在设置中关闭）
  if (config.value.settings.autoCheckSelf && api.isTauri) void checkSelf(true);
  await api.onProgress((p) => {
    if (p.status === "progressing") {
      downloads.value[p.itemId] = p;
    } else if (p.status === "done") {
      delete downloads.value[p.itemId];
      downloadingIds.delete(p.itemId);
      donePaths.value[p.itemId] = p.path;
      if (p.itemId === SELF_DL_ID) {
        selfDownloading.value = false;
        toast(`${p.fileName} 下载完成，运行安装包即可完成升级`, "ok");
        setTimeout(() => delete donePaths.value[p.itemId], 60_000);
      } else {
        toast(`${p.fileName} 下载完成`, "ok");
        setTimeout(() => delete donePaths.value[p.itemId], 60_000);
      }
      void refreshDownloaded();
    } else if (p.status === "cancelled") {
      delete downloads.value[p.itemId];
      downloadingIds.delete(p.itemId);
      if (p.itemId === SELF_DL_ID) selfDownloading.value = false;
    } else if (p.status === "error") {
      delete downloads.value[p.itemId];
      downloadingIds.delete(p.itemId);
      if (p.itemId === SELF_DL_ID) selfDownloading.value = false;
      toast(`下载失败：${p.error}`, "err");
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
      if (info.hasUpdate && !silent) toast(`发现新版本 v${info.latestVersion}`, "ok");
      if (info.hasUpdate && silent) {
        toast(`Aurora 有新版本 v${info.latestVersion}，点击左下角版本号查看`, "ok");
      }
      if (!info.hasUpdate && !silent) toast(`Aurora 已是最新（v${info.currentVersion}）`);
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
  if (ver && !containsVersion(stemOnly(name), ver)) {
    const m = name.match(/^(.*?)(\.[A-Za-z0-9]{1,6})$/);
    name = m ? `${m[1]}-${ver}${m[2]}` : `${name}-${ver}`;
  }
  selfDownloading.value = true;
  try {
    await api.download({
      itemId: SELF_DL_ID,
      url: asset.url,
      fileName: name,
      destDir: config.value.settings.downloadDir,
      proxyPrefix: config.value.settings.downloadProxy,
    });
  } catch (e) {
    if (!String(e).includes("取消")) toast(`Aurora 安装包下载失败：${e}`, "err");
  } finally {
    selfDownloading.value = false;
  }
}

function notify(text: string, kind: "ok" | "err" | "info") {
  toast(text, kind);
}

/** VSCode 插件检查结果写入配置，跨会话/切视图恢复 */
function saveVscodeChecks(list: VsixCheck[]) {
  config.value.vscodeChecks = list;
  persist();
}

const counts = computed<Record<string, number>>(() => {
  const c: Record<string, number> = {
    all: 0,
    update: 0,
    uptodate: 0,
    untracked: 0,
    error: 0,
    idle: 0,
  };
  for (const it of config.value.items) {
    c.all++;
    const s = statusOf(it);
    if (s !== "checking") c[s]++;
  }
  return c;
});

const filterDefs = [
  { key: "all", label: "全部" },
  { key: "update", label: "可更新" },
  { key: "untracked", label: "待登记" },
  { key: "error", label: "检测失败" },
  { key: "uptodate", label: "已最新" },
] as const;

const visibleItems = computed(() => {
  const rank: Record<ItemStatus, number> = {
    update: 0,
    error: 1,
    untracked: 2,
    idle: 2,
    checking: 2,
    uptodate: 3,
  };
  return config.value.items
    .filter((it) => filter.value === "all" || statusOf(it) === filter.value)
    .slice()
    .sort((a, b) => rank[statusOf(a)] - rank[statusOf(b)]);
});

async function checkOne(item: SoftItem, silent = false) {
  if (checkingIds.has(item.id)) return;
  checkingIds.add(item.id);
  try {
    const out = await api.check(item, config.value);
    item.checkedAt = Date.now();
    if (out.error) {
      item.lastError = out.error;
      if (!silent) toast(`${item.name}：${out.error}`, "err");
    } else {
      item.lastError = "";
      item.latestVersion = out.version;
      item.releaseUrl = out.releaseUrl;
      item.assets = out.assets;
      item.suggested = out.suggested;
      if (!silent) {
        if (!item.installedVersion) toast(`${item.name} 最新版本 ${out.version}`);
        else if (compareVersion(out.version, item.installedVersion) > 0)
          toast(`${item.name} 有新版本 ${out.version}`, "ok");
        else toast(`${item.name} 已是最新`);
      }
    }
    persist();
  } catch (e) {
    item.lastError = String(e);
    item.checkedAt = Date.now();
    persist();
    if (!silent) toast(`${item.name} 检测失败: ${e}`, "err");
  } finally {
    checkingIds.delete(item.id);
  }
}

async function checkAll() {
  if (checkAllRunning.value || !config.value.items.length) return;
  checkAllRunning.value = true;
  checkAllDone.value = 0;
  const items = [...config.value.items];
  let idx = 0;
  const worker = async () => {
    while (idx < items.length) {
      const it = items[idx++];
      await checkOne(it, true);
      checkAllDone.value++;
    }
  };
  await Promise.all([worker(), worker(), worker(), worker()]);
  checkAllRunning.value = false;
  const upd = counts.value.update;
  toast(
    upd > 0 ? `检查完成，${upd} 个软件可更新` : "检查完成，全部都是最新",
    upd > 0 ? "ok" : "info",
  );
}

function pickFileName(item: SoftItem, a: Asset): string {
  const ver = item.latestVersion.trim();
  let name = a.name.trim();
  if (!/\.[A-Za-z0-9]{1,6}$/.test(name)) {
    return `${item.id || slugify(item.name) || "setup"}-${ver || "latest"}.exe`;
  }
  // 文件名不含版本信息时追加版本号，便于日后与下载目录中的文件匹配
  if (ver && !containsVersion(stemOnly(name), ver)) {
    const m = name.match(/^(.*?)(\.[A-Za-z0-9]{1,6})$/);
    name = m ? `${m[1]}-${ver}${m[2]}` : `${name}-${ver}`;
  }
  return name;
}

function stemOnly(name: string): string {
  return name.replace(/\.[A-Za-z0-9]{1,6}$/, "");
}

/** 扫描下载目录文件列表，配合 downloadedPaths 找出各软件已下载的最新安装包 */
async function refreshDownloaded() {
  const dir = config.value.settings.downloadDir.trim();
  if (!dir) {
    downloadedFiles.value = [];
    return;
  }
  try {
    downloadedFiles.value = await api.listDownloads(dir);
  } catch {
    downloadedFiles.value = [];
  }
}

const downloadedPaths = computed<Record<string, string>>(() => {
  const dir = config.value.settings.downloadDir.trim();
  const map: Record<string, string> = {};
  if (!dir || !downloadedFiles.value.length) return map;
  for (const it of config.value.items) {
    if (!it.latestVersion) continue;
    const names = it.assets.map((a) => a.name).filter(Boolean);
    const hit = matchDownloaded(downloadedFiles.value, it.latestVersion, names);
    if (hit) map[it.id] = joinPath(dir, hit);
  }
  return map;
});

async function startDownload(item: SoftItem, asset?: Asset) {
  const a = asset ?? item.assets[item.suggested] ?? item.assets[0];
  if (!a) return;
  if (!config.value.settings.downloadDir.trim()) {
    toast("请先在设置里填写下载目录", "err");
    settingsOpen.value = true;
    return;
  }
  downloadingIds.add(item.id);
  try {
    await api.download({
      itemId: item.id,
      url: a.url,
      fileName: pickFileName(item, a),
      destDir: config.value.settings.downloadDir,
      proxyPrefix: config.value.settings.downloadProxy,
    });
  } catch (e) {
    if (!String(e).includes("取消")) toast(`${item.name} 下载失败：${e}`, "err");
  } finally {
    downloadingIds.delete(item.id);
  }
}

function markInstalled(item: SoftItem) {
  item.installedVersion = item.latestVersion;
  persist();
}

function openAdd() {
  editorItem.value = null;
  editorOpen.value = true;
}
function openEdit(it: SoftItem) {
  editorItem.value = it;
  editorOpen.value = true;
}

function onSaveEditor(item: SoftItem) {
  if (!item.id) {
    item.id = `${slugify(item.name)}-${Math.random().toString(36).slice(2, 6)}`;
    config.value.items.push(item);
    editorOpen.value = false;
    persist();
    void checkOne(item);
    return;
  }
  const idx = config.value.items.findIndex((i) => i.id === item.id);
  if (idx >= 0) config.value.items[idx] = item;
  editorOpen.value = false;
  persist();
  void checkOne(item);
}

function onDeleteEditor(id: string) {
  if (!window.confirm("确定删除这个软件吗？")) return;
  config.value.items = config.value.items.filter((i) => i.id !== id);
  delete downloads.value[id];
  donePaths.value = Object.fromEntries(
    Object.entries(donePaths.value).filter(([k]) => k !== id),
  );
  editorOpen.value = false;
  persist();
}

function saveSettings(s: Settings) {
  config.value.settings = s;
  settingsOpen.value = false;
  persist();
  void refreshDownloaded();
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
        <button v-if="view === 'radar'" class="btn ghost" @click="openAdd">
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
          :disabled="checkAllRunning || !config.items.length"
          @click="checkAll"
        >
          <span v-if="checkAllRunning" class="spin light" aria-hidden="true"></span>
          {{
            checkAllRunning
              ? `检查中 ${checkAllDone}/${config.items.length}`
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

    <nav
      v-if="view === 'radar' && ready && config.items.length"
      class="filters"
    >
      <button
        v-for="f in filterDefs"
        :key="f.key"
        class="chip"
        :class="{ on: filter === f.key }"
        @click="filter = f.key"
      >
        {{ f.label }}<span class="count">{{ counts[f.key] }}</span>
      </button>
    </nav>

    <main
      v-show="view === 'radar'"
      class="list"
      :class="{ 'is-empty': !visibleItems.length }"
    >
      <div v-if="!ready" class="hint">读取配置中…</div>
      <div v-else-if="!config.items.length" class="hint empty">
        <p>还没有监控任何软件</p>
        <button class="btn primary" @click="openAdd">＋ 添加第一个软件</button>
      </div>
      <div v-else-if="!visibleItems.length" class="hint">这个筛选条件下没有软件</div>
      <template v-else>
        <SoftRow
          v-for="it in visibleItems"
          :key="it.id"
          :item="it"
          :status="statusOf(it)"
          :checking="checkingIds.has(it.id)"
          :downloading="downloadingIds.has(it.id)"
          :dl="downloads[it.id] ?? null"
          :done-path="donePaths[it.id] ?? ''"
          :downloaded-path="downloadedPaths[it.id] ?? ''"
          @check="checkOne(it)"
          @edit="openEdit(it)"
          @download="(a) => startDownload(it, a ?? undefined)"
          @pick="pickerItem = it"
          @cancel="api.cancel(it.id)"
          @mark="markInstalled(it)"
          @set-installed="
            (v) => {
              it.installedVersion = v;
              persist();
            }
          "
          @open-path="(p, r) => openLocal(p, r)"
          @open-url="openUrl"
        />
      </template>
    </main>

    <VscodePanel
      v-show="view === 'vscode'"
      ref="vsPanel"
      :settings="config.settings"
      :initial-checks="config.vscodeChecks"
      @open-settings="settingsOpen = true"
      @open-path="openLocal"
      @notify="notify"
      @stats="(t, u) => (vsStats = { total: t, updates: u })"
      @save-checks="saveVscodeChecks"
    />

    <footer class="statusbar">
      <template v-if="view === 'radar'">
        <span>
          {{ config.items.length }} 项受监控 ·
          <b :class="{ amber: counts.update > 0 }">{{ counts.update }}</b> 项可更新
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

    <ItemEditor
      v-if="editorOpen"
      :item="editorItem"
      @save="onSaveEditor"
      @delete="onDeleteEditor"
      @close="editorOpen = false"
    />
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
    <AssetPicker
      v-if="pickerItem"
      :item="pickerItem"
      @download="
        (a) => {
          const it = pickerItem;
          pickerItem = null;
          if (it) startDownload(it, a);
        }
      "
      @close="pickerItem = null"
    />
    <SelfUpdateDialog
      v-if="selfDialogOpen && selfInfo"
      :info="selfInfo"
      :checking="selfChecking"
      :downloading="selfDownloading"
      :dl="downloads[SELF_DL_ID] ?? null"
      @check="checkSelf()"
      @download="startSelfDownload"
      @open-url="openUrl"
      @close="selfDialogOpen = false"
    />

    <div class="toasts" aria-live="polite">
      <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
        {{ t.text }}
      </div>
    </div>
  </div>
</template>
