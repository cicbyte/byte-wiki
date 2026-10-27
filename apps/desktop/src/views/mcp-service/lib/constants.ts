/**
 * MCP 服务页静态数据：监听地址选项 + 工具定义 + 接入指南文案
 */

/** 服务监听地址选项（a-select options） */
export const hostOptions = [
  { label: "127.0.0.1", value: "127.0.0.1" },
  { label: "0.0.0.0", value: "0.0.0.0" },
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
  {
    name: "git_status",
    desc: "git 工作区状态：暂存/修改/未跟踪文件清单。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
    ],
  },
  {
    name: "git_diff",
    desc: "git 差异文本：base 给 rev 看区间差异；缺省工作区 vs HEAD；path 可过滤。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "base", type: "string", required: false, desc: "基准 rev（如 HEAD~3）；缺省=工作区" },
      { name: "path", type: "string", required: false, desc: "只看某路径" },
    ],
  },
  {
    name: "git_log",
    desc: "git 提交历史（sha/作者/日期/主题），可按路径过滤。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "n", type: "number", required: false, desc: "条数（缺省 20）" },
      { name: "path", type: "string", required: false, desc: "只看某路径" },
    ],
  },
  {
    name: "git_show",
    desc: "某次提交的明细（stat + 补丁）。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "rev", type: "string", required: true, desc: "提交 rev" },
      { name: "path", type: "string", required: false, desc: "只看某路径" },
    ],
  },
];

/** 写入工具（阶段 2：AI 的手） */
export const writeTools: ToolDef[] = [
  {
    name: "create_card",
    desc: "创建知识卡（人工知识区，生成永不覆盖、可被问答检索）。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "title", type: "string", required: true, desc: "卡片标题" },
      { name: "content", type: "string", required: true, desc: "正文 Markdown" },
    ],
  },
  {
    name: "edit_wiki_doc",
    desc: "定向编辑 wiki 文档/知识卡：search/replace 块全有或全无（每块恰好命中一次）。",
    params: [
      { name: "repo", type: "string", required: true, desc: "仓库绝对路径" },
      { name: "doc_path", type: "string", required: true, desc: "文档路径" },
      { name: "replacements", type: "string", required: true, desc: '[{search, replace}] 数组' },
    ],
  },
];

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
