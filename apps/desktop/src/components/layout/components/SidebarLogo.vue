<script setup lang="ts">
/**
 * Sidebar 顶部 logo header（高度同 navbar）
 * 左侧 logo+文字，点击回首页（由父组件处理路由跳转）
 */
import Logo from "@/components/common/Logo.vue";

defineProps<{ collapsed: boolean }>();
defineEmits<{ (e: "home"): void }>();
</script>

<template>
  <div class="sidebar-logo">
    <div class="logo-brand" @click="$emit('home')">
      <Logo :size="26" class="logo-img" />
      <span v-if="!collapsed" class="logo-text">byte-wiki</span>
    </div>
  </div>
</template>

<style scoped>
/* ===== logo header ===== */
.sidebar-logo {
  display: flex;
  align-items: center;
  height: var(--navbar-height);
  padding: 0 var(--spacing-md);
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.logo-brand {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  cursor: pointer;
  min-width: 0;
  /* Logo 用 currentColor 继承此色，自动跟随主题（浅色=深、深色=浅） */
  color: var(--text-primary);
}
.logo-img {
  flex-shrink: 0;
  /* 透明 logo 无背景方块，display:block 避免行内基线导致的 1px 缝隙 */
  display: block;
}
.logo-text {
  font-size: 14px;
  font-weight: 700;
  white-space: nowrap;
  color: var(--color-title);
}
</style>
