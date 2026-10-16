/**
 * Wiki 生成 command 包装 + 进度事件订阅
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 页面级进度（后端按 id 推增量，前端合并出清单） */
export interface PageProgress {
  id: string;
  title: string;
  /** 章节链展示（"系统架构 / 后端"） */
  section: string;
  importance: "high" | "medium" | "low" | string;
  status: "pending" | "generating" | "done" | "failed";
  /** .bytewiki/ 内目标路径（树骨架定位 + 生成后衔接磁盘文档） */
  path?: string | null;
  /** 前端本地累积的实时正文（流式按页拼接；后端不回传） */
  content?: string;
}

/** 生成进度事件（与 Rust `GenerateEvent` 对应） */
export interface GenerateEvent {
  phase: "indexing" | "outlining" | "generating" | "assembling" | "done" | "error" | string;
  current: number;
  total: number;
  message: string;
  prompt_tokens: number;
  completion_tokens: number;
  /** 大纲确定事件携带全量骨架；此后每页携带单条变更 */
  pages?: PageProgress[];
}

/** 生成结果摘要（与 Rust `GenerateSummary` 对应） */
export interface GenerateSummary {
  output_dir: string;
  pages_generated: number;
  pages_failed: number;
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

/** 模型流式增量（右下角终端；与 Rust `StreamChunk` 对应） */
export interface LlmStreamChunk {
  /** 调用标识（如「《核心库》撰写中」） */
  call: string;
  /** meta（调用开始/结束标记）/ think / content */
  channel: "meta" | "think" | "content" | string;
  delta: string;
  /** 页面 id（正文按页累积，实时查看用） */
  page_id?: string | null;
}

export const GENERATE_EVENT = "wiki://generate";
export const LLM_STREAM_EVENT = "wiki://llm-stream";

export const generateApi = {
  generateWiki: (path: string) => invoke<GenerateSummary>("generate_wiki", { path }),

  listWikiDocs: (path: string) => invoke<WikiDocEntry[]>("list_wiki_docs", { path }),

  /** 大纲既定顺序的页面路径表 (rel_path, title)——文档树按大纲序渲染 */
  wikiOutlinePaths: (path: string) => invoke<[string, string][]>("wiki_outline_paths", { path }),

  readWikiDoc: (repoPath: string, docPath: string) =>
    invoke<string>("read_wiki_doc", { repoPath, docPath }),

  repoChat: (path: string, question: string, history: [string, string][]) =>
    invoke<ChatAnswer>("repo_chat", { path, question, history }),

  rebuildEmbeddings: (path: string) => invoke<EmbedSummary>("rebuild_embeddings", { path }),

  /** 订阅生成进度事件，返回取消函数 */
  onGenerateEvent: (handler: (e: GenerateEvent) => void): Promise<UnlistenFn> =>
    listen<GenerateEvent>(GENERATE_EVENT, (ev) => handler(ev.payload)),

  /** 订阅模型流式增量（高频），返回取消函数 */
  onLlmStream: (handler: (c: LlmStreamChunk) => void): Promise<UnlistenFn> =>
    listen<LlmStreamChunk>(LLM_STREAM_EVENT, (ev) => handler(ev.payload)),
};
