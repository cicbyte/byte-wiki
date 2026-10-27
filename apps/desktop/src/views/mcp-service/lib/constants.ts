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

/** 只读工具（与 core 的 MCP server #[tool] 定义保持一致；repo 参数=仓库绝对路径，branch 缺省=当前分支） */
export const readTools: ToolDef[] = [
  {
    name: "status",
    desc: "返回 byte-wiki 服务状态：版本、数据目录、已登记仓库数。",
    params: [],
  },
  {
    name: "list_repos",
    desc: "列出已登记的仓库：名称、路径、分支、文件数、最近生成时间。",
    params: [],
  },
  {
    name: "wiki_outline",
    desc: "仓库 Wiki 文档清单（路径/类型/标题），doc_path 供 read_wiki_doc 使用。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径（list_repos 可查）" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
  },
  {
    name: "read_wiki_doc",
    desc: "读取 Wiki 文档原文（Markdown）；cards/ 前缀读人工知识卡。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "doc_path", type: "string", required: true, desc: "文档路径（wiki_outline 可查）" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
  },
  {
    name: "ask_repo",
    desc: "仓库 RAG 问答：基于 Wiki/知识卡的向量检索回答，返回答案与出处。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "question", type: "string", required: true, desc: "问题" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
  },
  {
    name: "callgraph_overview",
    desc: "调用图概览（JSON）：统计、入口清单、模块聚合边、滞后状态。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
  },
  {
    name: "symbol_search",
    desc: "符号搜索：按 qualname/file 模糊匹配函数节点（定位种子节点用）。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "pattern", type: "string", required: true, desc: "模糊模式（≥2 字符）" },
      { name: "limit", type: "number", required: false, desc: "条数上限（缺省 20）" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
  },
  {
    name: "callgraph_focus",
    desc: "种子节点的 N 跳双向邻域子图（JSON：nodes/edges）。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "seed", type: "string", required: true, desc: "种子节点 ID（symbol_search 可查）" },
      { name: "depth", type: "number", required: false, desc: "跳数 1-6（缺省 2）" },
      { name: "cap", type: "number", required: false, desc: "节点数上限（缺省 300）" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
  },
  {
    name: "callgraph_chain",
    desc: "走读链（JSON）：DFS 先序展开执行流，含主路径与断点边界报告。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "node", type: "string", required: true, desc: "起点节点 ID" },
      { name: "cap", type: "number", required: false, desc: "块数上限（缺省 200）" },
      { name: "branch", type: "string", required: false, desc: "分支（缺省=当前分支）" },
    ],
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
      "command": "bw",
      "args": ["mcp", "serve"]
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
