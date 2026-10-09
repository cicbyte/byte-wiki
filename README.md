# byte-wiki

> 本地优先（local-first）的仓库级知识引擎：为本地 Git 仓库自动生成结构化 Wiki（架构 / 模块 / 图谱），并支持基于仓库内容的 RAG 问答。代码默认不出本机。

## 它做什么

- **一键生成 Wiki**（FR-01/02）：选择本地 Git 仓库，自动完成 索引 → 规划 → 逐模块文档（拓扑序）→ 顶层架构 → 组装写入仓库内 `.bytewiki/` 目录
- **结构化浏览**（FR-03/04）：架构 / 模块 / 图谱三分区树形导航；Mermaid 模块依赖图由真实代码引用生成（非 LLM 臆测），支持缩放
- **函数级调用链**：tree-sitter 多语言（Py/JS/TS/Go/Java/Rust）提取函数调用图，项目空间「调用链」tab 原生浏览——入口执行流聚焦、双向调用跳转、点函数 Monaco 定位打开源码；产物 `callgraph.json` 随仓库共享
- **RAG 问答**（FR-05）：基于 Wiki 文档与代码分块的向量检索，流式输出、回答带出处、多轮对话
- **模型可配置**（FR-09）：默认本地 Ollama，可切换 OpenAI 兼容远程端点（设置 → 模型，支持连接测试）
- **随仓库共享**（FR-08）：Wiki 是纯 Markdown + Mermaid 文本，提交 `.bytewiki/` 即团队共享；克隆后打开仓库直接加载，零 LLM 成本

## 快速开始

### 桌面端（推荐）

```bash
# 前置：Rust（MSVC）、Node.js 18+、Ollama（可选，本地模型）
cd apps/desktop
npm install
npm run tauri dev    # 开发
npm run tauri build  # 打包
```

1. 启动本机 [Ollama](https://ollama.com) 并拉取模型（如 `ollama pull qwen3.5:9b` 与 `ollama pull nomic-embed-text`）
2. 打开应用 → 设置 → 模型：确认 provider 为 Ollama，填模型名，点「测试连接」
3. 首页打开一个本地 Git 仓库 → 「一键生成 Wiki」→ 浏览 / 问答

### CLI

```bash
cargo run -p bs -- --help
# backup / restore：数据目录备份恢复
# callchain <repo>：函数级调用链（产物落 <repo>/.bytewiki/graphs/callchain/）
# mcp / api：MCP 与 REST 服务（骨架）

cargo run -p byte-wiki-callchain -- <src>   # standalone：任意目录 + 交互式 HTML 三件套
```

## `.bytewiki/` 目录结构

```
.bytewiki/
  index.json          # 版本、模型、生成时间、文件清单
  architecture.md     # 顶层架构文档
  modules/<模块>.md   # 各模块技术文档（职责/结构/接口/依赖）
  graphs/dependency.mmd # Mermaid 模块依赖图
```

> `modules/` 为机器生成区，重新生成会被覆盖；人工知识请写 `cards/`（受保护，M2 完整支持）。

## 技术栈

Tauri 2（Rust）+ Vue 3 + 自研主题体系（复用自 byte-wiki 桌面骨架）；
核心库 `crates/core`：索引（`ignore` + 多语言 import 提取）、生成流水线、LLM 客户端（Ollama / OpenAI 兼容，流式）、RAG（SQLite 向量 + 余弦检索）、MCP/REST 服务骨架。
CLI 与桌面端共享同一套核心业务。

## 开发

```bash
cargo test -p byte-wiki-core          # 核心单测（含 mock LLM）
cargo test -p byte-wiki-core real_e2e_smoke -- --ignored --nocapture  # 真实 ollama 端到端
cd apps/desktop && npm run build      # 前端类型检查 + 构建
```

## 路线图

- **M2（已交付）**：变更检测（文档滞后徽标 + 变更清单）、手动增量更新（仅重生成受影响模块及其下游，未变文档不动）、`cards/` 人工知识卡（创建/删除，生成永不覆盖，可被问答检索）
- **M2（受阻）**：SQLCipher 加密 —— vendored OpenSSL 需 Strawberry Perl + NASM 工具链，安装后即可启用（见 PRD 开放问题）
- **M3**：Knowledge Engine 三源融合（Wiki + Card + Memory）、macOS/Linux

## 许可证

见 [LICENSE](LICENSE)。
