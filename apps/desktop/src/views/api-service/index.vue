<script setup lang="ts">
/**
 * REST API 服务页 /api-service
 *
 * 管理内置 REST API 服务器的启停，查看端点 URL、API Key 和端点文档。
 * REST API 面向第三方应用（脚本/Web/移动端），标准 HTTP + JSON。
 *
 * 结构：
 * - 服务启停/状态：./lib/useApiServer
 * - 静态数据：./lib/constants
 * - 子区域：./components/ServerPanel | ApiDocsPane
 * 本文件仅保留页面级状态（apiTab）、生命周期与布局。
 */
import { ref, onMounted } from "vue";
import { useLayoutStore } from "@/stores/layout";
import ServerPanel from "./components/ServerPanel.vue";
import ApiDocsPane from "./components/ApiDocsPane.vue";
import { hostOptions } from "./lib/constants";
import { useApiServer } from "./lib/useApiServer";

const layout = useLayoutStore();

const {
  apiRunning,
  apiUrl,
  apiPort,
  apiHost,
  apiUseAuth,
  apiKey,
  apiKeyInput,
  apiKeyVisible,
  apiKeySaving,
  apiToggling,
  docsLoading,
  checkStatus,
  toggleServer,
  copyText,
  saveApiKey,
  openDocsInBrowser,
} = useApiServer();

/** 二级 tab */
const apiTab = ref<"server" | "docs">("server");

onMounted(() => {
  checkStatus();
  layout.setStatusItems([{ key: "view", text: "REST API" }]);
});
</script>

<template>
  <div class="api-svc-view">
    <h1 class="page-title">REST API 服务</h1>

    <!-- 二级 tab -->
    <div class="sub-tabs">
      <button class="sub-tab" :class="{ active: apiTab === 'server' }" @click="apiTab = 'server'">服务管理</button>
      <button class="sub-tab" :class="{ active: apiTab === 'docs' }" @click="apiTab = 'docs'; docsLoading = true">API 文档</button>
    </div>

    <!-- 服务管理 -->
    <div v-if="apiTab === 'server'" class="sub-pane">
      <ServerPanel
        v-model:host="apiHost"
        v-model:port="apiPort"
        v-model:use-auth="apiUseAuth"
        v-model:key-input="apiKeyInput"
        v-model:key-visible="apiKeyVisible"
        :api-running="apiRunning"
        :api-url="apiUrl"
        :api-key="apiKey"
        :api-toggling="apiToggling"
        :api-key-saving="apiKeySaving"
        :host-options="hostOptions"
        @toggle="toggleServer"
        @save-key="saveApiKey"
        @copy="copyText"
      />
    </div>

    <!-- API 文档 -->
    <div v-else-if="apiTab === 'docs'" class="sub-pane">
      <ApiDocsPane
        v-model:docs-loading="docsLoading"
        :api-running="apiRunning"
        :api-url="apiUrl"
        :api-port="apiPort"
        @open-docs="openDocsInBrowser"
      />
    </div>
  </div>
</template>

<style scoped>
.api-svc-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--spacing-md);
  overflow-y: auto;
  gap: var(--spacing-md);
}
.page-title { font-size: 15px; font-weight: 600; color: var(--text-primary); }

/* 二级 tab */
.sub-tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border-light); flex-shrink: 0; }
.sub-tab { padding: 8px 16px; border: none; border-bottom: 2px solid transparent; background: transparent; color: var(--text-tertiary); font-size: 13px; font-weight: 500; cursor: pointer; margin-bottom: -1px; transition: all var(--transition-fast); }
.sub-tab:hover { color: var(--text-primary); }
.sub-tab.active { color: var(--color-primary); border-bottom-color: var(--color-primary); font-weight: 600; }
.sub-pane { display: flex; flex-direction: column; gap: var(--spacing-md); flex: 1; min-height: 0; }
</style>
