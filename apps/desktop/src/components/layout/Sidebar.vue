<script setup lang="ts">
/**
 * Sidebar 层级菜单导航（对齐 byte-wiki-桌面布局设计.md §3 + reference Sidebar）
 *
 * 顶部 logo header（高度同 navbar）：左侧 logo+文字
 * 主体：分组菜单（手风琴展开，路由联动高亮）
 * 折叠态：仅图标，hover flyout
 *
 * 本文件为组合入口：持有响应式状态（expanded / collapsedHover），
 * 通过 props 下发子组件、emit 回收变更；静态菜单数据见 ./lib/constants。
 */
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useLayoutStore } from "@/stores/layout";
import { useMenuExpansion } from "./lib/useMenuExpansion";
import SidebarLogo from "./components/SidebarLogo.vue";
import SidebarNav from "./components/SidebarNav.vue";

const route = useRoute();
const router = useRouter();
const layout = useLayoutStore();

const { expanded, toggleGroup } = useMenuExpansion();
const collapsedHover = ref<string | null>(null);

/** 项目空间模式：当前路由 /repo/:id 时提取仓库 id（左侧菜单切换为项目专属菜单） */
const repoId = computed(() => {
  const m = route.path.match(/^\/repo\/(\d+)/);
  return m ? m[1] : null;
});

function navigate(key: string) {
  router.push(key);
}
</script>

<template>
  <div class="sidebar" :class="{ collapsed: layout.sidebarCollapsed }">
    <!-- logo header：仅 logo + 文字（收缩按钮在 Navbar 左侧） -->
    <SidebarLogo :collapsed="layout.sidebarCollapsed" @home="navigate('/')" />

    <!-- 导航 -->
    <SidebarNav
      :collapsed="layout.sidebarCollapsed"
      :current-path="route.path"
      :repo-id="repoId"
      :expanded="expanded"
      :collapsed-hover="collapsedHover"
      @toggle-group="toggleGroup"
      @navigate="navigate"
      @hover-group="collapsedHover = $event"
    />

  </div>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative;
  z-index: 2;
  transition: width var(--transition-normal);
  overflow: hidden;
  flex-shrink: 0;
  width: var(--sidebar-width-expanded);
}
.sidebar.collapsed {
  width: var(--sidebar-width);
}

/*
 * 折叠态对子组件根元素（.sidebar-logo / .sidebar-bottom）的微调。
 * 这两条规则依赖父级 .sidebar.collapsed 状态，故保留在主入口；
 * 子组件根元素会继承父级 scoped 属性，可被正确匹配。
 */
.sidebar.collapsed .sidebar-logo {
  justify-content: center;
  padding: 0;
}
.sidebar.collapsed .sidebar-bottom {
  justify-content: center;
}
</style>
