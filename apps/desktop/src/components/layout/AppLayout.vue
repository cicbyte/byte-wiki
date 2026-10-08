<script setup lang="ts">
/**
 * 应用布局（row 结构，避免 header/footer 重复堆叠）
 *
 * app-layout (flex ROW)
 *   ├── wp-layer / wp-mask   壁纸层 + 遮罩层（最底；遮罩色跟随明暗）
 *   ├── Sidebar.vue           全高：logo header + 导航 + 底部
 *   └── .app-right (flex COLUMN)
 *       ├── Navbar.vue        45px
 *       ├── MainContent.vue   内容区
 *       └── .global-footer    48px 状态栏
 *
 * 壁纸机制（对齐 ax）：壁纸层/遮罩层在最底，各面板用
 *   color-mix(var(--bg-xxx), transparent var(--wp-panel-alpha,0%)) + backdrop-filter
 *   自行做面板级毛玻璃半透明（变量由 wallpaper store apply 注入）。
 */
import { computed, onMounted } from "vue";
import { useRoute } from "vue-router";
import { FolderOutlined } from "@ant-design/icons-vue";
import { useLayoutStore } from "@/stores/layout";
import { useThemeStore } from "@/stores/theme";
import { useWallpaperStore } from "@/stores/wallpaper";
import { useWikiStore } from "@/stores/wiki";
import Navbar from "./Navbar.vue";
import Sidebar from "./Sidebar.vue";
import MainContent from "./MainContent.vue";

const layout = useLayoutStore();
const themeStore = useThemeStore();
const wallpaper = useWallpaperStore();
const wiki = useWikiStore();
const route = useRoute();

/** 项目空间模式：footer 显示当前仓库（菜单已去掉仓库分组） */
const showRepoBadge = computed(
  () => route.path.startsWith("/repo/") && !!wiki.currentRepo,
);

onMounted(() => {
  wallpaper.init();
});
</script>

<template>
  <div class="app-layout">
    <div
      v-if="wallpaper.enabled && wallpaper.hasWallpaper"
      class="wp-layer"
      :style="{ backgroundImage: `url(${wallpaper.url})`, filter: `blur(${wallpaper.blur}px) saturate(1.1)` }"
    ></div>
    <div
      v-if="wallpaper.enabled && wallpaper.hasWallpaper"
      class="wp-mask"
      :style="{ background: wallpaper.maskColor }"
    ></div>
    <Sidebar />
    <div class="app-right">
      <Navbar />
      <MainContent />
      <div class="global-footer">
        <div class="status-bar">
          <template v-for="(item, idx) in layout.statusItems" :key="item.key">
            <span class="status-item">
              <span class="status-value">{{ item.text }}</span>
              <span
                v-if="idx < layout.statusItems.length - 1"
                class="status-sep"
              ></span>
            </span>
          </template>
          <span v-if="showRepoBadge" class="repo-badge" :title="wiki.currentRepo!.path">
            <FolderOutlined /> {{ wiki.currentRepo!.name }}<em v-if="wiki.currentRepo!.branch">@{{ wiki.currentRepo!.branch }}</em>
          </span>
          <span class="status-spacer"></span>
          <span class="theme-status">{{ themeStore.resolvedAppearance === 'dark' ? '深色' : '浅色' }}</span>
          <div
            class="mode-switch"
            :class="{ edit: layout.editMode }"
            @click="layout.toggleEditMode()"
          >
            <span class="mode-opt" :class="{ on: !layout.editMode }">阅读</span>
            <span class="mode-opt" :class="{ on: layout.editMode }">编辑</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.app-layout {
  position: relative;
  display: flex;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  background: var(--bg-base);
  color: var(--text-primary);
}
/* 壁纸层（最底）+ 遮罩层（跟随明暗压暗/压亮，保证文字可读） */
.wp-layer,
.wp-mask {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
.wp-layer {
  z-index: 0;
  background-size: cover;
  background-position: center;
  background-repeat: no-repeat;
}
.wp-mask {
  z-index: 1;
}
.app-right {
  position: relative;
  z-index: 2;
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
.global-footer {
  display: flex;
  align-items: center;
  padding: 0 var(--spacing-lg);
  height: var(--statusbar-height);
  border-top: 1px solid var(--border-frame);
  background: var(--bg-footer);
  flex-shrink: 0;
  overflow: hidden;
}
.status-bar {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  width: 100%;
  font-size: 12px;
  color: var(--text-secondary);
  user-select: none;
}
.status-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}
.status-value {
  color: var(--text-primary);
  font-weight: 500;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.status-sep {
  color: var(--text-tertiary);
  opacity: 0.4;
}
.status-sep::after {
  content: "·";
}
.status-spacer {
  flex: 1;
}
/* 当前仓库（项目空间模式，footer 右侧） */
.repo-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--color-primary);
  background: var(--color-primary-bg);
  padding: 2px 10px;
  border-radius: var(--radius-xs);
  font-family: "JetBrains Mono", monospace;
  max-width: 340px;
  overflow: hidden;
}
.repo-badge em {
  font-style: normal;
  color: var(--text-secondary);
}
.theme-status {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  padding: 2px 8px;
  border-radius: var(--radius-xs);
  background: var(--bg-input);
}
/* 阅读/编辑 模式开关（segmented 风格） */
.mode-switch {
  display: inline-flex;
  align-items: center;
  background: var(--bg-input);
  border-radius: 10px;
  padding: 2px;
  cursor: pointer;
  user-select: none;
  font-size: 11px;
  font-family: "JetBrains Mono", monospace;
}
.mode-opt {
  padding: 2px 10px;
  border-radius: 8px;
  color: var(--text-tertiary);
  transition: all var(--transition-fast);
}
.mode-opt.on {
  background: var(--color-primary);
  color: #fff;
  font-weight: 500;
}
</style>
