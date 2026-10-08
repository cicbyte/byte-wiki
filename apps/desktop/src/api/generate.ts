/**
 * Wiki 生成 command 包装 + 进度事件订阅
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 生成进度事件（与 Rust `GenerateEvent` 对应） */
export interface GenerateEvent {
  phase: "indexing" | "planning" | "generating" | "assembling" | "done" | "error" | string;
  current: number;
  total: number;
  message: string;
  prompt_tokens: number;
  completion_tokens: number;
}

/** 生成结果摘要（与 Rust `GenerateSummary` 对应） */
export interface GenerateSummary {
  output_dir: string;
  modules_generated: number;
  prompt_tokens: number;
  completion_tokens: number;
  elapsed_ms: number;
}

/** wiki 文档清单项 (path, kind, title) */
export type WikiDocEntry = [string, string, string];

/** 仓库问答结果（与 Rust `ChatAnswer` 对应） */
export interface ChatAnswer {
  text: string;
  sources: string[];
  prompt_tokens: number;
  completion_tokens: number;
}

/** 向量重建摘要（与 Rust `EmbedSummary` 对应） */
export interface EmbedSummary {
  chunked: number;
  embedded: number;
}

export const GENERATE_EVENT = "wiki://generate";

export const generateApi = {
  generateWiki: (path: string) => invoke<GenerateSummary>("generate_wiki", { path }),

  listWikiDocs: (path: string) => invoke<WikiDocEntry[]>("list_wiki_docs", { path }),

  readWikiDoc: (repoPath: string, docPath: string) =>
    invoke<string>("read_wiki_doc", { repoPath, docPath }),

  repoChat: (path: string, question: string, history: [string, string][]) =>
    invoke<ChatAnswer>("repo_chat", { path, question, history }),

  rebuildEmbeddings: (path: string) => invoke<EmbedSummary>("rebuild_embeddings", { path }),

  /** 订阅生成进度事件，返回取消函数 */
  onGenerateEvent: (handler: (e: GenerateEvent) => void): Promise<UnlistenFn> =>
    listen<GenerateEvent>(GENERATE_EVENT, (ev) => handler(ev.payload)),
};
