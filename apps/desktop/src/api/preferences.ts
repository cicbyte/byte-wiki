import { invoke } from "@tauri-apps/api/core";

export const preferencesApi = {
  /** 读取导入时排除的目录名列表（config.toml preferences.exclude_dirs） */
  getExcludeDirs: () => invoke<string[]>("get_exclude_dirs"),

  /** 设置导入时排除的目录名列表（即时生效） */
  setExcludeDirs: (dirs: string[]) =>
    invoke<void>("set_exclude_dirs", { dirs }),

  /** 保存 API Keys 到 config（restKey / mcpKey 可选，传 null 跳过） */
  setApiKeys: (restKey?: string | null, mcpKey?: string | null) =>
    invoke<void>("set_api_keys", { restKey: restKey ?? null, mcpKey: mcpKey ?? null }),

  /** 用系统默认浏览器打开 URL */
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};
