# 工作日志

## 2026-09-21 调用链存储 SQLite 化（#573）

设计文档：bcode 文档中枢 `docs/design/callchain-sqlite.md`。

**变更内容**
- 新增 `crates/core/src/callchain_db.rs`：`callgraph.db`（SQLite/WAL，nodes 与 edges 均 WITHOUT ROWID，edges 复合主键 (src,dst) + idx_edges_dst）读写层；db/json 双后端统一加载为类型化 `GraphData`（边归一化取最早调用行、重复节点 id 保首个——ZCode 的 armsRumShared.js/.ts 迁移并存双胞胎实测命中）。
- `crates/core/src/callchain.rs`：生成时双写 db（查询主存储）+ json（互操作导出：前端中小图整包、克隆共享、外部工具仍读它）；读取优先 db、缺失回落 json；`AppService` 挂进程内图缓存（(路径, mtime, len) 失效，LRU ≤3 仓），概览/子图/搜索/走读链四查询改走同一类型化内存索引 + 概览 OnceLock 派生视图；`read_callgraph` 还原 json 文档形状（CLI/测试消费方无感）。
- `crates/core/src/service/mod.rs`：AppService 增加 `callgraph_cache` 字段。
- `crates/core/src/generator/mod.rs`：`scan_wiki_dir` 跳过 `callgraph.db*`（db 与 WAL sidecar 不进文档树，json 仍是登记口径）。

**验收结果**
- core 91 测试全绿（新增 db 往返/重写覆盖/重复 id 去重/WAL 截断/db-json 四查询一致性/缓存失效 8 项）。
- 矩阵基线精确一致：ripgrep 2949/4227/1、agrona 3837/2903、click 1763/1789/0。
- ZCode（26248 节点/59967 边）：db 39.8MB 落盘；查询计时（release、min-of-5）overview 缓存命中 0ms、search 5ms、focus 40ms、chain 22ms；首次加载（全表扫 + 概览计算）146-189ms，为每仓每会话一次，替代原先每次查询 300-500ms 的重复 json 解析。

## 2026-09-21 走读页崩溃修复：服务端链节点漏发 routes（#577）

**变更内容**
- `crates/core/src/callchain.rs`：`callgraph_chain` 轻量节点补发 `routes`/`async`（#571 起漏发 routes，走读页标题徽标/存卡标题/全链导出均消费，链一加载即 `routes.length` 崩溃）；一致性测试补链节点 routes 断言。
- `apps/desktop/src/views/wiki/Walkthrough.vue`：六处 `node.routes` 访问加可选链防御。

**验收结果**：core 调用链测试 8 项全绿；前端 vue-tsc 无错误。

## 2026-09-21 Wiki 章节来源跳转不定位修复（#579）

**变更内容**
- `apps/desktop/src/views/wiki/components/FileBrowse.vue`：引用定位抽出 `handleReveal`（消费后清除 store 目标，避免重进文件页重复定位）；除 seq watch 外，`onMounted` 兜底消费一次——根因是文件页 `v-else-if` 按需挂载，真实时序为先 `revealFile` 设 store、路由切换后组件才挂载，无 `immediate` 的 watch 拿不到挂载前已设置的值。
- `apps/desktop/src/stores/wiki.ts`：新增 `clearReveal()`。

**验收结果**：vue-tsc 无错误（HMR 生效，点击章节来源即展开祖先目录 + 打开文件 + 滚动高亮行区间）。

二层修复（同日，行定位竞态）：路径展开与文件打开已生效，但行滚动/高亮仍丢失——`applyReveal` 只等「编辑器实例存在」，而 content 设置后 prop 传播与 SnippetEditor 内部 `setValue` 均为异步，reveal 对空模型执行（滚不动、装饰被随后的 setValue 清掉），定位只消费一次即永久丢失。改为轮询「模型内容 === 目标文件内容」（100ms × 40 上限，期间切换文件则放弃）。

三层调整（同日，交互方案改为抽屉）：验证反馈路径展开仍不生效，改为「源码抽屉就地查看」——引用链接不再跳文件页。新建 `apps/desktop/src/views/wiki/components/SourceDrawer.vue`（与 CallGraphPanel「打开函数/打开整个文件」同款交互：右侧 62% 抽屉、Monaco 只读、双 nextTick 后滚动居中 + 行区间高亮）；`RepoSpace.vue` 文档页与知识卡页的 `@open-file` 均接抽屉，抽屉内「在文件页打开（展开路径）」按钮保留原 reveal 链路作为显式动作。

滚动定位根因修正（同日）：靠下行目标滚不到（两个抽屉同病）——reveal 执行时编辑器仍处 `v-show="!loading"` 隐藏态，Monaco 对 display:none 容器是 0×0 布局，`revealLineInCenter` 被夹成空操作，`automaticLayout` 的重排发生在 reveal 之后不补发滚动。两处统一改为：内容就绪先解除隐藏 → 双 nextTick → `ed.layout()` 强制重测 → reveal + 装饰。

## 2026-09-21 切换 Wiki 文档先显示再抖动修复（#580）

**变更内容**（`apps/desktop/src/views/wiki/components/WikiViewer.vue`）
- mermaid 异步渲染占位：`.md-editor-mermaid:not(:has(svg))` 隐藏渲染前的图源码文本闪现 + `min-height: 120px` 预留高度——此前源码先当正文渲染、几十到几百毫秒后换 SVG 高度突变，把下方内容顶一下（每页强制 ≥2 张图，故每次切文档必现）。
- 切文档滚动复位：`load()` 内容就绪后 `scrollTo` 顶部——此前旧文档滚动位置被带进新文档。

**验收结果**：vue-tsc 无错误（HMR 生效）。

## 2026-09-21 VSCode 扩展：查看 Wiki 与调用链（#581）

在既有 snippets 扩展（apps/vscode，byte-stash 移植、esbuild+TS）上版本归零为 0.0.1，不新建平行项目。

**变更内容**
- `apps/cli/src/cmd/callchain.rs`：`callchain-query` 补 `--overview` / `--focus <seed>`（--depth/--cap）/ `--chain <id>` 三个查询子命令（AppService 既有快路径）。
- `crates/core/src/callchain.rs`：概览入口轻量 json 补 `lineno/end_lineno`（外部壳点入口直接跳函数体）。
- `apps/vscode/`：活动栏 Byte Wiki 容器 + 两个视图——Wiki 页面树（`wikiTree.ts` 直读 `.bytewiki/branches/<分支>`）、调用链入口树（`callchainTree.ts`，`bs callchain-query --overview`）；页面 webview 预览（`wikiPreview.ts` + `media/render.js`：markdown-it UMD + mermaid 本地 vendor 离线渲染，`bw-file://` 引用 → 编辑器选中队列行区间，相对 .md 链接切文档）；走读链大纲（`chainView.ts` + `media/chain.js`：主路径标 ★、块/调用点击跳转）；生成调用链 / 节点搜索命令；`sourceLocator.ts` 编辑器定位 + 装饰高亮。既有片段/模板命令全部保留；复用 `cliClient.runBs` 与 `byteWiki.cliPath` 配置。

**验收结果**：core 调用链测试 8 项绿；扩展 `tsc --noEmit` + esbuild 构建通过；扩展消费的三类 JSON（overview/chain/grep）经 `bs --json` 实测形状对齐（zoxide）；ZCode overview 经 CLI 0.4s（含进程启动）。待用户 VSCode F5 实测交互。
