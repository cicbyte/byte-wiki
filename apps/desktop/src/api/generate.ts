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
  /** 归属项目（前端按项目过滤；本进程内事件必填） */
  repo_id?: number | null;
  /** 归属分支（任务启动时捕获） */
  branch?: string | null;
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
  repo_id?: number | null;
  branch?: string | null;
}

export const GENERATE_EVENT = "wiki://generate";
export const LLM_STREAM_EVENT = "wiki://llm-stream";

/** 后台生成任务快照（与 Rust `GenTask` 对应） */
export interface GenTask {
  repo_id: number;
  /** 归属分支（生成启动时捕获） */
  branch: string;
  repo_name: string;
  repo_path: string;
  kind: "generate" | "update" | string;
  phase: string;
  current: number;
  total: number;
  message: string;
  pages: PageProgress[];
  started_ms: number;
  finished: boolean;
  error: string;
  prompt_tokens: number;
  completion_tokens: number;
}

/** 项目总览聚合（与 Rust `RepoOverview` 对应） */
export interface RepoOverview {
  files_indexed: number;
  modules: number;
  edges: number;
  /** 语言分布 [lang, 文件数] */
  langs: [string, number][];
  /** 模块 TOP [module, 文件数] */
  top_modules: [string, number][];
  wiki_sections: number;
  wiki_pages: number;
  cards: number;
  chunks: number;
  /** 最近活动 [kind, detail, created_at] */
  recent_activity: [string, string, string][];
  /** 代码规模 */
  total_lines: number;
  code_lines: number;
  blank_lines: number;
  size_bytes: number;
  /** 按语言行数分布 [lang, 文件数, 行数]（行数降序 TOP8） */
  lang_lines: [string, number, number][];
}

export const generateApi = {
  generateWiki: (path: string) => invoke<GenerateSummary>("generate_wiki", { path }),

  /** 项目总览聚合（只读） */
  repoOverview: (path: string) => invoke<RepoOverview>("repo_overview", { path }),

  /** 后台生成任务快照（全局任务页/页面重入水合） */
  listGenTasks: () => invoke<GenTask[]>("list_gen_tasks"),

  clearGenTasks: () => invoke<void>("clear_gen_tasks"),

  listWikiDocs: (path: string) => invoke<WikiDocEntry[]>("list_wiki_docs", { path }),

  /** 大纲既定顺序的页面路径表 (rel_path, title)——文档树按大纲序渲染 */
  wikiOutlinePaths: (path: string) => invoke<[string, string][]>("wiki_outline_paths", { path }),

  readWikiDoc: (repoPath: string, docPath: string) =>
    invoke<string>("read_wiki_doc", { repoPath, docPath }),

  /** 已生成 wiki 产物的分支列表（文档页只读切换选项） */
  generatedBranches: (path: string) => invoke<string[]>("generated_branches", { path }),

  /** 指定查看分支的文档清单（只读视图；branch 缺省=当前分支） */
  listWikiDocsFor: (path: string, branch?: string) =>
    invoke<WikiDocEntry[]>("list_wiki_docs_for", { path, branch }),

  /** 读取指定分支产物（只读） */
  readWikiDocFor: (repoPath: string, docPath: string, viewBranch: string) =>
    invoke<string>("read_wiki_doc_for", { repoPath, docPath, viewBranch }),

  repoChat: (path: string, question: string, history: [string, string][], branch?: string | null) =>
    invoke<ChatAnswer>("repo_chat", { path, question, history, branch: branch ?? null }),

  rebuildEmbeddings: (path: string) => invoke<EmbedSummary>("rebuild_embeddings", { path }),

  /** 订阅生成进度事件，返回取消函数 */
  onGenerateEvent: (handler: (e: GenerateEvent) => void): Promise<UnlistenFn> =>
    listen<GenerateEvent>(GENERATE_EVENT, (ev) => handler(ev.payload)),

  /** 订阅模型流式增量（高频），返回取消函数 */
  onLlmStream: (handler: (c: LlmStreamChunk) => void): Promise<UnlistenFn> =>
    listen<LlmStreamChunk>(LLM_STREAM_EVENT, (ev) => handler(ev.payload)),
};
