/**
 * 桌面端 API 封装：通过 Tauri invoke 调用 Rust 端 command（src-tauri/src/lib.rs）
 *
 * 每个 Rust #[tauri::command] 在此对应一个类型安全的包装。
 *
 * 实现按域拆分到 ./<domain>.ts，本文件仅做聚合：将各域子对象展开合并为单一 `api`，
 * 对外保持平铺方法名（api.mcpServerStart / api.pickDirectory … 调用方式不变）。
 */
import { dialogApi } from "./dialog";
import { backupApi } from "./backup";
import { wallpaperApi } from "./wallpaper";
import { preferencesApi } from "./preferences";
import { mcpServerApi } from "./mcp-server";
import { apiServerApi } from "./api-server";
import { systemApi } from "./system";
import { llmApi } from "./llm";
import { repoApi } from "./repo";
import { generateApi } from "./generate";
import { callchainApi } from "./callchain";
import { watchApi, cardsApi } from "./watch";

export const api = {
  ...dialogApi,
  ...backupApi,
  ...wallpaperApi,
  ...preferencesApi,
  ...mcpServerApi,
  ...apiServerApi,
  ...systemApi,
  ...llmApi,
  ...repoApi,
  ...generateApi,
  ...callchainApi,
  ...watchApi,
  ...cardsApi,
};
