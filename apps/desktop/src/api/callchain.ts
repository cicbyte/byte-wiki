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
  /** 公共 API（导出符号：Rust pub / Go 大写 / JS-TS export / Python 顶层公开 / Java public） */
  is_exported?: boolean;
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

/** 走读页单个函数源码切片 */
export interface ChainSource {
  id: string;
  code: string;
  truncated: boolean;
}

/** 调用链规则（config preferences.callchain） */
export interface CallchainRules {
  /** 不收录进调用图的文件 glob */
  exclude_globs: string[];
  /** 仍收录但不标入口的文件 glob（入口噪声治理） */
  entry_exclude_globs: string[];
  /** 自定义入口 qualname 正则（叠加内置识别） */
  entry_regex?: string | null;
}

export const callchainApi = {
  /** 生成调用链（进度经 CALLCHAIN_EVENT 推送） */
  callchainGenerate: (path: string) => invoke<CallchainSummary>("generate_callchain", { path }),

  /** 读取既有调用图 JSON 文本（复用 read_wiki_doc；不存在时后端报「文档不存在」） */
  callchainRead: (repoPath: string) =>
    invoke<string>("read_wiki_doc", { repoPath, docPath: CALLGRAPH_DOC_PATH }),

  /** 读取仓库内源码文件（Monaco 定位打开，路径防护在后端） */
  readRepoFile: (repoPath: string, relPath: string) =>
    invoke<string>("read_repo_file", { repoPath, relPath }),

  /** 走读页批量源码切片：(节点id, 相对路径, 起行, 止行)[]（文件去重 + 服务端切片） */
  getChainSources: (
    repoPath: string,
    items: [string, string, number, number][],
  ) => invoke<ChainSource[]>("get_chain_sources", { repoPath, items }),

  /** 订阅生成进度（已解析文件数），返回取消函数 */
  callchainOnProgress: (handler: (n: number) => void): Promise<UnlistenFn> =>
    listen<number>(CALLCHAIN_EVENT, (ev) => handler(ev.payload)),

  /** 读取调用链规则 */
  getRules: () => invoke<CallchainRules>("get_callchain_rules"),

  /** 保存调用链规则（重新生成调用链后生效） */
  setRules: (rules: CallchainRules) => invoke<void>("set_callchain_rules", { rules }),
};
