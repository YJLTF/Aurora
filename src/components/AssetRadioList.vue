<script setup lang="ts">
import type { Asset } from "../types";
import { fmtSize } from "../utils";

/** 安装包单选列表：AssetPicker 弹窗与 Aurora 自更新弹窗共用 */
defineProps<{
  assets: Asset[];
  /** 推荐项下标（列表内打「推荐」标） */
  suggested: number;
  /** 当前选中项的 url（v-model:selected） */
  selected: string;
}>();

defineEmits<{
  (e: "update:selected", url: string): void;
}>();

// radio 的 name 需要按实例区分，避免同页多个弹窗互相干扰
const group = `asset-${Math.random().toString(36).slice(2, 8)}`;
</script>

<template>
  <label
    v-for="a in assets"
    :key="a.url"
    class="asset"
    :class="{ on: a.url === selected }"
  >
    <input
      type="radio"
      :name="group"
      :value="a.url"
      :model-value="selected"
      @change="$emit('update:selected', a.url)"
    />
    <span class="aname mono">{{ a.name }}</span>
    <span class="asize">{{ fmtSize(a.size) }}</span>
    <span v-if="assets[suggested]?.url === a.url" class="arec">推荐</span>
  </label>
</template>
