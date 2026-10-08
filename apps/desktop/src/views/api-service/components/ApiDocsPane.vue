<script setup lang="ts">
/**
 * API 文档面板（api-service → API 文档 tab）
 *
 * 内嵌 Swagger UI iframe；docsLoading 由主入口控制（切 tab 时置 true，iframe load 后置 false），
 * 通过 defineModel 双向同步。
 */
import { GlobalOutlined } from "@ant-design/icons-vue";

defineProps<{
  apiRunning: boolean;
  apiUrl: string;
  apiPort: number;
}>();

const docsLoading = defineModel<boolean>("docsLoading");

const emit = defineEmits<{
  (e: "open-docs"): void;
}>();

function openDocsInBrowser() {
  emit("open-docs");
}
</script>

<template>
  <!-- Swagger UI iframe -->
  <div v-if="apiRunning && apiUrl" class="swagger-wrap">
    <div class="swagger-toolbar">
      <span class="swagger-label">交互式 API 文档</span>
      <button class="mini-btn" @click="openDocsInBrowser"><GlobalOutlined /> 浏览器中打开</button>
    </div>
    <div class="swagger-frame-wrap">
      <div v-if="docsLoading" class="swagger-loading">
        <div class="spinner"></div>
        <span>正在加载 Swagger UI...</span>
      </div>
      <iframe
        :src="apiUrl + '/docs'"
        class="swagger-frame"
        @load="docsLoading = false"
      />
    </div>
  </div>
  <div v-else class="swagger-empty">
    <p class="hint">启动 REST API 服务后，此处显示交互式 API 文档（Swagger UI）。</p>
    <p class="hint">也可在浏览器打开 <code>http://localhost:{{ apiPort }}/api/v1/docs</code></p>
  </div>
</template>

<style scoped>
.swagger-wrap { flex: 1; min-height: 400px; display: flex; flex-direction: column; border: 1px solid var(--border-light); border-radius: var(--radius-md); overflow: hidden; }
.swagger-toolbar { display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; border-bottom: 1px solid var(--border-light); background: var(--bg-surface); flex-shrink: 0; }
.swagger-label { font-size: 12px; font-weight: 600; color: var(--text-secondary); }
/* 工具栏按钮：本组件作用域内定义，复用语义变量以适配各主题（避免裸 button 默认白底在深色下不可见） */
.mini-btn {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-sm);
  background: var(--bg-base);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--transition-fast);
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.mini-btn:hover { border-color: var(--color-primary-border); color: var(--color-primary); }
.swagger-frame-wrap { flex: 1; position: relative; min-height: 0; }
.swagger-frame { width: 100%; height: 100%; border: none; background: #fff; }
.swagger-loading { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; background: var(--bg-base); z-index: 1; }
.swagger-loading span { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 28px; height: 28px;
  border: 3px solid var(--border-light);
  border-top-color: var(--color-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
.swagger-empty { display: flex; flex-direction: column; gap: 8px; padding: var(--spacing-lg); align-items: center; justify-content: center; text-align: center; }

/* 提示文字（本面板内使用） */
.hint { color: var(--text-tertiary); font-size: 12px; line-height: 1.6; }
.hint code { background: var(--bg-input); padding: 1px 5px; border-radius: 3px; font-family: "JetBrains Mono", monospace; font-size: 11px; }
</style>
