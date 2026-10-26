/**
 * 调用链画布确定性布局（路线图阶段 A，docs/design/callgraph-visualization-roadmap.md）
 *
 * - tree（架构分层）：按角色分层——入口 → 导出 → 其他；层内按 模块→文件→行号
 *   排序摊开、超宽换行。模块占连续列段，模块色一望成「色带」。
 * - circles（同心环）：同角色三环，环内同 角色/模块→文件→行号 顺序铺角度——
 *   同模块成连续扇区；环半径随节点数外扩（周长满足最小弧距）且不侵入内环。
 *
 * 语义映射对照 GitNexus 的 TYPE_TO_LAYER/环锚定（gitnexus-web/src/lib/
 * tree-layout.ts、circles-layout.ts）：它们按节点**类型**（容器/文件/类型/函数）
 * 分层，我们的图节点都是函数，类型维度退化为**角色**（entry/exported/other）。
 * 布局只产出坐标（Map<id,{x,y}>），画布用 preset 位置渲染——与渲染引擎解耦，
 * 阶段 C 迁移 Sigma 时可整组搬运。
 */
import type { CallGraphNode } from "@/api/callchain";

export type LayoutMode = "flow" | "tree" | "circles";

export interface NodePos {
  x: number;
  y: number;
}

/** 角色层：0 入口 → 1 导出 → 2 其他 */
function roleOf(n: CallGraphNode): 0 | 1 | 2 {
  return n.is_entry ? 0 : n.is_exported ? 1 : 2;
}

/** 统一排序键：模块 → 文件 → 行号（两种布局共用，保证同模块/文件聚簇） */
function sortedByGroup(nodes: CallGraphNode[]): CallGraphNode[] {
  return [...nodes].sort(
    (a, b) =>
      a.module.localeCompare(b.module) ||
      a.file.localeCompare(b.file) ||
      a.lineno - b.lineno ||
      a.id.localeCompare(b.id),
  );
}

// ===== tree：角色分层 + 层内分组换行 =====

const TREE_COL_W = 130;
const TREE_ROW_H = 70;
/** 相邻角色层的最小垂直间距（层内换行时按行数实际延伸） */
const TREE_LAYER_GAP = 220;
/** 每行节点数上限（超宽换行，行距正常 ROW_H） */
const TREE_WRAP = 36;

export function treeLayout(nodes: CallGraphNode[]): Map<string, NodePos> {
  const out = new Map<string, NodePos>();
  const layers: CallGraphNode[][] = [[], [], []];
  for (const n of nodes) layers[roleOf(n)].push(n);
  let y = 0;
  for (const layer of layers) {
    const ordered = sortedByGroup(layer);
    if (!ordered.length) {
      y += TREE_LAYER_GAP;
      continue;
    }
    const rows = Math.ceil(ordered.length / TREE_WRAP);
    ordered.forEach((n, i) => {
      out.set(n.id, {
        x: (i % TREE_WRAP) * TREE_COL_W,
        y: y + Math.floor(i / TREE_WRAP) * TREE_ROW_H,
      });
    });
    y += rows * TREE_ROW_H + TREE_LAYER_GAP;
  }
  return out;
}

// ===== circles：角色三环 + 同模块连续扇区 =====

const CIRCLES_MIN_R = [220, 420, 660];
/** 每节点最小弧长（px）——环随节点数外扩 */
const CIRCLES_ARC = 26;
/** 环间最小净距 */
const CIRCLES_RING_GAP = 120;

export function circlesLayout(nodes: CallGraphNode[]): Map<string, NodePos> {
  const out = new Map<string, NodePos>();
  const rings: CallGraphNode[][] = [[], [], []];
  for (const n of nodes) rings[roleOf(n)].push(n);
  let prevR = 0;
  for (const [ri, ring] of rings.entries()) {
    const ordered = sortedByGroup(ring);
    if (!ordered.length) continue;
    const needed = (ordered.length * CIRCLES_ARC) / (2 * Math.PI);
    const r = Math.max(CIRCLES_MIN_R[ri], needed, prevR + CIRCLES_RING_GAP);
    prevR = r;
    ordered.forEach((n, i) => {
      // 起点正上方（-90°）顺时针铺：同模块占连续扇区
      const angle = -Math.PI / 2 + (2 * Math.PI * i) / ordered.length;
      out.set(n.id, { x: Math.cos(angle) * r, y: Math.sin(angle) * r });
    });
  }
  return out;
}

/** 布局入口：flow 返回 null（走画布原有的有向分层冻结管线） */
export function computeLayout(
  mode: LayoutMode,
  nodes: CallGraphNode[],
): Map<string, NodePos> | null {
  if (mode === "tree") return treeLayout(nodes);
  if (mode === "circles") return circlesLayout(nodes);
  return null;
}
