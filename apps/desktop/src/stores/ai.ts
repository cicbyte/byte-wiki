/**
 * AI 会话 store：侧栏开关 + 编辑器选中注入总线
 *
 * - dockOpen：AI 对话侧栏（RepoSpace 内嵌布局右侧，VSCode Secondary Side Bar 式，
 *   挤压内容区而非浮层）；
 * - pending：编辑器右键「解释这段代码 / 添加到 AI 对话」的注入请求，
 *   ChatPanel watch seq 消费（explain=组装提问直接发送，append=引用块填入输入框）。
 */
import { defineStore, acceptHMRUpdate } from "pinia";
import { ref } from "vue";

export interface AiSelectionCtx {
  /** 递增序号：同内容重复注入也能触发 watch */
  seq: number;
  mode: "explain" | "append";
  code: string;
  file: string;
  startLine: number;
  endLine: number;
}

type SelectionInput = Omit<AiSelectionCtx, "seq" | "mode">;

let seq = 0;
/** 最近一次已被 ChatPanel 消费的 seq（幂等：侧栏重挂载不重复消费旧请求） */
let lastConsumed = 0;

export const useAiStore = defineStore("ai", () => {
  const dockOpen = ref(false);
  const pending = ref<AiSelectionCtx | null>(null);

  function explainSelection(ctx: SelectionInput) {
    pending.value = { ...ctx, seq: ++seq, mode: "explain" };
    dockOpen.value = true;
  }

  function appendSelection(ctx: SelectionInput) {
    pending.value = { ...ctx, seq: ++seq, mode: "append" };
    dockOpen.value = true;
  }

  /** 取走待处理注入（无或已消费过返回 null）。ChatPanel 挂载时与 watch 都经此消费，
   * 覆盖「注入触发侧栏刚挂载、watch 注册晚于 store 写入」的竞态。 */
  function consumePending(): AiSelectionCtx | null {
    const p = pending.value;
    if (!p || p.seq === lastConsumed) return null;
    lastConsumed = p.seq;
    return p;
  }

  return { dockOpen, pending, explainSelection, appendSelection, consumePending };
});

// Pinia HMR：store 热替换后保持同一实例（否则新旧模块并存，旧实例缺新方法）
if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useAiStore, import.meta.hot));
}
