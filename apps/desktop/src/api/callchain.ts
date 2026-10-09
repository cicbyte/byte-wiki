/**
 * 调用链 command 包装 + 进度事件订阅
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 生成结果摘要（与 Rust `CallchainSummary` 对应） */
export interface CallchainSummary {
  out_path: string;
  nodes: number;
  edges: number;
  entries: number;
  modules: number;
  langs: Record<string, number>;
  duration_ms: number;
}

/** 调用图节点（app 管线产物不含 source） */
export interface CallGraphNode {
  id: string;
  module: string;
  qualname: string;
  name: string;
  file: string;
  lineno: number;
  end_lineno: number;
  routes: string[];
  async?: boolean;
  is_entry: boolean;
  source?: string;
  callers: number;
  callees: number;
}

/** 调用边（kind: local 精确同文件 / heuristic 同名启发） */
export interface CallgraphEdge {
  src: string;
  dst: string;
  line: number;
  kind: "local" | "heuristic" | string;
}

export interface CallgraphData {
  root: string;
  nodes: CallGraphNode[];
  edges: CallgraphEdge[];
  modules: Record<string, { file: string; funcs: number }>;
  langs: Record<string, number>;
}

export const CALLCHAIN_EVENT = "wiki://callchain";

/** 调用图产物在 .bytewiki/ 内的路径（read_wiki_doc 口径） */
export const CALLGRAPH_DOC_PATH = "graphs/callchain/callgraph.json";

export const callchainApi = {
  /** 生成调用链（进度经 CALLCHAIN_EVENT 推送） */
  callchainGenerate: (path: string) => invoke<CallchainSummary>("generate_callchain", { path }),

  /** 读取既有调用图 JSON 文本（复用 read_wiki_doc；不存在时后端报「文档不存在」） */
  callchainRead: (repoPath: string) =>
    invoke<string>("read_wiki_doc", { repoPath, docPath: CALLGRAPH_DOC_PATH }),

  /** 读取仓库内源码文件（Monaco 定位打开，路径防护在后端） */
  readRepoFile: (repoPath: string, relPath: string) =>
    invoke<string>("read_repo_file", { repoPath, relPath }),

  /** 订阅生成进度（已解析文件数），返回取消函数 */
  callchainOnProgress: (handler: (n: number) => void): Promise<UnlistenFn> =>
    listen<number>(CALLCHAIN_EVENT, (ev) => handler(ev.payload)),
};
