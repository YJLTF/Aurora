<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { Asset, DownloadProgress, SelfUpdateInfo } from "../types";
import { fmtSize } from "../utils";

const props = defineProps<{
  info: SelfUpdateInfo;
  checking: boolean;
  downloading: boolean;
  dl: DownloadProgress | null;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "check"): void;
  (e: "download", asset: Asset): void;
  (e: "openUrl", url: string): void;
  (e: "pause"): void;
  (e: "resume"): void;
  (e: "cancel"): void;
}>();

const selected = ref(props.info.assets[props.info.suggested]?.url ?? props.info.assets[0]?.url ?? "");

// 重新检查后资产列表可能变化，重置选中项
watch(
  () => props.info,
  (i) => {
    selected.value = i.assets[i.suggested]?.url ?? i.assets[0]?.url ?? "";
  },
);

const state = computed(() =>
  props.info.error ? "error" : props.info.hasUpdate ? "update" : "uptodate",
);

const pct = computed(() => {
  const d = props.dl;
  if (!d || !d.total) return 0;
  return Math.min(100, Math.round((d.received / d.total) * 100));
});

function picked(): Asset | null {
  return props.info.assets.find((a) => a.url === selected.value) ?? null;
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Aurora 更新">
      <header class="dlg-head">
        <h2>Aurora 软件更新</h2>
        <button class="btn ghost iconbtn" title="关闭" @click="emit('close')">✕</button>
      </header>

      <div class="dlg-body">
        <div class="self-versions">
          <div class="vcell">
            <span class="vlabel">当前版本</span>
            <span class="vval mono">v{{ info.currentVersion }}</span>
          </div>
          <span class="varrow" :class="{ lit: state === 'update' }">→</span>
          <div class="vcell">
            <span class="vlabel">最新版本</span>
            <span
              v-if="state !== 'error'"
              class="vval mono"
              :class="{ hot: state === 'update', ok: state === 'uptodate' }"
            >
              v{{ info.latestVersion }}</span
            >
            <span v-else class="vval ghosttext">—</span>
          </div>
          <span v-if="state === 'update'" class="pill update">可更新</span>
          <span v-else-if="state === 'uptodate'" class="pill uptodate">已最新</span>
          <span v-else class="pill error">检查失败</span>
        </div>

        <p v-if="state === 'error'" class="err">{{ info.error }}</p>
        <p v-else-if="state === 'uptodate'" class="fhint">
          Aurora 已是最新版本。发布页可查看历史版本：
          <a href="#" class="link" @click.prevent="emit('openUrl', info.releaseUrl)">GitHub Releases ↗</a>
        </p>

        <div v-if="state === 'update' && info.notes" class="field">
          <span class="flabel">更新说明</span>
          <pre class="release-notes mono">{{ info.notes }}</pre>
        </div>

        <div v-if="state === 'update' && info.assets.length" class="field">
          <span class="flabel">升级安装包</span>
          <div class="asset-col">
            <label
              v-for="a in info.assets"
              :key="a.url"
              class="asset"
              :class="{ on: a.url === selected }"
            >
              <input v-model="selected" type="radio" name="self-asset" :value="a.url" />
              <span class="aname mono">{{ a.name }}</span>
              <span class="asize">{{ fmtSize(a.size) }}</span>
              <span v-if="info.assets[info.suggested]?.url === a.url" class="arec">推荐</span>
            </label>
          </div>
        </div>

        <div v-if="dl && dl.status !== 'done'" class="dl" role="status">
          <div class="bar" :class="{ indet: !dl.total || dl.status === 'error' }">
            <div class="fill" :style="{ width: pct + '%' }"></div>
          </div>
          <span class="dl-text">{{
            dl.status === "paused"
              ? `${dl.fileName} · 已暂停 · ${pct}%`
              : dl.status === "error"
                ? `${dl.fileName} · ${dl.error}`
                : dl.total
                  ? `${dl.fileName} · ${pct}% · ${(dl.received / 1048576).toFixed(1)}/${(dl.total / 1048576).toFixed(1)} MB`
                  : `${dl.fileName} · ${(dl.received / 1048576).toFixed(1)} MB`
          }}</span>
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
      </div>

      <footer class="dlg-foot">
        <button class="btn ghost sm" :disabled="checking || downloading" @click="emit('check')">
          <span v-if="checking" class="spin" aria-hidden="true"></span>
          重新检查
        </button>
        <span class="spacer"></span>
        <button
          v-if="info.releaseUrl"
          class="btn ghost"
          :disabled="downloading"
          @click="emit('openUrl', info.releaseUrl)"
        >
          打开发布页
        </button>
        <button
          v-if="state === 'update' && picked()"
          class="btn primary"
          :disabled="checking || downloading"
          @click="picked() && emit('download', picked()!)"
        >
          <span v-if="downloading" class="spin light" aria-hidden="true"></span>
          下载安装包
        </button>
      </footer>
    </div>
  </div>
</template>
