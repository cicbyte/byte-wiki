<script setup lang="ts">
/**
 * 调用链面板（项目空间 → 调用链 tab）
 *
 * 函数级调用图的浏览与走读：vis-network 画布（模块配色/入口菱形/启发边虚线）+
 * 入口面板（点击只看该入口的可达执行流）+ 节点详情（双向调用可点跳转）+
 * Monaco 源码定位打开（读实时文件——图是快照，代码是当前，配合滞后语义）。
 *
 * 交互从已验证的 standalone 资产（assets/callchain_app.js）移植，踩坑教训全部保留：
 * 坐标固化（层级布局是动态的，筛选/聚焦触发整图重排会拖死大图）、
 * 搜索 250ms 防抖、fit/focus 去 animation。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { message } from "ant-design-vue";
import { SearchOutlined, ReloadOutlined, AimOutlined, CodeOutlined } from "@ant-design/icons-vue";
import { DataSet, Network } from "vis-network/standalone";
import { api } from "@/api";
import type { CallgraphData, CallGraphNode, CallgraphEdge } from "@/api/callchain";
import EmptyState from "@/components/common/EmptyState.vue";
import SnippetEditor from "@/components/editor/SnippetEditor.vue";

const props = defineProps<{ repoPath: string }>();

/** 模块配色（与资产同表：色相间隔大，色弱可辨） */
const PALETTE = [
  "#5aa2ff", "#54d08c", "#f0b429", "#ef6f6c", "#b58cff", "#4ec9d4",
  "#f78fb3", "#9ad64f", "#ff9f6b", "#6fa8ff", "#c9a1ff", "#5fd0a8",
  "#f6d365", "#7dd8e8",
];

// ===== 状态 =====
const loading = ref(true);
const missing = ref(false); // 产物不存在（未生成过）
const generating = ref(false);
const progressText = ref("");
const data = ref<CallgraphData | null>(null);
const search = ref("");
const moduleFilter = ref("");
const viewFilter = ref<"" | "entry" | "hub">("");
const entrySearch = ref("");
const selectedId = ref<string | null>(null);
const focusNote = ref("");
const sourceState = reactive({
  open: false,
  loading: false,
  file: "",
  lang: "",
  content: "",
  line: 1,
});

// ===== 画布对象（非响应式）=====
const canvasEl = ref<HTMLDivElement>();
const editorEl = ref<InstanceType<typeof SnippetEditor>>();
let network: Network | null = null;
let vnodes: DataSet<Record<string, unknown>> | null = null;
let vedges: DataSet<Record<string, unknown>> | null = null;
let nodeById = new Map<string, CallGraphNode>();
let outEdges = new Map<string, CallgraphEdge[]>();
let inEdges = new Map<string, CallgraphEdge[]>();
/** 响应式：图例与模块下拉依赖 */
const modulesList = ref<string[]>([]);
const modColor = ref(new Map<string, string>());
let freezeTimer: ReturnType<typeof setTimeout> | null = null;
let searchTimer: ReturnType<typeof setTimeout> | null = null;

const stats = computed(() => {
  const d = data.value;
  if (!d) return null;
  return {
    nodes: d.nodes.length,
    edges: d.edges.length,
    modules: Object.keys(d.modules).length,
    entries: d.nodes.filter((n) => n.is_entry).length,
  };
});
const moduleOptions = computed(() =>
  modulesList.value.map((m) => ({ label: `${m} (${data.value?.modules[m]?.funcs ?? 0})`, value: m })),
);
const legend = computed(() => modulesList.value.slice(0, 12));
const legendMore = computed(() => Math.max(0, modulesList.value.length - 12));

/** 入口面板：路由入口在前，自定义入口在后 */
const entryGroups = computed(() => {
  const d = data.value;
  if (!d) return [];
  const kw = entrySearch.value.trim().toLowerCase();
  const match = (n: CallGraphNode) =>
    !kw || (n.qualname + " " + n.routes.join(" ")).toLowerCase().includes(kw);
  const routes = d.nodes.filter((n) => n.is_entry && n.routes.length && match(n));
  const customs = d.nodes.filter((n) => n.is_entry && !n.routes.length && match(n));
  const groups: { label: string; items: CallGraphNode[] }[] = [];
  if (routes.length) groups.push({ label: `路由入口 (${routes.length})`, items: routes });
  if (customs.length) groups.push({ label: `自定义入口 (${customs.length})`, items: customs });
  return groups;
});

const selected = computed(() => (selectedId.value ? nodeById.get(selectedId.value) ?? null : null));
const callersOf = computed<CallgraphEdge[]>(() =>
  selectedId.value ? inEdges.get(selectedId.value) ?? [] : [],
);
const calleesOf = computed<CallgraphEdge[]>(() =>
  selectedId.value ? outEdges.get(selectedId.value) ?? [] : [],
);

// ===== 数据加载与建图 =====
async function loadData() {
  loading.value = true;
  missing.value = false;
  try {
    const text = await api.callchainRead(props.repoPath);
    data.value = JSON.parse(text) as CallgraphData;
    await nextTick();
    buildGraph();
  } catch {
    data.value = null;
    missing.value = true;
  } finally {
    loading.value = false;
  }
}

function buildGraph() {
  const d = data.value;
  if (!d || !canvasEl.value) return;
  network?.destroy();
  network = null;
  nodeById = new Map(d.nodes.map((n) => [n.id, n]));
  outEdges = new Map();
  inEdges = new Map();
  for (const e of d.edges) {
    (outEdges.get(e.src) ?? outEdges.set(e.src, []).get(e.src)!).push(e);
    (inEdges.get(e.dst) ?? inEdges.set(e.dst, []).get(e.dst)!).push(e);
  }
  modulesList.value = Object.keys(d.modules).sort();
  modColor.value = new Map(modulesList.value.map((m, i) => [m, PALETTE[i % PALETTE.length]]));

  const nodesArr = d.nodes.map((n) => ({
    id: n.id,
    label: n.qualname.length > 34 ? n.qualname.slice(0, 33) + "…" : n.qualname,
    title: `${n.module}\n${n.file}:${n.lineno}`,
    color: {
      background: modColor.value.get(n.module),
      border: n.is_entry ? "#ffcf7d" : modColor.value.get(n.module),
      highlight: { background: "#ffffff", border: "#5aa2ff" },
    },
    shape: n.is_entry ? "diamond" : n.routes.length ? "box" : "ellipse",
    font: { color: "#dbe4f5", size: 11 },
    borderWidth: n.is_entry ? 2 : 1,
  }));
  const edgesArr = d.edges.map((e, i) => ({
    id: `e${i}`,
    from: e.src,
    to: e.dst,
    arrows: "to",
    dashes: e.kind === "heuristic",
    label: e.kind === "heuristic" ? "~" : "",
    color: { color: "#3a4767", highlight: "#ffcf7d", hover: "#5aa2ff" },
    font: { size: 9, color: "#8496b8" },
  }));

  vnodes = new DataSet(nodesArr);
  vedges = new DataSet(edgesArr);
  network = new Network(canvasEl.value, { nodes: vnodes, edges: vedges }, {
    layout: { hierarchical: { enabled: true, direction: "UD", sortMethod: "directed", levelSeparation: 110, nodeSpacing: 90 } },
    physics: { enabled: false },
    interaction: { hover: true, keyboard: true, multiselect: true },
    edges: { smooth: { enabled: true, type: "cubicBezier", roundness: 0.5 } },
  });

  // 坐标固化：初次分层布局完成后关闭 hierarchical 并回写坐标——
  // 否则之后每次筛选/聚焦的 hidden 批量更新都触发整图重排，大图直接拖死渲染进程
  freezeTimer = setTimeout(() => {
    if (!network || !vnodes) return;
    try {
      const pos = network.getPositions();
      network.setOptions({ layout: { hierarchical: { enabled: false } } });
      vnodes.update(Object.keys(pos).map((id) => ({ id, x: pos[id].x, y: pos[id].y })));
    } catch {
      /* 固化失败维持原行为 */
    }
  }, 600);

  network.on("click", (params: any) => {
    if (params.nodes.length) {
      selectedId.value = params.nodes[0];
    }
  });
  network.on("doubleClick", (params: any) => {
    if (params.nodes.length) focusChain(params.nodes[0]);
  });
}

// ===== 筛选 / 聚焦 =====
function applyFilter() {
  if (!vnodes || !data.value) return;
  const q = search.value.trim().toLowerCase();
  const mv = moduleFilter.value;
  const ev = viewFilter.value;
  const updates = data.value.nodes.map((n) => {
    let show = true;
    if (mv && n.module !== mv) show = false;
    if (ev === "entry" && !n.is_entry) show = false;
    if (ev === "hub" && n.callers < 10) show = false;
    if (q) {
      const hay = (n.qualname + " " + n.module + " " + n.routes.join(" ")).toLowerCase();
      if (!hay.includes(q)) show = false;
    }
    return { id: n.id, hidden: !show };
  });
  vnodes.update(updates);
}

function onSearchInput() {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(applyFilter, 250);
}

/** 可达闭包：入口 → 全部后代 */
function descendantsOf(id: string): Set<string> {
  const keep = new Set([id]);
  let frontier = [id];
  while (frontier.length) {
    const next: string[] = [];
    for (const x of frontier) {
      for (const e of outEdges.get(x) ?? []) {
        if (!keep.has(e.dst)) {
          keep.add(e.dst);
          next.push(e.dst);
        }
      }
    }
    frontier = next;
  }
  return keep;
}

function showOnly(keep: Set<string>, centerId: string, note: string) {
  vnodes?.update((data.value?.nodes ?? []).map((n) => ({ id: n.id, hidden: !keep.has(n.id) })));
  selectedId.value = centerId;
  network?.selectNodes([centerId]);
  network?.fit({ animation: false });
  focusNote.value = note;
}

function focusFromEntry(id: string) {
  const keep = descendantsOf(id);
  showOnly(keep, id, `已聚焦该入口执行流（${keep.size} 节点）· 重置视图恢复`);
}

/** 双击：上下游闭包（全部祖先 + 后代） */
function focusChain(id: string) {
  const keep = descendantsOf(id);
  let frontier = [id];
  while (frontier.length) {
    const next: string[] = [];
    for (const x of frontier) {
      for (const e of inEdges.get(x) ?? []) {
        if (!keep.has(e.src)) {
          keep.add(e.src);
          next.push(e.src);
        }
      }
    }
    frontier = next;
  }
  showOnly(keep, id, `已聚焦链路（${keep.size} 节点）· 重置视图恢复`);
}

function resetView() {
  search.value = "";
  moduleFilter.value = "";
  viewFilter.value = "";
  focusNote.value = "";
  applyFilter();
  network?.fit({ animation: false });
}

function jumpTo(id: string) {
  selectedId.value = id;
  network?.selectNodes([id]);
  network?.focus(id, { scale: 1.1, animation: false });
}

// ===== 生成 =====
async function generate() {
  if (generating.value) return;
  generating.value = true;
  progressText.value = "";
  try {
    const s = await api.callchainGenerate(props.repoPath);
    message.success(
      `调用链已生成：函数 ${s.nodes} · 边 ${s.edges} · 入口 ${s.entries}（${s.duration_ms}ms）`,
    );
    await loadData();
  } catch (e) {
    message.error(typeof e === "string" ? e : "生成失败");
  } finally {
    generating.value = false;
    progressText.value = "";
  }
}

let unlisten: Awaited<ReturnType<typeof api.callchainOnProgress>> | null = null;
onMounted(async () => {
  unlisten = await api.callchainOnProgress((n) => {
    progressText.value = `已解析 ${n} 个源文件…`;
  });
  await loadData();
});
onBeforeUnmount(() => {
  unlisten?.();
  if (freezeTimer) clearTimeout(freezeTimer);
  if (searchTimer) clearTimeout(searchTimer);
  network?.destroy();
  network = null;
});

// ===== Monaco 源码定位 =====
const EXT_LANG: Record<string, string> = {
  py: "python", pyi: "python",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  go: "go", java: "java", rs: "rust",
};

async function openSource(n: CallGraphNode) {
  sourceState.open = true;
  sourceState.loading = true;
  sourceState.file = `${n.file}:${n.lineno}-${n.end_lineno}`;
  sourceState.lang = EXT_LANG[n.file.split(".").pop() ?? ""] ?? "plaintext";
  sourceState.line = n.lineno;
  sourceState.content = "";
  try {
    sourceState.content = await api.readRepoFile(props.repoPath, n.file);
    await nextTick();
    const ed = editorEl.value?.getEditor();
    if (ed) {
      ed.revealLineInCenter(n.lineno);
      ed.setPosition({ lineNumber: n.lineno, column: 1 });
    }
  } catch (e) {
    message.error(typeof e === "string" ? e : "读取源码失败");
    sourceState.open = false;
  } finally {
    sourceState.loading = false;
  }
}

function labelOf(n: CallGraphNode): string {
  return n.qualname.split(".").pop() || n.qualname;
}
function subOf(n: CallGraphNode): string {
  return n.routes[0] || n.module.split("/").pop() || n.module;
}
</script>

<template>
  <div class="cg-panel">
    <!-- 工具栏 -->
    <div class="cg-toolbar">
      <div class="cg-toolbar-left">
        <button class="cg-btn primary" :disabled="generating" @click="generate">
          <ReloadOutlined :spin="generating" />
          {{ generating ? "生成中…" : data ? "重新生成" : "生成调用链" }}
        </button>
        <span v-if="stats" class="cg-stats">
          函数 {{ stats.nodes }} · 调用边 {{ stats.edges }} · 模块 {{ stats.modules }} · 入口 {{ stats.entries }}
        </span>
        <span v-if="generating && progressText" class="cg-progress">{{ progressText }}</span>
        <span v-if="focusNote" class="cg-focus-note"><AimOutlined /> {{ focusNote }}</span>
      </div>
      <div class="cg-toolbar-right">
        <a-input
          v-model:value="search"
          placeholder="搜索 函数名 / 模块 / 路由…"
          style="width: 210px"
          allow-clear
          @input="onSearchInput"
        >
          <template #prefix><SearchOutlined /></template>
        </a-input>
        <a-select
          v-model:value="moduleFilter"
          :options="moduleOptions"
          placeholder="全部模块"
          style="width: 170px"
          allow-clear
          @change="applyFilter"
        />
        <a-select
          v-model:value="viewFilter"
          :options="[
            { label: '全部函数', value: '' },
            { label: '仅入口', value: 'entry' },
            { label: '仅 Hub（被调 ≥10）', value: 'hub' },
          ]"
          style="width: 150px"
          @change="applyFilter"
        />
        <button class="cg-btn" @click="resetView">重置视图</button>
      </div>
    </div>

    <!-- 主体三栏 -->
    <div class="cg-main">
      <!-- 入口面板 -->
      <aside class="cg-entries">
        <div class="cg-entries-head">
          <span>入口（点击看执行流）</span>
        </div>
        <a-input v-model:value="entrySearch" placeholder="过滤入口…" size="small" allow-clear />
        <div class="cg-entry-list">
          <template v-for="g in entryGroups" :key="g.label">
            <div class="cg-entry-group">{{ g.label }}</div>
            <div
              v-for="n in g.items"
              :key="n.id"
              class="cg-entry-item"
              :class="{ custom: !n.routes.length, active: n.id === selectedId }"
              @click="focusFromEntry(n.id)"
            >
              <b>{{ labelOf(n) }}</b>
              <span>{{ subOf(n) }}</span>
            </div>
          </template>
          <div v-if="!entryGroups.length" class="cg-entry-empty">
            {{ data ? "无入口（路由装饰器自动识别，或语言无此概念）" : "" }}
          </div>
        </div>
      </aside>

      <!-- 画布 -->
      <div class="cg-canvas">
        <div ref="canvasEl" class="cg-canvas-inner" />
        <div v-if="loading" class="cg-overlay"><EmptyState text="加载调用图…" /></div>
        <div v-else-if="missing" class="cg-overlay">
          <EmptyState
            text="尚未生成调用链"
            hint="tree-sitter 多语言提取函数级调用图，产物随仓库共享（.bytewiki/graphs/callchain/）"
          />
          <button class="cg-btn primary" :disabled="generating" @click="generate">
            <ReloadOutlined :spin="generating" /> {{ generating ? "生成中…" : "生成调用链" }}
          </button>
        </div>
        <!-- 图例 -->
        <div v-if="data && !missing" class="cg-legend">
          <span v-for="m in legend" :key="m" class="cg-legend-item">
            <i class="cg-swatch" :style="{ background: modColor.get(m) }" />{{ m }}
          </span>
          <span v-if="legendMore" class="cg-legend-item">…共 {{ modulesList.length }} 个</span>
          <span class="cg-legend-item"><i class="cg-swatch entry" />入口</span>
          <span class="cg-legend-item cg-legend-dash">虚线 = 启发边（同名归约，非精确）</span>
        </div>
      </div>

      <!-- 详情侧栏 -->
      <aside class="cg-detail">
        <template v-if="selected">
          <div class="cg-card">
            <h3>{{ selected.qualname }}</h3>
            <div class="cg-meta">📦 {{ selected.module }}</div>
            <div class="cg-meta cg-meta-file">
              📄 {{ selected.file }}:{{ selected.lineno }}-{{ selected.end_lineno }}
              <span v-if="selected.async"> · async</span>
            </div>
            <div class="cg-meta">入度 {{ selected.callers }} · 出度 {{ selected.callees }}</div>
            <div class="cg-tags">
              <span v-for="r in selected.routes" :key="r" class="cg-tag route">{{ r }}</span>
              <span v-if="selected.is_entry" class="cg-tag entry">入口</span>
            </div>
            <button class="cg-btn source-btn" @click="openSource(selected)">
              <CodeOutlined /> 打开源码
            </button>
          </div>
          <div class="cg-card">
            <h3>被谁调用（{{ callersOf.length }}）</h3>
            <div v-if="!callersOf.length" class="cg-empty-inline">无（入口/未被调用）</div>
            <div
              v-for="(e, i) in callersOf"
              :key="'in' + i"
              class="cg-edge-link"
              @click="jumpTo(e.src)"
            >
              ← {{ nodeById.get(e.src)?.qualname ?? e.src }}
              <em>{{ nodeById.get(e.src)?.module }} :{{ e.line }}</em>
            </div>
          </div>
          <div class="cg-card">
            <h3>调用了谁（{{ calleesOf.length }}）</h3>
            <div v-if="!calleesOf.length" class="cg-empty-inline">无（叶子函数）</div>
            <div
              v-for="(e, i) in calleesOf"
              :key="'out' + i"
              class="cg-edge-link"
              @click="jumpTo(e.dst)"
            >
              → {{ nodeById.get(e.dst)?.qualname ?? e.dst }}
              <em>{{ nodeById.get(e.dst)?.module }} :{{ e.line }}{{ e.kind === "heuristic" ? " ~" : "" }}</em>
            </div>
          </div>
        </template>
        <div v-else class="cg-detail-empty">
          单击节点看详情 · 双击聚焦上下游<br />点左侧入口只看该入口执行流
        </div>
      </aside>
    </div>

    <!-- 源码抽屉（Monaco 读实时文件 + 定位行） -->
    <a-drawer
      v-model:open="sourceState.open"
      :title="sourceState.file"
      width="62%"
      placement="right"
      :body-style="{ padding: '0', display: 'flex', flexDirection: 'column' }"
    >
      <div class="cg-source-wrap">
        <div v-if="sourceState.loading" class="cg-source-loading">读取源码…</div>
        <SnippetEditor
          v-show="!sourceState.loading"
          ref="editorEl"
          v-model="sourceState.content"
          :language="sourceState.lang"
          :readonly="true"
          :auto-detect="false"
        />
      </div>
    </a-drawer>
  </div>
</template>

<style scoped>
.cg-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}

/* 工具栏 */
.cg-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-md);
  flex-wrap: wrap;
  flex-shrink: 0;
}
.cg-toolbar-left,
.cg-toolbar-right {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  flex-wrap: wrap;
}
.cg-stats {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
.cg-progress {
  font-size: 12px;
  color: var(--color-primary);
}
.cg-focus-note {
  font-size: 12px;
  color: #ffcf7d;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.cg-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--border-base);
  background: var(--bg-surface);
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 13px;
  transition: all var(--transition-fast);
}
.cg-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}
.cg-btn.primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #fff;
}
.cg-btn.primary:hover:not(:disabled) {
  background: var(--color-primary-hover);
  color: #fff;
}
.cg-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 主体三栏 */
.cg-main {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--spacing-sm);
}
.cg-entries {
  width: 230px;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow: hidden;
}
.cg-entries-head {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
}
.cg-entry-list {
  flex: 1;
  overflow-y: auto;
}
.cg-entry-group {
  font-size: 11px;
  color: var(--text-tertiary);
  margin: 10px 0 4px;
  border-bottom: 1px dashed var(--border-light);
  padding-bottom: 2px;
}
.cg-entry-item {
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  border: 1px solid transparent;
  transition: all var(--transition-fast);
}
.cg-entry-item:hover {
  background: var(--bg-elevated);
  border-color: var(--color-primary-border);
}
.cg-entry-item.active {
  background: var(--color-primary-bg);
  border-color: var(--color-primary-border);
}
.cg-entry-item b {
  display: block;
  font-size: 12.5px;
  font-weight: 600;
  word-break: break-all;
  color: var(--text-primary);
}
.cg-entry-item span {
  font-size: 11px;
  color: #7dd8e8;
}
.cg-entry-item.custom span {
  color: #ffcf7d;
}
.cg-entry-empty {
  font-size: 12px;
  color: var(--text-tertiary);
  padding: 12px 4px;
  line-height: 1.6;
}

/* 画布 */
.cg-canvas {
  flex: 1;
  min-width: 0;
  position: relative;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
}
.cg-canvas-inner {
  position: absolute;
  inset: 0;
}
.cg-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  background: var(--bg-card);
  z-index: 5;
}
.cg-legend {
  position: absolute;
  left: 10px;
  bottom: 8px;
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  max-width: calc(100% - 20px);
  pointer-events: none;
}
.cg-legend-item {
  font-size: 11px;
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.cg-swatch {
  display: inline-block;
  width: 10px;
  height: 10px;
  border-radius: 2px;
}
.cg-swatch.entry {
  background: transparent;
  border: 2px solid #ffcf7d;
}
.cg-legend-dash {
  border-bottom: 1px dashed var(--border-base);
}

/* 详情侧栏 */
.cg-detail {
  width: 360px;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow-y: auto;
  padding: 12px;
}
.cg-detail-empty {
  color: var(--text-tertiary);
  font-size: 12px;
  text-align: center;
  padding: 40px 12px;
  line-height: 2;
}
.cg-card {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  padding: 10px 12px;
  margin-bottom: 10px;
  background: var(--bg-surface);
}
.cg-card h3 {
  margin: 0 0 6px;
  font-size: 14px;
  word-break: break-all;
  color: var(--text-primary);
}
.cg-meta {
  color: var(--text-tertiary);
  font-size: 12px;
  margin-bottom: 3px;
  word-break: break-all;
}
.cg-meta-file {
  cursor: pointer;
}
.cg-tags {
  margin-top: 6px;
}
.cg-tag {
  display: inline-block;
  border-radius: 4px;
  padding: 1px 7px;
  font-size: 11px;
  margin-right: 5px;
  margin-top: 4px;
}
.cg-tag.route {
  background: #12333d;
  color: #7dd8e8;
}
.cg-tag.entry {
  background: #3d2f12;
  color: #ffcf7d;
  border: 1px solid #6b5320;
}
.source-btn {
  margin-top: 10px;
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
}
.cg-edge-link {
  font-size: 12px;
  color: var(--color-primary);
  cursor: pointer;
  padding: 3px 2px;
  border-radius: var(--radius-xs);
  word-break: break-all;
}
.cg-edge-link:hover {
  background: var(--bg-elevated);
}
.cg-edge-link em {
  font-style: normal;
  color: var(--text-tertiary);
  font-size: 11px;
  margin-left: 4px;
}
.cg-empty-inline {
  color: var(--text-tertiary);
  font-size: 12px;
  padding: 6px 0;
}

/* 源码抽屉 */
.cg-source-wrap {
  flex: 1;
  min-height: 0;
  position: relative;
}
.cg-source-loading {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  font-size: 13px;
  background: var(--bg-card);
  z-index: 2;
}
</style>
