# 工作日志

## 2026-09-14　callchain P1：函数级调用链落地（引擎 + core 集成 + CLI + 桌面端）

需求与决策见 `dev-docs/callchain-需求与交接.md`（v2：产物与渲染分离、core 无条件依赖引擎、app 产物不内嵌 source）。

### 变更内容

**引擎 `crates/callchain`（重写 lib.rs，修复 main.rs）**
- 三处正确性修正：python async 函数识别（0.23 语法树中 async 是无名关键字子节点）、方法调用 recv 分叉（仅 heuristic，不再虚标 local）、同文件重复 qualname later-wins 去重
- 类/容器名入栈：python class、js/ts class、java class/interface/enum/record、go 接收者前缀、rust impl/trait —— qualname 达成文档承诺的 `Class.method`
- `ExtractOptions`：excludes 注入（内置表补 `.bytewiki`，重复运行不自污染）、embed_source 开关（app 管线 false）、进度回调进 extract 签名
- walkdir → `ignore`（遵循 .gitignore）、路径排序保证产物确定性、1MB 大文件跳过、TSX 独立 grammar、扩展名与 core `lang_of` 对齐（pyi/mts/cts）
- 单测 10 项：五语言小样本 + 三处修正各有针对性用例 + excludes/embed_source/entry 正则

**core `crates/core`（无条件依赖 byte-wiki-callchain）**
- 新增 `callchain.rs`：`AppService::generate_callchain`（仓库校验、排除口径与索引一致、产物落 `.bytewiki/graphs/callchain/callgraph.json` 无 source、wiki_docs 登记 kind=graph）+ `read_callgraph`
- `db/index_store.rs`：`upsert_generated_doc`（生成区单条 upsert）
- `generator/mod.rs`：assemble_wiki 整体替换时保留 `graphs/callchain/` 并补登（产物不随 wiki 重生成丢失）；scan_existing_wiki 识别调用链产物（克隆加载）

**CLI `apps/cli`**：`bs callchain <repo>` 子命令（--json 支持）

**桌面端 `apps/desktop`**
- Rust：`commands/callchain.rs`（generate_callchain 走 spawn_blocking + `wiki://callchain` 进度事件；read_repo_file 带路径穿越防护与 4MB 上限）
- 前端：项目空间第五 tab「调用链」（router `/repo/:id/callgraph` + repoMenu 平铺五项 + RepoSpace pane）；`CallGraphPanel.vue`（vis-network npm 打包零 CDN、入口面板执行流聚焦、坐标固化防大图重排崩溃、搜索 250ms 防抖、详情双向调用跳转、Monaco 定位打开源码）；`api/callchain.ts`
- 依赖：+ vis-network 10

**存量缺陷修复（实施中发现）**
- `.gitignore` 的 `lib/` 模式（Python venv 遗留）导致前端 6 个 lib 模块从未入库、任何 clone 无法构建——锚定为 `/lib/`，并从使用点重建：layout 菜单常量与 useMenuExpansion、api/mcp 服务常量与 useApiServer/useMcpServer、settings 默认排除目录

**文档**：README（功能列表 + CLI 用法）、根 Cargo.toml profile 注释（<30MB 目标放宽口径）、dev-docs v2 实施状态

### 验收结果

- `cargo test -p byte-wiki-callchain` 10/10；`cargo test -p byte-wiki-core` 43 通过；workspace 构建（含 desktop）零 error
- 五仓库 E2E（ignored 测试 + standalone 直跑）：bid-eval 340 函数/543 边/51 入口（backend 对照 Python 版基线 337/592/82——函数对齐；边偏低属预期，缺 import 精确边层；路由入口与实测 53 个装饰器归并吻合）；byte-code 1143/1745；byte-code-cli 147/327；review-task 14/2
- 前端 vue-tsc + vite build 通过；桌面端 GUI 交互（大图筛选/聚焦、Monaco 定位手感）待人工走查

### 追加（同日）

- 按用户决策移除 `crates/callchain/reference/`（Python 参考实现不随仓库走）：内容先存档至 bcode 平台 docs（`callchain/reference-extract-callchain.md`，含与 Rust 版差异说明），验证可读后删除；dev-docs 附录 C 资料索引同步更新。`assets/` 保留（standalone HTML 导出的编译期内嵌资源）。
