<script setup lang="ts">
/**
 * Panel —— 通用面板外壳
 *
 * 统一 .panel 外壳 + 可选 head（标题/计数/图标/actions）。
 * head 内容：优先用 head slot；否则用 title/icon/count props + actions slot。
 * body 由默认 slot 自由填充（直接放 .panel-body / detail-tabs / EmptyState 等）。
 * empty slot 依赖外壳 position:relative 实现 absolute 居中。
 *
 * 注意：head/body 相关 class（.head-title/.head-count/.head-icon/.head-actions/
 * .icon-action/.panel-body）用 :deep 暴露，使其同时作用于 Panel 内部渲染与
 * 各 slot 传入的内容（scoped 默认不穿透 slot）。
 */
import type { Component } from "vue";

defineProps<{
  title?: string;
  count?: string | number;
  icon?: Component;
}>();
</script>

<template>
  <section class="panel">
    <header
      v-if="$slots.head || title || icon || count !== undefined"
      class="panel-head"
    >
      <slot name="head">
        <component :is="icon" v-if="icon" class="head-icon" />
        <span v-if="title" class="head-title">{{ title }}</span>
        <span v-if="count !== undefined" class="head-count">{{ count }}</span>
        <div v-if="$slots.actions" class="head-actions"><slot name="actions" /></div>
      </slot>
    </header>
    <slot />
    <slot name="empty" />
  </section>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
  min-height: 0;
  position: relative; /* empty slot absolute 居中的定位锚点 */
}
.panel-head {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  height: 44px;
  padding: 0 var(--spacing-md);
  border-bottom: 1px solid var(--border-light);
  background: var(--bg-card);
  flex-shrink: 0;
}
:deep(.head-icon) {
  font-size: 15px;
  color: var(--color-primary);
}
:deep(.head-title) {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  line-height: 1.5;
}
:deep(.head-count) {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-input);
  padding: 1px 7px;
  border-radius: 10px;
}
:deep(.head-actions) {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
}
:deep(.icon-action) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 14px;
  transition: all var(--transition-fast);
}
:deep(.icon-action:hover) {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
:deep(.icon-action.danger:hover) {
  color: var(--color-danger);
}
/* 默认 slot 里若用 <div class="panel-body"> 作可滚动正文容器，:deep 穿透到 slot 内容 */
:deep(.panel-body) {
  flex: 1;
  overflow-y: auto;
  padding: var(--spacing-sm);
  position: relative;
}
</style>
