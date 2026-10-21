/**
 * 调用链数据访问 —— `bw callchain-query` 封装（#581）。
 *
 * 后端命令对照（apps/cli/src/cmd/callchain.rs）：
 *   overview → bw --json callchain-query <repo> --overview
 *   chain    → bw --json callchain-query <repo> --chain <id> --limit <n>
 *   search   → bw --json callchain-query <repo> --grep <pat> --limit <n>
 *   generate → bw --json open <repo>（登记幂等）+ bw --json callchain <repo>
 *
 * 读取走 AppService 的 db 优先 + mtime 缓存路径（#573），毫秒级；
 * 未生成产物时 bs 以非零退出，runBs 抛 BsError。
 */
import { runBs } from "./cliClient";

export interface OverviewEntry {
  id: string;
  qualname: string;
  name: string;
  file: string;
  module: string;
  is_entry: boolean;
  is_exported?: boolean;
  routes?: string[];
  /** 概览轻量字段（#581 起补齐：外部壳点入口可直接跳函数体） */
  lineno?: number;
  end_lineno?: number;
}

export interface Overview {
  total_nodes: number;
  total_edges: number;
  total_entries: number;
  entries_are_api: boolean;
  entries: OverviewEntry[];
  modules: string[];
}

export interface ChainCallLink {
  target_id: string;
  target_num: string;
  target_name: string;
  line: number;
  is_ref: boolean;
}

export interface ChainEntry {
  id: string;
  num: string;
  node: {
    id: string;
    qualname: string;
    file: string;
    lineno: number;
    end_lineno: number;
    is_entry?: boolean;
    routes?: string[];
  };
  calls: ChainCallLink[];
}

export interface Chain {
  root: string;
  entries: ChainEntry[];
  main_path: string[];
  truncated: boolean;
}

function parse<T>(out: string): T {
  return JSON.parse(out.trim()) as T;
}

export async function callchainOverview(wsPath: string): Promise<Overview> {
  return parse<Overview>(await runBs(["--json", "callchain-query", wsPath, "--overview"]));
}

export async function callchainChain(wsPath: string, rootId: string, cap = 120): Promise<Chain> {
  return parse<Chain>(
    await runBs(["--json", "callchain-query", wsPath, "--chain", rootId, "--limit", String(cap)]),
  );
}

export async function callchainSearch(wsPath: string, pattern: string, limit = 30): Promise<OverviewEntry[]> {
  const r = parse<{ nodes: OverviewEntry[] }>(
    await runBs(["--json", "callchain-query", wsPath, "--grep", pattern, "--limit", String(limit)]),
  );
  return r.nodes ?? [];
}

/** 生成（登记幂等 + 提取）。大仓可达分钟级，放宽超时到 10 分钟。 */
export async function callchainGenerate(wsPath: string): Promise<void> {
  await runBs(["--json", "open", wsPath], undefined, 30_000);
  await runBs(["--json", "callchain", wsPath], undefined, 600_000);
}
