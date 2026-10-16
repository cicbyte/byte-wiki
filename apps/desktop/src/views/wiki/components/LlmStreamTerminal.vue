<script setup lang="ts">
/**
 * 模型流式终端：右下角浮动窗口，实时滚 LLM 调用的思考/正文原文
 *
 * 通道着色：meta=调用开始/完成标记（主色）、think=思考（暗灰斜体）、
 * content=正文（亮）。自动滚底（用户上滚查看历史时暂停跟随）。
 * 高频追加只重绘文本节点；缓冲由 store 截断。
 */
import { nextTick, ref, watch } from "vue";
import {
  ClearOutlined,
  ShrinkOutlined,
  ExpandOutlined,
  CloseOutlined,
  CodeOutlined,
} from "@ant-design/icons-vue";
import { useLlmStreamStore } from "@/stores/llmStream";

const stream = useLlmStreamStore();

const bodyEl = ref<HTMLElement | null>(null);
/** 用户上滚时暂停自动跟随，滚回底部恢复 */
const follow = ref(true);

function onScroll() {
  const el = bodyEl.value;
  if (!el) return;
  follow.value = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
}

watch(
  () => stream.segments.length,
  async () => {
    if (!follow.value) return;
    await nextTick();
    const el = bodyEl.value;
    if (el) el.scrollTop = el.scrollHeight;
  },
);

// 段内文本追加（同段合并）不触发 length 变化，补充总量监听
watch(
  () => stream.totalChars,
  async () => {
    if (!follow.value) return;
    await nextTick();
    const el = bodyEl.value;
    if (el) el.scrollTop = el.scrollHeight;
  },
);

function fmtChars(n: number): string {
  return n >= 10000 ? `${(n / 10000).toFixed(1)}万` : `${n}`;
}

function closeAndClear() {
  stream.setOpen(false);
  stream.clear();
}
</script>

<template>
  <!-- 收起态：右下角悬浮按钮 -->
  <button
    v-if="!stream.open && stream.segments.length"
    class="stream-fab"
    title="模型流式终端"
    @click="stream.setOpen(true)"
  >
    <CodeOutlined />
    <span>流式终端</span>
    <em>{{ fmtChars(stream.totalChars) }} 字</em>
  </button>

  <!-- 展开态：终端浮窗 -->
  <div v-if="stream.open" class="stream-term">
    <div class="term-head">
      <span class="term-dot"></span>
      <span class="term-title">模型流式终端</span>
      <span class="term-meta">{{ fmtChars(stream.totalChars) }} 字</span>
      <span class="term-actions">
        <button class="tbtn" title="清空" @click="stream.clear()"><ClearOutlined /></button>
        <button class="tbtn" title="收起" @click="stream.setOpen(false)"><ShrinkOutlined /></button>
        <button class="tbtn" title="关闭并清空" @click="closeAndClear"><CloseOutlined /></button>
      </span>
    </div>
    <div ref="bodyEl" class="term-body" @scroll="onScroll">
      <pre class="term-pre"><span
v-for="(s, i) in stream.segments"
:key="i"
class="seg"
:class="s.ch"
>{{ s.text }}</span><span v-if="!stream.segments.length" class="seg empty">（等待模型输出…）</span></pre>
    </div>
    <div class="term-foot">
      <ExpandOutlined class="foot-icon" />
      meta=调用标记 · 灰=思考 · 亮=正文 · 上滚暂停跟随
    </div>
  </div>
</template>

<style scoped>
.stream-fab {
  position: fixed;
  right: 18px;
  bottom: 18px;
  z-index: 900;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 7px 14px;
  border-radius: 999px;
  border: 1px solid #30363d;
  background: #161b22;
  color: #c9d1d9;
  font-size: 12px;
  cursor: pointer;
  box-shadow: 0 4px 18px rgba(0, 0, 0, 0.35);
}
.stream-fab:hover {
  border-color: var(--color-primary);
  color: var(--color-primary);
}
.stream-fab em {
  font-style: normal;
  font-size: 11px;
  color: #8b949e;
  font-family: "JetBrains Mono", monospace;
}

.stream-term {
  position: fixed;
  right: 18px;
  bottom: 18px;
  z-index: 900;
  width: 560px;
  max-width: calc(100vw - 36px);
  height: 340px;
  display: flex;
  flex-direction: column;
  border-radius: 10px;
  border: 1px solid #30363d;
  background: #0d1117;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.45);
  overflow: hidden;
}
.term-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  background: #161b22;
  border-bottom: 1px solid #30363d;
  flex-shrink: 0;
  cursor: default;
}
.term-dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #3fb950;
  box-shadow: 0 0 6px #3fb95080;
}
.term-title {
  font-size: 12px;
  font-weight: 600;
  color: #c9d1d9;
  letter-spacing: 0.03em;
}
.term-meta {
  font-size: 11px;
  color: #8b949e;
  font-family: "JetBrains Mono", monospace;
}
.term-actions {
  margin-left: auto;
  display: inline-flex;
  gap: 2px;
}
.tbtn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: #8b949e;
  border-radius: 5px;
  cursor: pointer;
  font-size: 12px;
}
.tbtn:hover {
  background: #21262d;
  color: #c9d1d9;
}
.term-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 12px;
}
.term-pre {
  margin: 0;
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: 11px;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
}
.seg.content {
  color: #d5dde5;
}
.seg.think {
  color: #768390;
  font-style: italic;
}
.seg.meta {
  color: #58a6ff;
  font-weight: 600;
}
.seg.empty {
  color: #484f58;
}
.term-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px;
  border-top: 1px solid #21262d;
  font-size: 10.5px;
  color: #6e7681;
  flex-shrink: 0;
}
.foot-icon {
  font-size: 10px;
}
/* 终端滚动条 */
.term-body::-webkit-scrollbar {
  width: 8px;
}
.term-body::-webkit-scrollbar-thumb {
  background: #30363d;
  border-radius: 4px;
}
.term-body::-webkit-scrollbar-track {
  background: transparent;
}
</style>
