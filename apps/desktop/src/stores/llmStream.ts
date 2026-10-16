/**
 * 模型流式终端状态：右下角浮窗实时滚 LLM 双通道原文
 *
 * 高频小事件直接追加（同通道相邻段合并），缓冲截断保尾部，
 * 首次流到达自动打开浮窗，之后手动控制。
 */
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { LlmStreamChunk } from "@/api/generate";

export interface StreamSeg {
  ch: "meta" | "think" | "content";
  text: string;
}

/** 缓冲字符上限（保尾部）
 *  超限从头丢整段，避免高频追加撑爆内存/渲染 */
const MAX_CHARS = 150_000;

export const useLlmStreamStore = defineStore("llmStream", () => {
  const segments = ref<StreamSeg[]>([]);
  const open = ref(false);
  /** 是否自动打开过（只在会话内第一次流到达时自动弹） */
  const autoOpened = ref(false);

  const totalChars = computed(() => segments.value.reduce((n, s) => n + s.text.length, 0));

  function trim() {
    let total = segments.value.reduce((n, s) => n + s.text.length, 0);
    while (total > MAX_CHARS && segments.value.length > 1) {
      const dropped = segments.value.shift()!;
      total -= dropped.text.length;
    }
  }

  function append(chunk: LlmStreamChunk) {
    if (!open.value && !autoOpened.value) {
      open.value = true;
    }
    autoOpened.value = true;
    const ch = (["meta", "think", "content"] as const).includes(chunk.channel as never)
      ? (chunk.channel as StreamSeg["ch"])
      : "content";
    const segs = segments.value;
    const last = segs[segs.length - 1];
    if (last && last.ch === ch) {
      last.text += chunk.delta;
    } else {
      segs.push({ ch, text: chunk.delta });
    }
    trim();
  }

  function clear() {
    segments.value = [];
  }

  function setOpen(v: boolean) {
    open.value = v;
  }

  return { segments, open, autoOpened, totalChars, append, clear, setOpen };
});
