---
name: byte-wiki-cli
description: 通过 `bs` 命令行向 byte-wiki 本地知识库存储与查阅内容。当用户想要导入代码项目、保存代码片段、Markdown 笔记或可复用代码模板，或搜索、读取、浏览已存内容时使用——提到 "byte-wiki"、"bs 命令"、"把这个项目/代码/笔记存一下"、"找我存的..." 时触发。所有命令支持 `--json` 输出，适合脚本与 AI 工作流程序化读写。
---

# byte-wiki CLI（`bs`）

`bs` 是 byte-wiki 的命令行工具——纯本地、单人、零部署的知识库。核心就两件事：**存**（导入项目、记录片段/笔记/模板）和**查**（搜索、读取、浏览）。所有命令支持 `--json`，天然适合脚本与 AI 工作流。

## 何时使用

- 用户说"把这个存一下"、"导入这个项目"、"存个片段/笔记/模板"
- 用户想搜索或读取自己库存里的代码/笔记
- 你需要程序化读取某个已存项目的文件树或文件内容
- 用户要整理分类、标签
- 自动化脚本需要结构化读写知识库

## 找到 bs 二进制

CLI 可执行文件名为 `bs`。如果不在 PATH：

- `~/.cargo/bin/bs`（`cargo install` 安装后）
- `<仓库>/target/debug/bs` 或 `target/release/bs`（源码编译）
- 或在 byte-wiki 仓库内 `cargo build --workspace` 后用 `target/debug/bs`

找不到就问用户怎么调用 `bs`，别假设不存在。

## `--json` 标志（务必先读）

每个命令都支持 `--json`，输出机器可读 JSON，放在子命令前：

```bash
bs --json <命令> [选项]
```

JSON 模式的行为：
- 输出合法 JSON（add/get 返回对象，list/search 返回数组，美化缩进）
- **跳过 remove 的交互确认**——直接执行，返回 `{"removed":"<id>"}`
- 错误走 stderr + 非零退出码，stdout 始终是干净的 JSON

配合 `jq` 提取字段：
```bash
# 存笔记并捕获 shortId
cat design.md | bs --json note add --title "设计文档" | jq -r .short_id
# 列项目只取标题
bs --json list | jq '.[].title'
```

## 存储（写入知识库）★ 第一重点

> 内容创建类命令的正文/代码来源统一为：显式参数 > `--file` > **stdin**（支持管道）。创建后用 `--json` 捕获返回的 `short_id`（8 字符，后续 get/search/remove 的主键）。

### 项目（导入代码目录）

```bash
bs add <路径> [--title 标题] [--description 描述] [--tag 标签]... [--lang 语言]... [--category 分类] [--exclude 目录名]...
```

自动遵循源目录 `.gitignore`（含嵌套，无需 `.git` 存在）；设置页「偏好」的排除目录列表（默认 node_modules/.git/target/dist 等）也合并生效；`--exclude` 临时覆盖。

### 片段（单条代码）

```bash
bs snippet add --title 标题 [--code "代码" | --file f.rs | stdin] [--lang rust] [--description 描述] [--tag 标签]... [--category 分类]
```

代码来源：`--code` > `--file` > stdin。管道示例：
```bash
curl -s https://example.com/api.go | bs snippet add --title "API 处理器" --lang go
```

### 笔记（Markdown）

```bash
bs note add --title 标题 [--content "正文" | --file f.md | stdin] [--description 描述] [--tag 标签]... [--category 分类]
```

正文来源：`--content` > `--file` > stdin。

### 模板（带变量的代码骨架）

模板正文用 `${name}` / `${name:默认}` 占位符，展开时替换为实际值。

```bash
bs template add --title 标题 [--body "正文" | --file f | stdin] [--lang 语言] [--trigger 触发词] [--description 描述] [--tag 标签]... [--category 分类]
```

## 查阅（读取知识库）★ 第二重点

### 列出

```bash
bs list [--tag 标签] [--lang 语言] [--favorited] [--limit N]    # 项目
bs snippet list [--lang 语言] [--tag 标签] [--limit N]
bs note list [--tag 标签] [--category 分类] [--limit N]
bs template list [--lang 语言] [--trigger 触发词] [--tag 标签] [--limit N]
bs category list                                                 # 分类（含各类内容计数）
```

### 读取详情 / 文件树 / 文件内容

```bash
bs get <shortId>                    # 项目元数据
bs get <shortId> --tree             # 项目文件树（不读内容）
bs get <shortId> --file <相对路径>   # 项目内单文件内容
bs snippet get <shortId>
bs note get <shortId>
bs template get <shortId>
```

### 搜索（支持中文模糊匹配）

```bash
bs search "<关键词>" [--limit N] [--offset M]          # 项目
bs snippet search "<关键词>" [--limit N] [--offset M]   # 范围：代码+标题+标签+描述
bs note search "<关键词>" [--limit N] [--offset M]      # 范围：正文+标题+标签
bs template search "<关键词>" [--limit N] [--offset M]
```

### 统计

```bash
bs stats    # 项目/片段/笔记/标签/语言/分类数量 + 存储占用
```

## 修改与删除

### 局部修改（patch，行号范围替换，省 token）

```bash
bs patch <shortId> --file <相对路径> --start 10 --end 20 --new "新内容"   # 项目内文件
bs snippet patch <shortId> --start 2 --end 4 --new "新内容"               # 片段代码
bs note patch <shortId> --start 1 --end 3 --new "新内容"                  # 笔记正文
```

行号 1-based，含两端。**先用 get 读取内容确认行号，再 patch**；越界或 start>end 报错。比整条重传省 90%+ token。`--new` 省略时从 stdin 读（适合多行管道）。

### 删除（软删除→回收站，可恢复）

```bash
bs remove <shortId> [-y]
bs snippet remove <shortId> [-y]
bs note remove <shortId> [-y]
bs template remove <shortId> [-y]
```

## 分类管理

```bash
bs category create <名称>                       # → {"id":N,"name":"..."}
bs category rename --id <id> --name <新名称>
bs category delete --id <id> [-y]               # 内容保留，仅解除分类绑定
```

## 备份与恢复

```bash
bs backup [--output <路径>] [--no-compress]     # 默认 zip（DB+全部内容）；省略输出走 data_root/backups/
bs restore <备份文件或目录> [-y]                  # 覆盖当前数据
```

备份前自动 WAL checkpoint 确保一致性。恢复后桌面端在运行需重启。

## 数据位置

默认 `~/.cicbyte/apps/byte-wiki/`。`BYTE_WIKI_HOME` 覆盖 app 根目录（测试/隔离用）：

```bash
BYTE_WIKI_HOME=/tmp/my-lib bs stats
```

```
~/.cicbyte/apps/byte-wiki/
├── config/config.toml     # data_dir/偏好/备份设置（固定在 app 根）
└── data/                  # 可在设置页移到任意路径（如外置硬盘、网盘）
    ├── db/bytewiki.db    # SQLite（元数据/标签/分类/FTS 索引）
    ├── projects/{shortId}/
    ├── snippets/{前2字符}/{第3-4字符}/{shortId}.{ext}   # 前缀分桶
    └── notes/{前2字符}/{第3-4字符}/{shortId}.md
```

config/ 固定 app 根；data/ 可移动。片段/笔记用前缀分桶（`{前2}/{3-4}/`）避免单目录万级文件平铺。

## AI 代理注意事项

- **要解析输出就用 `--json`**；人类可读模式含 ANSI 颜色/emoji，难解析。
- `short_id` 是 8 字符字符串（如 `abc12345`），get/search/remove 的主键。创建后从输出捕获。
- `remove` 是**软删除**（进回收站，桌面端可恢复）。别警告用户"不可恢复"——它是可恢复的。
- `category delete` **不删内容**，只移除分类绑定，项目/片段/笔记仍存在。
- 标签是自由文本，可多个（`--tag a --tag b`），无独立标签管理命令，靠内容隐式存在。
- **改内容用 patch（行号范围），别整条重写。** 先 get 确认行号，再 patch，省 90%+ token。
- **搜索支持中文**：FTS5 之外有 LIKE 模糊兜底，中文关键词（如"数据库设计"）能命中标题/描述/标签。
- **图表（diagram）CLI 无子命令**，仅在桌面端管理。

## 实测踩过的坑

- **`bs add .` 自动遵循 `.gitignore`**（node_modules/target/dist 等自动跳过），也可用 `--exclude` 临时覆盖或在设置页管理排除列表。
- **`bs list` 没有 `--pinned`**（置顶功能已移除），筛收藏用 `--favorited`。
- **选项名是 `--category` / `--description`**，不是 `--cat` / `--desc`。
- **`snippet search` 搜** 代码+标题+标签+描述；**`note search` 搜** 正文+标题+标签。按关键词可能出现的位置选对命令。
- **搜索结果用 `--offset` 翻页**，默认 20 条，`--offset 20` 取第二页，`--offset 40` 取第三页。
