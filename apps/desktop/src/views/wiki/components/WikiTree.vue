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
import { computed, ref } from "vue";
import {
  ApartmentOutlined,
  AppstoreOutlined,
  FileTextOutlined,
  FolderOutlined,
  CaretRightOutlined,
  LoadingOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
} from "@ant-design/icons-vue";
import type { WikiDocEntry, PageProgress } from "@/api/generate";

const props = defineProps<{
  docs: WikiDocEntry[];
  current: string;
  /** 生成中的页面骨架（大纲确定后非空） */
  genPages?: PageProgress[];
  generating?: boolean;
  /** 生成早期（索引/大纲设计）树内占位提示 */
  genHint?: string;
}>();

const emit = defineEmits<{ (e: "select", path: string): void }>();

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

/** 由路径集合构建目录层级树（磁盘文档与生成骨架两个入口共用） */
function buildNodes(paths: string[], kind: "docs" | "skeleton"): VisibleNode[] {
  if (!paths.length) return [];
  const withStatus = (path: string): { status?: string; importance?: string } => {
    if (kind !== "skeleton") return {};
    const p = (props.genPages || []).find((g) => g.path === path);
    return { status: p?.status, importance: p?.importance };
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
    for (const d of [...dirs.keys()].sort()) {
      const dirPath = prefix ? `${prefix}/${d}` : d;
      out.push({ label: d, path: dirPath, isDir: true, depth });
      if (!collapsed.value.has(dirPath)) walk(dirPath, depth + 1);
    }
    for (const f of files.sort((a, b) => a.localeCompare(b))) {
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
  buildNodes((props.genPages || []).map((g) => g.path).filter(Boolean) as string[], "skeleton"),
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

const doneCount = computed(
  () => (props.genPages || []).filter((p) => p.status === "done" || p.status === "failed").length,
);
</script>

<template>
  <nav class="wiki-tree">
    <!-- 生成早期：索引 / 大纲设计中（骨架未出） -->
    <div v-if="generating && !skeletonMode" class="gen-waiting">
      <LoadingOutlined class="spin" />
      <span>{{ genHint || "正在准备大纲…" }}</span>
    </div>

    <template v-else>
      <div v-if="!generating && arch.length" class="tree-group">
        <div class="group-head">
          <ApartmentOutlined />
          <span>总览</span>
          <span class="group-count">{{ arch.length }}</span>
        </div>
        <ul class="group-list">
          <li
            v-for="[path, , title] in arch"
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

      <div v-if="(skeletonMode ? skeletonNodes : diskNodes).length" class="tree-group">
        <div class="group-head">
          <AppstoreOutlined v-if="!generating" />
          <LoadingOutlined v-else class="spin inline" />
          <span>文档</span>
          <span v-if="generating" class="group-count live">
            {{ doneCount }}/{{ genPages!.length }}
          </span>
          <span v-else class="group-count">{{ diskPageEntries.length }}</span>
        </div>
        <ul class="group-list">
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

      <div v-if="!generating && !diskNodes.length" class="gen-tip empty-tree">
        暂无文档 · 生成 Wiki 后按章节展示
      </div>
    </template>
  </nav>
</template>

<style scoped>
.wiki-tree {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
  overflow-y: auto;
  height: 100%;
  padding: var(--spacing-sm);
}
.tree-group {
  display: flex;
  flex-direction: column;
}
.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
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
.gen-tip.empty-tree {
  padding: 24px 10px;
}
</style>
