<script setup lang="ts">
/**
 * 配置面板（mcp-service → 配置 tab）
 *
 * 渲染 stdio / HTTP 接入说明（MdPreview）。自管只读视图：
 * - 配置文案来自 ../lib/constants 的 mcpGuide
 * - 主题跟随全局 themeStore
 */
import { MdPreview } from "md-editor-v3";
import "md-editor-v3/lib/style.css";
import { useThemeStore } from "@/stores/theme";
import { mcpGuide } from "../lib/constants";

const themeStore = useThemeStore();
</script>

<template>
  <div class="mcp-guide">
    <MdPreview :model-value="mcpGuide" :theme="themeStore.isDark ? 'dark' : 'light'" />
  </div>
</template>

<style scoped>
/* MdPreview */
.mcp-guide { border: 1px solid var(--border-light); border-radius: var(--radius-md); overflow: hidden; }
.mcp-guide :deep(.md-editor), .mcp-guide :deep(.md-editor-preview-wrapper), .mcp-guide :deep(.md-editor-preview) { background: var(--bg-surface) !important; }
.mcp-guide :deep(.md-editor-preview) { padding: var(--spacing-lg) var(--spacing-xl) !important; font-size: 13px; line-height: 1.7; }
.mcp-guide :deep(.md-editor-preview h2) { margin-top: var(--spacing-lg); margin-bottom: var(--spacing-sm); font-size: 15px; padding-bottom: 6px; border-bottom: 1px solid var(--border-light); }
.mcp-guide :deep(.md-editor-preview h2:first-child) { margin-top: 0; }
.mcp-guide :deep(.md-editor-preview pre) { margin: var(--spacing-sm) 0 !important; }
.mcp-guide :deep(.md-editor-toolbar), .mcp-guide :deep(.md-editor-footer) { display: none !important; }
.mcp-guide :deep(pre) { background: var(--bg-input) !important; border: 1px solid var(--border-light); border-radius: var(--radius-sm); }
</style>
