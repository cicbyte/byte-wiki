<script setup lang="ts">
/**
 * Navbar 自绘标题栏（对齐 reference Navbar）
 *
 * 左：收缩按钮（紧贴 Sidebar 右侧） + 面包屑
 * 中：全局搜索框（保留布局位，Wiki 搜索在后续版本接入）
 * 右：AI 问答（项目空间）+ 明暗切换 + 设置 + 窗口控制
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

/** 项目空间才显示 AI 入口（对话需要仓库上下文） */
const inProjectSpace = computed(() => route.path.startsWith("/repo/"));
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
        v-if="inProjectSpace"
        class="icon-btn ai-toggle"
        :class="{ on: layout.aiDockOpen }"
        title="AI 问答"
        @click="layout.aiDockOpen = !layout.aiDockOpen"
      >
        <!-- 自绘 AI spark 图标：fill=currentColor 跟随文字色，明暗模式自动适配 -->
        <svg class="ai-icon" viewBox="1.73 4.97 12.73 13.18" fill="currentColor" aria-hidden="true">
          <path d="M12.3328 15.1812C12.3579 15.1814 12.3824 15.1885 12.4036 15.2019C12.4248 15.2153 12.4419 15.2343 12.4528 15.2568L12.8456 16.0903C12.8579 16.1179 12.8812 16.1404 12.9088 16.1535L13.7423 16.5448C13.7597 16.5526 13.7752 16.5641 13.7877 16.5785C13.8003 16.5929 13.8096 16.6098 13.8149 16.6281C13.8203 16.6464 13.8217 16.6657 13.819 16.6846C13.8162 16.7035 13.8094 16.7215 13.799 16.7375C13.7852 16.7589 13.7655 16.7758 13.7423 16.7863L12.9088 17.179C12.881 17.1919 12.8584 17.214 12.8448 17.2415L12.4536 18.075C12.4429 18.098 12.4258 18.1175 12.4045 18.1312C12.3831 18.1448 12.3582 18.1521 12.3328 18.1521C12.3075 18.1521 12.2826 18.1448 12.2612 18.1312C12.2398 18.1175 12.2228 18.098 12.2121 18.075L11.8208 17.2423C11.8075 17.2144 11.7849 17.192 11.7568 17.179L10.9234 16.7863C10.9008 16.7753 10.8818 16.7582 10.8684 16.7369C10.8551 16.7156 10.848 16.691 10.848 16.6659C10.848 16.6408 10.8551 16.6162 10.8684 16.5949C10.8818 16.5736 10.9008 16.5565 10.9234 16.5455L11.7568 16.1543C11.785 16.141 11.8076 16.1184 11.8208 16.0903L12.2121 15.2568C12.2232 15.2342 12.2404 15.2152 12.2617 15.2018C12.283 15.1885 12.3077 15.1813 12.3328 15.1812ZM7.83611 4.96954C7.96589 4.96369 8.09508 4.99037 8.21193 5.04714C8.32878 5.10391 8.42959 5.18899 8.5052 5.29463L8.56775 5.40009L10.1532 8.59354L10.1707 8.62845L10.2034 8.6459L14.0987 10.5048C14.2037 10.5555 14.2928 10.6342 14.3561 10.7322C14.4194 10.8301 14.4545 10.9437 14.4575 11.0603C14.4605 11.1769 14.4313 11.2921 14.3731 11.3932C14.315 11.4944 14.2301 11.5775 14.1277 11.6335L10.2979 13.7034H10.2994L10.2681 13.7208L10.2536 13.7528L8.72848 17.0444C8.6665 17.178 8.56793 17.2912 8.4442 17.3709C8.32047 17.4506 8.17667 17.4937 8.02948 17.495C7.88228 17.4963 7.73774 17.4558 7.61263 17.3782C7.48752 17.3007 7.38697 17.1892 7.32266 17.0568L5.61793 13.5506L5.58593 13.5368L5.58229 13.5346L2.18957 12.0074C2.05357 11.9455 1.93822 11.8458 1.85726 11.7202C1.77631 11.5946 1.73314 11.4484 1.73291 11.299C1.73268 11.1495 1.7754 11.0032 1.85597 10.8774C1.93654 10.7515 2.05158 10.6515 2.18739 10.5892L5.74811 8.97099L5.78666 8.95281L5.80339 8.91209L7.14593 5.46409L7.19902 5.35281C7.26495 5.24098 7.35774 5.14735 7.46899 5.08043C7.58023 5.0135 7.70642 4.97539 7.83611 4.96954ZM6.98157 9.37099V9.37318L6.98084 9.37536L6.89357 9.59863C6.81939 9.78627 6.67539 9.93827 6.49211 10.0226L6.2732 10.123H6.27102L3.68775 11.2968L6.0972 12.3819L6.29139 12.4677C6.41648 12.5237 6.52411 12.611 6.60484 12.7208L6.67539 12.8372L6.76993 13.0321L8.01211 15.5848L9.1052 13.2241L9.18739 13.0466L9.24775 12.9397C9.31757 12.8357 9.41066 12.7506 9.52048 12.6903L9.68993 12.5972L9.69429 12.595L12.4347 11.1121L9.65866 9.78699L9.47393 9.69754C9.31841 9.62266 9.19207 9.49837 9.11466 9.34409L9.02302 9.15936H9.02157L7.92557 6.94627L6.98157 9.37099Z" />
        </svg>
      </button>
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
.icon-btn.ai-toggle {
  color: var(--text-tertiary);
  transition: color var(--transition-fast), background var(--transition-fast);
}
.icon-btn.ai-toggle.on {
  color: var(--color-primary);
  background: var(--color-primary-bg);
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
/* AI spark 图标：与明暗切换图标同尺寸口径 */
.ai-icon {
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
