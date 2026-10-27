# 快速开始

## 前置条件

- **Rust（MSVC 工具链）** — 构建桌面端与 CLI
- **Node.js 18+** — 前端
- **[Ollama](https://ollama.com)**（可选，推荐）— 本地模型，代码不出本机

```bash
# 拉取对话与嵌入模型（示例）
ollama pull qwen3.5:9b
ollama pull nomic-embed-text
```

不装 Ollama 也可以：设置 → 模型里切换到任意 OpenAI 兼容远程端点。

## 启动桌面端

```bash
cd apps/desktop
npm install
npm run tauri dev    # 开发
npm run tauri build  # 打包
```

## 三步上手

1. **配置模型**：打开应用 → 设置 → 模型，确认 provider 为 Ollama，填模型名，点「测试连接」
2. **打开仓库并生成**：首页打开一个本地 Git 仓库 → 「一键生成 Wiki」。生成产物写入仓库内 `.bytewiki/` 目录
3. **浏览与追问**：左侧文档树读架构与模块文档；「调用链」看函数级执行流；「对话」直接向知识库提问（回答带出处，点击弹出对应源码）

## CLI

```bash
cargo run -p byte-wiki-cli -- --help            # 二进制名 bw

bw callchain <repo>              # 生成调用链（增量：未变文件免解析）
bw callchain <repo> --refine     # LSP 精确化（需本机 rust-analyzer）
bw callchain-query <repo> --overview            # 概览 / 入口清单
bw callchain-query <repo> --chain <节点ID>      # 走读链（含断点报告）
bw export <repo> --out <dir>     # Wiki 导出静态 HTML 站
```

另有 `backup` / `restore`（数据目录备份恢复）、`repo`、`mcp serve`（MCP 服务）、REST 服务等子命令。

## 产物与共享

`.bytewiki/` 是纯 Markdown + Mermaid：

```
.bytewiki/
  index.json            # 版本、模型、生成时间、文件清单
  outline.json          # 文档树大纲
  pages/…               # 模块文档（机器生成区，重新生成会被覆盖）
  cards/                # 人工知识卡（受保护，生成永不覆盖）
  graphs/callchain/     # callgraph.json + callgraph.db
  branches/<分支>/      # 非当前分支的产物（按分支隔离）
```

提交 `.bytewiki/` 即团队共享；克隆后打开仓库直接加载，**零 LLM 成本**。
