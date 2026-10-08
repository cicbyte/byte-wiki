<script setup lang="ts">
/**
 * Sidebar 底部：与右侧 global-footer 水平对齐，预留版本/状态信息
 */
defineProps<{ collapsed: boolean }>();
</script>

<template>
  <div class="sidebar-bottom">
    <span v-if="!collapsed" class="sidebar-version">byte-wiki v0.1</span>
  </div>
</template>

<style scoped>
/* ===== 底部 ===== */
.sidebar-bottom {
  height: var(--statusbar-height);
  padding: 0 var(--spacing-md);
  display: flex;
  align-items: center;
  border-top: 1px solid var(--border-light);
  flex-shrink: 0;
  overflow: hidden;
}
.sidebar-version {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
</style>
