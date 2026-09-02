<script setup lang="ts">
import { computed } from "vue";
import type { DownloadProgress } from "../types";

/** 单条下载进度行：进度条 + 状态文案 + 暂停/继续/重试/取消 */
const props = defineProps<{ dl: DownloadProgress }>();

const emit = defineEmits<{
  (e: "pause"): void;
  (e: "resume"): void;
  (e: "cancel"): void;
}>();

const pct = computed(() =>
  props.dl.total
    ? Math.min(100, Math.round((props.dl.received / props.dl.total) * 100))
    : 0,
);

const size = computed(() =>
  props.dl.total
    ? `${pct.value}% · ${(props.dl.received / 1048576).toFixed(1)}/${(props.dl.total / 1048576).toFixed(1)} MB`
    : `${(props.dl.received / 1048576).toFixed(1)} MB`,
);

const text = computed(() => {
  const d = props.dl;
  if (d.status === "paused") return `${d.fileName} · 已暂停 · ${size.value}`;
  if (d.status === "error") return `${d.fileName} · ${d.error}`;
  return `${d.fileName} · ${size.value}`;
});
</script>

<template>
  <div class="dl" role="status" :data-status="dl.status">
    <div class="bar" :class="{ indet: !dl.total }">
      <div class="fill" :style="{ width: pct + '%' }"></div>
    </div>
    <span class="dl-text">{{ text }}</span>
    <button v-if="dl.status === 'progressing'" class="mini" @click="emit('pause')">
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
</template>
