/**
 * 仓库（Repo Wiki）command 包装
 */
import { invoke } from "@tauri-apps/api/core";

/** 仓库登记信息（与 Rust `RepoInfo` 对应） */
export interface RepoInfo {
  id: number;
  path: string;
  name: string;
  branch: string | null;
  file_count: number;
  last_opened_at: string | null;
  last_indexed_at: string | null;
  last_generated_at: string | null;
}

/** 索引摘要（与 Rust `IndexSummary` 对应） */
export interface IndexSummary {
  files_indexed: number;
  modules: { name: string; file_count: number; langs: string[] }[];
  edges: number;
}

export const repoApi = {
  openRepo: (path: string) => invoke<RepoInfo>("open_repo", { path }),

  recentRepos: (limit = 10) => invoke<RepoInfo[]>("recent_repos", { limit }),

  indexRepo: (path: string) => invoke<IndexSummary>("index_repo", { path }),
};
