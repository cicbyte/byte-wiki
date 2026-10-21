<script setup lang="ts">
/**
 * 链路走读页 /repo/:id/walkthrough?node=<id>
 *
 * 把 bid-eval 时代手工维护的「调用链走读地图」产品化：选一个入口或任意函数，
 * 按 DFS 先序（子节点按首次调用行排序）把整条可达链的代码顺序渲染到单页——
 * 大纲树（去重 + 引用折叠，主路径默认展开）+ 懒挂载 Monaco 代码块 +
 * 调用行高亮标注（点击跳目标块）。图是快照，代码读实时文件（与滞后徽标语义一致）。
 */
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { message } from "ant-design-vue";
import {
  ArrowLeftOutlined,
  BookOutlined,
  NodeExpandOutlined,
  NodeCollapseOutlined,
  DoubleLeftOutlined,
  DoubleRightOutlined,
  CopyOutlined,
  SaveOutlined,
} from "@ant-design/icons-vue";
import { api } from "@/api";
import { callchainApi } from "@/api/callchain";
import type { CallgraphData, CallGraphNode, CallgraphEdge } from "@/api/callchain";
import type { WalkCallLink } from "./WalkBlock.vue";
import WalkBlock from "./WalkBlock.vue";
import EmptyState from "@/components/common/EmptyState.vue";
import { useWikiStore } from "@/stores/wiki";

const route = useRoute();
const router = useRouter();
const wiki = useWikiStore();

const loading = ref(true);
const loadError = ref("");
const data = ref<CallgraphData | null>(null);
const rootNodeId = ref("");
const codeMap = ref<Map<string, { code: string; truncated: boolean }>>(new Map());

/** 走读链结构：先序渲染序 + 每块的外呼链接（含已渲染引用） */
interface WalkEntry {
  id: string;
  num: string;
  node: CallGraphNode;
  calls: WalkCallLink[];
}
const entries = ref<WalkEntry[]>([]);
/** 主路径节点集合（默认展开） */
const expandedIds = ref<Set<string>>(new Set());
const activeId = ref("");
/** 服务端 DFS 主路径（mainPathOnly 用；不再依赖整包 edges） */
const chainMainPath = ref<string[]>([]);

// ===== 链计算 =====
function buildChain(graph: CallgraphData, rootId: string) {
  const nodeById = new Map(graph.nodes.map((n) => [n.id, n]));
  const out = new Map<string, CallgraphEdge[]>();
  for (const e of graph.edges) {
    (out.get(e.src) ?? out.set(e.src, []).get(e.src)!).push(e);
  }
  const root = nodeById.get(rootId);
  if (!root) return null;

  // 子树深度（带环防护），用于主路径贪心
  const depthCache = new Map<string, number>();
  const depthOf = (id: string, seen: Set<string>): number => {
    if (depthCache.has(id)) return depthCache.get(id)!;
    if (seen.has(id)) return 0;
    seen.add(id);
    let d = 1;
    for (const e of out.get(id) ?? []) {
      if (nodeById.has(e.dst)) d = Math.max(d, 1 + depthOf(e.dst, seen));
    }
    seen.delete(id);
    depthCache.set(id, d);
    return d;
  };

  const num = new Map<string, string>();
  const order: WalkEntry[] = [];
  const mainPath = new Set<string>([rootId]);

  // DFS 先序：子节点按首次调用行号排序；已访问节点仅作引用不再渲染
  const visit = (id: string, prefix: string, seen: Set<string>, isMain: boolean) => {
    const node = nodeById.get(id)!;
    const myNum = prefix;
    num.set(id, myNum);
    seen.add(id);
    const calls: WalkCallLink[] = [];
    const children: { edge: CallgraphEdge; first: boolean }[] = [];
    // 同目标多边取首次调用行
    const byTarget = new Map<string, CallgraphEdge>();
    for (const e of (out.get(id) ?? []).slice().sort((a, b) => a.line - b.line)) {
      if (!nodeById.has(e.dst)) continue;
      if (!byTarget.has(e.dst)) {
        byTarget.set(e.dst, e);
        children.push({ edge: e, first: true });
      }
    }
    order.push({ id, num: myNum, node, calls });

    // 主路径延伸：贪心取子树最深的子节点
    let mainChild: string | null = null;
    let bestDepth = -1;
    for (const c of children) {
      const d = depthOf(c.edge.dst, new Set());
      if (d > bestDepth) {
        bestDepth = d;
        mainChild = c.edge.dst;
      }
    }

    for (const c of children) {
      const t = c.edge.dst;
      const isMainChild = isMain && t === mainChild;
      if (isMainChild) mainPath.add(t);
      if (seen.has(t)) {
        // 引用：目标已在前文渲染，仅生成跳转链接
        calls.push({
          targetId: t,
          targetNum: num.get(t) ?? "?",
          targetName: nodeById.get(t)!.qualname,
          line: c.edge.line,
          kind: c.edge.kind,
        });
      } else {
        const childNum = `${myNum}.${order.filter((o) => o.num.startsWith(myNum + ".")).length + 1}`;
        calls.push({
          targetId: t,
          targetNum: childNum,
          targetName: nodeById.get(t)!.qualname,
          line: c.edge.line,
          kind: c.edge.kind,
        });
        visit(t, childNum, seen, isMainChild);
      }
    }
    // 主路径节点默认展开
    if (isMain) expandedIds.value.add(id);
  };

  visit(rootId, "1", new Set(), true);
  return { order, mainPath };
}

// ===== 加载 =====
async function load() {
  loading.value = true;
  loadError.value = "";
  try {
    const info = await wiki.ensureRepo(Number(route.params.id));
    if (!info) {
      loadError.value = "仓库不可用";
      return;
    }
    const qn = String(route.query.node ?? "");
    if (!qn) {
      loadError.value = "未指定有效节点";
      return;
    }
    // 服务端 DFS 链（大图 27MB 不再整包过 IPC；语义与原前端 buildChain 一致）
    const chain = await callchainApi.chain(info.path, qn, 400);
    if (!chain || !chain.entries.length) {
      loadError.value = "节点不存在";
      return;
    }
    rootNodeId.value = qn;
    entries.value = chain.entries.map((e) => ({
      id: e.id,
      num: e.num,
      node: e.node,
      calls: e.calls.map((c) => ({
        targetId: c.target_id,
        targetNum: c.target_num,
        targetName: c.target_name,
        line: c.line,
        kind: c.is_ref ? "ref" : "heuristic",
      })),
    }));
    chainMainPath.value = chain.main_path;
    expandedIds.value = new Set(chain.main_path);
    if (chain.truncated) {
      message.warning(`链路过大，已截断到前 ${entries.value.length} 个函数（按先序）`);
    }
    if (entries.value.length > 200) {
      message.warning(`链路较大（${entries.value.length} 个函数），已渲染全部大纲，代码按需展开`);
    }
    activeId.value = qn;
    // 批量取源码（文件去重在服务端）
    const items = entries.value.map((e) => [
      e.id,
      e.node.file,
      e.node.lineno,
      e.node.end_lineno,
    ] as [string, string, number, number]);
    try {
      const sources = await api.getChainSources(info.path, items);
      const m = new Map(sources.map((s) => [s.id, { code: s.code, truncated: s.truncated }]));
      codeMap.value = m;
    } catch (e) {
      message.error(typeof e === "string" ? e : "源码加载失败");
    }
  } catch (e) {
    loadError.value = typeof e === "string" ? e : "加载失败（先生成调用链）";
  } finally {
    loading.value = false;
  }
}
onMounted(load);

// ===== 视图状态与交互 =====
const rootEntry = computed(() => entries.value[0] ?? null);
const stats = computed(() => {
  if (!entries.value.length) return null;
  const files = new Set(entries.value.map((e) => e.node.file));
  const lines = entries.value.reduce((s, e) => s + (e.node.end_lineno - e.node.lineno + 1), 0);
  return { fns: entries.value.length, files: files.size, lines };
});

/** 大纲折叠（收起为细轨，代码区吃满宽度） */
const outlineCollapsed = ref(localStorage.getItem("bw:wk-outline-collapsed") === "1");
function toggleOutline() {
  outlineCollapsed.value = !outlineCollapsed.value;
  localStorage.setItem("bw:wk-outline-collapsed", outlineCollapsed.value ? "1" : "0");
}

const blockRefs = new Map<string, HTMLElement>();
function setBlockRef(id: string, el: unknown) {
  if (el) {
    // WalkBlock 是组件：函数 ref 拿到的是组件实例，DOM 在 $el（根节点 <section>）
    const dom = (el as { $el?: HTMLElement }).$el ?? (el as HTMLElement);
    blockRefs.set(id, dom);
  } else blockRefs.delete(id);
}

function toggleBlock(id: string) {
  const next = new Set(expandedIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expandedIds.value = next;
}
function expandAll() {
  expandedIds.value = new Set(entries.value.map((e) => e.id));
}
function collapseAll() {
  expandedIds.value = new Set(rootNodeId.value ? [rootNodeId.value] : []);
}
function mainPathOnly() {
  // 服务端 DFS 已算好主路径（贪心最深子树），直接采用
  expandedIds.value = new Set(chainMainPath.value.length ? chainMainPath.value : [rootNodeId.value]);
}

function jumpTo(id: string) {
  if (!expandedIds.value.has(id)) toggleBlock(id);
  activeId.value = id;
  const scroll = () => blockRefs.get(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  requestAnimationFrame(scroll);
  // 点击折叠项时：展开后编辑器懒挂载、块高度异步增长，补一次迟到滚动校准落点
  setTimeout(scroll, 350);
}
function onVisible(id: string) {
  activeId.value = id;
}
// ===== 导出 / 复制（AI 友好的全链 Markdown） =====
const LANG_OF: Record<string, string> = {
  py: "python", pyi: "python",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  go: "go", java: "java", rs: "rust",
};

/** includeTitle：复制版 = true（独立文档需自带 H1）；存卡版 = false（createCard 自动写 `# title`，自带会双标题） */
function buildMarkdown(includeTitle: boolean): string {
  const root = rootEntry.value;
  if (!root || !stats.value) return "";
  const repo = wiki.currentRepo;
  const L: string[] = [];
  const routes = root.node.routes.length ? `（${root.node.routes.join(" / ")}）` : "";
  if (includeTitle) L.push(`# 链路走读：${root.node.qualname}${routes}`, "");
  L.push(
    `> 仓库：${repo?.name ?? "?"}${repo?.branch ? ` @ ${repo.branch}` : ""} · 起点：${root.node.file}:${root.node.lineno} · ` +
      `${stats.value.fns} 函数 / ${stats.value.files} 文件 · 由 byte-wiki 调用链生成（图为快照，代码为导出时实时文件）`,
    "",
  );
  L.push("## 执行流大纲", "");
  for (const e of entries.value) {
    const indent = "  ".repeat(e.num.split(".").length - 1);
    const r = e.node.routes.length ? ` \`${e.node.routes[0]}\`` : "";
    L.push(`${indent}- ${e.num} ${e.node.qualname} — \`${e.node.file}:${e.node.lineno}-${e.node.end_lineno}\`${r}`);
  }
  for (const e of entries.value) {
    const src = codeMap.value.get(e.id);
    L.push("", `## ${e.num}. ${e.node.qualname}`, "");
    L.push(`\`${e.node.file}:${e.node.lineno}-${e.node.end_lineno}\`${e.node.routes.length ? ` · ${e.node.routes.join(" / ")}` : ""}`, "");
    // 4 反引号围栏：代码内含 ``` 也不破
    L.push("````" + (LANG_OF[e.node.file.split(".").pop() ?? ""] ?? ""));
    L.push(src?.code ?? "（源码未加载）");
    if (src?.truncated) L.push("<!-- 函数超 1200 行已截断 -->");
    L.push("````");
    if (e.calls.length) {
      L.push(
        "",
        "调用：" +
          e.calls
            .map((c) => `\`${e.node.file}:${c.line}\` → ${c.targetNum} ${c.targetName}${c.kind === "heuristic" ? "（同名启发）" : ""}`)
            .join("；"),
      );
    }
  }
  return L.join("\n") + "\n";
}

const exporting = ref(false);
async function copyAll() {
  const md = buildMarkdown(true);
  if (!md) return;
  try {
    await navigator.clipboard.writeText(md);
    message.success(`已复制全链 Markdown（${(md.length / 1000).toFixed(1)}k 字符），可直接粘贴给 AI`);
  } catch {
    message.error("复制失败");
  }
}
async function exportCard() {
  const root = rootEntry.value;
  if (!root || exporting.value) return;
  exporting.value = true;
  try {
    const md = buildMarkdown(false);
    // title 带路由信息：createCard 自动写 `# title` 作为唯一 H1 并用于 wiki_docs 登记
    const routes = root.node.routes.length ? `（${root.node.routes[0]}）` : "";
    const rel = await api.createCard(
      wiki.currentRepo!.path,
      `链路走读：${root.node.qualname}${routes}`,
      md,
    );
    message.success(`已存为知识卡（${rel}），RAG 可检索，文件在 .bytewiki/cards/ 下`);
  } catch (e) {
    message.error(typeof e === "string" ? e : "导出失败");
  } finally {
    exporting.value = false;
  }
}

function backToGraph() {
  router.push({ path: `/repo/${route.params.id}/callgraph`, query: { node: rootNodeId.value } });
}
</script>

<template>
  <div class="wk-page">
    <!-- 顶栏 -->
    <div class="wk-toolbar">
      <button class="btn sm" @click="backToGraph">
        <ArrowLeftOutlined /> 返回调用链
      </button>
      <div class="wk-title">
        <BookOutlined class="wk-title-icon" />
        <span v-if="rootEntry">走读：{{ rootEntry.node.qualname }}</span>
        <span v-if="rootEntry?.node.routes.length" class="wk-title-route">{{ rootEntry.node.routes[0] }}</span>
      </div>
      <span v-if="stats" class="wk-stats">
        {{ stats.fns }} 函数 · {{ stats.files }} 文件 · {{ stats.lines }} 行
      </span>
      <div class="wk-toolbar-actions">
        <button class="btn sm primary" :disabled="!rootEntry" @click="copyAll"><CopyOutlined /> 复制全链</button>
        <button class="btn sm" :disabled="!rootEntry || exporting" @click="exportCard"><SaveOutlined /> 存为知识卡</button>
        <button class="btn sm" title="展开全部代码" @click="expandAll"><NodeExpandOutlined /> 全展开</button>
        <button class="btn sm" title="折叠为起点" @click="collapseAll"><NodeCollapseOutlined /> 全折叠</button>
        <button class="btn sm" @click="mainPathOnly">仅主路径</button>
      </div>
    </div>

    <div v-if="loading" class="wk-state"><EmptyState text="构建走读链…" /></div>
    <div v-else-if="loadError" class="wk-state">
      <EmptyState text="无法走读" :hint="loadError + '——请先在调用链页生成并选择节点'" />
      <button class="btn" @click="backToGraph"><ArrowLeftOutlined /> 去调用链页</button>
    </div>

    <div v-else class="wk-main">
      <!-- 大纲（可折叠：收起为细轨，代码区吃满宽度） -->
      <aside v-if="!outlineCollapsed" class="wk-outline">
        <div class="wk-outline-head">
          <span>执行流大纲</span>
          <button class="wk-outline-fold" title="收起大纲" @click="toggleOutline">
            <DoubleLeftOutlined />
          </button>
        </div>
        <div class="wk-outline-list">
          <button
            v-for="e in entries"
            :key="e.id"
            class="wk-outline-item"
            :class="{ active: e.id === activeId, collapsed: !expandedIds.has(e.id) }"
            :style="{ paddingLeft: 8 + (e.num.split('.').length - 1) * 12 + 'px' }"
            :title="`${e.num} ${e.node.qualname} — ${e.node.file}:${e.node.lineno}`"
            @click="jumpTo(e.id)"
          >
            <span class="wk-outline-num">{{ e.num }}</span>
            <span class="wk-outline-name">{{ e.node.qualname.split(".").pop() }}</span>
          </button>
        </div>
      </aside>

      <button v-if="outlineCollapsed" class="wk-outline-rail" title="展开执行流大纲" @click="toggleOutline">
        <DoubleRightOutlined />
        <span class="wk-outline-rail-text">大纲 {{ entries.length }}</span>
      </button>

      <!-- 代码块流 -->
      <div class="wk-stream">
        <WalkBlock
          v-for="(e, i) in entries"
          :key="e.id"
          :ref="(el) => setBlockRef(e.id, el)"
          :num="e.num"
          :qualname="e.node.qualname"
          :file="e.node.file"
          :start-line="e.node.lineno"
          :end-line="e.node.end_lineno"
          :lang="'plaintext'"
          :code="codeMap.get(e.id)?.code ?? ''"
          :truncated="codeMap.get(e.id)?.truncated ?? false"
          :routes="e.node.routes"
          :call-links="e.calls"
          :expanded="expandedIds.has(e.id)"
          :index="i"
          @toggle="toggleBlock(e.id)"
          @jump="jumpTo"
          @visible="onVisible(e.id)"
        />
        <div v-if="!codeMap.size && !loading" class="wk-src-empty">源码未加载（文件可能已移动）</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wk-page {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-base);
  padding: 10px 12px 12px;
  gap: 10px;
  box-sizing: border-box;
}
.wk-toolbar {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
  flex-shrink: 0;
}
.wk-title {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  min-width: 0;
}
.wk-title-icon {
  color: var(--color-primary);
}
.wk-title-route {
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  color: #3a9fae;
  background: rgba(125, 216, 232, 0.12);
  border: 1px solid rgba(125, 216, 232, 0.45);
  border-radius: var(--radius-xs);
  padding: 1px 7px;
}
.wk-stats {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
.wk-toolbar-actions {
  display: flex;
  gap: 6px;
  margin-left: auto;
}
.wk-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}
.wk-main {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 10px;
}
.wk-outline {
  width: 250px;
  flex-shrink: 0;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-panel, 14px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.wk-outline-head {
  padding: 10px 12px 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.wk-outline-list {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}
.wk-outline-item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  transition: background var(--transition-fast);
}
.wk-outline-item:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.wk-outline-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}
.wk-outline-item.collapsed .wk-outline-name {
  color: var(--text-tertiary);
}
.wk-outline-num {
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  color: var(--color-primary);
  flex-shrink: 0;
  min-width: 26px;
}
.wk-outline-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wk-outline-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.wk-outline-fold {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  border-radius: var(--radius-xs);
  cursor: pointer;
}
.wk-outline-fold:hover {
  background: var(--bg-elevated);
  color: var(--color-primary);
}
.wk-outline-rail {
  width: 34px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 10px 0;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-panel, 14px);
  color: var(--text-tertiary);
  cursor: pointer;
  transition: color var(--transition-fast);
}
.wk-outline-rail:hover {
  color: var(--color-primary);
}
.wk-outline-rail-text {
  writing-mode: vertical-rl;
  font-size: 11px;
  letter-spacing: 2px;
}
.wk-stream {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 2px 4px 40px 2px;
}
.wk-src-empty {
  color: var(--text-tertiary);
  font-size: 12.5px;
  padding: 20px;
  text-align: center;
}
</style>
