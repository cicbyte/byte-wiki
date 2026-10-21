<script setup lang="ts">
/**
 * Wiki 文档渲染：markdown（md-editor-v3 MdPreview，项目既有依赖）
 * + mermaid（本地实例渲染，离线可用；大图点击进全屏查看器缩放/导出）
 * + 链接拦截：`bw-file://路径?L=起-止`（生成器产出的源码引用）→ 文件浏览页定位；
 *   相对 `.md` 链接（总览页目录）→ 文档切换
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { MdPreview } from "md-editor-v3";
// mermaid 本地实例接线 + 主题预初始化（详见 composables/mermaidSetup.ts 头注释：
// 同步 instance 下 md-editor 不会自行 initialize，不预初始化首屏就是出厂淡紫主题）
import { applyMermaidTheme, setupMermaid } from "@/composables/mermaidSetup";
// 组件样式必须自持：此前仅 McpConfigPane 引过 lib/style.css（懒加载路由），
// 没访问过 MCP 页时本组件渲染裸奔（代码块工具栏/语言标签/复制按钮全无样式）
import "md-editor-v3/lib/style.css";
import { message } from "ant-design-vue";
import { ZoomInOutlined, ZoomOutOutlined, ReloadOutlined, LoadingOutlined } from "@ant-design/icons-vue";
import { api } from "@/api";
import { useThemeStore } from "@/stores/theme";
import DiagramLightbox from "./DiagramLightbox.vue";



const props = defineProps<{
  repoPath: string;
  docPath: string;
  /** 生成中实时内容（骨架页点击查看；非空时优先于磁盘读取） */
  liveContent?: string | null;
  /** 当前页骨架状态（generating 时才亮徽标；收尾期 done 页静默） */
  liveStatus?: string | null;
  /** 只读查看分支（非空时磁盘读取走该分支目录） */
  viewBranch?: string | null;
}>();

const emit = defineEmits<{
  (e: "open-doc", path: string): void;
  (e: "open-file", target: { path: string; startLine?: number; endLine?: number }): void;
}>();

const themeStore = useThemeStore();

setupMermaid();
// 主题预初始化 + 跟随切换（md-editor 自身只在主题变化时才 initialize，
// 首屏渲染前必须先就位，否则出厂淡紫主题）
applyMermaidTheme(themeStore.isDark);
watch(
  () => themeStore.isDark,
  (dark) => applyMermaidTheme(dark),
);

const content = ref("");
const loading = ref(false);
/** 图谱缩放（.mmd 文档有效） */
const zoom = ref(1);
const isGraph = computed(() => props.docPath.endsWith(".mmd"));

// ===== 图表全屏查看器 =====
const lightboxOpen = ref(false);
const lightboxSvg = ref<SVGSVGElement | null>(null);
const lightboxLabel = ref("");
/** 观察锚点用稳定的 viewer-body：live-empty 占位与 md-host 间切换时
 * md-host 会卸载重挂，observer 挂它上面会丢 */
const viewerBody = ref<HTMLElement | null>(null);
/** mermaid 渲染是 BUILD_FINISHED 后的异步动作，内容（含实时流式）也会整体
 * 重渲染——用 MutationObserver 监听 DOM 变化，给新出现的图挂点击放大 */
let figObserver: MutationObserver | null = null;
let figMarkTimer: number | null = null;

function markFigures() {
  const host = viewerBody.value;
  if (!host) return;
  host.querySelectorAll<HTMLElement>(".md-editor-mermaid").forEach((fig, idx) => {
    if (fig.dataset.figEnhanced || !fig.querySelector("svg")) return;
    fig.dataset.figEnhanced = "1";
    fig.classList.add("fig-zoomable");
    fig.title = "点击全屏查看（滚轮缩放 · 可导出 SVG / PNG）";
    fig.dataset.figIdx = String(idx + 1);
  });
}

function observeFigures() {
  figObserver?.disconnect();
  if (!viewerBody.value) return;
  figObserver = new MutationObserver(() => {
    if (figMarkTimer) clearTimeout(figMarkTimer);
    figMarkTimer = window.setTimeout(markFigures, 150);
  });
  figObserver.observe(viewerBody.value, { childList: true, subtree: true });
  markFigures();
}

/** 剥掉开头的 YAML frontmatter：cards/ 文档带 title/created/source 头，
 * 直接渲染会把 `---` 变分割线、meta 变正文，且 md-editor-v3 还会把 title 再渲染成一个 H1（标题重复） */
function stripFrontmatter(raw: string): string {
  return raw.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "").replace(/^\s+/, "");
}

async function load() {
  loading.value = true;
  zoom.value = 1;
  try {
    const raw = props.viewBranch
      ? await api.readWikiDocFor(props.repoPath, props.docPath, props.viewBranch)
      : await api.readWikiDoc(props.repoPath, props.docPath);
    content.value = isGraph.value ? "```mermaid\n" + raw + "\n```" : stripFrontmatter(raw);
    // 切文档滚顶复位：不复位会把上一篇的滚动位置带进新文档（短文被夹、长文落到中部）
    viewerBody.value?.scrollTo({ top: 0 });
  } catch (e: any) {
    content.value = "";
    message.error(typeof e === "string" ? e : "读取文档失败");
  } finally {
    loading.value = false;
  }
}

watch(() => props.docPath, load, { immediate: true });

onMounted(async () => {
  await nextTick();
  observeFigures();
});

onUnmounted(() => {
  figObserver?.disconnect();
  if (figMarkTimer) clearTimeout(figMarkTimer);
});

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
  // 图表（mermaid 容器）：进全屏查看器（缩放/平移/导出），优先于链接逻辑
  const target = ev.target as HTMLElement;
  const fig = target.closest?.(".md-editor-mermaid");
  if (fig) {
    const svg = fig.querySelector("svg");
    if (svg) {
      ev.preventDefault();
      lightboxSvg.value = svg;
      lightboxLabel.value = `图${(fig as HTMLElement).dataset.figIdx || ""}`;
      lightboxOpen.value = true;
      return;
    }
  }
  const anchor = target.closest?.("a");
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
      <span v-if="viewBranch" class="live-badge readonly-badge" :title="`正在查看「${viewBranch}」分支的产物（只读）`">
        查看 {{ viewBranch }}
      </span>
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
    <div ref="viewerBody" class="viewer-body" :class="{ graph: isGraph }" @click.capture="onBodyClick">
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

    <!-- 图表全屏查看器（点击文档内 mermaid 图打开） -->
    <DiagramLightbox
      v-model:open="lightboxOpen"
      :svg="lightboxSvg"
      :title="docPath"
      :fig-label="lightboxLabel"
      :dark="themeStore.isDark"
    />
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
  /* 统一面板头：与树头部 / AI 对话头部分隔线同高同水平线 */
  display: flex;
  align-items: center;
  gap: 8px;
  height: 38px;
  box-sizing: border-box;
  padding: 0 14px;
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
.readonly-badge {
  color: var(--color-warning, #ea8a3c);
  background: color-mix(in oklch, #ea8a3c 12%, transparent);
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
/* 文档内图表：可点击放大的暗示（悬停描边 + 光标） */
.md-host :deep(.md-editor-mermaid.fig-zoomable) {
  position: relative;
  cursor: zoom-in;
  border-radius: var(--radius-sm);
  transition: box-shadow 0.15s ease;
}
.md-host :deep(.md-editor-mermaid.fig-zoomable:hover) {
  box-shadow: 0 0 0 1px var(--color-primary-border), 0 2px 10px rgba(0, 0, 0, 0.08);
}
/* mermaid 异步渲染占位：渲染完成前容器里是图源码文本（先当正文显示，几十到几百
   毫秒后换成 SVG 高度突变，把下方内容顶一下——「切文档先显示再抖一下」的主源）。
   隐藏源码闪现 + 预留最小高度，把 pop-in 落差压到最小 */
.md-host :deep(.md-editor-mermaid:not(:has(svg))) {
  min-height: 120px;
  visibility: hidden;
}
/* 深色模式下 md-editor-v3 自带纯黑背景（--md-bk-color:#000）且 preview 内边距为 0，
   会在卡片内形成一个文本贴边的黑框 —— 置透明使其融入卡片背景（与浅色模式一致） */
.wiki-viewer :deep(.md-editor) {
  background-color: transparent;
}
</style>
