# Byte-Wiki for VSCode

在 VSCode 中查看 byte-wiki 生成的**仓库 Wiki**（markdown + mermaid，引用一键跳源码行）与**函数级调用链**（入口清单、走读链、点击跳函数体）；以及插入 [byte-wiki](../../) 的**代码片段**与**代码模板**（Tera 变量预渲染）。

Wiki/调用链视图直读仓库 `.bytewiki/` 产物 + `bs callchain-query`（db 主存储毫秒级查询），无需打开桌面端；片段/模板支持 **CLI / HTTP 两种数据通道**，默认自动选择。

## 快速开始（F5 开发调试）

1. `cd apps/vscode && npm install`
2. 用 VSCode 打开 `apps/vscode`，按 **F5**（launch.json 已配好）→ 弹出扩展开发宿主窗口
3. 在开发宿主里打开一个已生成过 Wiki/调用链的仓库（如测试矩阵仓），活动栏出现 **Byte Wiki** 容器：
   - **Wiki 页面**：点击文档 → webview 渲染（mermaid 本地 vendor 离线可用）；正文里的 `路径:行` 引用链接 → 直接在编辑器中选中并高亮对应行区间；相对 .md 链接 → 切换文档
   - **调用链入口**：入口清单（零入口库显示公共 API 面）→ 点击跳函数体（编辑器选中队列）；行内 **⺆** 按钮打开**走读链**（DFS 先序大纲，主路径标 ★，块与调用均可点击跳转）；标题栏 ▶ 可直接生成（走 `bs open` + `bs callchain`），🔍 搜索节点

> `bs` 不在 PATH 时，在设置里配 `byteWiki.cliPath` 指向绝对路径（如仓库根 `target/release/bs.exe`）。

## 安装

### 1. 获取 `.vsix`

**方式 A · 从源码构建**（需 Node ≥ 18）

```bash
cd apps/vscode
npm install
npm run build    # esbuild 打包到 dist/extension.js
npm run package  # 产出 byte-wiki-vscode-<版本>.vsix
```

**方式 B · 直接用现成包**

已有 `byte-wiki-vscode-<版本>.vsix`（他人分发 / CI 产物）则跳过上一步。

### 2. 安装到 VSCode

**方式 A · 图形界面**

1. 打开扩展面板（`Ctrl+Shift+X`）
2. 右上角 `…` 菜单 → **从 VSIX 安装…（Install from VSIX…）**
3. 选中 `byte-wiki-vscode-0.0.1.vsix`
4. 安装完成，按提示重新加载窗口

**方式 B · 命令行**

```bash
code --install-extension byte-wiki-vscode-0.0.1.vsix
```

> VSCode Insiders / Cursor / Windsurf 等，把 `code` 换成对应命令（`code-insiders` / `cursor` / `windsurf`）。

### 3. 验证

命令面板（`Ctrl+Shift+P`）输入 `Byte-Wiki`，能看到「插入片段或模板」等命令即安装成功。


## 数据通道

| | CLI（推荐） | HTTP |
|---|---|---|
| 触发条件 | `bs` 在 PATH（或配 `cliPath`） | 桌面端开启 REST + 填 `apiKey` |
| 依赖桌面端 | ❌ 不需要 | ✅ 需要 |
| 数据来源 | `bs` 进程直接读本地库 | REST 请求 |
| 远程访问 | ❌ 仅本地 | ✅ |

配置 `byteWiki.mode`：
- **`auto`**（默认）：检测到 `bs` 就用 CLI，否则 HTTP
- **`cli`**：强制 CLI
- **`http`**：强制 HTTP

## 配置

| 设置 | 说明 |
|---|---|
| `byteWiki.mode` | `auto` / `cli` / `http`（默认 `auto`） |
| `byteWiki.cliPath` | `bs` 命令名或路径（默认 `bs`） |
| `byteWiki.baseUrl` | HTTP 模式：REST API 地址 |
| `byteWiki.apiKey` | HTTP 模式：API Key |
| `byteWiki.enableTriggers` | 是否启用编辑器 trigger 补全（默认 `true`） |
| `byteWiki.matchLanguage` | 补全是否按语言过滤（默认 `true`；关闭则纯名称匹配，适合 md/笔记） |

## 使用

### 1. prefix 智能补全（推荐）
编辑器输入模板的 **trigger**（或片段/模板标题词）→ 智能提示 → 选中 → 若模板含变量则逐个填写 → 插入。

### 2. 命令面板
`Ctrl+Shift+P` → `Byte-Wiki: 插入片段或模板` → 搜索 → 选中。

### 3. 右键菜单
编辑器右键 → `Byte-Wiki ▶` → 插入片段 / 插入模板。

## 捕获与导入

把代码沉淀进 byte-wiki，无需切到桌面端。

- **编辑器选中 → 存片段/模板**：选中代码 → 右键 `Byte-Wiki ▶` → 存选中为片段/模板 → 填标题/语言/标签/分类（模板可设触发词）。
- **文件 → 存片段/模板**：资源管理器右键文件 → 存文件为片段/模板（整文件入库）。
- **文件夹 → 导入为项目**：右键文件夹（或编辑器右键「导入为项目」用当前工作区）→ 填标题 → 进度条导入（走 `bs add`，默认排除 node_modules 等）。

> 新建的模板/片段立即可被补全与 QuickPick 看到（自动刷新缓存）。

## 模板变量语法

模板正文用 **Tera**（Jinja2 子集）：

- `{{ name }}` —— 插入时弹框填值（MVP 仅标量）
- `{{ name | upper }}` —— 过滤器（后端渲染，全支持）
- 无变量模板直接插入

不支持数组 / 循环（`{% for %}`）—— 与桌面端 MVP 范围一致。

## 已知限制

- CLI 模式每次操作 spawn 进程（冷启动开销），列表有 3s 缓存缓解
- HTTP 模式即便本地访问也需填 `apiKey`（后端 `allow_local_no_key` 尚未生效）
- 列表上限 200 条
- language 过滤：模板 `language` 为空 = 通用；否则按当前文档语言匹配（含 ts/js/rs/py 等别名）
