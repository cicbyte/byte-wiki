<script setup lang="ts">
/**
 * StatCard —— 概览统计卡（图标 + 数值 + 标签，可选「查看→」入口）
 *
 * variant 决定图标颜色：proj(主色) / snip(成功) / note(警告)。
 * go=true 时显示「查看 →」提示（如分类概览卡跳转列表页）。
 */
import type { Component } from "vue";

withDefaults(
  defineProps<{
    icon: Component;
    variant?: "proj" | "snip" | "note";
    value: string | number;
    label: string;
    go?: boolean;
  }>(),
  { variant: "proj", go: false },
);
defineEmits<{ (e: "click"): void }>();
</script>

<template>
  <div class="stat-card" @click="$emit('click')">
    <component :is="icon" :class="['stat-icon', variant]" />
    <div class="stat-value">{{ value }}</div>
    <div class="stat-label">{{ label }}</div>
    <div v-if="go" class="stat-go">查看 →</div>
  </div>
</template>

<style scoped>
.stat-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: var(--spacing-lg) var(--spacing-md);
  background: var(--bg-elevated);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.stat-card:hover {
  border-color: var(--color-primary-border);
  transform: translateY(-2px);
}
.stat-icon {
  font-size: 22px;
  margin-bottom: 8px;
}
.stat-icon.proj {
  color: var(--color-primary);
}
.stat-icon.snip {
  color: var(--color-success);
}
.stat-icon.note {
  color: var(--color-warning);
}
.stat-value {
  font-size: 30px;
  font-weight: 700;
  color: var(--text-primary);
  font-family: "JetBrains Mono", monospace;
  line-height: 1;
}
.stat-label {
  font-size: 13px;
  color: var(--text-tertiary);
  margin-top: 4px;
}
.stat-go {
  font-size: 11px;
  color: var(--color-primary);
  margin-top: 8px;
}
</style>
