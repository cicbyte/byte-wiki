/**
 * 模型日志 command 包装（调试支撑：LLM 调用全量 JSONL 留痕）
 */
import { invoke } from "@tauri-apps/api/core";

/** 单次 LLM 调用记录（与 Rust `LlmLogEntry` 对应，JSONL 一行） */
export interface LlmLogEntry {
  ts: string;
  repo_id: number;
  repo_name: string;
  /** outline / page / overview / chat */
  stage: string;
  provider: string;
  model: string;
  title: string;
  attempt: number;
  duration_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  ok: boolean;
  error: string;
  system: string;
  user: string;
  response: string;
}

export const llmLogApi = {
  /** 尾部日志（最新在后）+ 当前文件体积（字节） */
  listLlmLogs: (path: string, limit?: number) =>
    invoke<[LlmLogEntry[], number]>("list_llm_logs", { path, limit }),

  clearLlmLogs: (path: string) => invoke<void>("clear_llm_logs", { path }),
};
