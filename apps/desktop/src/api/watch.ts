/**
 * 变更检测 + 增量更新 + 知识卡 command 包装（M2）
 */
import { invoke } from "@tauri-apps/api/core";

/** 变更文件（与 Rust `FileChange` 对应） */
export interface FileChange {
  path: string;
  kind: "added" | "removed" | "modified" | string;
  module: string;
}

/** 滞后报告（与 Rust `StaleReport` 对应） */
export interface StaleReport {
  stale: boolean;
  changes: FileChange[];
  affected_modules: string[];
  last_generated_at: string | null;
}

/** 增量更新摘要（与 Rust `UpdateSummary` 对应） */
export interface UpdateSummary {
  regenerated_pages: number;
  changed_files: number;
  prompt_tokens: number;
  completion_tokens: number;
  elapsed_ms: number;
  unchanged: boolean;
}

export const watchApi = {
  checkStaleness: (path: string) => invoke<StaleReport>("check_staleness", { path }),

  updateWiki: (path: string) => invoke<UpdateSummary>("update_wiki", { path }),
};

export const cardsApi = {
  createCard: (path: string, title: string, content: string) =>
    invoke<string>("create_card", { path, title, content }),

  deleteCard: (path: string, cardRel: string) =>
    invoke<void>("delete_card", { path, cardRel }),
};
