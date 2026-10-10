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
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { message } from "ant-design-vue";
import {
  SearchOutlined,
  ReloadOutlined,
  AimOutlined,
  CodeOutlined,
  DoubleLeftOutlined,
  DoubleRightOutlined,
  CaretRightOutlined,
  ExpandOutlined,
  ShrinkOutlined,
  CopyOutlined,
  ClusterOutlined,
  ClearOutlined,
} from "@ant-design/icons-vue";
import { DataSet, Network } from "vis-network/standalone";
import { api } from "@/api";
import type { CallgraphData, CallGraphNode, CallgraphEdge } from "@/api/callchain";
import { useThemeStore } from "@/stores/theme";
import { monaco } from "@/monaco/setup";
import EmptyState from "@/components/common/EmptyState.vue";
import SnippetEditor from "@/components/editor/SnippetEditor.vue";

const props = defineProps<{ repoPath: string }>();

const themeStore = useThemeStore();

/** 画布配色跟随主题（固定色在浅色主题下不可读） */
const canvasColors = computed(() =>
  themeStore.isDark
    ? { font: "#dbe4f5", edge: "#3a4767", edgeFont: "#8496b8" }
    : { font: "#33415e", edge: "#c3cde4", edgeFont: "#8a97b8" },
);

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
const verbFilter = ref(""); // "" = 全部动词
/** 入口面板折叠（收起为细轨，画布全宽；localStorage 记忆） */
const panelCollapsed = ref(localStorage.getItem("bw:cg-entries-collapsed") === "1");
/** 展开的入口分组（module key；自定义入口用 CUSTOM_KEY） */
const expandedGroups = ref<Set<string>>(new Set());

// ===== 三栏宽度拖拽（左入口栏/右详情栏可拖，画布弹性自适应） =====
const ENTRIES_W_DEFAULT = 248;
const DETAIL_W_DEFAULT = 360;
const entriesWidth = ref(Number(localStorage.getItem("bw:cg-entries-w")) || ENTRIES_W_DEFAULT);
const detailWidth = ref(Number(localStorage.getItem("bw:cg-detail-w")) || DETAIL_W_DEFAULT);
const resizing = ref<"" | "entries" | "detail">("");

/** 夹片拖拽：按下记录起点，move 按方向差值改宽（右栏取反向），抬起重存 localStorage */
function startResize(which: "entries" | "detail", e: MouseEvent) {
  e.preventDefault();
  resizing.value = which;
  const startX = e.clientX;
  const startW = which === "entries" ? entriesWidth.value : detailWidth.value;
  const onMove = (ev: MouseEvent) => {
    const dx = ev.clientX - startX;
    const w = which === "entries" ? startW + dx : startW - dx;
    if (which === "entries") {
      entriesWidth.value = Math.min(420, Math.max(200, w));
    } else {
      detailWidth.value = Math.min(560, Math.max(280, w));
    }
  };
  const onUp = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
    resizing.value = "";
    localStorage.setItem(
      which === "entries" ? "bw:cg-entries-w" : "bw:cg-detail-w",
      String(which === "entries" ? entriesWidth.value : detailWidth.value),
    );
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

/** 双击夹片复位该栏默认宽 */
function resetResize(which: "entries" | "detail") {
  if (which === "entries") entriesWidth.value = ENTRIES_W_DEFAULT;
  else detailWidth.value = DETAIL_W_DEFAULT;
  localStorage.removeItem(which === "entries" ? "bw:cg-entries-w" : "bw:cg-detail-w");
}

// ===== 画布全屏：原生 Fullscreen 优先，应用内全屏兜底 =====
const canvasBox = ref<HTMLDivElement>();
const isFullscreen = ref(false);
const canvasZen = ref(false);
/** 沉浸态：原生全屏或应用内全屏（隐藏工具栏与两栏，画布铺满） */
const immersive = computed(() => isFullscreen.value || canvasZen.value);

async function toggleCanvasFullscreen() {
  const el = canvasBox.value;
  if (!el) return;
  if (!document.fullscreenElement) {
    try {
      await el.requestFullscreen();
    } catch {
      canvasZen.value = true; // WebView 不支持时兜底：应用内全屏
    }
  } else {
    await document.exitFullscreen();
  }
}

function exitImmersive() {
  if (document.fullscreenElement) {
    document.exitFullscreen();
  }
  canvasZen.value = false;
}

function onFullscreenChange() {
  isFullscreen.value = !!document.fullscreenElement;
}
/** 应用内全屏没有原生 Esc，自行监听退出 */
function onZenEsc(e: KeyboardEvent) {
  if (e.key === "Escape" && canvasZen.value) exitImmersive();
}
const selectedId = ref<string | null>(null);
const focusNote = ref("");
const sourceState = reactive({
  open: false,
  loading: false,
  qualname: "",
  file: "",
  lang: "",
  content: "",
  line: 1,
  endLine: 1,
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

// ===== 入口面板：按模块分组 + 方法徽标单行化 + 动词筛选 =====

/** 单条入口的展示模型：verb 从路由标签首段解析（"GET /x" → GET；"GF a.B" → GF；无路由 → ""） */
interface EntryItem {
  node: CallGraphNode;
  verb: string;
  label: string;
}

/** 自定义入口（约定入口，无路由标签）的分组 key */
const CUSTOM_KEY = "__custom__";
/** 组内排序：动词惯例序，无动词的排最后 */
const VERB_ORDER = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "ANY", "GF", ""];

function parseEntry(n: CallGraphNode): EntryItem {
  const r0 = n.routes[0] ?? "";
  if (!r0) return { node: n, verb: "", label: n.qualname.split(".").pop() || n.qualname };
  if (r0.startsWith("GF ")) return { node: n, verb: "GF", label: r0.slice(3) };
  const sp = r0.indexOf(" ");
  return sp > 0
    ? { node: n, verb: r0.slice(0, sp), label: r0.slice(sp + 1) }
    : { node: n, verb: r0, label: n.qualname.split(".").pop() || n.qualname };
}

/** 搜索 + 动词筛选后的入口集合 */
const filteredEntries = computed<EntryItem[]>(() => {
  const d = data.value;
  if (!d) return [];
  const kw = entrySearch.value.trim().toLowerCase();
  const vf = verbFilter.value;
  return d.nodes
    .filter((n) => n.is_entry)
    .map(parseEntry)
    .filter(
      (it) =>
        (!vf || it.verb === vf) &&
        (!kw ||
          (it.node.qualname + " " + (it.node.routes[0] ?? "") + " " + it.node.module)
            .toLowerCase()
            .includes(kw)),
    );
});

/** 动词筛选（动词 → 入口数） */
const verbChips = computed<[string, number][]>(() => {
  const counts = new Map<string, number>();
  for (const n of data.value?.nodes ?? []) {
    if (!n.is_entry) continue;
    const { verb } = parseEntry(n);
    if (verb) counts.set(verb, (counts.get(verb) ?? 0) + 1);
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1]);
});

/** 动词下拉选项（与工具栏 a-select 形态一致，控件不混搭） */
const verbOptions = computed(() => [
  { label: "全部动词", value: "" },
  ...verbChips.value.map(([v, c]) => ({ label: `${v}（${c}）`, value: v })),
]);

/** 分组：路由入口按模块聚簇（自然对应 controller/文件），自定义入口垫底 */
const entryGroups = computed<{ key: string; label: string; tooltip: string; items: EntryItem[] }[]>(() => {
  const byKey = new Map<string, EntryItem[]>();
  for (const it of filteredEntries.value) {
    const key = it.node.routes.length ? it.node.module : CUSTOM_KEY;
    (byKey.get(key) ?? byKey.set(key, []).get(key)!).push(it);
  }
  const groups = [...byKey.entries()].map(([key, items]) => {
    items.sort(
      (a, b) =>
        VERB_ORDER.indexOf(a.verb) - VERB_ORDER.indexOf(b.verb) || a.label.localeCompare(b.label),
    );
    if (key === CUSTOM_KEY) {
      return { key, label: "自定义入口", tooltip: "fn main / #[command] / @Scheduled 等约定入口", items };
    }
    // 组名取路径末两段（前缀太长挤压），完整路径进 tooltip
    const segs = key.split("/");
    const label = segs.length > 1 ? segs.slice(-2).join("/") : key;
    return { key, label, tooltip: key, items };
  });
  groups.sort((a, b) => (a.key === CUSTOM_KEY ? 1 : b.key === CUSTOM_KEY ? -1 : a.label.localeCompare(b.label)));
  return groups;
});

const entryCount = computed(() => (data.value?.nodes ?? []).filter((n) => n.is_entry).length);

function verbClass(verb: string): string {
  return verb ? `m-${verb.toLowerCase()}` : "m-none";
}

function toggleGroup(key: string) {
  const next = new Set(expandedGroups.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  expandedGroups.value = next;
}

function togglePanel() {
  panelCollapsed.value = !panelCollapsed.value;
  localStorage.setItem("bw:cg-entries-collapsed", panelCollapsed.value ? "1" : "0");
}

/** 主题切换 → 重建画布（节点/边固定色在明暗主题下互换，重建即套用新配色） */
watch(
  () => themeStore.isDark,
  () => {
    if (data.value && !missing.value) buildGraph();
  },
);

/** 选中入口节点时自动展开其所在分组 */
watch(selectedId, (id) => {
  if (!id) return;
  const n = nodeById.get(id);
  if (!n || !n.is_entry) return;
  const key = n.routes.length ? n.module : CUSTOM_KEY;
  if (!expandedGroups.value.has(key)) {
    const next = new Set(expandedGroups.value);
    next.add(key);
    expandedGroups.value = next;
  }
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

  // 入口分组默认展开前 3 组（按全量入口的模块分组，不受筛选影响；自定义入口垫底）
  const entryMods = new Set<string>();
  for (const n of d.nodes) {
    if (n.is_entry) entryMods.add(n.routes.length ? n.module : CUSTOM_KEY);
  }
  const groupKeys = [...entryMods].filter((k) => k !== CUSTOM_KEY).sort();
  if (entryMods.has(CUSTOM_KEY)) groupKeys.push(CUSTOM_KEY);
  expandedGroups.value = new Set(groupKeys.slice(0, 3));

  const nodesArr = d.nodes.map((n) => ({
    id: n.id,
    label: n.qualname.length > 34 ? n.qualname.slice(0, 33) + "…" : n.qualname,
    title: `${n.module}\n${n.file}:${n.lineno}`,
    color: {
      background: modColor.value.get(n.module),
      border: n.is_entry ? "#e8a33d" : modColor.value.get(n.module),
      highlight: { background: "#ffffff", border: "#5aa2ff" },
    },
    shape: n.is_entry ? "diamond" : n.routes.length ? "box" : "ellipse",
    font: { color: canvasColors.value.font, size: 12 },
    borderWidth: n.is_entry ? 2 : 1,
  }));
  const edgesArr = d.edges.map((e, i) => ({
    id: `e${i}`,
    from: e.src,
    to: e.dst,
    arrows: "to",
    dashes: e.kind === "heuristic",
    label: e.kind === "heuristic" ? "~" : "",
    color: { color: canvasColors.value.edge, highlight: "#e8a33d", hover: "#5aa2ff" },
    font: { size: 10, color: canvasColors.value.edgeFont },
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
  // 右键菜单（vis 事件名是老 vis.js 遗留的 oncontext）
  network.on("oncontext", openCtxMenu);
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

function focusFromEntry(id: string, note?: string) {
  const keep = descendantsOf(id);
  showOnly(keep, id, note ?? `已聚焦该入口执行流（${keep.size} 节点）· 重置视图恢复`);
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

// ===== 画布右键菜单 =====
const ctxMenu = reactive({
  show: false,
  x: 0,
  y: 0,
  node: null as CallGraphNode | null,
});

function openCtxMenu(params: any) {
  params.event?.preventDefault?.();
  const node = params.nodes?.length ? nodeById.get(params.nodes[0]) ?? null : null;
  if (node) {
    selectedId.value = node.id; // 右键即选中，详情栏同步
  }
  ctxMenu.node = node;
  ctxMenu.x = params.pointer?.DOM?.x ?? 0;
  ctxMenu.y = params.pointer?.DOM?.y ?? 0;
  ctxMenu.show = true;
  // 边界翻转：贴边时菜单往回展开（估宽 220 / 每项 32 + 头 34）
  nextTick(() => {
    const box = canvasBox.value;
    if (!box) return;
    const w = 224;
    const h = ctxMenu.node ? 34 + 32 * 5 + 8 : 32 * 2 + 4;
    if (ctxMenu.x + w > box.clientWidth - 8) ctxMenu.x = box.clientWidth - w - 8;
    if (ctxMenu.y + h > box.clientHeight - 8) ctxMenu.y = box.clientHeight - h - 8;
    ctxMenu.x = Math.max(4, ctxMenu.x);
    ctxMenu.y = Math.max(4, ctxMenu.y);
  });
}

function closeCtxMenu() {
  ctxMenu.show = false;
}

function onCtxGlobalDown(e: MouseEvent) {
  if (ctxMenu.show && !(e.target as HTMLElement)?.closest?.(".cg-ctxmenu")) closeCtxMenu();
}

function onCtxKey(e: KeyboardEvent) {
  if (e.key === "Escape" && ctxMenu.show) closeCtxMenu();
}

function focusDownstreamCtx() {
  const n = ctxMenu.node;
  if (n) focusFromEntry(n.id, `已聚焦下游执行流（${descendantsOf(n.id).size} 节点）· 重置视图恢复`);
}

/** 菜单动作：先关菜单再执行（模板内联事件直接调用本函数，不能再返回闭包） */
function ctxAction(fn: () => void) {
  closeCtxMenu();
  fn();
}

async function copyText(t: string, tip: string) {
  try {
    await navigator.clipboard.writeText(t);
    message.success(tip);
  } catch {
    message.error("复制失败");
  }
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

/** 容器尺寸变化（拖拽/折叠）→ 画布重绘（rAF 合帧，拖拽不掉帧） */
let resizeObs: ResizeObserver | null = null;
let redrawPending = false;
function scheduleRedraw() {
  if (redrawPending) return;
  redrawPending = true;
  requestAnimationFrame(() => {
    redrawPending = false;
    network?.redraw();
  });
}

let unlisten: Awaited<ReturnType<typeof api.callchainOnProgress>> | null = null;
onMounted(async () => {
  resizeObs = new ResizeObserver(scheduleRedraw);
  if (canvasEl.value) resizeObs.observe(canvasEl.value);
  document.addEventListener("fullscreenchange", onFullscreenChange);
  window.addEventListener("keydown", onZenEsc);
  document.addEventListener("mousedown", onCtxGlobalDown, true);
  window.addEventListener("keydown", onCtxKey);
  unlisten = await api.callchainOnProgress((n) => {
    progressText.value = `已解析 ${n} 个源文件…`;
  });
  await loadData();
});
onBeforeUnmount(() => {
  resizeObs?.disconnect();
  document.removeEventListener("fullscreenchange", onFullscreenChange);
  window.removeEventListener("keydown", onZenEsc);
  document.removeEventListener("mousedown", onCtxGlobalDown, true);
  window.removeEventListener("keydown", onCtxKey);
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
  sourceState.qualname = n.qualname;
  sourceState.file = `${n.file}:${n.lineno}-${n.end_lineno}`;
  sourceState.lang = EXT_LANG[n.file.split(".").pop() ?? ""] ?? "plaintext";
  sourceState.line = n.lineno;
  sourceState.endLine = n.end_lineno;
  sourceState.content = "";
  try {
    sourceState.content = await api.readRepoFile(props.repoPath, n.file);
    // 双 nextTick：确保 a-drawer 内容挂载 + SnippetEditor 的 modelValue watch 完成 setValue 后再定位
    await nextTick();
    await nextTick();
    const ed = editorEl.value?.getEditor();
    if (ed) {
      ed.revealLineInCenter(n.lineno);
      ed.setPosition({ lineNumber: n.lineno, column: 1 });
      // 函数整段高亮：行背景 + 行号栏标记（每次打开重建，先清旧集合）
      srcDecorations?.clear();
      srcDecorations = ed.createDecorationsCollection([
        {
          range: new monaco.Range(n.lineno, 1, n.end_lineno, 2147483647),
          options: {
            isWholeLine: true,
            className: "cg-src-highlight",
            linesDecorationsClassName: "cg-src-line-mark",
          },
        },
      ]);
    }
  } catch (e) {
    message.error(typeof e === "string" ? e : "读取源码失败");
    sourceState.open = false;
  } finally {
    sourceState.loading = false;
  }
}

/** 源码抽屉的函数高亮装饰集合（打开时重建） */
let srcDecorations: { clear(): void } | null = null;
</script>

<template>
  <div class="cg-panel" :class="{ resizing: !!resizing }">
    <!-- 工具栏（沉浸态隐藏） -->
    <div v-show="!immersive" class="cg-toolbar">
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
        <button class="cg-btn" title="画布全屏（Esc 退出）" @click="toggleCanvasFullscreen">
          <ExpandOutlined /> 全屏
        </button>
      </div>
    </div>

    <!-- 主体三栏 -->
    <div class="cg-main">
      <!-- 入口面板（沉浸态隐藏；可拖宽/可折叠：收起为细轨，画布全宽） -->
      <aside
        v-show="!immersive"
        class="cg-entries"
        :class="{ collapsed: panelCollapsed }"
        :style="panelCollapsed ? undefined : { width: entriesWidth + 'px' }"
      >
        <!-- 折叠态：细轨只留展开按钮 -->
        <div v-if="panelCollapsed" class="cg-entries-rail">
          <button class="cg-rail-btn" title="展开入口面板" @click="togglePanel">
            <DoubleRightOutlined />
          </button>
        </div>

        <!-- 展开态 -->
        <template v-else>
          <div class="cg-entries-head">
            <span>入口（{{ entryCount }}）</span>
            <button class="cg-fold-btn" title="收起入口面板" @click="togglePanel">
              <DoubleLeftOutlined />
            </button>
          </div>
          <a-input v-model:value="entrySearch" placeholder="搜索 路径 / 函数 / 模块…" size="small" allow-clear />
          <a-select
            v-if="verbChips.length > 1"
            v-model:value="verbFilter"
            :options="verbOptions"
            size="small"
            style="width: 100%"
          />
          <div class="cg-entry-list">
            <div v-for="g in entryGroups" :key="g.key" class="cg-entry-groupwrap">
              <div class="cg-entry-group" :title="g.tooltip" @click="toggleGroup(g.key)">
                <CaretRightOutlined class="cg-group-caret" :class="{ open: expandedGroups.has(g.key) }" />
                <span class="cg-group-label">{{ g.label }}</span>
                <span class="cg-group-count">{{ g.items.length }}</span>
              </div>
              <div v-show="expandedGroups.has(g.key)" class="cg-entry-children">
                <div
                  v-for="it in g.items"
                  :key="it.node.id"
                  class="cg-entry-item"
                  :class="{ active: it.node.id === selectedId }"
                  :title="`${it.node.qualname}\n${it.node.routes[0] ?? '(约定入口)'}\n${it.node.file}`"
                  @click="focusFromEntry(it.node.id)"
                >
                  <span v-if="it.verb" class="cg-verb" :class="verbClass(it.verb)">{{ it.verb }}</span>
                  <span class="cg-entry-path">{{ it.label }}</span>
                </div>
              </div>
            </div>
            <div v-if="!entryGroups.length" class="cg-entry-empty">
              {{ data ? (entrySearch || verbFilter ? "无匹配入口" : "无入口（框架路由自动识别）") : "" }}
            </div>
          </div>
        </template>
      </aside>

      <!-- 左夹片：拖拽调入口栏宽，双击复位 -->
      <div
        v-show="!immersive && !panelCollapsed"
        class="cg-resizer"
        :class="{ active: resizing === 'entries' }"
        title="拖拽调整宽度 · 双击复位"
        @mousedown="startResize('entries', $event)"
        @dblclick="resetResize('entries')"
      ></div>

      <!-- 画布 -->
      <div ref="canvasBox" class="cg-canvas">
        <!-- 沉浸态浮动退出按钮 -->
        <button v-if="immersive" class="cg-exit-fs" title="退出全屏（Esc）" @click="exitImmersive">
          <ShrinkOutlined /> 退出全屏
        </button>
        <div ref="canvasEl" class="cg-canvas-inner" />

        <!-- 右键上下文菜单 -->
        <div
          v-if="ctxMenu.show"
          class="cg-ctxmenu"
          :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
        >
          <template v-if="ctxMenu.node">
            <div class="cg-ctx-head" :title="ctxMenu.node.id">{{ ctxMenu.node.qualname }}</div>
            <div class="cg-ctx-item" @click="ctxAction(() => openSource(ctxMenu.node!))">
              <CodeOutlined /> 打开源码
            </div>
            <div class="cg-ctx-item" @click="ctxAction(() => focusChain(ctxMenu.node!.id))">
              <AimOutlined /> 聚焦上下游链路
            </div>
            <div class="cg-ctx-item" @click="ctxAction(focusDownstreamCtx)">
              <ClusterOutlined /> 只看下游执行流
            </div>
            <div class="cg-ctx-sep" />
            <div class="cg-ctx-item" @click="ctxAction(() => copyText(ctxMenu.node!.id, '已复制限定名'))">
              <CopyOutlined /> 复制限定名
            </div>
            <div class="cg-ctx-item" @click="ctxAction(() => copyText(ctxMenu.node!.qualname, '已复制函数名'))">
              <CopyOutlined /> 复制函数名
            </div>
          </template>
          <template v-else>
            <div class="cg-ctx-item" @click="ctxAction(resetView)"><ClearOutlined /> 重置视图</div>
            <div class="cg-ctx-item" @click="ctxAction(toggleCanvasFullscreen)">
              <ExpandOutlined /> 全屏画布
            </div>
          </template>
        </div>
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

      <!-- 右夹片：拖拽调详情栏宽，双击复位 -->
      <div
        v-show="!immersive"
        class="cg-resizer"
        :class="{ active: resizing === 'detail' }"
        title="拖拽调整宽度 · 双击复位"
        @mousedown="startResize('detail', $event)"
        @dblclick="resetResize('detail')"
      ></div>

      <!-- 详情侧栏（沉浸态隐藏） -->
      <aside v-show="!immersive" class="cg-detail" :style="{ width: detailWidth + 'px' }">
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
      :title="`${sourceState.qualname} — ${sourceState.file}`"
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
/* 宽度夹片：拖拽调宽，双击复位 */
.cg-resizer {
  position: relative;
  width: 8px;
  margin: 0 -5px;
  flex-shrink: 0;
  cursor: col-resize;
  z-index: 3;
}
.cg-resizer::after {
  content: "";
  position: absolute;
  top: 8%;
  bottom: 8%;
  left: 50%;
  width: 2px;
  transform: translateX(-50%);
  border-radius: 1px;
  background: transparent;
  transition: background var(--transition-fast);
}
.cg-resizer:hover::after {
  background: var(--color-primary-border);
}
.cg-resizer.active::after {
  background: var(--color-primary);
}
/* 拖拽中：全区域统一光标 + 禁止选中文本；宽度过渡停用避免拖拽跟手延迟 */
.cg-panel.resizing,
.cg-panel.resizing * {
  cursor: col-resize !important;
  user-select: none;
}
.cg-panel:not(.resizing) .cg-entries {
  transition: width var(--transition-normal);
}
/* 入口面板（宽度由内联样式驱动：拖拽/折叠；可拖宽/可折叠） */
.cg-entries {
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;
}
.cg-entries.collapsed {
  width: 36px;
  padding: 8px 4px;
  align-items: center;
}
.cg-entries-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-secondary);
}
.cg-fold-btn,
.cg-rail-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.cg-fold-btn:hover,
.cg-rail-btn:hover {
  background: var(--bg-elevated);
  color: var(--color-primary);
}
/* 折叠细轨（仅图标） */
.cg-entries-rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  flex: 1;
}
.cg-rail-btn {
  width: 28px;
  height: 28px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-md);
}
/* 分组列表 */
.cg-entry-list {
  flex: 1;
  overflow-y: auto;
  margin: 0 -4px;
  padding: 0 4px;
}
.cg-entry-groupwrap {
  margin-bottom: 2px;
}
.cg-entry-group {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 4px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  user-select: none;
  transition: all var(--transition-fast);
}
.cg-entry-group:hover {
  color: var(--text-secondary);
}
.cg-group-caret {
  font-size: 10px;
  transition: transform var(--transition-fast);
  opacity: 0.7;
}
.cg-group-caret.open {
  transform: rotate(90deg);
}
.cg-group-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cg-group-count {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-disabled);
  flex-shrink: 0;
}
/* 子项：文件树式左发丝线，结构感来自层级而非色块 */
.cg-entry-children {
  margin-left: 9px;
  padding-left: 8px;
  border-left: 1px solid var(--border-light);
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.cg-entry-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.cg-entry-item:hover {
  background: var(--bg-elevated);
}
.cg-entry-item.active {
  background: var(--color-primary-bg);
}
.cg-entry-item.active .cg-entry-path {
  color: var(--color-primary);
  font-weight: 600;
}
/* 动词：纯色文字（无底无框），低饱和双主题可读 */
.cg-verb {
  flex-shrink: 0;
  min-width: 48px;
  font-size: 11px;
  font-weight: 600;
  font-family: "JetBrains Mono", monospace;
}
.m-get { color: #3d9a6f; }
.m-post { color: #4b8fd6; }
.m-put { color: #bd8a1e; }
.m-delete { color: #d24a4e; }
.m-patch { color: #8f74e0; }
.m-head,
.m-options { color: #3a9fae; }
.m-any,
.m-gf { color: #75839f; }
.m-none { display: none; }
.cg-entry-path {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cg-entry-empty {
  font-size: 12.5px;
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
/* 原生全屏态：占满整屏、去边框圆角 */
.cg-canvas:fullscreen {
  border: none;
  border-radius: 0;
  background: var(--bg-base);
}
/* 右键上下文菜单 */
.cg-ctxmenu {
  position: absolute;
  z-index: 20;
  min-width: 200px;
  max-width: 260px;
  padding: 4px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-base);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
}
.cg-ctx-head {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  font-family: "JetBrains Mono", monospace;
  padding: 6px 10px 8px;
  border-bottom: 1px solid var(--border-light);
  margin-bottom: 4px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.cg-ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  font-size: 13px;
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
}
.cg-ctx-item .anticon {
  font-size: 13px;
  color: var(--text-tertiary);
}
.cg-ctx-item:hover {
  background: var(--bg-input);
  color: var(--color-primary);
}
.cg-ctx-item:hover .anticon {
  color: var(--color-primary);
}
.cg-ctx-sep {
  height: 1px;
  background: var(--border-light);
  margin: 4px 6px;
}

/* 沉浸态浮动退出按钮（半透明贴角，hover 实心） */
.cg-exit-fs {
  position: absolute;
  top: 12px;
  right: 12px;
  z-index: 10;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border: 1px solid var(--border-base);
  background: var(--bg-surface);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  cursor: pointer;
  font-size: 12.5px;
  opacity: 0.55;
  transition: all var(--transition-fast);
}
.cg-exit-fs:hover {
  opacity: 1;
  color: var(--color-primary);
  border-color: var(--color-primary-border);
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

/* 详情侧栏（宽度由内联样式驱动：拖拽） */
.cg-detail {
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
/* 函数区间高亮（Monaco 装饰类是编辑器内部 DOM，经 :deep 命中） */
:deep(.cg-src-highlight) {
  background: color-mix(in oklch, var(--color-primary) 13%, transparent);
}
:deep(.cg-src-line-mark) {
  margin-left: 3px;
  border-left: 3px solid var(--color-primary);
}
</style>
