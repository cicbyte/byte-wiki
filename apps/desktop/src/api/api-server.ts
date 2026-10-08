import { invoke } from "@tauri-apps/api/core";

export const apiServerApi = {
  // ============ REST API 服务管理 ============

  /** 启动 REST API 服务 */
  apiServerStart: (port: number, host?: string, customKey?: string) =>
    invoke<{
      running: boolean;
      port: number;
      url: string;
      api_key?: string;
    }>("start_api_server", { host: host ?? null, port, customKey: customKey ?? null }),

  /** 停止 REST API 服务 */
  apiServerStop: () => invoke<void>("stop_api_server"),

  /** 查询 REST API 服务状态 + API Key */
  apiServerStatus: () =>
    invoke<{
      running: boolean;
      port: number;
      url: string;
      api_key?: string;
    }>("api_server_status"),
};
