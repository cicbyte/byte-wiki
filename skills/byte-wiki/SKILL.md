---
name: byte-wiki
description: 通过 bw CLI 读写仓库级知识（Wiki/知识卡/调用链）与 git 差异，供 AI 高效了解项目、跟进变更、沉淀与修订知识。当用户要"了解这个项目"、"看下最近改了什么"、"更新/修订 wiki"、"记个笔记/知识卡"，或提到 byte-wiki、bw 命令、项目知识库时使用。
---

# byte-wiki（bw CLI 工作流）

本地优先的仓库级知识引擎：把 Git 仓库变成结构化 Wiki + 函数级调用链 + RAG 问答，产物在仓库内 `.bytewiki/` 随 git 共享。本 skill 用 `bw` 命令驱动全部能力（无需 MCP 配置）。

**前置**：`bw` 在 PATH 上（byte-wiki 仓库 `cargo install --path apps/cli`，或用绝对路径）。仓库需已登记（`bw open <repo>`，桌面端打开过即已登记）。所有命令支持 `--json`。

## 场景速查

### 了解项目（新接手/换仓库）
```bash
bw repos                                    # 已登记仓库清单（拿 repo 绝对路径）
bw wiki <repo> outline                      # 文档清单：[类型] 路径 — 标题
bw wiki <repo> read overview.md             # 读总览，再按 outline 深入模块页
bw callchain-query <repo> --overview        # 函数/边/入口统计 + 入口清单
bw callchain-query <repo> --chain <节点ID>  # 执行流走读（含断点边界报告）
```

### 跟进变更（"最近改了什么"）
```bash
bw git <repo> status                        # 工作区状态
bw git <repo> diff --base HEAD~5            # 最近 5 个提交的差异
bw git <repo> diff --path src/core/foo.rs   # 单文件（缺省 base=工作区 vs HEAD）
bw git <repo> log -n 10                     # 提交历史
bw git <repo> show <sha>                    # 某次提交的 stat+补丁
```
差异 → 判断受影响模块：`bw callchain-query <repo> --focus <节点> --depth 2` 看影响面。

### 沉淀知识（结论/决策/备忘）
```bash
bw wiki <repo> card --title "XX 设计决策" --file note.md   # 写入 .bytewiki/cards/，生成永不覆盖、可被问答检索
```

### 修订 Wiki（基于差异的定向编辑）
```bash
bw wiki <repo> read pages/系统架构/核心库.md > /tmp/old.md   # 先读原文
# 写 replacements JSON：[{"search":"原文精确片段","replace":"修订后"}]
bw wiki <repo> edit --doc pages/系统架构/核心库.md --replacements /tmp/reps.json
```
**编辑语义（重要）**：search/replace 块**全有或全无**——每块 search 必须在文中恰好命中一次；任一块失败则整体不写入（修好 search 重试即可）。先 read 拿原文片段再构造 search，别凭记忆写。`cards/` 前缀同样可编辑。

## 注意

- 编辑/建卡后 RAG 检索需在桌面端「重建向量索引」刷新（编辑本身不向量化）。
- 生成产物按 git 分支隔离（`.bytewiki/branches/<分支>/`）；`cards/` 跨分支共享。
- 调用链增量生成：`bw callchain <repo>`（内容指纹判真变更）；`--refine` 走 rust-analyzer 精确化。
- MCP 等价面：`bw mcp serve`（stdio）暴露同样的 15 个工具（含 git 4 只读 + 写入 2）。
