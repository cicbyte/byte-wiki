<script setup lang="ts">
/**
 * Wiki 导航树：总览 / 文档（页面按章节路径嵌套）
 *
 * 生成进行中切换为「骨架实时树」：大纲一确定即按目标路径渲染章节树，
 * pending 置灰、generating 高亮脉动（当前页一眼可见）、done 打勾、
 * failed 标红；点击任意页面即可（生成中）实时查看 /（完成后）阅读。
 * 只放文档类内容：知识卡有独立 tab、调用链图谱有独立面板、
 * 依赖图在新版总览页内以 Mermaid 呈现，不进文档树。
 * 旧版模块文档（kind=module）作为兼容分组兜底展示。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  ApartmentOutlined,
  AppstoreOutlined,
  FileTextOutlined,
  FolderOutlined,
  CaretRightOutlined,
  LoadingOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  SyncOutlined,
  MoreOutlined,
  EyeOutlined,
  RollbackOutlined,
  ThunderboltOutlined,
  ClusterOutlined,
} from "@ant-design/icons-vue";
import type { WikiDocEntry, PageProgress } from "@/api/generate";

const props = defineProps<{
  docs: WikiDocEntry[];
  current: string;
  /** 大纲既定顺序的页面路径（磁盘树排序依据；空则回退字母序） */
  order?: string[];
  /** 生成中的页面骨架（大纲确定后非空；其顺序即大纲序） */
  genPages?: PageProgress[];
  generating?: boolean;
  /** 生成早期（索引/大纲设计）树内占位提示 */
  genHint?: string;
  /** 滞后变更数（>0 显示红点徽标与更新按钮） */
  staleCount?: number;
  /** 当前 git 分支（非空时树头部展示） */
  branch?: string;
  /** 只读查看分支（null=当前分支；非当前分支=只读模式） */
  viewBranch?: string | null;
  /** 可查看分支选项（branches/ 下有产物的目录） */
  branchOptions?: string[];
  /** 操作禁用（生成/更新/重建进行中） */
  actionsDisabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "select", path: string): void;
  (e: "action", kind: "update" | "regenerate" | "rebuild"): void;
  (e: "switchBranch", branch: string): void;
}>();

interface VisibleNode {
  label: string;
  path: string; // dir 节点为合成 key
  isDir: boolean;
  depth: number;
  status?: string;
  importance?: string;
}

/** 目录折叠（默认全展开，记录用户折叠项；骨架树与磁盘树共用） */
const collapsed = ref(new Set<string>());

/**
 * 由路径集合构建目录层级树（磁盘文档与生成骨架两个入口共用）。
 * 排序：优先按大纲既定顺序（order / genPages 顺序）——章节与页面的
 * 阅读顺序是 LLM 大纲的设计意图，按码点排序对中文近似乱序；
 * 无顺序信息时回退 localeCompare。
 */
function buildNodes(paths: string[], kind: "docs" | "skeleton"): VisibleNode[] {
  if (!paths.length) return [];
  const withStatus = (path: string): { status?: string; importance?: string } => {
    if (kind !== "skeleton") return {};
    const p = (props.genPages || []).find((g) => g.path === path);
    return { status: p?.status, importance: p?.importance };
  };
  // 顺序索引：磁盘树取 order prop；骨架树取 genPages 顺序（= 大纲序）
  const orderList =
    kind === "skeleton"
      ? (props.genPages || []).map((g) => g.path).filter((p): p is string => !!p)
      : props.order || [];
  const orderIdx = new Map<string, number>();
  orderList.forEach((p, i) => orderIdx.set(p, i));
  /** 目录序 = 其下页面在大纲中的最小序 */
  const dirIdx = (dirPath: string): number => {
    let min = Infinity;
    for (const [p, i] of orderIdx) {
      if (p.startsWith(`${dirPath}/`) && i < min) min = i;
    }
    return min;
  };
  const out: VisibleNode[] = [];
  const walk = (prefix: string, depth: number) => {
    // 只处理本前缀子树：其它分支的路径若不跳过，会被从错误偏移切片
    // 当成本目录的文件，层层递归后节点数指数爆炸（曾把 38 页渲染成上万个节点）
    const pfx = `${prefix}/`;
    const dirs = new Map<string, true>();
    const files: string[] = [];
    for (const full of paths) {
      if (!full.startsWith(pfx)) continue;
      const rel = full.slice(pfx.length);
      const slash = rel.indexOf("/");
      if (slash < 0) {
        files.push(full);
      } else {
        dirs.set(rel.slice(0, slash), true);
      }
    }
    const byOrder = (a: string, b: string, ia?: number, ib?: number) =>
      (ia ?? Infinity) - (ib ?? Infinity) || a.localeCompare(b);
    const dirNames = [...dirs.keys()].sort((a, b) =>
      byOrder(a, b, dirIdx(`${prefix}/${a}`), dirIdx(`${prefix}/${b}`)),
    );
    for (const d of dirNames) {
      const dirPath = prefix ? `${prefix}/${d}` : d;
      out.push({ label: d, path: dirPath, isDir: true, depth });
      if (!collapsed.value.has(dirPath)) walk(dirPath, depth + 1);
    }
    for (const f of files.sort((a, b) => byOrder(a, b, orderIdx.get(a), orderIdx.get(b)))) {
      const name = f.split("/").pop() || f;
      const label = kind === "docs" ? docTitle(f) : name.replace(/\.md$/, "");
      out.push({ label, path: f, isDir: false, depth, ...withStatus(f) });
    }
  };
  walk("pages", 0);
  return out;
}

/** wiki_docs 标题（从清单取，取不到退回文件名） */
function docTitle(path: string): string {
  return props.docs.find(([p]) => p === path)?.[2] || path.split("/").pop() || path;
}

const arch = computed(() => props.docs.filter(([, kind]) => kind === "architecture"));
const modules = computed(() => props.docs.filter(([, kind]) => kind === "module")); // 旧版兼容
const diskPageEntries = computed(() => props.docs.filter(([, kind]) => kind === "page"));

/**
 * 骨架模式：骨架存在即显示（生成中实时状态；失败/中断后保留进度树——
 * done/failed 一目了然，已完成页仍可点击回看；成功后父组件清骨架切磁盘树）
 */
const skeletonMode = computed(() => (props.genPages?.length ?? 0) > 0);

const diskNodes = computed(() =>
  buildNodes(diskPageEntries.value.map(([p]) => p), "docs"),
);
const skeletonNodes = computed(() =>
  buildNodes((props.genPages || []).map((g) => g.path).filter((p): p is string => !!p) as string[], "skeleton"),
);

function toggleDir(path: string) {
  const next = new Set(collapsed.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  collapsed.value = next;
}

/** 只读查看模式（非当前分支：操作隐藏，仅查阅） */
const readonlyView = computed(
  () => !!props.viewBranch && !!props.branch && props.viewBranch !== props.branch,
);
/** 头部分支下拉选项（当前分支置顶带标记） */
const branchItems = computed(() => {
  const cur = props.branch || "";
  const opts = props.branchOptions || [];
  const rest = opts.filter((b) => b !== cur);
  return [
    { value: cur, label: cur ? `${cur}（当前）` : "当前分支", current: true },
    ...rest.map((b) => ({ value: b, label: b, current: false })),
  ];
});

const branchMenuOpen = ref(false);
const switchEl = ref<HTMLElement | null>(null);
function onDocClick(e: MouseEvent) {
  if (branchMenuOpen.value && switchEl.value && !switchEl.value.contains(e.target as Node)) {
    branchMenuOpen.value = false;
  }
}
onMounted(() => document.addEventListener("mousedown", onDocClick));
onUnmounted(() => document.removeEventListener("mousedown", onDocClick));

/** 空态直达：已生成且非当前的分支（一键只读查看） */
const switchableBranches = computed(() =>
  (props.branchOptions || []).filter((b) => b !== (props.branch || "")),
);

const doneCount = computed(
  () => (props.genPages || []).filter((p) => p.status === "done" || p.status === "failed").length,
);
</script>

<template>
  <nav class="wiki-tree">
    <!-- 树头部：状态 + 紧凑操作（原整行工具栏收敛于此） -->
    <div class="tree-bar">
      <div class="bar-left">
        <span class="bar-title">文档</span>
        <span v-if="skeletonMode" class="bar-count live">
          <LoadingOutlined class="spin inline" />{{ doneCount }}/{{ genPages!.length }}
        </span>
        <span v-else class="bar-count">{{ diskPageEntries.length }}</span>
        <span
          v-if="(staleCount ?? 0) > 0"
          class="stale-dot"
          :title="`「${branch ?? ''}」分支检测到 ${staleCount} 个文件变更，文档可能滞后`"
        >
          <span class="dot"></span>
        </span>
        <div
          v-if="branchItems.length > 1 || readonlyView"
          ref="switchEl"
          class="branch-switch"
          :class="{ viewing: readonlyView }"
          @mousedown.prevent
        >
          <button class="branch-switch-btn" @click.stop="branchMenuOpen = !branchMenuOpen">
            <span class="branch-switch-name">{{ viewBranch || branch }}</span>
            <CaretRightOutlined class="branch-caret" :class="{ open: branchMenuOpen }" />
          </button>
          <div v-if="branchMenuOpen" class="branch-menu">
            <button
              v-for="b in branchItems"
              :key="b.value"
              class="branch-menu-item"
              :class="{ active: (viewBranch || branch) === b.value }"
              @click="branchMenuOpen = false; emit('switchBranch', b.value)"
            >
              <CheckCircleOutlined v-if="b.current" class="cur-mark" />
              <span class="branch-menu-name">{{ b.value }}</span>
              <span v-if="b.current" class="cur-tag">当前</span>
            </button>
          </div>
        </div>
        <span v-else-if="branch" class="bar-branch" :title="`当前分支：${branch}（文档按分支独立）`">
          {{ branch }}
        </span>
      </div>
      <span v-if="!readonlyView" class="tree-bar-actions">
        <button
          v-if="(staleCount ?? 0) > 0"
          class="bar-btn"
          title="更新（仅重生成受影响页面）"
          :disabled="actionsDisabled"
          @click="emit('action', 'update')"
        >
          <SyncOutlined />
        </button>
        <a-dropdown placement="bottomRight" :trigger="['click']">
          <button class="bar-btn" title="更多操作" :disabled="actionsDisabled">
            <MoreOutlined />
          </button>
          <template #overlay>
            <a-menu @click="(i: any) => emit('action', i.key)">
              <a-menu-item key="regenerate" :disabled="actionsDisabled">
                <ThunderboltOutlined /> 重新生成 Wiki
              </a-menu-item>
              <a-menu-item key="rebuild" :disabled="actionsDisabled">
                <ClusterOutlined /> 重建向量索引
              </a-menu-item>
            </a-menu>
          </template>
        </a-dropdown>
      </span>
    </div>

    <!-- 只读查看提示：轻量行内（查看分支在头部下拉已示明，此处仅给动作） -->
    <div v-if="readonlyView" class="readonly-hint">
      <EyeOutlined class="rh-icon" />
      <span class="rh-text">只读视图 · 操作仅对 {{ branch }} 生效</span>
      <button class="rh-back" @click="emit('switchBranch', branch!)">
        <RollbackOutlined /> 回 {{ branch }}
      </button>
    </div>

    <!-- 生成早期：索引 / 大纲设计中（骨架未出） -->
    <div v-if="generating && !skeletonMode" class="gen-waiting">
      <LoadingOutlined class="spin" />
      <span>{{ genHint || "正在准备大纲…" }}</span>
    </div>

    <template v-else>
      <div v-if="(skeletonMode ? skeletonNodes : diskNodes).length || (!generating && arch.length)" class="tree-group">
        <ul class="group-list">
          <!-- 总览：固定首项（生成中隐藏，避免与骨架混淆） -->
          <li
            v-for="[path, , title] in (!generating ? arch : [])"
            :key="path"
            class="tree-item node"
            :class="{ active: current === path }"
            :title="path"
            @click="emit('select', path)"
          >
            <ApartmentOutlined class="node-icon" />
            <span class="node-label">{{ title }}</span>
          </li>
          <li
            v-for="n in skeletonMode ? skeletonNodes : diskNodes"
            :key="n.path"
            class="tree-item node"
            :class="{
              active: !n.isDir && current === n.path,
              'gen-pending': n.status === 'pending',
              'gen-active': n.status === 'generating',
              'gen-done': n.status === 'done',
              'gen-failed': n.status === 'failed',
            }"
            :style="{ paddingLeft: 10 + n.depth * 14 + 'px' }"
            :title="n.path"
            @click="n.isDir ? toggleDir(n.path) : emit('select', n.path)"
          >
            <CaretRightOutlined v-if="n.isDir" class="node-caret" :class="{ open: !collapsed.has(n.path) }" />
            <FolderOutlined v-if="n.isDir" class="node-icon dir" />
            <LoadingOutlined v-else-if="n.status === 'generating'" class="node-icon spin primary" />
            <CheckCircleOutlined v-else-if="n.status === 'done'" class="node-icon ok" />
            <CloseCircleOutlined v-else-if="n.status === 'failed'" class="node-icon bad" />
            <FileTextOutlined v-else class="node-icon" :class="{ dim: n.status === 'pending' }" />
            <span class="node-label">{{ n.label }}</span>
            <span v-if="n.importance === 'high'" class="imp">核心</span>
          </li>
        </ul>
      </div>

      <div v-if="skeletonMode" class="gen-tip">
        <template v-if="!generating">生成已中断 · 已完成页面可点击回看，重跑将命中缓存秒级续完</template>
        <template v-else-if="genPages!.every((p) => p.status === 'done' || p.status === 'failed')">
          页面已全部完成 · 正在收尾（组装写入 / 向量索引）
        </template>
        <template v-else>点击页面可实时查看生成内容</template>
      </div>

      <div v-if="!generating && modules.length" class="tree-group">
        <div class="group-head">
          <AppstoreOutlined />
          <span>模块（旧版）</span>
          <span class="group-count">{{ modules.length }}</span>
        </div>
        <ul class="group-list">
          <li
            v-for="[path, , title] in modules"
            :key="path"
            class="tree-item"
            :class="{ active: current === path }"
            :title="path"
            @click="emit('select', path)"
          >
            {{ title }}
          </li>
        </ul>
      </div>

      <!-- 空态：居中四行，全部围绕「暂无文档」 -->
      <div v-if="!generating && !diskNodes.length && !arch.length && !skeletonMode" class="empty-tree">
        <div class="et-title">暂无文档</div>
        <div v-if="branch" class="et-line">当前分支 <span class="et-mono">{{ branch }}</span></div>
        <div v-if="switchableBranches.length" class="et-line">
          查看
          <button v-for="b in switchableBranches" :key="b" class="et-link" @click="emit('switchBranch', b)">
            {{ b }}
          </button>
        </div>
      </div>
    </template>
  </nav>
</template>

<style scoped>
/* 树头部：状态徽标 + 紧凑图标操作 */
.tree-bar {
  /* 统一面板头：与内容工具栏 / AI 对话头部分隔线同高同水平线 */
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  height: 38px;
  box-sizing: border-box;
  padding: 0 8px 0 12px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border-light);
}
.bar-left {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  /* 头部单行：任何情况下「文档」标题不换行。
     注意不要加 overflow:hidden——分支下拉菜单（绝对定位向下展开）会被裁掉 */
  white-space: nowrap;
  flex: 1;
}
.bar-title {
  flex-shrink: 0;
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.04em;
}
.bar-count {
  flex-shrink: 0;
  font-family: "JetBrains Mono", monospace;
  font-size: 10.5px;
  color: var(--text-tertiary);
  background: var(--bg-input);
  border-radius: var(--radius-xs);
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.bar-count.live {
  color: var(--color-primary);
  background: var(--color-primary-bg);
}
.bar-count.live .spin {
  font-size: 9px;
}
/* 自绘分支切换（替代 ant-select：紧凑贴合树头部尺度） */
.branch-switch {
  position: relative;
  min-width: 0;
  flex-shrink: 1;
}
.branch-switch-btn {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  max-width: 130px;
  border: 1px solid transparent;
  background: var(--bg-input);
  color: var(--text-tertiary);
  font-size: 10.5px;
  font-family: "JetBrains Mono", monospace;
  border-radius: var(--radius-xs);
  padding: 1px 6px;
  cursor: pointer;
  line-height: 17px;
}
.branch-switch-btn:hover {
  color: var(--color-primary);
}
/* 只读查看态：主色点标示非当前 */
.branch-switch.viewing .branch-switch-btn {
  color: var(--color-primary);
  background: var(--color-primary-bg);
}
.branch-switch-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.branch-caret {
  font-size: 8px;
  transition: transform var(--transition-fast);
  flex-shrink: 0;
}
.branch-caret.open {
  transform: rotate(90deg);
}
.branch-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 30;
  min-width: 150px;
  max-width: 220px;
  background: var(--bg-elevated, var(--bg-card));
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.16);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.branch-menu-item {
  display: flex;
  align-items: center;
  gap: 6px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  font-family: "JetBrains Mono", monospace;
  padding: 4px 8px;
  border-radius: var(--radius-xs);
  cursor: pointer;
  text-align: left;
}
.branch-menu-item:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}
.branch-menu-item.active {
  color: var(--color-primary);
  background: var(--color-primary-bg);
  font-weight: 600;
}
.branch-menu-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cur-mark {
  font-size: 10px;
  color: var(--color-success, #52c41a);
  flex-shrink: 0;
}
.cur-tag {
  font-size: 9.5px;
  color: var(--text-tertiary);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-xs);
  padding: 0 4px;
  line-height: 14px;
  flex-shrink: 0;
}

/* 只读提示：行内轻量（不再整块橙色） */
.readonly-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 2px 2px 4px;
  padding: 0 4px;
  font-size: 10.5px;
  color: var(--text-disabled);
}
.rh-icon {
  font-size: 11px;
}
.rh-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.rh-back {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  border: none;
  background: transparent;
  color: var(--color-primary);
  font-size: 10.5px;
  cursor: pointer;
  padding: 1px 4px;
  border-radius: var(--radius-xs);
  white-space: nowrap;
  flex-shrink: 0;
}
.rh-back:hover {
  background: var(--color-primary-bg);
}
.bar-branch {
  font-size: 10.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-input);
  border-radius: var(--radius-xs);
  padding: 0 7px;
  line-height: 17px;
  /* 弹性收缩的最后一项：宽度紧张时先省略分支名 */
  min-width: 0;
  flex-shrink: 1;
  flex-grow: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.stale-dot {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  margin-left: -3px;
}
.stale-dot .dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #ea8a3c;
  animation: stale-blink 1.6s ease-in-out infinite;
}
.stale-dot .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #ea8a3c;
  animation: stale-blink 1.6s ease-in-out infinite;
}
@keyframes stale-blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}
.tree-bar-actions {
  display: inline-flex;
  gap: 2px;
}
.bar-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  border-radius: var(--radius-xs);
  cursor: pointer;
  font-size: 13px;
}
.bar-btn:hover:not(:disabled) {
  background: var(--bg-input);
  color: var(--color-primary);
}
.bar-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.wiki-tree {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
  overflow-y: auto;
  height: 100%;
  padding: 0;
}
.tree-group {
  display: flex;
  flex-direction: column;
}
.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}
.group-count {
  font-family: "JetBrains Mono", monospace;
  font-size: 10px;
  background: var(--bg-input);
  border-radius: var(--radius-xs);
  padding: 0 6px;
}
.group-count.live {
  color: var(--color-primary);
  background: var(--color-primary-bg);
}
.group-list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.tree-item {
  padding: 5px 10px 5px 26px;
  font-size: 13px;
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: all var(--transition-fast);
}
.tree-item:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.tree-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}
.tree-item.node {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 4px;
  padding-bottom: 4px;
}
.node-caret {
  font-size: 9px;
  color: var(--text-tertiary);
  transition: transform var(--transition-fast);
  flex-shrink: 0;
}
.node-caret.open {
  transform: rotate(90deg);
}
.node-icon {
  font-size: 13px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.node-icon.dir {
  color: var(--color-warning);
}
.node-icon.dim {
  opacity: 0.55;
}
.node-icon.primary {
  color: var(--color-primary);
}
.node-icon.ok {
  color: var(--color-success, #52c41a);
}
.node-icon.bad {
  color: var(--color-danger, #d06050);
}
.node-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.imp {
  font-size: 10px;
  color: var(--color-warning);
  border: 1px solid currentColor;
  border-radius: var(--radius-xs);
  padding: 0 4px;
  flex-shrink: 0;
  line-height: 15px;
}

/* ===== 生成骨架状态 ===== */
/* 未生成：置灰 */
.tree-item.gen-pending {
  color: var(--text-disabled);
}
.tree-item.gen-pending:hover {
  color: var(--text-secondary);
}
/* 进行中：高亮 + 呼吸脉动 */
.tree-item.gen-active {
  color: var(--color-primary);
  font-weight: 600;
  background: var(--color-primary-bg);
  animation: gen-pulse 1.6s ease-in-out infinite;
}
@keyframes gen-pulse {
  0%,
  100% {
    background: var(--color-primary-bg);
  }
  50% {
    background: color-mix(in oklch, var(--color-primary) 16%, transparent);
  }
}
.tree-item.gen-done {
  color: var(--text-primary);
}
.tree-item.gen-failed .node-label {
  color: var(--color-danger, #d06050);
}

/* 生成早期占位 */
.gen-waiting {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 40px 12px;
  color: var(--text-tertiary);
  font-size: 12px;
  text-align: center;
}
.spin {
  animation: spin 1.2s linear infinite;
  color: var(--color-primary);
  font-size: 16px;
}
.spin.inline {
  font-size: 12px;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.gen-tip {
  padding: 8px 10px 2px;
  font-size: 11px;
  color: var(--text-disabled);
  text-align: center;
}
/* 空态：居中三行，围绕「暂无文档」 */
.empty-tree {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 9px;
  text-align: center;
}
.et-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0.02em;
}
.et-line {
  font-size: 11.5px;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  gap: 5px;
}
.et-mono {
  font-family: "JetBrains Mono", monospace;
  color: var(--text-secondary);
}
.et-link {
  border: none;
  background: none;
  padding: 0;
  font-size: 11.5px;
  font-family: "JetBrains Mono", monospace;
  color: var(--color-primary);
  cursor: pointer;
}
.et-link:hover {
  text-decoration: underline;
}
</style>
