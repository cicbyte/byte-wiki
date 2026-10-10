<script setup lang="ts">
/**
 * Navbar 自绘标题栏（对齐 reference Navbar）
 *
 * 左：收缩按钮（紧贴 Sidebar 右侧） + 面包屑
 * 中：全局搜索框（保留布局位，Wiki 搜索在后续版本接入）
 * 右：明暗切换 + 设置 + 窗口控制
 */
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  MinusOutlined,
  BorderOutlined,
  CloseOutlined,
  SearchOutlined,
  SettingOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
} from "@ant-design/icons-vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useThemeStore } from "@/stores/theme";
import { useLayoutStore } from "@/stores/layout";
import { useWikiStore } from "@/stores/wiki";

const route = useRoute();
const router = useRouter();
const themeStore = useThemeStore();
const layout = useLayoutStore();
const wiki = useWikiStore();
const q = ref("");
const appWindow = getCurrentWindow();

/**
 * 面包屑（可点层级）：
 * - 项目空间：项目列表 → 仓库名（均可点返回 /）→ 当前页
 * - 全局：分组 / 页名（route.meta）
 */
const breadcrumbItems = computed(() => {
  const title = (route.meta.title as string) || "byte-wiki";
  if (route.path.startsWith("/repo/")) {
    const items: { label: string; to?: string }[] = [
      { label: "项目列表", to: "/" },
    ];
    if (wiki.currentRepo) {
      items.push({ label: wiki.currentRepo.name, to: "/" });
    }
    items.push({ label: title });
    return items;
  }
  const group = (route.meta.group as string) || "";
  const items: { label: string; to?: string }[] = [];
  if (group) items.push({ label: group });
  items.push({ label: title });
  return items;
});
</script>

<template>
  <div class="navbar" data-tauri-drag-region>
    <div class="navbar-left" data-tauri-drag-region>
      <!-- 收缩按钮：Navbar 最左侧，紧贴 Sidebar 右侧 -->
      <button
        class="sidebar-toggle"
        data-tauri-drag-region="false"
        :title="layout.sidebarCollapsed ? '展开菜单' : '收起菜单'"
        @click="layout.toggleSidebar()"
      >
        <MenuUnfoldOutlined v-if="layout.sidebarCollapsed" />
        <MenuFoldOutlined v-else />
      </button>

      <a-breadcrumb class="breadcrumb" separator="/" data-tauri-drag-region="false">
        <a-breadcrumb-item
          v-for="(item, idx) in breadcrumbItems"
          :key="idx"
          :class="{
            'crumb-current': idx === breadcrumbItems.length - 1,
            'crumb-link': !!item.to,
          }"
          @click="item.to && router.push(item.to)"
        >
          {{ item.label }}
        </a-breadcrumb-item>
      </a-breadcrumb>
    </div>

    <div class="navbar-center" data-tauri-drag-region>
      <div class="search-wrapper" data-tauri-drag-region="false">
        <a-input
          v-model:value="q"
          placeholder="搜索 Wiki…"
          class="search-box"
          allow-clear
          disabled
        >
          <template #prefix>
            <SearchOutlined style="color: var(--text-tertiary)" />
          </template>
        </a-input>
      </div>
    </div>

    <div class="navbar-right" data-tauri-drag-region>
      <button
        class="icon-btn"
        :title="themeStore.isDark ? '切换到浅色' : '切换到深色'"
        @click="themeStore.setMode(themeStore.isDark ? 'light' : 'dark')"
      >
        <!-- 暗色时显示太阳（点击切到亮色）；亮色时显示月亮（点击切到暗色）。Feather 线性图标，currentColor 跟随文字色 -->
        <svg v-if="themeStore.isDark" class="theme-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="5" />
          <line x1="12" y1="1" x2="12" y2="3" />
          <line x1="12" y1="21" x2="12" y2="23" />
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
          <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
          <line x1="1" y1="12" x2="3" y2="12" />
          <line x1="21" y1="12" x2="23" y2="12" />
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
          <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
        </svg>
        <svg v-else class="theme-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      </button>
      <button
        class="icon-btn"
        title="设置"
        @click="router.push('/settings')"
      >
        <SettingOutlined />
      </button>
      <span class="divider" />
      <button class="win-btn" title="最小化" @click="appWindow.minimize()">
        <MinusOutlined />
      </button>
      <button class="win-btn" title="最大化" @click="appWindow.toggleMaximize()">
        <BorderOutlined />
      </button>
      <button class="win-btn win-close" title="关闭" @click="appWindow.close()">
        <CloseOutlined />
      </button>
    </div>
  </div>
</template>

<style scoped>
.navbar {
  height: var(--navbar-height);
  display: flex;
  align-items: center;
  padding: 0 var(--spacing-md);
  flex-shrink: 0;
  user-select: none;
}
.navbar-left {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  min-width: 120px;
}
.sidebar-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  border-radius: var(--radius-xs);
  cursor: pointer;
  transition: all var(--transition-fast);
  flex-shrink: 0;
  font-size: 15px;
}
.sidebar-toggle:hover {
  background: var(--bg-elevated);
  color: var(--color-primary);
}
.navbar-center {
  flex: 1;
  display: flex;
  justify-content: center;
  padding: 0 var(--spacing-md);
}
.search-box {
  max-width: 420px;
  width: 100%;
}
/* 搜索框容器（Wiki 搜索接入后在此挂下拉浮层） */
.search-wrapper {
  position: relative;
  width: 100%;
  max-width: 420px;
}
.navbar-right {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 120px;
  justify-content: flex-end;
}
.breadcrumb {
  font-size: 13px;
  color: var(--text-secondary);
}
/* 上级分组：弱色 */
.breadcrumb :deep(.ant-breadcrumb-link) {
  color: var(--text-tertiary);
}
/* 分隔符：弱化 */
.breadcrumb :deep(.ant-breadcrumb-separator) {
  color: var(--text-disabled);
}
/* 当前页：主色加粗，清晰层级 */
.breadcrumb :deep(.crumb-current > .ant-breadcrumb-link) {
  color: var(--color-primary);
  font-weight: 600;
}
/* 可点上级：hover 主色 + 下划线提示可返回 */
.breadcrumb :deep(.crumb-link > .ant-breadcrumb-link) {
  cursor: pointer;
  transition: color var(--transition-fast);
}
.breadcrumb :deep(.crumb-link > .ant-breadcrumb-link:hover) {
  color: var(--color-primary);
  text-decoration: underline;
}
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 15px;
  transition: all var(--transition-fast);
}
.theme-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}
.icon-btn:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.divider {
  width: 1px;
  height: 18px;
  background: var(--border-base);
  margin: 0 4px;
}
.win-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: var(--radius-xs);
  font-size: 13px;
  transition: all var(--transition-fast);
}
.win-btn:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.win-close:hover {
  background: var(--color-danger);
  color: #fff;
}
</style>
