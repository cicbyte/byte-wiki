<script setup lang="ts">
/**
 * Wiki 导航树：总览 / 文档（页面按章节路径嵌套）
 *
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
} from "@ant-design/icons-vue";
import type { WikiDocEntry } from "@/api/generate";

const props = defineProps<{
  docs: WikiDocEntry[];
  current: string;
}>();

const emit = defineEmits<{ (e: "select", path: string): void }>();

interface VisibleNode {
  label: string;
  path: string; // dir 节点为合成 key
  isDir: boolean;
  depth: number;
}

/** 文档树：由 pages/ 路径构建目录层级（默认全展开，记录用户折叠项） */
const collapsed = ref(new Set<string>());

function buildPageNodes(entries: WikiDocEntry[]): VisibleNode[] {
  if (!entries.length) return [];
  const out: VisibleNode[] = [];
  const walk = (prefix: string, depth: number) => {
    const dirs = new Map<string, true>();
    const files: WikiDocEntry[] = [];
    for (const e of entries) {
      const rel = e[0].slice(prefix.length ? prefix.length + 1 : 0);
      const slash = rel.indexOf("/");
      if (slash < 0) {
        files.push(e);
      } else {
        dirs.set(rel.slice(0, slash), true);
      }
    }
    for (const d of [...dirs.keys()].sort()) {
      const dirPath = prefix ? `${prefix}/${d}` : d;
      out.push({ label: d, path: dirPath, isDir: true, depth });
      if (!collapsed.value.has(dirPath)) walk(dirPath, depth + 1);
    }
    for (const f of files.sort((a, b) => a[0].localeCompare(b[0]))) {
      out.push({ label: f[2] || f[0], path: f[0], isDir: false, depth });
    }
  };
  walk("pages", 0);
  return out;
}

const pageEntries = computed(() => props.docs.filter(([, kind]) => kind === "page"));
const pageNodes = computed(() => buildPageNodes(pageEntries.value));

function toggleDir(path: string) {
  const next = new Set(collapsed.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  collapsed.value = next;
}

const arch = computed(() => props.docs.filter(([, kind]) => kind === "architecture"));
const modules = computed(() => props.docs.filter(([, kind]) => kind === "module")); // 旧版兼容
</script>

<template>
  <nav class="wiki-tree">
    <div v-if="arch.length" class="tree-group">
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

    <div v-if="pageNodes.length" class="tree-group">
      <div class="group-head">
        <AppstoreOutlined />
        <span>文档</span>
        <span class="group-count">{{ pageEntries.length }}</span>
      </div>
      <ul class="group-list">
        <li
          v-for="n in pageNodes"
          :key="n.path"
          class="tree-item node"
          :class="{ active: !n.isDir && current === n.path }"
          :style="{ paddingLeft: 10 + n.depth * 14 + 'px' }"
          :title="n.path"
          @click="n.isDir ? toggleDir(n.path) : emit('select', n.path)"
        >
          <CaretRightOutlined v-if="n.isDir" class="node-caret" :class="{ open: !collapsed.has(n.path) }" />
          <FolderOutlined v-if="n.isDir" class="node-icon dir" />
          <FileTextOutlined v-else class="node-icon" />
          <span class="node-label">{{ n.label }}</span>
        </li>
      </ul>
    </div>

    <div v-if="modules.length" class="tree-group">
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
.node-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
