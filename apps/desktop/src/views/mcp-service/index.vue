<script setup lang="ts">
/**
 * MCP 服务页 /mcp-service
 *
 * 管理内置 MCP HTTP 服务器的启停，查看端点 URL 和工具列表。
 *
 * 结构：
 * - 服务启停/状态：./lib/useMcpServer
 * - 静态数据（工具定义 / 配置文案 / 地址选项）：./lib/constants
 * - 子区域：./components/McpServerPanel | McpConfigPane | McpToolsPane
 * 本文件仅保留页面级状态（mcpTab）、生命周期与布局。
 */
import { ref, onMounted } from "vue";
import { useLayoutStore } from "@/stores/layout";
import McpServerPanel from "./components/McpServerPanel.vue";
import McpConfigPane from "./components/McpConfigPane.vue";
import McpToolsPane from "./components/McpToolsPane.vue";
import { hostOptions, readTools, writeTools } from "./lib/constants";
import { useMcpServer } from "./lib/useMcpServer";

const layout = useLayoutStore();

const {
  mcpRunning,
  mcpUrl,
  mcpPort,
  mcpHost,
  mcpUseAuth,
  mcpKey,
  mcpKeyInput,
  mcpKeyVisible,
  mcpKeySaving,
  mcpToggling,
  checkMcpStatus,
  toggleMcpServer,
  copyMcpUrl,
  copyText,
  saveMcpKey,
} = useMcpServer();

/** 二级 tab */
const mcpTab = ref<"server" | "config" | "tools">("server");

onMounted(() => {
  checkMcpStatus();
  layout.setStatusItems([{ key: "view", text: "MCP 服务" }]);
});
</script>

<template>
  <div class="mcp-svc-view">
    <h1 class="page-title">MCP 服务</h1>

    <!-- 二级 tab -->
    <div class="mcp-sub-tabs">
      <button class="sub-tab" :class="{ active: mcpTab === 'server' }" @click="mcpTab = 'server'">服务管理</button>
      <button class="sub-tab" :class="{ active: mcpTab === 'config' }" @click="mcpTab = 'config'">配置</button>
      <button class="sub-tab" :class="{ active: mcpTab === 'tools' }" @click="mcpTab = 'tools'">工具列表（{{ readTools.length + writeTools.length }}）</button>
    </div>

    <!-- 服务管理 -->
    <div v-if="mcpTab === 'server'" class="sub-pane">
      <McpServerPanel
        v-model:host="mcpHost"
        v-model:port="mcpPort"
        v-model:use-auth="mcpUseAuth"
        v-model:key-input="mcpKeyInput"
        v-model:key-visible="mcpKeyVisible"
        :mcp-running="mcpRunning"
        :mcp-url="mcpUrl"
        :mcp-key="mcpKey"
        :mcp-toggling="mcpToggling"
        :mcp-key-saving="mcpKeySaving"
        :host-options="hostOptions"
        @toggle="toggleMcpServer"
        @save-key="saveMcpKey"
        @copy-url="copyMcpUrl"
        @copy="copyText"
      />
    </div>

    <!-- 配置 -->
    <div v-else-if="mcpTab === 'config'" class="mcp-sub-pane">
      <McpConfigPane />
    </div>

    <!-- 工具列表 -->
    <div v-else-if="mcpTab === 'tools'" class="mcp-sub-pane">
      <McpToolsPane />
    </div>
  </div>
</template>

<style scoped>
.mcp-svc-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--spacing-md);
  overflow-y: auto;
  gap: var(--spacing-md);
}
.page-title { font-size: 15px; font-weight: 600; color: var(--text-primary); }

/* 二级 tab */
.mcp-sub-tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border-light); }
.sub-tab { padding: 8px 16px; border: none; border-bottom: 2px solid transparent; background: transparent; color: var(--text-tertiary); font-size: 13px; font-weight: 500; cursor: pointer; margin-bottom: -1px; transition: all var(--transition-fast); }
.sub-tab:hover { color: var(--text-primary); }
.sub-tab.active { color: var(--color-primary); border-bottom-color: var(--color-primary); font-weight: 600; }
.sub-pane { display: flex; flex-direction: column; gap: var(--spacing-md); min-height: 200px; }
</style>
