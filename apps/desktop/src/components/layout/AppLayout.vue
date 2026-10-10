<script setup lang="ts">
/**
 * 应用布局：画布 + 悬浮圆角三面板（自 byte-writer 迁移）+ 壁纸层
 *
 * app-layout（画布底色，四周留缝）
 *   ├── wp-layer / wp-mask   壁纸层 + 遮罩层（最底；遮罩色跟随明暗）
 *   ├── Sidebar.vue          独立悬浮面板（14px 圆角 + panel 阴影）
 *   └── .app-right (COLUMN, 8px 缝隙)
 *       ├── Navbar.vue       顶栏面板（45px）
 *       └── .app-content     内容面板（MainContent 吃满，无状态栏）
 *
 * 面板三件套：圆角 + 细边框 + 呼吸阴影（panelBreathe 8s，hover 即停换 hover 档）。
 * 壁纸机制：各面板经 color-mix(--bg-*, --wp-panel-alpha) 派生半透明 + backdrop-filter。
 */
import { onMounted } from "vue";
import { useWallpaperStore } from "@/stores/wallpaper";
import Navbar from "./Navbar.vue";
import Sidebar from "./Sidebar.vue";
import MainContent from "./MainContent.vue";

const wallpaper = useWallpaperStore();

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
    <Sidebar class="shell-panel" />
    <div class="app-right">
      <Navbar class="shell-panel" />
      <div class="app-content shell-panel">
        <MainContent />
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
  background: var(--bg-base); /* 画布：面板缝隙间透出（壁纸态透壁纸） */
  color: var(--text-primary);
  gap: 8px;
  padding: 8px 10px 10px;
  box-sizing: border-box;
}
/* ===== 悬浮面板三件套：圆角 + 细边框 + 呼吸阴影（hover 即停换档） ===== */
.shell-panel {
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-panel, 14px);
  box-shadow: var(--shadow-panel);
  animation: shell-breathe 8s ease-in-out infinite;
  overflow: hidden;
  transition: box-shadow 0.35s ease;
}
.shell-panel:hover {
  animation: none;
  box-shadow: var(--shadow-panel-hover);
}
@keyframes shell-breathe {
  0%, 100% { box-shadow: var(--shadow-panel); }
  50% { box-shadow: var(--shadow-panel-breathe); }
}
@media (prefers-reduced-motion: reduce) {
  .shell-panel { animation: none; }
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
  gap: 8px;
}
/* 顶栏面板固定高；内容面板吃满剩余 */
.app-right > .shell-panel:first-child {
  flex: none;
}
.app-content {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.app-content > :deep(.main-content) {
  flex: 1;
  min-height: 0;
}
</style>
