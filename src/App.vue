<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { api } from "./api";
import { compareVersion } from "./types";
import type {
  Asset,
  Config,
  DownloadProgress,
  ItemStatus,
  Settings,
  SoftItem,
} from "./types";
import { containsVersion, joinPath, matchDownloaded, slugify } from "./utils";
import SoftRow from "./components/SoftRow.vue";
import ItemEditor from "./components/ItemEditor.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import AssetPicker from "./components/AssetPicker.vue";

const config = ref<Config>({
  settings: {
    downloadDir: "",
    githubApiBase: "https://api.github.com",
    downloadProxy: "",
    githubToken: "",
  },
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
  await api.onProgress((p) => {
    if (p.status === "progressing") {
      downloads.value[p.itemId] = p;
    } else if (p.status === "done") {
      delete downloads.value[p.itemId];
      downloadingIds.delete(p.itemId);
      donePaths.value[p.itemId] = p.path;
      toast(`${p.fileName} 下载完成`, "ok");
      setTimeout(() => delete donePaths.value[p.itemId], 60_000);
      void refreshDownloaded();
    } else if (p.status === "cancelled") {
      delete downloads.value[p.itemId];
      downloadingIds.delete(p.itemId);
    } else if (p.status === "error") {
      delete downloads.value[p.itemId];
      downloadingIds.delete(p.itemId);
      toast(`下载失败：${p.error}`, "err");
    }
  });
});

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
        <button class="btn ghost" @click="openAdd">＋ 添加软件</button>
        <button class="btn ghost" @click="settingsOpen = true">设置</button>
        <button
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
      </div>
    </header>

    <nav v-if="ready && config.items.length" class="filters">
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

    <main class="list" :class="{ 'is-empty': !visibleItems.length }">
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

    <footer class="statusbar">
      <span>
        {{ config.items.length }} 项受监控 ·
        <b :class="{ amber: counts.update > 0 }">{{ counts.update }}</b> 项可更新
      </span>
      <span class="grow"></span>
      <button
        v-if="config.settings.downloadDir"
        class="linklike"
        title="打开下载目录"
        @click="openLocal(config.settings.downloadDir)"
      >
        下载目录：{{ config.settings.downloadDir }}
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
      @save="saveSettings"
      @close="settingsOpen = false"
      @open-dir="openLocal(config.settings.downloadDir)"
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

    <div class="toasts" aria-live="polite">
      <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
        {{ t.text }}
      </div>
    </div>
  </div>
</template>
