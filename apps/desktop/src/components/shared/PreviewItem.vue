<script setup lang="ts">
/**
 * PreviewItem —— 通用列表项（图标 + 标题 + 可选 meta/lang/desc）
 *
 * 统一 categories .preview-item / languages .list-item / tags .rel-row。
 * variant 决定图标颜色；右侧副信息三选一：
 *   meta = 灰色 mono（如 short_id）
 *   lang = 语言 badge（绿色）
 *   desc = 灰色省略文字（如 description）
 * 也可用 meta slot 完全自定义右侧。
 */
import type { Component } from "vue";

withDefaults(
  defineProps<{
    icon?: Component;
    variant?: "proj" | "snip" | "note";
    title: string;
    meta?: string;
    lang?: string;
    desc?: string;
  }>(),
  { variant: "proj" },
);
defineEmits<{ (e: "click"): void }>();
</script>

<template>
  <div class="preview-item" @click="$emit('click')">
    <component :is="icon" v-if="icon" :class="['preview-icon', variant]" />
    <span class="preview-title">{{ title }}</span>
    <slot name="meta">
      <span v-if="lang" class="preview-lang">{{ lang }}</span>
      <span v-else-if="desc" class="preview-desc">{{ desc }}</span>
      <span v-else-if="meta" class="preview-meta">{{ meta }}</span>
    </slot>
  </div>
</template>

<style scoped>
.preview-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--transition-fast);
}
.preview-item:hover {
  background: var(--bg-elevated);
}
.preview-icon {
  font-size: 14px;
  flex-shrink: 0;
}
.preview-icon.proj {
  color: var(--color-primary);
}
.preview-icon.snip {
  color: var(--color-success);
}
.preview-icon.note {
  color: var(--color-warning);
}
.preview-title {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.preview-meta {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  flex-shrink: 0;
}
.preview-lang {
  font-size: 11px;
  color: var(--color-success);
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-input);
  padding: 1px 6px;
  border-radius: 4px;
  flex-shrink: 0;
}
.preview-desc {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex-shrink: 0;
  max-width: 45%;
}
</style>
