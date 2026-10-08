import { invoke } from "@tauri-apps/api/core";

export const mcpServerApi = {
  // ============ MCP HTTP 服务管理 ============

  /** 启动 MCP HTTP 服务，返回端点 URL */
  mcpServerStart: (port: number, host?: string, useAuth?: boolean) =>
    invoke<string>("start_mcp_server", { host: host ?? null, port, useAuth: useAuth ?? false }),

  /** 停止 MCP HTTP 服务 */
  mcpServerStop: () => invoke<void>("stop_mcp_server"),

  /** 查询 MCP 服务状态 */
  mcpServerStatus: () =>
    invoke<{ running: boolean; port: number; url: string; mcp_key: string }>(
      "mcp_server_status"
    ),
};
