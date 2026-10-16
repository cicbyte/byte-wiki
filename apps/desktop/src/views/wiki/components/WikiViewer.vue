<script setup lang="ts">
/**
 * Wiki 文档渲染：markdown（md-editor-v3 MdPreview，项目既有依赖）
 * + mermaid（.mmd 图谱包成 mermaid 代码块渲染，支持缩放/平移）
 * + 链接拦截：`bw-file://路径?L=起-止`（生成器产出的源码引用）→ 文件浏览页定位；
 *   相对 `.md` 链接（总览页目录）→ 文档切换
 */
import { computed, ref, watch } from "vue";
import { MdPreview } from "md-editor-v3";
// 组件样式必须自持：此前仅 McpConfigPane 引过 lib/style.css（懒加载路由），
// 没访问过 MCP 页时本组件渲染裸奔（代码块工具栏/语言标签/复制按钮全无样式）
import "md-editor-v3/lib/style.css";
import { message } from "ant-design-vue";
import { ZoomInOutlined, ZoomOutOutlined, ReloadOutlined, LoadingOutlined } from "@ant-design/icons-vue";
import { api } from "@/api";
import { useThemeStore } from "@/stores/theme";

const props = defineProps<{
  repoPath: string;
  docPath: string;
  /** 生成中实时内容（骨架页点击查看；非空时优先于磁盘读取） */
  liveContent?: string | null;
  /** 当前页骨架状态（generating 时才亮徽标；收尾期 done 页静默） */
  liveStatus?: string | null;
}>();

const emit = defineEmits<{
  (e: "open-doc", path: string): void;
  (e: "open-file", target: { path: string; startLine?: number; endLine?: number }): void;
}>();

const themeStore = useThemeStore();

const content = ref("");
const loading = ref(false);
/** 图谱缩放（.mmd 文档有效） */
const zoom = ref(1);
const isGraph = computed(() => props.docPath.endsWith(".mmd"));

/** 剥掉开头的 YAML frontmatter：cards/ 文档带 title/created/source 头，
 * 直接渲染会把 `---` 变分割线、meta 变正文，且 md-editor-v3 还会把 title 再渲染成一个 H1（标题重复） */
function stripFrontmatter(raw: string): string {
  return raw.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "").replace(/^\s+/, "");
}

async function load() {
  loading.value = true;
  zoom.value = 1;
  try {
    const raw = await api.readWikiDoc(props.repoPath, props.docPath);
    content.value = isGraph.value ? "```mermaid\n" + raw + "\n```" : stripFrontmatter(raw);
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

// ===== 链接拦截 =====
function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

function onBodyClick(ev: MouseEvent) {
  const anchor = (ev.target as HTMLElement).closest?.("a");
  if (!anchor) return;
  const href = anchor.getAttribute("href") || "";
  if (href.startsWith("bw-file://")) {
    ev.preventDefault();
    let rest = href.slice("bw-file://".length);
    let query = "";
    const qi = rest.indexOf("?");
    if (qi >= 0) {
      query = rest.slice(qi + 1);
      rest = rest.slice(0, qi);
    }
    const target: { path: string; startLine?: number; endLine?: number } = { path: safeDecode(rest) };
    const m = /(?:^|&)L=(\d+)(?:-(\d+))?/.exec(query);
    if (m) {
      target.startLine = Number(m[1]);
      target.endLine = m[2] ? Number(m[2]) : target.startLine;
    }
    emit("open-file", target);
    return;
  }
  // 相对 .md 链接（总览页目录）：相对当前文档目录解析
  if (!/^[a-z][a-z0-9+.-]*:/i.test(href) && !href.startsWith("#") && href.endsWith(".md")) {
    ev.preventDefault();
    const base = props.docPath.split("/").slice(0, -1);
    for (const part of safeDecode(href).split("/")) {
      if (part === "..") base.pop();
      else if (part && part !== ".") base.push(part);
    }
    emit("open-doc", base.join("/"));
  }
}
</script>

<template>
  <div class="wiki-viewer">
    <div class="viewer-toolbar">
      <span class="doc-path">{{ docPath }}</span>
      <span v-if="liveContent != null && liveStatus === 'generating'" class="live-badge" title="页面正在生成，内容实时刷新">
        <span class="live-dot"></span>
        实时生成中
      </span>
      <span v-if="loading" class="loading-hint">加载中…</span>
      <template v-if="isGraph">
        <button class="zoom-btn" title="放大" @click="zoomBy(0.1)"><ZoomInOutlined /></button>
        <button class="zoom-btn" title="缩小" @click="zoomBy(-0.1)"><ZoomOutOutlined /></button>
        <button class="zoom-btn" title="重置缩放" @click="zoom = 1"><ReloadOutlined /></button>
        <span class="zoom-label">{{ Math.round(zoom * 100) }}%</span>
      </template>
    </div>
    <div class="viewer-body" :class="{ graph: isGraph }" @click.capture="onBodyClick">
      <!-- 实时模式且尚无正文：排队/思考占位 -->
      <div v-if="liveContent != null && !content" class="live-empty">
        <LoadingOutlined spin />
        <span>页面排队等待生成 · 模型思考中正文会实时出现</span>
      </div>
      <div
        v-else
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
.live-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  font-weight: 600;
  color: var(--color-primary);
  background: var(--color-primary-bg);
  border-radius: var(--radius-xs);
  padding: 1px 8px;
  white-space: nowrap;
}
.live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-primary);
  animation: live-blink 1.1s ease-in-out infinite;
}
@keyframes live-blink {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.25;
  }
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
.live-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--text-tertiary);
  font-size: 13px;
}
.live-empty :deep(.anticon) {
  color: var(--color-primary);
  font-size: 16px;
}
/* 图谱模式：允许横向平移查看大图 */
.viewer-body.graph {
  padding: var(--spacing-md);
}
.md-host {
  max-width: 100%;
}
/* 深色模式下 md-editor-v3 自带纯黑背景（--md-bk-color:#000）且 preview 内边距为 0，
   会在卡片内形成一个文本贴边的黑框 —— 置透明使其融入卡片背景（与浅色模式一致） */
.wiki-viewer :deep(.md-editor) {
  background-color: transparent;
}
</style>
