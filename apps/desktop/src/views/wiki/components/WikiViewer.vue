<script setup lang="ts">
/**
 * Wiki 文档渲染：markdown（md-editor-v3 MdPreview，项目既有依赖）
 * + mermaid（.mmd 图谱包成 mermaid 代码块渲染，支持缩放/平移）
 */
import { computed, ref, watch } from "vue";
import { MdPreview } from "md-editor-v3";
import { message } from "ant-design-vue";
import { ZoomInOutlined, ZoomOutOutlined, ReloadOutlined } from "@ant-design/icons-vue";
import { api } from "@/api";
import { useThemeStore } from "@/stores/theme";

const props = defineProps<{
  repoPath: string;
  docPath: string;
}>();

const themeStore = useThemeStore();

const content = ref("");
const loading = ref(false);
/** 图谱缩放（.mmd 文档有效） */
const zoom = ref(1);
const isGraph = computed(() => props.docPath.endsWith(".mmd"));

async function load() {
  loading.value = true;
  zoom.value = 1;
  try {
    const raw = await api.readWikiDoc(props.repoPath, props.docPath);
    content.value = isGraph.value ? "```mermaid\n" + raw + "\n```" : raw;
  } catch (e: any) {
    content.value = "";
    message.error(typeof e === "string" ? e : "读取文档失败");
  } finally {
    loading.value = false;
  }
}

watch(() => props.docPath, load, { immediate: true });

function zoomBy(delta: number) {
  zoom.value = Math.min(3, Math.max(0.3, Math.round((zoom.value + delta) * 10) / 10));
}
</script>

<template>
  <div class="wiki-viewer">
    <div class="viewer-toolbar">
      <span class="doc-path">{{ docPath }}</span>
      <span v-if="loading" class="loading-hint">加载中…</span>
      <template v-if="isGraph">
        <button class="zoom-btn" title="放大" @click="zoomBy(0.1)"><ZoomInOutlined /></button>
        <button class="zoom-btn" title="缩小" @click="zoomBy(-0.1)"><ZoomOutOutlined /></button>
        <button class="zoom-btn" title="重置缩放" @click="zoom = 1"><ReloadOutlined /></button>
        <span class="zoom-label">{{ Math.round(zoom * 100) }}%</span>
      </template>
    </div>
    <div class="viewer-body" :class="{ graph: isGraph }">
      <div
        class="md-host"
        :style="isGraph ? { transform: `scale(${zoom})`, 'transform-origin': 'top left' } : undefined"
      >
        <MdPreview
          :model-value="content"
          :theme="themeStore.isDark ? 'dark' : 'light'"
          class="wiki-md"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.wiki-viewer {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 0;
}
.viewer-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.doc-path {
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  color: var(--text-tertiary);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.loading-hint {
  font-size: 11px;
  color: var(--text-tertiary);
}
.zoom-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-xs);
  background: var(--bg-surface);
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
}
.zoom-btn:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}
.zoom-label {
  font-size: 11px;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-tertiary);
  min-width: 36px;
}
.viewer-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--spacing-md) var(--spacing-lg);
}
/* 图谱模式：允许横向平移查看大图 */
.viewer-body.graph {
  padding: var(--spacing-sm);
}
.md-host {
  max-width: 100%;
}
</style>
