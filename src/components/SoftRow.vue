<script setup lang="ts">
import { computed, ref } from "vue";
import type { Asset, DownloadProgress, ItemStatus, SoftItem } from "../types";
import { timeAgo } from "../utils";

const props = defineProps<{
  item: SoftItem;
  status: ItemStatus;
  checking: boolean;
  downloading: boolean;
  dl: DownloadProgress | null;
  donePath: string;
  downloadedPath: string;
}>();

const emit = defineEmits<{
  (e: "check"): void;
  (e: "edit"): void;
  (e: "download", asset: Asset | null): void;
  (e: "pick"): void;
  (e: "cancel"): void;
  (e: "pause"): void;
  (e: "resume"): void;
  (e: "mark"): void;
  (e: "setInstalled", version: string): void;
  (e: "openPath", path: string, reveal?: boolean): void;
  (e: "openUrl", url: string): void;
}>();

const statusLabel = computed(
  () =>
    ({
      checking: "检查中",
      update: "可更新",
      uptodate: "已最新",
      untracked: "待登记",
      error: "检测失败",
      idle: "未检测",
    })[props.status],
);

const sourceLabel = computed(() => {
  const s = props.item.source;
  if (s.type === "github") return `GitHub · ${s.repo}`;
  try {
    return `页面 · ${new URL(s.checkUrl).host}`;
  } catch {
    return "页面 · 自定义";
  }
});

const sourceTitle = computed(() => {
  const s = props.item.source;
  return s.type === "github" ? `https://github.com/${s.repo}/releases` : s.checkUrl;
});

const latestLabel = computed(() => props.item.latestVersion || "—");
const hasAssets = computed(() => props.item.assets.length > 0);

const pct = computed(() => {
  const d = props.dl;
  if (!d) return 0;
  if (!d.total) return 0;
  return Math.min(100, Math.round((d.received / d.total) * 100));
});

/** 进度条旁的状态文案：区分 下载中/已暂停/失败 */
const dlText = computed(() => {
  const d = props.dl;
  if (!d) return "";
  const size = d.total
    ? `${pct.value}% · ${(d.received / 1048576).toFixed(1)}/${(d.total / 1048576).toFixed(1)} MB`
    : `${(d.received / 1048576).toFixed(1)} MB`;
  if (d.status === "paused") return `${d.fileName} · 已暂停 · ${size}`;
  if (d.status === "error") return `${d.fileName} · ${d.error}`;
  return `${d.fileName} · ${size}`;
});

const doneName = computed(
  () => props.donePath.split(/[\\/]/).pop() ?? props.donePath,
);

const downloadedName = computed(
  () => props.downloadedPath.split(/[\\/]/).pop() ?? props.downloadedPath,
);

// 本地版本行内编辑
const editing = ref(false);
const draft = ref("");
const vFocus = { mounted: (el: HTMLElement) => el.focus() };

function startEdit() {
  draft.value = props.item.installedVersion;
  editing.value = true;
}
function commit() {
  if (!editing.value) return;
  editing.value = false;
  const v = draft.value.trim();
  if (v !== props.item.installedVersion) emit("setInstalled", v);
}
</script>

<template>
  <article class="row" :data-status="status">
    <div class="appicon" aria-hidden="true">{{ item.icon }}</div>

    <div class="meta">
      <div class="name-line">
        <span class="name">{{ item.name || item.id }}</span>
        <span class="pill" :class="status">{{ statusLabel }}</span>
      </div>
      <div class="sub">
        <span class="src" :title="sourceTitle">{{ sourceLabel }}</span>
        <a
          v-if="item.releaseUrl"
          href="#"
          class="link"
          @click.prevent="emit('openUrl', item.releaseUrl)"
          >发布页 ↗</a
        >
        <a
          v-else-if="item.homepage"
          href="#"
          class="link"
          @click.prevent="emit('openUrl', item.homepage)"
          >主页 ↗</a
        >
        <span v-if="item.checkedAt" class="checked-at">{{ timeAgo(item.checkedAt) }}检查</span>
      </div>
      <div
        v-if="status === 'error' && item.lastError"
        class="err"
        :title="item.lastError"
      >
        {{ item.lastError }}
      </div>

      <div v-if="dl" class="dl" role="status">
        <div class="bar" :class="{ indet: !dl.total }">
          <div class="fill" :style="{ width: pct + '%' }"></div>
        </div>
        <span class="dl-text">{{ dlText }}</span>
        <button
          v-if="dl.status === 'progressing'"
          class="mini"
          @click="emit('pause')"
        >
          暂停
        </button>
        <button
          v-else-if="dl.status === 'paused' || dl.status === 'error'"
          class="mini"
          @click="emit('resume')"
        >
          {{ dl.status === "paused" ? "继续" : "重试" }}
        </button>
        <button class="mini danger" @click="emit('cancel')">取消</button>
      </div>
      <div v-else-if="donePath" class="dl-done">
        <span class="ok-mark">✓</span>
        <span class="dl-text" :title="donePath">{{ doneName }}</span>
        <button class="mini" @click="emit('openPath', donePath)">打开</button>
      </div>
      <div
        v-else-if="downloadedPath && status !== 'checking'"
        class="dl-done"
        role="status"
      >
        <span class="ok-mark">✓</span>
        <span class="dl-text" :title="downloadedPath">
          最新版本安装包已下载 · {{ downloadedName }}
        </span>
        <button
          class="mini"
          title="在资源管理器中定位该文件"
          @click="emit('openPath', downloadedPath, true)"
        >
          定位
        </button>
      </div>
    </div>

    <div class="versions">
      <div class="vcell">
        <span class="vlabel">本地</span>
        <input
          v-if="editing"
          v-model="draft"
          v-focus
          class="vedit"
          spellcheck="false"
          @keydown.enter.prevent="commit"
          @keydown.esc.prevent="editing = false"
          @blur="commit"
        />
        <button
          v-else
          class="vval clickable"
          :class="{ ghosttext: !item.installedVersion }"
          :title="item.installedVersion ? '点击修改本地版本' : '点击登记当前安装的版本'"
          @click="startEdit"
        >
          {{ item.installedVersion || "未登记" }}
        </button>
      </div>
      <span class="varrow" :class="{ lit: status === 'update' }">→</span>
      <div class="vcell">
        <span class="vlabel">最新</span>
        <div class="vline">
          <span
            class="vval"
            :class="{ hot: status === 'update', ok: status === 'uptodate' }"
          >
            {{ latestLabel }}</span
          >
          <button
            v-if="status === 'update'"
            class="mark"
            title="把最新版本登记为本地版本"
            @click="emit('mark')"
          >
            设为已装
          </button>
        </div>
      </div>
    </div>

    <div class="ops">
      <button
        class="btn ghost sm"
        :disabled="checking || downloading"
        @click="emit('check')"
      >
        <span v-if="checking" class="spin" aria-hidden="true"></span>
        {{ checking ? "检查中" : "检查" }}
      </button>
      <button
        v-if="hasAssets"
        class="btn primary sm"
        :disabled="downloading || !item.latestVersion"
        @click="emit('download', null)"
      >
        下载<span
          v-if="item.assets.length > 1"
          class="caret"
          title="选择其他安装包"
          @click.stop="emit('pick')"
          >▾</span
        >
      </button>
      <button class="btn ghost sm iconbtn" title="编辑" @click="emit('edit')">
        ✎
      </button>
    </div>
  </article>
</template>
