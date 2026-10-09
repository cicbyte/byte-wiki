/**
 * MCP 服务页静态数据：监听地址选项 + 工具定义 + 接入指南文案
 */

/** 服务监听地址选项（a-select options） */
export const hostOptions = [
  { label: "127.0.0.1（仅本机）", value: "127.0.0.1" },
  { label: "0.0.0.0（局域网可访问）", value: "0.0.0.0" },
];

/** MCP 工具参数定义 */
export interface ToolParam {
  name: string;
  type: "string" | "number" | "boolean";
  required: boolean;
  desc: string;
}

export interface ToolDef {
  name: string;
  desc: string;
  params: ToolParam[];
}

/** 只读工具（与 src-tauri / core 的 MCP server #[tool] 定义保持一致） */
export const readTools: ToolDef[] = [
  {
    name: "status",
    desc: "返回 byte-wiki 服务状态：版本、数据目录、已登记仓库数。",
    params: [],
  },
];

/** 修改类工具（当前无——写入类工具随 M3 开放） */
export const writeTools: ToolDef[] = [];

/** stdio / HTTP 接入说明（配置 tab，MdPreview 渲染） */
export const mcpGuide = `## stdio 接入（Claude Desktop / Cursor 等）

在客户端的 MCP 配置中添加：

\`\`\`json
{
  "mcpServers": {
    "byte-wiki": {
      "command": "bs",
      "args": ["mcp"]
    }
  }
}
\`\`\`

## HTTP 接入（Streamable HTTP）

在「服务管理」页启动服务后，端点形如 \`http://localhost:18030/mcp\`：

\`\`\`json
{
  "mcpServers": {
    "byte-wiki": {
      "url": "http://localhost:18030/mcp"
    }
  }
}
\`\`\`

启用认证时，请求需携带 \`Authorization: Bearer <MCP Key>\`（Key 在服务管理页生成/保存）。
`;
