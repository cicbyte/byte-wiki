<script setup lang="ts">
/**
 * Wiki 导航树：架构 / 模块（分组）/ 图谱 三区（FR-03）
 *
 * 按现有样式规范自研（两层层级列表 + active 高亮），不引入新组件库。
 */
import { computed } from "vue";
import {
  ApartmentOutlined,
  AppstoreOutlined,
  NodeIndexOutlined,
  BookOutlined,
} from "@ant-design/icons-vue";
import type { WikiDocEntry } from "@/api/generate";

const props = defineProps<{
  docs: WikiDocEntry[];
  current: string;
}>();

const emit = defineEmits<{ (e: "select", path: string): void }>();

const groups = computed(() => {
  const arch = props.docs.filter(([, kind]) => kind === "architecture");
  const modules = props.docs.filter(([, kind]) => kind === "module");
  const graphs = props.docs.filter(([, kind]) => kind === "graph");
  const cards = props.docs.filter(([, kind]) => kind === "card");
  return [
    { key: "arch", label: "架构", icon: ApartmentOutlined, items: arch },
    { key: "card", label: "知识卡", icon: BookOutlined, items: cards },
    { key: "module", label: "模块", icon: AppstoreOutlined, items: modules },
    { key: "graph", label: "图谱", icon: NodeIndexOutlined, items: graphs },
  ].filter((g) => g.items.length > 0);
});
</script>

<template>
  <nav class="wiki-tree">
    <div v-for="g in groups" :key="g.key" class="tree-group">
      <div class="group-head">
        <component :is="g.icon" />
        <span>{{ g.label }}</span>
        <span class="group-count">{{ g.items.length }}</span>
      </div>
      <ul class="group-list">
        <li
          v-for="[path, , title] in g.items"
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
  padding: 6px 10px 6px 26px;
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
</style>
