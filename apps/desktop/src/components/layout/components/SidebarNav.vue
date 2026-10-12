<script setup lang="ts">
/**
 * Sidebar 主体导航（平铺模式，参考 byte-writer SidebarNav）。
 *
 * 全部菜单项常显直达：小标题（side-label 风格的分组标签）仅作视觉分隔，
 * 不折叠——分组折叠只增加点击成本且收起时当前路由高亮不可见。
 * 折叠态为纯图标列（小标题隐藏，tooltip 补全名）。
 */
import { computed } from "vue";
import {
  menuGroups as globalGroups,
  topItems as globalTopItems,
  repoMenu,
  labelOfItem,
} from "../lib/constants";

const props = withDefaults(
  defineProps<{
    collapsed: boolean;
    currentPath: string;
    /** 项目空间模式：/repo/:id 前缀时由父组件传入仓库 id */
    repoId?: string | null;
  }>(),
  { repoId: null },
);

/** 双模式菜单：项目空间用项目菜单，否则全局菜单（响应式跟随路由） */
const menu = computed(() =>
  props.repoId ? repoMenu(props.repoId) : { topItems: globalTopItems, menuGroups: globalGroups },
);
const topItems = computed(() => menu.value.topItems);
const menuGroups = computed(() => menu.value.menuGroups);

/** 高亮：精确匹配当前路由 */
function isActive(key: string) {
  return props.currentPath === key;
}

const emit = defineEmits<{
  (e: "navigate", key: string): void;
}>();
</script>

<template>
  <nav class="sidebar-nav">
    <!-- 顶层独立项（不属任何分组） -->
    <template v-if="!collapsed">
      <div
        v-for="item in topItems"
        :key="item.key"
        class="nav-item top-item"
        :class="{ active: isActive(item.key) }"
        @click="emit('navigate', item.key)"
      >
        <component :is="item.icon" class="nav-icon" />
        <span class="nav-label">{{ labelOfItem(item) }}</span>
      </div>
    </template>
    <template v-else>
      <div
        v-for="item in topItems"
        :key="item.key"
        class="nav-item top-item-collapsed"
        :class="{ active: isActive(item.key) }"
        :title="labelOfItem(item)"
        @click="emit('navigate', item.key)"
      >
        <component :is="item.icon" class="nav-icon" />
      </div>
    </template>

    <!-- 分组平铺：小标题仅视觉分隔，子项常显直达 -->
    <template v-if="!collapsed">
      <div v-for="group in menuGroups" :key="group.key" class="nav-group">
        <div class="nav-label-title">{{ group.label }}</div>
        <div
          v-for="child in group.children"
          :key="child.key"
          class="nav-item"
          :class="{ active: isActive(child.key) }"
          @click="emit('navigate', child.key)"
        >
          <component :is="child.icon" class="nav-icon" />
          <span class="nav-label">{{ labelOfItem(child) }}</span>
        </div>
      </div>
    </template>
    <template v-else>
      <template v-for="group in menuGroups" :key="group.key">
        <div
          v-for="child in group.children"
          :key="child.key"
          class="nav-item nav-item-collapsed"
          :class="{ active: isActive(child.key) }"
          :title="labelOfItem(child)"
          @click="emit('navigate', child.key)"
        >
          <component :is="child.icon" class="nav-icon" />
        </div>
      </template>
    </template>
  </nav>
</template>

<style scoped>
.sidebar-nav {
  flex: 1;
  overflow-y: auto;
  padding: var(--spacing-sm);
}
.sidebar-nav::-webkit-scrollbar {
  width: 5px;
}
.sidebar-nav::-webkit-scrollbar-thumb {
  background: var(--border-base);
  border-radius: 3px;
}

/* 顶层独立项（比普通菜单项略重） */
.nav-item.top-item {
  padding: 9px 12px;
  font-weight: 500;
}
.nav-item.top-item-collapsed {
  justify-content: center;
  padding: 10px;
  margin-bottom: 2px;
}

/* 分组小标题：仅视觉分隔，不可点击 */
.nav-group {
  margin-bottom: 2px;
}
.nav-label-title {
  padding: 14px 10px 4px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.05em;
  user-select: none;
  white-space: nowrap;
}
.nav-group:first-of-type .nav-label-title {
  padding-top: 6px;
}

/* 折叠态：隐藏小标题，子项并入图标列 */
.nav-item-collapsed {
  justify-content: center;
  padding: 10px;
}

/* ===== 菜单项：30px 行高胶囊、低透明度层次（悬浮轻于选中）、120ms 无位移 ===== */
.nav-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  height: 30px;
  padding: 0 8px;
  margin-bottom: 2px;
  border-radius: 8px;
  cursor: pointer;
  transition: background-color 120ms ease, color 120ms ease;
  color: var(--text-secondary);
  font-size: 13px;
  white-space: nowrap;
  user-select: none;
}
.nav-item:hover {
  background: var(--color-nav-hover);
  color: var(--text-primary);
}
.nav-item.active {
  background: var(--color-nav-active);
  color: var(--text-primary);
  font-weight: 550;
}
.nav-icon {
  font-size: 15px;
  color: var(--text-tertiary);
  flex-shrink: 0;
  transition: color 120ms ease;
}
.nav-item:hover .nav-icon,
.nav-item.active .nav-icon {
  color: var(--text-primary);
}

.nav-label {
  font-size: 13px;
}
</style>
