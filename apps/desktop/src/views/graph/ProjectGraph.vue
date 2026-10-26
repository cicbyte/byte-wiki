<script setup lang="ts">
/**
 * 项目图谱展示页（/repo/:id/graph）
 *
 * 为「展示酷炫效果」而生的独立页面：全幅画布 + 极简悬浮件，与调用链工作台
 * （筛选/详情/生成）分离——工具页求全，本页求美。效果四件套：
 *   1) 入场演出：节点按角色（入口→导出→其他）分波渐现 + 镜头从中心拉到全景；
 *   2) 默认同心环布局（graph-layouts，模块色成扇区）；
 *   3) hover 聚光灯：悬停节点邻域提亮、其余压暗；
 *   4) 点击脉冲 + 镜头飞行 + 悬浮卡片（不进完整详情栏）。
 * 数据复用调用链产物：小图整图（大图禁整包过 IPC 的既有约定），大图自动
 * 聚焦首入口子图。布局数学与画布页共享 graph-layouts（换渲染引擎可整组搬运）。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  ArrowLeftOutlined,
  ClusterOutlined,
  ApartmentOutlined,
  PieChartOutlined,
  NodeIndexOutlined,
} from "@ant-design/icons-vue";
import { DataSet, Network } from "vis-network/standalone";
import { api } from "@/api";
import { callchainApi } from "@/api/callchain";
import type { CallgraphData, CallgraphEdge, CallGraphNode, CallgraphOverview } from "@/api/callchain";
import { computeLayout, type LayoutMode } from "@/views/wiki/components/graph-layouts";
import EmptyState from "@/components/common/EmptyState.vue";
import { useWikiStore } from "@/stores/wiki";

const route = useRoute();
const router = useRouter();
const wiki = useWikiStore();

// ===== 状态 =====
const loading = ref(true);
const missing = ref(false);
const data = ref<CallgraphData | null>(null);
const overview = ref<CallgraphOverview | null>(null);
const bigMode = ref(false);
const layoutMode = ref<LayoutMode>("circles");
const layoutPopOpen = ref(false);
const repoName = ref("");

const canvasEl = ref<HTMLElement>();
/** 点击悬浮卡片（展示页不做完整详情栏） */
const card = ref<{ node: CallGraphNode; x: number; y: number } | null>(null);

const stats = computed(() => {
  if (bigMode.value && overview.value) {
    return {
      nodes: overview.value.total_nodes,
      edges: overview.value.total_edges,
      entries: overview.value.total_entries,
      modules: overview.value.modules.length,
    };
  }
  const d = data.value;
  return {
    nodes: d?.nodes.length ?? 0,
    edges: d?.edges.length ?? 0,
    entries: d?.nodes.filter((n) => n.is_entry).length ?? 0,
    modules: d ? new Set(d.nodes.map((n) => n.module)).size : 0,
  };
});

const layoutLabel = computed(() =>
  layoutMode.value === "circles" ? "同心环" : layoutMode.value === "tree" ? "架构分层" : "有向分层",
);

// ===== 画布 =====
const PALETTE = [
  "#5aa2ff", "#54d08c", "#f0b429", "#ef6f6c", "#b58cff", "#4ec9d4",
  "#f78fb3", "#9ad64f", "#ff9f6b", "#6fa8ff", "#c9a1ff", "#5fd0a8",
  "#f6d365", "#7dd8e8",
];

let network: Network | null = null;
let vnodes: DataSet<Record<string, unknown>> | null = null;
let vedges: DataSet<Record<string, unknown>> | null = null;
let nodeById = new Map<string, CallGraphNode>();
let outEdges = new Map<string, CallgraphEdge[]>();
let inEdges = new Map<string, CallgraphEdge[]>();
/** 入场演出进行中（期间禁用聚光灯，避免和渐现打架） */
let entering = false;
let enterRaf = 0;
let pulseRaf = 0;

function reducedMotion(): boolean {
  return !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

function roleOf(n: CallGraphNode): 0 | 1 | 2 {
  return n.is_entry ? 0 : n.is_exported ? 1 : 2;
}

function buildGraph() {
  const d = data.value;
  if (!d || !canvasEl.value) return;
  nodeById = new Map(d.nodes.map((n) => [n.id, n]));
  outEdges = new Map();
  inEdges = new Map();
  for (const e of d.edges) {
    (outEdges.get(e.src) ?? outEdges.set(e.src, []).get(e.src)!).push(e);
    (inEdges.get(e.dst) ?? inEdges.set(e.dst, []).get(e.dst)!).push(e);
  }
  const mods = Array.from(new Set(d.nodes.map((n) => n.module))).sort();
  const modColor = new Map(mods.map((m, i) => [m, PALETTE[i % PALETTE.length]]));

  const skipEntrance = reducedMotion();
  const nodesArr = d.nodes.map((n) => ({
    id: n.id,
    label: n.qualname.length > 30 ? n.qualname.slice(0, 29) + "…" : n.qualname,
    title: `${n.module}\n${n.file}:${n.lineno}`,
    color: {
      background: modColor.get(n.module),
      border: n.is_entry ? "#e8a33d" : modColor.get(n.module),
      highlight: { background: modColor.get(n.module), border: "#5aa2ff" },
    },
    shape: n.is_entry ? "diamond" : n.routes.length ? "box" : "ellipse",
    font: { color: "#e4e4ed", size: 13 },
    borderWidth: n.is_entry ? 2 : 1,
    borderWidthSelected: 4,
    opacity: skipEntrance ? 1 : 0,
  }));
  const edgesArr = d.edges.map((e, i) => ({
    id: `e${i}`,
    from: e.src,
    to: e.dst,
    arrows: { to: { scaleFactor: 0.45 } },
    dashes: e.kind === "heuristic" || e.kind === "event",
    color: {
      color: e.kind === "event" ? "#9d8bd8aa" : "#8b93a7aa",
      highlight: "#e8a33d",
      hover: "#5aa2ff",
    },
    smooth: { enabled: true, type: "cubicBezier", roundness: 0.5 },
  }));

  vnodes = new DataSet(nodesArr);
  vedges = new DataSet(edgesArr);
  network?.destroy();
  network = new Network(
    canvasEl.value,
    { nodes: vnodes, edges: vedges },
    {
      layout: { hierarchical: { enabled: false } },
      physics: { enabled: false },
      interaction: { hover: true, tooltipDelay: 120 },
      edges: { smooth: { enabled: true, type: "cubicBezier", roundness: 0.5 } },
    },
  );
  attachEvents(network);
  applyPositions();
  if (!skipEntrance) entrance();
}

/** 确定性布局坐标灌入（flow 模式退化为按角色分层——展示页默认 circles） */
function applyPositions() {
  const d = data.value;
  if (!d || !vnodes || !network) return;
  const mode: LayoutMode = layoutMode.value === "flow" ? "tree" : layoutMode.value;
  const pos = computeLayout(mode, d.nodes);
  if (pos) {
    vnodes.update([...pos].map(([id, p]) => ({ id, x: p.x, y: p.y })));
    network.fit({ animation: false });
  }
}

/** 入场演出：角色分波渐现（入口→导出→其他，波间隔 220ms）+ 镜头拉全景 */
function entrance() {
  const d = data.value;
  if (!d || !vnodes || !network) return;
  entering = true;
  // 镜头：从图心 2.4 倍近景拉到全景
  let cx = 0;
  let cy = 0;
  const pos = network.getPositions();
  const vals = Object.values(pos);
  if (vals.length) {
    cx = vals.reduce((s, p) => s + p.x, 0) / vals.length;
    cy = vals.reduce((s, p) => s + p.y, 0) / vals.length;
  }
  network.moveTo({ position: { x: cx, y: cy }, scale: 2.4, animation: false });
  network.fit({ animation: { duration: 1000, easingFunction: "easeOutCubic" } });

  // 超大图逐帧渐现会掉帧：退化为三波瞬开
  if (d.nodes.length > 2500) {
    const waves = [0, 1, 2].map((r) => d.nodes.filter((n) => roleOf(n) === r));
    waves.forEach((group, i) => {
      setTimeout(() => {
        vnodes!.update(group.map((n) => ({ id: n.id, opacity: 1 })));
        if (i === 2) entering = false;
      }, 150 + i * 220);
    });
    return;
  }

  const delay = new Map(d.nodes.map((n) => [n.id, roleOf(n) * 220]));
  const FADE = 480;
  const total = 440 + FADE;
  const t0 = performance.now();
  const step = () => {
    const t = performance.now() - t0;
    vnodes!.update(
      d.nodes.map((n) => {
        const p = Math.min(1, Math.max(0, (t - (delay.get(n.id) ?? 0)) / FADE));
        return { id: n.id, opacity: p };
      }),
    );
    if (t < total) {
      enterRaf = requestAnimationFrame(step);
    } else {
      entering = false;
      enterRaf = 0;
    }
  };
  enterRaf = requestAnimationFrame(step);
}

/** hover 聚光灯：1 跳邻域提亮、其余压暗（入场演出期间禁用） */
function spotlight(id: string | null) {
  if (!vnodes || !data.value || entering) return;
  if (!id) {
    vnodes.update(data.value.nodes.map((n) => ({ id: n.id, opacity: 1 })));
    return;
  }
  const near = new Set<string>([id]);
  for (const e of outEdges.get(id) ?? []) near.add(e.dst);
  for (const e of inEdges.get(id) ?? []) near.add(e.src);
  vnodes.update(data.value.nodes.map((n) => ({ id: n.id, opacity: near.has(n.id) ? 1 : 0.12 })));
}

function baseBorder(id: string): number {
  return nodeById.get(id)?.is_entry ? 2 : 1;
}

/** 选中脉冲：1 跳邻域边框呼吸一圈（减动效/大图自动关） */
function pulseAround(id: string) {
  if (pulseRaf) cancelAnimationFrame(pulseRaf);
  if (!vnodes || !data.value) return;
  if (data.value.nodes.length > 3000 || reducedMotion()) return;
  const ring = new Set<string>([id]);
  for (const e of outEdges.get(id) ?? []) ring.add(e.dst);
  for (const e of inEdges.get(id) ?? []) ring.add(e.src);
  const ids = [...ring];
  const t0 = performance.now();
  const DUR = 900;
  const step = () => {
    const t = (performance.now() - t0) / DUR;
    const amp = t >= 1 ? 0 : Math.sin(Math.PI * t) * 3;
    vnodes!.update(ids.map((nid) => ({ id: nid, borderWidth: baseBorder(nid) + amp })));
    pulseRaf = t >= 1 ? 0 : requestAnimationFrame(step);
  };
  pulseRaf = requestAnimationFrame(step);
}

function attachEvents(net: Network) {
  net.on("hoverNode", (params: any) => spotlight(String(params.node)));
  net.on("blurNode", () => spotlight(null));
  net.on("click", (params: any) => {
    if (params.nodes.length) {
      const id = String(params.nodes[0]);
      // 镜头飞行 + 脉冲 + 悬浮卡片
      network?.focus(id, {
        scale: 1.15,
        animation: reducedMotion() ? false : { duration: 500, easingFunction: "easeInOutQuad" },
      });
      pulseAround(id);
      const dom = network?.canvasToDOM(network.getPositions([id])[id] ?? { x: 0, y: 0 });
      const node = nodeById.get(id);
      if (node && dom) {
        card.value = { node, x: Math.min(dom.x + 18, (canvasEl.value?.clientWidth ?? 800) - 280), y: Math.max(12, dom.y - 12) };
      }
    } else {
      card.value = null;
    }
  });
  net.on("dragStart", () => {
    card.value = null;
  });
}

function closeCard() {
  card.value = null;
}

function openInCallgraph() {
  if (!card.value) return;
  router.push({ path: `/repo/${route.params.id}/callgraph`, query: { node: card.value.node.id } });
}

function setLayoutMode(mode: LayoutMode) {
  layoutPopOpen.value = false;
  layoutMode.value = mode;
  card.value = null;
  if (enterRaf) cancelAnimationFrame(enterRaf);
  entering = false;
  buildGraph();
}

// ===== 数据加载（与调用链画布同口径：小图整图 / 大图聚焦首入口） =====
const BIG_GRAPH_THRESHOLD = 4000;

async function loadData() {
  loading.value = true;
  missing.value = false;
  try {
    const info = await wiki.ensureRepo(Number(route.params.id));
    if (!info) {
      missing.value = true;
      return;
    }
    repoName.value = wiki.currentRepo?.name ?? info.name ?? "项目";
    const ov = await callchainApi.overview(info.path, wiki.viewBranch || null);
    if (ov && ov.total_nodes > BIG_GRAPH_THRESHOLD) {
      bigMode.value = true;
      overview.value = ov;
      const first = ov.entries[0];
      if (!first) {
        missing.value = true;
        return;
      }
      const sub = await callchainApi.focus(info.path, first.id, 2, 400, wiki.viewBranch || null);
      if (!sub) {
        missing.value = true;
        return;
      }
      data.value = { nodes: sub.nodes, edges: sub.edges, root: "", langs: {}, modules: {} } as CallgraphData;
    } else {
      bigMode.value = false;
      overview.value = ov;
      const text = await api.callchainRead(info.path);
      data.value = JSON.parse(text) as CallgraphData;
    }
    await nextTick();
    buildGraph();
  } catch {
    missing.value = true;
  } finally {
    loading.value = false;
  }
}

function goBack() {
  router.push(`/repo/${route.params.id}`);
}

onMounted(loadData);
onBeforeUnmount(() => {
  if (enterRaf) cancelAnimationFrame(enterRaf);
  if (pulseRaf) cancelAnimationFrame(pulseRaf);
  network?.destroy();
});
</script>

<template>
  <div class="pg-page">
    <!-- 顶部信息条（展示页唯一的常驻 chrome） -->
    <header class="pg-topbar">
      <button class="pg-back" @click="goBack"><ArrowLeftOutlined /> 返回</button>
      <div class="pg-title">{{ repoName }} · 项目图谱</div>
      <div v-if="!missing && !loading" class="pg-stats">
        <span class="pg-stat"><b>{{ stats.nodes }}</b> 函数</span>
        <span class="pg-stat"><b>{{ stats.edges }}</b> 调用边</span>
        <span class="pg-stat"><b>{{ stats.entries }}</b> 入口</span>
        <span class="pg-stat"><b>{{ stats.modules }}</b> 模块</span>
        <span v-if="bigMode" class="pg-stat dim">大图聚焦首入口</span>
      </div>
      <div class="pg-actions">
        <a-popover v-model:open="layoutPopOpen" trigger="click" placement="bottomRight" overlay-class-name="bw-layout-pop">
          <template #content>
            <div class="bw-layout-list">
              <button class="bw-layout-row" :class="{ on: layoutMode === 'circles' }" @click="setLayoutMode('circles')">
                <PieChartOutlined /> 同心环（默认）
              </button>
              <button class="bw-layout-row" :class="{ on: layoutMode === 'tree' }" @click="setLayoutMode('tree')">
                <ApartmentOutlined /> 架构分层
              </button>
              <button class="bw-layout-row" :class="{ on: layoutMode === 'flow' }" @click="setLayoutMode('flow')">
                <NodeIndexOutlined /> 有向分层
              </button>
            </div>
          </template>
          <button class="pg-action" :title="`布局：${layoutLabel}`"><ClusterOutlined /></button>
        </a-popover>
      </div>
    </header>

    <div v-if="loading" class="pg-state"><EmptyState text="构建项目图谱…" /></div>
    <div v-else-if="missing" class="pg-state">
      <EmptyState text="暂无项目图谱" hint="请先在调用链页生成函数级调用图" />
      <button class="btn primary" @click="router.push(`/repo/${route.params.id}/callgraph`)">去调用链页</button>
    </div>

    <template v-else>
      <div ref="canvasEl" class="pg-canvas" @click.self="closeCard"></div>

      <!-- 点击悬浮卡片（扁平：边框分层，无阴影） -->
      <div v-if="card" class="pg-card" :style="{ left: card.x + 'px', top: card.y + 'px' }">
        <div class="pg-card-head">
          <span class="pg-card-name" :title="card.node.id">{{ card.node.qualname }}</span>
          <button class="pg-card-close" @click="closeCard">×</button>
        </div>
        <div class="pg-card-loc">{{ card.node.file }}:{{ card.node.lineno }}-{{ card.node.end_lineno }}</div>
        <div class="pg-card-meta">
          <span>{{ card.node.module }}</span>
          <span>入 {{ card.node.callers }} / 出 {{ card.node.callees }}</span>
          <span v-if="card.node.is_entry" class="pg-card-entry">入口</span>
        </div>
        <div v-if="card.node.routes.length" class="pg-card-routes">
          <span v-for="r in card.node.routes" :key="r" class="pg-card-route">{{ r }}</span>
        </div>
        <button class="btn sm" @click="openInCallgraph">在调用链中查看 →</button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.pg-page {
  position: relative;
  height: 100vh;
  overflow: hidden;
  /* 展示页背景：极淡径向渐变让图心聚焦（扁平、无装饰） */
  background:
    radial-gradient(1200px 700px at 50% 45%, color-mix(in srgb, var(--color-primary) 5%, transparent), transparent),
    var(--bg-base);
}
.pg-topbar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 18px;
  pointer-events: none;
}
.pg-topbar > * {
  pointer-events: auto;
}
.pg-back {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--border-base);
  background: color-mix(in srgb, var(--bg-surface) 82%, transparent);
  color: var(--text-secondary);
  font-size: 12.5px;
  border-radius: 999px;
  padding: 4px 12px;
  cursor: pointer;
  transition: color var(--transition-fast), border-color var(--transition-fast);
}
.pg-back:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}
.pg-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.pg-stats {
  display: flex;
  align-items: baseline;
  gap: 14px;
  margin-left: 8px;
}
.pg-stat {
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.pg-stat b {
  font-size: 17px;
  font-weight: 700;
  color: var(--text-primary);
  margin-right: 3px;
  font-family: "JetBrains Mono", monospace;
}
.pg-stat.dim b {
  font-size: 12px;
}
.pg-actions {
  margin-left: auto;
  display: flex;
  gap: 8px;
}
.pg-action {
  border: 1px solid var(--border-base);
  background: color-mix(in srgb, var(--bg-surface) 82%, transparent);
  color: var(--text-secondary);
  width: 32px;
  height: 32px;
  border-radius: 999px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: color var(--transition-fast), border-color var(--transition-fast);
}
.pg-action:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}
.pg-canvas {
  position: absolute;
  inset: 0;
}
.pg-state {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}
/* 悬浮卡片（扁平：1px 边框分层，无阴影——全局审美约定） */
.pg-card {
  position: absolute;
  z-index: 12;
  width: 260px;
  background: var(--bg-surface);
  border: 1px solid var(--border-base);
  border-radius: var(--radius-md);
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.pg-card-head {
  display: flex;
  align-items: center;
  gap: 6px;
}
.pg-card-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
.pg-card-close {
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 15px;
  cursor: pointer;
  line-height: 1;
  padding: 2px 4px;
}
.pg-card-close:hover {
  color: var(--text-primary);
}
.pg-card-loc {
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pg-card-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--text-secondary);
}
.pg-card-entry {
  color: #e8a33d;
  border: 1px solid color-mix(in srgb, #e8a33d 50%, transparent);
  border-radius: var(--radius-xs);
  padding: 0 5px;
  font-size: 10.5px;
}
.pg-card-routes {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.pg-card-route {
  font-family: "JetBrains Mono", monospace;
  font-size: 10.5px;
  color: #3a9fae;
  background: rgba(125, 216, 232, 0.12);
  border: 1px solid rgba(125, 216, 232, 0.45);
  border-radius: var(--radius-xs);
  padding: 0 5px;
}
</style>
