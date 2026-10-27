# 功能详解

## Wiki 生成与增量更新

- **生成流水线**：索引 → 规划 → 逐模块文档（拓扑序）→ 顶层架构 → 组装写入 `.bytewiki/`
- **变更检测**：代码变更后文档带滞后徽标；增量更新只重生成受影响模块及其下游，未变文档不动
- **知识卡 `cards/`**：人工沉淀的 Markdown 卡片，生成永不覆盖，可被问答检索

## 函数级调用链

- **多语言**：Python / JavaScript / TypeScript / Go / Java / Rust（tree-sitter 提取）
- **SQLite 主存储**：`callgraph.db` 毫秒级查询，同时双写 `callgraph.json` 供互操作
- **增量生成**：内容指纹判真变更；新符号出现后原先连不上的调用点自动复活
- **链路走读**：从任意函数 DFS 先序展开执行流；链断处给边界报告（断在哪一行、候选目标）
- **事件合成边**：`emit('x')` ↔ `on('x')` 配对补全跨函数事件流，边带 `event` 标注（非精确跳数可见）
- **LSP 精确化**：rust-analyzer 把启发式边升级为精确边，连错目标的边会被改写（画布 ⚡ 按钮或 `bw callchain --refine`）
- **规则按项目**：文件排除 / 入口排除 / 自定义入口正则（画布 ⚙）

## 可视化

- **调用链画布**：三种布局一键切换（有向分层 / 架构分层 / 同心环）；视觉深度过滤（N 跳外淡出）、镜头飞行、爆炸半径脉冲
- **项目图谱页**：全幅沉浸展示——分波渐现入场、hover 聚光灯、点击悬浮卡片

## 分支体系

- Wiki / 调用链 / 问答产物**按 git 分支隔离**，右上角分支图标全局切换
- **滞后检测**：生成时的 HEAD 与当前不一致即提示，可重新生成对齐
- **同 commit 免重算**：切到与兄弟分支相同 commit 时直接复制产物
- 跨分支查看读 **git 快照**，行号与该分支文档一致

## RAG 问答

- 向量检索 Wiki 文档 + 代码分块，流式输出
- 回答**带出处**：点击抽屉弹出对应源码片段
- 多会话管理（重命名 / 删除 / 切换）

## 导出与共享

- **HTML 静态站**：VitePress 形态，Mermaid 与代码高亮可用，浏览器打印即得 PDF
- `.bytewiki/` 纯文本随仓库共享，克隆即加载

## MCP 服务

把仓库知识暴露给 AI agent（9 个只读工具）：

| 工具 | 作用 |
|---|---|
| `status` / `list_repos` | 服务状态 / 已登记仓库清单 |
| `wiki_outline` / `read_wiki_doc` | 文档清单 / 读原文 |
| `ask_repo` | RAG 问答（带出处） |
| `callgraph_overview` / `symbol_search` / `callgraph_focus` / `callgraph_chain` | 调用链概览 / 搜节点 / 子图 / 走读链 |

stdio 接入（Claude Desktop / Cursor）：

```json
{
  "mcpServers": {
    "byte-wiki": {
      "command": "bw",
      "args": ["mcp", "serve"]
    }
  }
}
```

也可在「服务 → MCP 服务」页启动 HTTP 模式（Streamable HTTP，可配 API Key）。

## 模型配置

设置 → 模型：默认本地 Ollama；可切换 OpenAI 兼容远程端点（自定义 base URL / API Key / 模型名），支持连接测试。对话与嵌入模型独立选择。
