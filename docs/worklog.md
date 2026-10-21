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
