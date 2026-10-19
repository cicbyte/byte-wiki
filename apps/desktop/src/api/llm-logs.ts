/**
 * 模型日志 command 包装（调试支撑：LLM 调用全量 JSONL 留痕）
 */
import { invoke } from "@tauri-apps/api/core";

/** 单次 LLM 调用记录（与 Rust `LlmLogEntry` 对应，JSONL 一行；详情懒加载） */
export interface LlmLogEntry {
  ts: string;
  repo_id: number;
  repo_name: string;
  /** 归属分支（分支维度引入前的旧行为空串） */
  branch: string;
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
  /** 响应来自缓存回放（未真实请求） */
  cached?: boolean;
  error: string;
  system: string;
  user: string;
  response: string;
}

/** 列表瘦身项（不含 prompt/响应全文，全文按 line 懒加载） */
export interface LlmLogListItem {
  /** 详情定位（当前文件内 0 起行号；轮转后失效需刷新） */
  line: number;
  ts: string;
  branch: string;
  stage: string;
  provider: string;
  model: string;
  title: string;
  attempt: number;
  duration_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  ok: boolean;
  cached: boolean;
  /** 截断后的错误预览 */
  error: string;
  system_chars: number;
  user_chars: number;
  response_chars: number;
}

export const llmLogApi = {
  /** 全量列表（瘦身项，旧→新）+ 当前文件体积（字节） */
  listLlmLogs: (path: string) =>
    invoke<[LlmLogListItem[], number]>("list_llm_logs", { path }),

  /** 单条完整日志（system/user/response 全文） */
  llmLogDetail: (path: string, line: number) =>
    invoke<LlmLogEntry>("llm_log_detail", { path, line }),

  clearLlmLogs: (path: string) => invoke<void>("clear_llm_logs", { path }),

  /** LLM 响应缓存统计（条数， 总字节） */
  llmCacheStats: (path: string) => invoke<[number, number]>("llm_cache_stats", { path }),

  clearLlmCache: (path: string) => invoke<void>("clear_llm_cache", { path }),
};
