<script setup lang="ts">
/**
 * Sidebar 侧栏（对齐 byte-writer-桌面布局设计.md §3 + reference Sidebar）
 *
 * 顶部 logo header（高度同 navbar）：左侧 logo+文字
 * 主体：平铺分组导航（小标题仅视觉分隔，子项常显直达）
 * 折叠态：纯图标列（tooltip 补全名）
 *
 * 本文件为组合入口：静态菜单数据见 ./lib/constants。
 */
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useLayoutStore } from "@/stores/layout";
import SidebarLogo from "./components/SidebarLogo.vue";
import SidebarNav from "./components/SidebarNav.vue";

const route = useRoute();
const router = useRouter();
const layout = useLayoutStore();

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
      @navigate="navigate"
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
