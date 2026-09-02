<script setup lang="ts">
/**
 * 软件雷达面板：清单列表、筛选条、单项/全部检查、下载与本地版本登记，
 * 以及 编辑软件（ItemEditor）/ 选择安装包（AssetPicker）两个雷达内弹窗。
 * 清单由父级传入（同一响应式数组，原地增删改），持久化通过 persist 事件交回父级。
 */
import { computed, reactive, ref, watch } from "vue";
import { api } from "../api";
import { dlStore } from "../download";
import { compareVersion } from "../types";
import type {
  Asset,
  DownloadProgress,
  ItemStatus,
  Settings,
  SoftItem,
} from "../types";
import {
  containsVersion,
  joinPath,
  matchDownloaded,
  slugify,
  stemOf,
  withVersionSuffix,
} from "../utils";
import SoftRow from "./SoftRow.vue";
import ItemEditor from "./ItemEditor.vue";
import AssetPicker from "./AssetPicker.vue";

const props = defineProps<{
  /** 配置是否已加载完成 */
  ready: boolean;
  /** 受监控清单（父级 config.items 的同一响应式数组） */
  items: SoftItem[];
  settings: Settings;
}>();

const emit = defineEmits<{
  (e: "openSettings"): void;
  (e: "openPath", path: string, reveal?: boolean): void;
  (e: "openUrl", url: string): void;
  (e: "notify", text: string, kind?: "ok" | "err" | "info"): void;
  /** 数据有变更，请父级持久化配置 */
  (e: "persist"): void;
  (e: "stats", total: number, updates: number): void;
}>();

const checkingIds = reactive(new Set<string>());
/** 已完成下载的条目：itemId → 文件完整路径（一分钟后自动消失） */
const donePaths = ref<Record<string, string>>({});
const downloadedFiles = ref<string[]>([]);
/** 全部检查进度（顶栏按钮读取） */
const busy = reactive({ running: false, done: 0, total: 0 });
const filter = ref<"all" | "update" | "untracked" | "error" | "uptodate">("all");
const editorItem = ref<SoftItem | null>(null);
const editorOpen = ref(false);
const pickerItem = ref<SoftItem | null>(null);

function statusOf(item: SoftItem): ItemStatus {
  if (checkingIds.has(item.id)) return "checking";
  if (item.lastError) return "error";
  if (!item.latestVersion) return "idle";
  if (!item.installedVersion) return "untracked";
  return compareVersion(item.latestVersion, item.installedVersion) > 0
    ? "update"
    : "uptodate";
}

const counts = computed<Record<string, number>>(() => {
  const c: Record<string, number> = {
    all: 0,
    update: 0,
    uptodate: 0,
    untracked: 0,
    error: 0,
  };
  for (const it of props.items) {
    c.all++;
    const s = statusOf(it);
    if (s !== "checking" && s in c) c[s]++;
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
  return props.items
    .filter((it) => filter.value === "all" || statusOf(it) === filter.value)
    .slice()
    .sort((a, b) => rank[statusOf(a)] - rank[statusOf(b)]);
});

// 状态栏汇总（受监控总数 / 可更新数）随数据变化上报
watch(
  () => [props.items.length, counts.value.update] as const,
  ([total, updates]) => emit("stats", total, updates),
  { immediate: true },
);

/** 扫描下载目录文件列表，配合 downloadedPaths 找出各软件已下载的最新安装包 */
async function refreshDownloaded() {
  const dir = props.settings.downloadDir.trim();
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

// 下载目录设置（异步载入或用户修改）变化时重扫
watch(
  () => props.settings.downloadDir,
  () => void refreshDownloaded(),
  { immediate: true },
);

/** 下载完成转发入口（App 的全局进度订阅调用）：登记完成条目并刷新「已下载识别」 */
function handleDone(p: DownloadProgress) {
  donePaths.value[p.itemId] = p.path;
  setTimeout(() => delete donePaths.value[p.itemId], 60_000);
  void refreshDownloaded();
}

async function checkOne(item: SoftItem, silent = false) {
  if (checkingIds.has(item.id)) return;
  checkingIds.add(item.id);
  try {
    const out = await api.check(item, props.settings);
    item.checkedAt = Date.now();
    if (out.error) {
      item.lastError = out.error;
      if (!silent) emit("notify", `${item.name}：${out.error}`, "err");
    } else {
      item.lastError = "";
      item.latestVersion = out.version;
      item.releaseUrl = out.releaseUrl;
      item.assets = out.assets;
      item.suggested = out.suggested;
      if (!silent) {
        if (!item.installedVersion) emit("notify", `${item.name} 最新版本 ${out.version}`);
        else if (compareVersion(out.version, item.installedVersion) > 0)
          emit("notify", `${item.name} 有新版本 ${out.version}`, "ok");
        else emit("notify", `${item.name} 已是最新`);
      }
    }
    emit("persist");
  } catch (e) {
    item.lastError = String(e);
    item.checkedAt = Date.now();
    emit("persist");
    if (!silent) emit("notify", `${item.name} 检测失败: ${e}`, "err");
  } finally {
    checkingIds.delete(item.id);
  }
}

/** 并发检查全部软件 */
async function checkAll() {
  if (busy.running || !props.items.length) return;
  busy.running = true;
  busy.done = 0;
  busy.total = props.items.length;
  const items = [...props.items];
  let idx = 0;
  const worker = async () => {
    while (idx < items.length) {
      await checkOne(items[idx++], true);
      busy.done++;
    }
  };
  await Promise.all([worker(), worker(), worker(), worker()]);
  busy.running = false;
  const upd = counts.value.update;
  emit(
    "notify",
    upd > 0 ? `检查完成，${upd} 个软件可更新` : "检查完成，全部都是最新",
    upd > 0 ? "ok" : "info",
  );
}

function pickFileName(item: SoftItem, a: Asset): string {
  const ver = item.latestVersion.trim();
  const name = a.name.trim();
  if (!/\.[A-Za-z0-9]{1,6}$/.test(name)) {
    return `${item.id || slugify(item.name) || "setup"}-${ver || "latest"}.exe`;
  }
  // 文件名不含版本信息时追加版本号，便于日后与下载目录中的文件匹配
  return ver && !containsVersion(stemOf(name), ver)
    ? withVersionSuffix(name, ver)
    : name;
}

const downloadedPaths = computed<Record<string, string>>(() => {
  const dir = props.settings.downloadDir.trim();
  const map: Record<string, string> = {};
  if (!dir || !downloadedFiles.value.length) return map;
  for (const it of props.items) {
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
  if (!props.settings.downloadDir.trim()) {
    emit("notify", "请先在设置里填写下载目录", "err");
    emit("openSettings");
    return;
  }
  await dlStore.queue({
    itemId: item.id,
    url: a.url,
    fileName: pickFileName(item, a),
    destDir: props.settings.downloadDir,
    proxyPrefix: props.settings.downloadProxy,
  });
}

function markInstalled(item: SoftItem) {
  item.installedVersion = item.latestVersion;
  emit("persist");
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
    props.items.push(item);
    editorOpen.value = false;
    emit("persist");
    void checkOne(item);
    return;
  }
  const idx = props.items.findIndex((i) => i.id === item.id);
  if (idx >= 0) props.items[idx] = item;
  editorOpen.value = false;
  emit("persist");
  void checkOne(item);
}

function onDeleteEditor(id: string) {
  if (!window.confirm("确定删除这个软件吗？")) return;
  const idx = props.items.findIndex((i) => i.id === id);
  if (idx >= 0) props.items.splice(idx, 1);
  dlStore.drop(id);
  delete donePaths.value[id];
  editorOpen.value = false;
  emit("persist");
}

/** 供顶栏调用：添加软件 / 检查全部 / 下载完成转发 */
defineExpose({ openAdd, checkAll, handleDone, busy });
</script>

<template>
  <div class="radar-wrap">
    <nav v-if="ready && items.length" class="filters">
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
      <div v-else-if="!items.length" class="hint empty">
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
          :downloading="dlStore.active.has(it.id)"
          :dl="dlStore.downloads[it.id] ?? null"
          :done-path="donePaths[it.id] ?? ''"
          :downloaded-path="downloadedPaths[it.id] ?? ''"
          @check="checkOne(it)"
          @edit="openEdit(it)"
          @download="(a: Asset | null) => startDownload(it, a ?? undefined)"
          @pick="pickerItem = it"
          @cancel="dlStore.cancel(it.id)"
          @mark="markInstalled(it)"
          @set-installed="
            (v: string) => {
              it.installedVersion = v;
              emit('persist');
            }
          "
          @open-path="(p: string, r?: boolean) => emit('openPath', p, r)"
          @open-url="(u: string) => emit('openUrl', u)"
          @pause="dlStore.pause(it.id)"
          @resume="dlStore.resume(it.id)"
        />
      </template>
    </main>

    <ItemEditor
      v-if="editorOpen"
      :item="editorItem"
      @save="onSaveEditor"
      @delete="onDeleteEditor"
      @close="editorOpen = false"
    />
    <AssetPicker
      v-if="pickerItem"
      :item="pickerItem"
      @download="
        (a: Asset) => {
          const it = pickerItem;
          pickerItem = null;
          if (it) startDownload(it, a);
        }
      "
      @close="pickerItem = null"
    />
  </div>
</template>
