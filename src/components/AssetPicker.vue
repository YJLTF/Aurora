<script setup lang="ts">
import { ref } from "vue";
import type { Asset, SoftItem } from "../types";
import AssetRadioList from "./AssetRadioList.vue";

const props = defineProps<{ item: SoftItem }>();

const emit = defineEmits<{
  (e: "download", asset: Asset): void;
  (e: "close"): void;
}>();

const selected = ref(props.item.assets[props.item.suggested]?.url ?? props.item.assets[0]?.url ?? "");

function picked(): Asset | null {
  return props.item.assets.find((a) => a.url === selected.value) ?? null;
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="dialog narrow" role="dialog" aria-modal="true" aria-label="选择安装包">
      <header class="dlg-head">
        <h2>选择安装包 · {{ item.name }}</h2>
        <button class="btn ghost iconbtn" title="关闭" @click="emit('close')">✕</button>
      </header>

      <div class="dlg-body asset-list">
        <p v-if="!item.assets.length" class="empty-hint">没有可下载的附件</p>
        <AssetRadioList
          v-else
          v-model:selected="selected"
          :assets="item.assets"
          :suggested="item.suggested"
        />
      </div>

      <footer class="dlg-foot">
        <span class="spacer"></span>
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="!picked()" @click="picked() && emit('download', picked()!)">
          下载
        </button>
      </footer>
    </div>
  </div>
</template>
