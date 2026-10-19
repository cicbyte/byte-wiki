<script setup lang="ts">
/**
 * 图表全屏查看器（Wiki 内嵌 mermaid / .mmd 图谱通用）
 *
 * 点击文档中的图表打开：滚轮以光标为中心缩放、拖拽平移、双击在
 * 「适应窗口 / 100%」间切换；可导出 SVG（序列化）与 PNG（canvas 2x
 * 栅格化、底色随主题），经系统保存对话框 + save_diagram 命令写盘。
 */
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import { message } from "ant-design-vue";
import { save } from "@tauri-apps/plugin-dialog";
import {
  ZoomInOutlined,
  ZoomOutOutlined,
  ExpandOutlined,
  CloseOutlined,
  DownloadOutlined,
  FileImageOutlined,
  LoadingOutlined,
} from "@ant-design/icons-vue";
import { api } from "@/api";

const props = defineProps<{
  open: boolean;
  /** 文档里的原始 svg（打开时克隆进舞台，不动原文档 DOM） */
  svg: SVGSVGElement | null;
  /** 文档路径（工具栏标题 + 导出默认文件名） */
  title: string;
  /** 图序标签（一页多图时区分，如「图 2」） */
  figLabel?: string;
  /** 应用为暗色主题（mermaid dark 主题节点近黑填充，白底会成黑色大色块；
   *  舞台底色与 PNG 底色需随主题走） */
  dark?: boolean;
}>();

const emit = defineEmits<{ (e: "update:open", v: boolean): void }>();

const stageEl = ref<HTMLElement | null>(null);
const canvasEl = ref<HTMLElement | null>(null);
const exporting = ref(false);

/** 舞台变换（translate 单位 px；scale 相对图原始尺寸；ref 供工具栏百分比显示） */
const scale = ref(1);
let tx = 0;
let ty = 0;
/** 克隆后的 svg 与其固有尺寸（viewBox 优先） */
let clone: SVGSVGElement | null = null;
let figW = 800;
let figH = 600;

function apply() {
  if (stageEl.value) {
    stageEl.value.style.transform = `translate(${tx}px, ${ty}px) scale(${scale.value})`;
  }
}

function clampScale(v: number): number {
  return Math.min(16, Math.max(0.05, v));
}

function intrinsicSize(svg: SVGSVGElement): { w: number; h: number } {
  const vb = svg.viewBox?.baseVal;
  if (vb && vb.width > 0 && vb.height > 0) return { w: vb.width, h: vb.height };
  const w = parseFloat(svg.getAttribute("width") || "") || 0;
  const h = parseFloat(svg.getAttribute("height") || "") || 0;
  return w > 0 && h > 0 ? { w, h } : { w: 800, h: 600 };
}

/** 克隆并按固有尺寸固定宽高（mermaid 的 style max-width / width=100% 交给舞台控制） */
function mountClone() {
  const stage = stageEl.value;
  if (!stage || !props.svg) return;
  stage.textContent = "";
  clone = props.svg.cloneNode(true) as SVGSVGElement;
  const { w, h } = intrinsicSize(props.svg);
  figW = w;
  figH = h;
  // mermaid 的主题 CSS 全部以根 svg 的 id 作前缀（#id .node rect{fill:…}），
  // 箭头/渐变/滤镜也以 url(#id-…) 引用。克隆不能丢 id——丢了内嵌样式整体
  // 失配，所有形状回落 SVG 默认黑填充（浅深两主题下都是黑色大色块）。
  // 这里整体改写为新 id（样式选择器与 url() 引用一并替换），导出与 PNG
  // 沿用同一克隆，单独打开导出文件同样有正确配色。
  const oldId = props.svg.getAttribute("id") || "";
  if (oldId) {
    const newId = `${oldId}-lb${Math.random().toString(36).slice(2, 8)}`;
    const hit = `#${oldId}`;
    const dst = `#${newId}`;
    clone.setAttribute("id", newId);
    clone.querySelectorAll("style").forEach((st) => {
      st.textContent = (st.textContent || "").split(hit).join(dst);
    });
    const all = [clone as Element, ...Array.from(clone.querySelectorAll("*"))];
    for (const el of all) {
      for (const attr of Array.from(el.attributes)) {
        if (attr.value.includes(hit)) {
          el.setAttribute(attr.name, attr.value.split(hit).join(dst));
        }
      }
    }
  }
  clone.removeAttribute("style");
  clone.setAttribute("width", String(w));
  clone.setAttribute("height", String(h));
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("xmlns:xlink", "http://www.w3.org/1999/xlink");
  stage.appendChild(clone);
}

function fitView() {
  const c = canvasEl.value;
  if (!c) return;
  const cw = c.clientWidth;
  const ch = c.clientHeight;
  scale.value = clampScale(Math.min(cw / figW, ch / figH) * 0.96);
  tx = (cw - figW * scale.value) / 2;
  ty = (ch - figH * scale.value) / 2;
  apply();
}

function set100() {
  const c = canvasEl.value;
  if (!c) return;
  scale.value = 1;
  tx = (c.clientWidth - figW) / 2;
  ty = (c.clientHeight - figH) / 2;
  apply();
}

/** 以 (cx, cy)（canvas 内坐标）为锚点缩放 factor 倍 */
function zoomAt(cx: number, cy: number, factor: number) {
  const next = clampScale(scale.value * factor);
  tx = cx - (cx - tx) * (next / scale.value);
  ty = cy - (cy - ty) * (next / scale.value);
  scale.value = next;
  apply();
}

function zoomCenter(factor: number) {
  const c = canvasEl.value;
  if (!c) return;
  zoomAt(c.clientWidth / 2, c.clientHeight / 2, factor);
}

// ===== 画布交互 =====
function onWheel(ev: WheelEvent) {
  ev.preventDefault();
  const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect();
  // 指数步进：手感均匀；deltaMode 兼容行模式（Firefox）
  const dy = ev.deltaMode === 1 ? ev.deltaY * 33 : ev.deltaY;
  zoomAt(ev.clientX - rect.left, ev.clientY - rect.top, Math.exp(-dy * 0.0016));
}

let dragX = 0;
let dragY = 0;
let dragging = false;
let dragMoved = false;

function onPointerDown(ev: PointerEvent) {
  if (ev.button !== 0) return;
  dragging = true;
  dragMoved = false;
  dragX = ev.clientX;
  dragY = ev.clientY;
  (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId);
}

function onPointerMove(ev: PointerEvent) {
  if (!dragging) return;
  const dx = ev.clientX - dragX;
  const dy = ev.clientY - dragY;
  if (!dragMoved && Math.hypot(dx, dy) > 3) dragMoved = true;
  if (dragMoved) {
    tx += dx;
    ty += dy;
    dragX = ev.clientX;
    dragY = ev.clientY;
    apply();
  }
}

function onPointerUp() {
  dragging = false;
}

function onDblClick() {
  // 近 100% → 适应；否则回 100%
  if (Math.abs(scale.value - 1) < 0.02) fitView();
  else set100();
}

function onCanvasClick(ev: MouseEvent) {
  // 空白处单击关闭（拖拽结束的 click 不算）
  if (ev.target === canvasEl.value && !dragMoved) close();
}

// ===== 键盘 =====
function onKeydown(ev: KeyboardEvent) {
  if (!props.open) return;
  if (ev.key === "Escape") close();
  else if (ev.key === "+" || ev.key === "=") zoomCenter(1.2);
  else if (ev.key === "-") zoomCenter(1 / 1.2);
  else if (ev.key === "0") fitView();
}

function onResize() {
  if (props.open) fitView();
}

function close() {
  emit("update:open", false);
}

// ===== 导出 =====
function fileBase(): string {
  const raw = props.figLabel ? `${props.title}-${props.figLabel}` : props.title;
  const cleaned = raw
    .replace(/\.(md|mmd)$/i, "")
    .replace(/[\\/:*?"<>|\s]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return cleaned || "diagram";
}

function svgText(): string {
  if (!clone) return "";
  const text = new XMLSerializer().serializeToString(clone);
  return `<?xml version="1.0" encoding="UTF-8"?>\n${text}`;
}

async function downloadSvg() {
  if (!clone) return;
  const text = svgText();
  const path = await save({
    defaultPath: `${fileBase()}.svg`,
    filters: [{ name: "SVG 图形", extensions: ["svg"] }],
  });
  if (!path) return;
  try {
    await api.saveDiagram(path, "svg", text);
    message.success("已保存 " + path);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "保存失败");
  }
}

/** SVG → 位图：Blob URL 载入（要求 svg 有固有宽高——mountClone 已固定），
 *  canvas 2x 栅格化并垫底色（mermaid 透明底直接导出会黑/糊；底色随主题） */
async function toPngDataUrl(): Promise<string> {
  const url = URL.createObjectURL(new Blob([svgText()], { type: "image/svg+xml;charset=utf-8" }));
  try {
    const img = new Image();
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("SVG 位图化失败"));
      img.src = url;
    });
    const px = 2;
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(figW * px);
    canvas.height = Math.round(figH * px);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 不可用");
    ctx.fillStyle = props.dark ? "#1e222a" : "#ffffff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL("image/png");
  } finally {
    URL.revokeObjectURL(url);
  }
}

async function downloadPng() {
  if (!clone || exporting.value) return;
  exporting.value = true;
  try {
    const dataUrl = await toPngDataUrl();
    const path = await save({
      defaultPath: `${fileBase()}.png`,
      filters: [{ name: "PNG 图片", extensions: ["png"] }],
    });
    if (!path) return;
    await api.saveDiagram(path, "png", dataUrl);
    message.success("已保存 " + path);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "导出失败");
  } finally {
    exporting.value = false;
  }
}

// ===== 生命周期钩子（随 open 挂卸） =====
watch(
  () => props.open,
  async (open) => {
    if (open) {
      await nextTick();
      mountClone();
      fitView();
      window.addEventListener("keydown", onKeydown);
      window.addEventListener("resize", onResize);
    } else {
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener("resize", onResize);
    }
  },
);

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", onResize);
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="dl-overlay" :class="{ dark }">
      <div class="dl-toolbar">
        <span class="dl-title" :title="title">{{ title }}<em v-if="figLabel"> · {{ figLabel }}</em></span>
        <div class="dl-actions">
          <button class="dl-btn" title="缩小（-）" @click="zoomCenter(1 / 1.2)"><ZoomOutOutlined /></button>
          <span class="dl-zoom">{{ Math.round(scale * 100) }}%</span>
          <button class="dl-btn" title="放大（+）" @click="zoomCenter(1.2)"><ZoomInOutlined /></button>
          <span class="dl-sep"></span>
          <button class="dl-btn txt" title="适应窗口（0）" @click="fitView"><ExpandOutlined /> 适应</button>
          <button class="dl-btn txt" title="原始尺寸" @click="set100">1:1</button>
          <span class="dl-sep"></span>
          <button class="dl-btn txt" title="导出矢量图" @click="downloadSvg"><DownloadOutlined /> SVG</button>
          <button class="dl-btn txt" title="导出位图（2x，底色随主题）" :disabled="exporting" @click="downloadPng">
            <LoadingOutlined v-if="exporting" spin />
            <FileImageOutlined v-else /> PNG
          </button>
          <span class="dl-sep"></span>
          <button class="dl-btn" title="关闭（Esc）" @click="close"><CloseOutlined /></button>
        </div>
      </div>
      <div
        ref="canvasEl"
        class="dl-canvas"
        @wheel.prevent="onWheel"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="onPointerUp"
        @dblclick="onDblClick"
        @click="onCanvasClick"
      >
        <div ref="stageEl" class="dl-stage"></div>
      </div>
      <div class="dl-hint">滚轮缩放 · 拖拽平移 · 双击 适应 / 1:1</div>
    </div>
  </Teleport>
</template>

<style scoped>
.dl-overlay {
  position: fixed;
  inset: 0;
  z-index: 21000;
  /* 暗色主题下图的节点本就是深色填充，舞台不能用白底 */
  --dl-fig-bg: #ffffff;
  background: rgba(12, 14, 18, 0.88);
  display: flex;
  flex-direction: column;
}
.dl-toolbar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 14px;
  background: rgba(20, 23, 28, 0.92);
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
}
.dl-title {
  font-size: 12.5px;
  color: rgba(255, 255, 255, 0.85);
  font-family: "JetBrains Mono", monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dl-title em {
  font-style: normal;
  color: rgba(255, 255, 255, 0.5);
}
.dl-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.dl-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  min-width: 28px;
  height: 28px;
  padding: 0 7px;
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: var(--radius-xs);
  background: transparent;
  color: rgba(255, 255, 255, 0.78);
  font-size: 12px;
  cursor: pointer;
}
.dl-btn:hover {
  color: #fff;
  border-color: rgba(255, 255, 255, 0.4);
  background: rgba(255, 255, 255, 0.06);
}
.dl-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.dl-btn.txt {
  font-size: 12px;
}
.dl-zoom {
  min-width: 44px;
  text-align: center;
  font-family: "JetBrains Mono", monospace;
  font-size: 11.5px;
  color: rgba(255, 255, 255, 0.6);
}
.dl-sep {
  width: 1px;
  height: 16px;
  background: rgba(255, 255, 255, 0.14);
  margin: 0 4px;
}
.dl-canvas {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.dl-canvas:active {
  cursor: grabbing;
}
.dl-stage {
  position: absolute;
  top: 0;
  left: 0;
  transform-origin: 0 0;
  will-change: transform;
  line-height: 0;
}
.dl-stage :deep(svg) {
  display: block;
  background: var(--dl-fig-bg, #fff);
  border-radius: 4px;
  box-shadow: 0 4px 28px rgba(0, 0, 0, 0.45);
}
.dl-overlay.dark {
  --dl-fig-bg: #1e222a;
}
.dl-hint {
  flex-shrink: 0;
  text-align: center;
  padding: 6px 0 8px;
  font-size: 11px;
  color: rgba(255, 255, 255, 0.4);
  user-select: none;
}
</style>
