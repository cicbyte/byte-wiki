<script setup lang="ts">
/**
 * Sidebar 主体导航
 *
 * - 顶层独立项（不属任何分组）
 * - 分组菜单：折叠态 hover flyout / 展开态手风琴
 *
 * 响应式状态（expanded / collapsedHover）由父组件持有并下发，
 * 修改通过 emit 上报；静态菜单数据从 lib/constants 直接导入。
 */
import { CaretRightFilled } from "@ant-design/icons-vue";
import {
  menuGroups,
  topItems,
  labelOf,
  groupOf,
} from "../lib/constants";

const props = defineProps<{
  collapsed: boolean;
  currentPath: string;
  expanded: Set<string>;
  collapsedHover: string | null;
}>();

const emit = defineEmits<{
  (e: "toggle-group", key: string): void;
  (e: "navigate", key: string): void;
  (e: "hover-group", key: string | null): void;
}>();

function isExpanded(key: string) {
  return props.expanded.has(key);
}
</script>

<template>
  <nav class="sidebar-nav">
    <!-- 顶层独立项（不属任何分组） -->
    <template v-if="!collapsed">
      <div
        v-for="item in topItems"
        :key="item.key"
        class="nav-item top-item"
        :class="{ active: currentPath === item.key }"
        @click="emit('navigate', item.key)"
      >
        <component :is="item.icon" class="nav-icon" />
        <span class="nav-label">{{ labelOf[item.key] }}</span>
      </div>
    </template>
    <template v-else>
      <div
        v-for="item in topItems"
        :key="item.key"
        class="nav-item top-item-collapsed"
        :class="{ active: currentPath === item.key }"
        :title="labelOf[item.key]"
        @click="emit('navigate', item.key)"
      >
        <component :is="item.icon" class="nav-icon" />
      </div>
    </template>

    <!-- 分组菜单 -->
    <template v-for="group in menuGroups" :key="group.key">
      <!-- 折叠态：分组图标 + hover flyout -->
      <div
        v-if="collapsed"
        class="nav-group-collapsed"
        @mouseenter="emit('hover-group', group.key)"
        @mouseleave="emit('hover-group', null)"
      >
        <div
          class="nav-item group-head"
          :class="{ 'has-active': groupOf(currentPath) === group.key }"
          @click="emit('toggle-group', group.key)"
        >
          <component :is="group.icon" class="nav-icon" />
        </div>
        <transition name="flyout">
          <div v-if="collapsedHover === group.key" class="flyout">
            <div class="flyout-title">{{ group.label }}</div>
            <div
              v-for="child in group.children"
              :key="child.key"
              class="nav-item flyout-item"
              :class="{ active: currentPath === child.key }"
              @click="emit('navigate', child.key)"
            >
              <component :is="child.icon" class="nav-icon" />
              <span class="nav-label">{{ labelOf[child.key] }}</span>
            </div>
          </div>
        </transition>
      </div>

      <!-- 展开态：分组标题 + 子项 -->
      <div v-else class="nav-group">
        <div
          class="nav-group-head"
          :class="{ 'has-active': groupOf(currentPath) === group.key }"
          @click="emit('toggle-group', group.key)"
        >
          <component :is="group.icon" class="group-head-icon" />
          <span class="group-head-label">{{ group.label }}</span>
          <CaretRightFilled class="group-caret" :class="{ open: isExpanded(group.key) }" />
        </div>
        <transition name="expand">
          <div v-show="isExpanded(group.key)" class="nav-children">
            <div
              v-for="child in group.children"
              :key="child.key"
              class="nav-item child"
              :class="{ active: currentPath === child.key }"
              @click="emit('navigate', child.key)"
            >
              <component :is="child.icon" class="nav-icon" />
              <span class="nav-label">{{ labelOf[child.key] }}</span>
            </div>
          </div>
        </transition>
      </div>
    </template>
  </nav>
</template>

<style scoped>
/* ===== nav ===== */
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

/* 顶层独立项（不属任何分组，比普通菜单项略重） */
.nav-item.top-item {
  padding: 9px 12px;
  font-weight: 500;
}
.nav-item.top-item-collapsed {
  justify-content: center;
  padding: 10px;
  margin-bottom: 2px;
}

.nav-group {
  margin-bottom: 2px;
}
.nav-group-head {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: 7px 10px;
  border-radius: var(--radius-md);
  cursor: pointer;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  transition: all var(--transition-fast);
  white-space: nowrap;
  user-select: none;
}
.nav-group-head:hover {
  color: var(--text-secondary);
  background: var(--bg-elevated);
}
.nav-group-head.has-active {
  color: var(--color-primary);
}
.group-head-icon {
  font-size: 14px;
  flex-shrink: 0;
}
.group-head-label {
  flex: 1;
}
.group-caret {
  font-size: 9px;
  transition: transform var(--transition-fast);
  opacity: 0.7;
}
.group-caret.open {
  transform: rotate(90deg);
}

.nav-children {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding-left: 14px;
  overflow: hidden;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: 8px 12px;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition-fast);
  color: var(--text-secondary);
  white-space: nowrap;
}
.nav-item:hover {
  background: var(--bg-elevated);
  color: var(--color-primary);
}
.nav-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 500;
}
.nav-item.child {
  padding: 7px 12px;
}
.nav-item.child::before {
  content: "";
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--border-base);
  flex-shrink: 0;
  margin-left: -7px;
}
.nav-item.child.active::before {
  background: var(--color-primary);
}
.nav-icon {
  font-size: 16px;
  flex-shrink: 0;
}
.nav-label {
  font-size: 13px;
}

/* ===== 折叠态 ===== */
.nav-group-collapsed {
  position: relative;
}
.nav-group-collapsed .nav-item.group-head {
  justify-content: center;
  padding: 10px;
  margin-bottom: 2px;
}
.nav-group-collapsed .nav-item.group-head.has-active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

.flyout {
  position: absolute;
  left: calc(100% + 8px);
  top: 0;
  min-width: 160px;
  background: var(--bg-elevated); /* 浮层需比 surface 更亮以浮起 */
  border: 1px solid var(--border-base);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  padding: 6px;
  z-index: 1100;
}
.flyout-title {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-tertiary);
  padding: 4px 10px;
}
.flyout-item {
  padding: 7px 10px;
}
.flyout-item::before {
  display: none;
}

.flyout-enter-active,
.flyout-leave-active {
  transition: opacity var(--transition-fast), transform var(--transition-fast);
}
.flyout-enter-from,
.flyout-leave-to {
  opacity: 0;
  transform: translateX(-4px);
}
.expand-enter-active,
.expand-leave-active {
  transition: all var(--transition-fast);
}
.expand-enter-from,
.expand-leave-to {
  opacity: 0;
  max-height: 0;
}
</style>
