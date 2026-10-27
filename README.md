# byte-wiki

> 本地优先的仓库级知识引擎——把 Git 仓库变成可浏览、可追问、可分享的结构化知识，代码不出本机。

[English](README.en.md) | 中文

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/built%20with-Rust-dea584?logo=rust)
![Tauri](https://img.shields.io/badge/desktop-Tauri%202-24c8d8)

## 目录

- [它能做什么](#它能做什么)
- [快速开始](#快速开始)
- [.bytewiki/ 产物结构](#bytewiki-产物结构)
- [配置说明](#配置说明)
- [架构](#架构)
- [开发](#开发)
- [许可证](#许可证)

## 它能做什么

### 知识生成

- **一键生成 Wiki** — 索引 → 规划 → 逐模块文档（拓扑序）→ 顶层架构 → 组装写入仓库内 `.bytewiki/`，全程本地
- **增量更新** — 变更检测只重生成受影响模块及其下游，未变文档不动；文档滞后有徽标提示
- **知识卡 `cards/`** — 人工沉淀的 Markdown 卡片，生成永不覆盖，可被问答检索

### 函数级调用链

- **多语言提取** — tree-sitter 解析 Python / JavaScript / TypeScript / Go / Java / Rust，产出函数级调用图
- **SQLite 主存储** — `callgraph.db` 毫秒级查询（概览 / 子图 / 搜索 / 走读链），同时双写 `callgraph.json` 供互操作
- **增量生成** — 内容指纹判「真的变了」，未变文件免重新解析；新符号出现后原先连不上的调用点自动复活
- **链路走读** — 从任意函数 DFS 先序展开整条执行流；链断处给出边界报告（断在哪一行、候选目标是谁）
- **事件合成边** — `emit('x')` ↔ `on('x')` 按字面量配对补全跨函数事件流，边带 `event` 标注（非精确跳数可见，绝不冒充精确）
- **LSP 精确化** — 调 rust-analyzer `textDocument/definition` 把启发式边升级为精确边，连错目标的边会被改写
- **规则按项目** — 文件排除 / 入口排除 / 自定义入口正则，每个仓库独立配置

### 可视化

- **调用链画布** — 三种布局一键切换（有向分层 / 架构分层 / 同心环），视觉深度过滤（N 跳外淡出）、镜头飞行、爆炸半径脉冲
- **项目图谱页** — 全幅沉浸式展示：分波渐现入场演出、hover 聚光灯（邻域提亮其余压暗）、点击悬浮卡片

### 分支体系

- Wiki / 调用链 / 问答按 **git 分支隔离产物**，右上角全局切换
- **滞后检测**（生成时 HEAD 与当前不一致即提示）与**同 commit 免重算**（兄弟分支直接复制产物）
- 跨分支查看读 **git 快照**，行号与该分支文档一致

### RAG 问答

- 基于 Wiki 文档 + 代码分块的向量检索，流式输出
- 回答**带出处**：点击抽屉弹出对应源码片段；多会话管理（重命名 / 删除）

### 导出与共享

- **HTML 静态站导出** — VitePress 形态，Mermaid 图与代码高亮可用，浏览器打印即得 PDF
- `.bytewiki/` 是纯 Markdown，提交即团队共享；克隆后打开仓库直接加载，**零 LLM 成本**

### 多端形态

- **Tauri 2 桌面端**（Vue 3）：完整体验
- **`bw` CLI**：生成 / 查询 / 备份恢复 / 导出，适合脚本与流水线
- **MCP 服务**：把仓库知识暴露给 AI agent
- **VSCode 扩展**：编辑器内浏览 Wiki 与调用链

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
3. 首页打开一个本地 Git 仓库 → 「一键生成 Wiki」→ 浏览 / 走读 / 问答

### CLI

```bash
cargo run -p byte-wiki-cli -- --help            # 二进制名 bw

bw callchain <repo>              # 生成调用链（增量：未变文件免解析）
bw callchain <repo> --refine     # LSP 精确化（需本机 rust-analyzer）
bw callchain-query <repo> --overview            # 概览 / 入口清单
bw callchain-query <repo> --chain <节点ID>      # 走读链（含断点报告）
bw export <repo> --out <dir>     # Wiki 导出静态 HTML 站
```

另有 `backup` / `restore`（数据目录备份恢复）、`repo`、`server`（MCP / REST）子命令。

## .bytewiki/ 产物结构

```
.bytewiki/
  index.json            # 版本、模型、生成时间、文件清单
  outline.json          # 文档树大纲
  pages/…               # 模块文档（机器生成区，重新生成会被覆盖）
  cards/                # 人工知识卡（受保护，生成永不覆盖）
  graphs/callchain/     # callgraph.json + callgraph.db
  branches/<分支>/      # 非当前分支的产物（按分支隔离）
```

## 配置说明

| 配置 | 位置 | 说明 |
|---|---|---|
| 模型 | 设置 → 模型 | 默认本地 Ollama；可切 OpenAI 兼容远程端点，支持连接测试 |
| 调用链规则 | 调用链页 ⚙ | 按仓库持久化：文件排除 / 入口排除 glob、自定义入口正则 |
| 排除目录 | 设置 | 索引与调用链生成共用的目录排除表 |

## 架构

```
crates/core        # 业务核心：索引、Wiki 生成流水线、LLM 客户端、RAG、
                   # 分支体系、调用链存储与查询、LSP 精确化、HTML 导出
crates/callchain   # tree-sitter 多语言调用图提取引擎（纯解析，无业务依赖）
apps/cli           # bw CLI
apps/desktop       # Tauri 2 + Vue 3 桌面端
apps/vscode        # VSCode 扩展
```

CLI、桌面端、VSCode 扩展共享同一个 `crates/core`，行为天然一致。

## 开发

```bash
cargo test -p byte-wiki-core    # 核心单测（含 mock LLM）
cargo test -p byte-wiki-callchain
cd apps/desktop && npm run typecheck && npm run build
```

## 许可证

[MIT](LICENSE) © 2026 cicbyte
