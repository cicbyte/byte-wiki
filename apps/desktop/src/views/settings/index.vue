<script setup lang="ts">
/**
 * 设置页 /settings（对齐 reference settings：左垂直菜单 + 右表单内容）
 *
 * 左 180px sider：分组菜单（图标+文字，点击切换 activeTab）
 * 右 flex:1：对应分区内容，可滚动
 *
 * 各分区拆分为独立子组件（自管状态 + 自带样式）：
 * - 数据 / 偏好 / 关于 → ./components/*
 * - 外观（主题/壁纸/动画）→ ./components/AppearanceForm 等既有子组件
 * - 帮助 → @/components/settings/HelpPane
 */
import { ref, onMounted } from "vue";
import {
  DatabaseOutlined,
  BgColorsOutlined,
  InfoCircleOutlined,
  QuestionCircleOutlined,
  SettingOutlined,
  RobotOutlined,
} from "@ant-design/icons-vue";
import AppearanceForm from "./components/AppearanceForm.vue";
import WallpaperSection from "./components/WallpaperSection.vue";
import AnimationForm from "./components/AnimationForm.vue";
import StorageSection from "./components/StorageSection.vue";
import PreferencesSection from "./components/PreferencesSection.vue";
import ModelSection from "./components/ModelSection.vue";
import AboutSection from "./components/AboutSection.vue";
import HelpPane from "@/components/settings/HelpPane.vue";
import { useLayoutStore } from "@/stores/layout";
// HelpPane 内部使用 MdPreview，其样式需在此引入（与原页面级行为一致）
import "md-editor-v3/lib/style.css";

const layout = useLayoutStore();

const tabs = [
  { key: "storage", icon: DatabaseOutlined, label: "数据" },
  { key: "preferences", icon: SettingOutlined, label: "偏好" },
  { key: "model", icon: RobotOutlined, label: "模型" },
  { key: "appearance", icon: BgColorsOutlined, label: "外观" },
  { key: "help", icon: QuestionCircleOutlined, label: "帮助" },
  { key: "about", icon: InfoCircleOutlined, label: "关于" },
];
const activeTab = ref("storage");
/** 外观二级 tab：主题 / 壁纸 */
const appearanceSub = ref<"theme" | "wallpaper" | "animation">("theme");

onMounted(() => {
  layout.setStatusItems([{ key: "view", text: "设置" }]);
});
</script>

<template>
  <div class="settings-view">
    <!-- 左：垂直分组菜单 -->
    <aside class="settings-sider">
      <div class="sider-title">设置</div>
      <ul class="group-menu">
        <li v-for="tab in tabs" :key="tab.key">
          <a
            class="menu-item"
            :class="{ active: activeTab === tab.key }"
            @click="activeTab = tab.key"
          >
            <component :is="tab.icon" />
            <span>{{ tab.label }}</span>
          </a>
        </li>
      </ul>
    </aside>

    <!-- 右：分区内容 -->
    <div class="settings-content">
      <!-- 数据 -->
      <StorageSection v-if="activeTab === 'storage'" />

      <!-- 偏好 -->
      <PreferencesSection v-else-if="activeTab === 'preferences'" />

      <!-- 模型 -->
      <ModelSection v-else-if="activeTab === 'model'" />

      <!-- 外观 -->
      <section v-else-if="activeTab === 'appearance'" class="pane pane-appearance">
        <h2 class="pane-title">外观</h2>
        <div class="sub-tabs">
          <button class="sub-tab" :class="{ active: appearanceSub === 'theme' }" @click="appearanceSub = 'theme'">主题</button>
          <button class="sub-tab" :class="{ active: appearanceSub === 'wallpaper' }" @click="appearanceSub = 'wallpaper'">壁纸</button>
          <button class="sub-tab" :class="{ active: appearanceSub === 'animation' }" @click="appearanceSub = 'animation'">动画</button>
        </div>
        <AppearanceForm v-show="appearanceSub === 'theme'" />
        <WallpaperSection v-if="appearanceSub === 'wallpaper'" />
        <AnimationForm v-if="appearanceSub === 'animation'" />
      </section>

      <!-- 帮助 -->
      <section v-else-if="activeTab === 'help'" class="pane" style="height: 100%; padding: 0;">
        <HelpPane />
      </section>

      <AboutSection v-else-if="activeTab === 'about'" />
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  display: flex;
  height: 100%;
  overflow: hidden;
  background: var(--bg-base);
}

/* 左侧分组菜单 */
.settings-sider {
  display: flex;
  flex-direction: column;
  min-width: 180px;
  width: 180px;
  border-right: 1px solid var(--border-light);
  padding: var(--spacing-sm);
  user-select: none;
  flex-shrink: 0;
  background: var(--bg-surface);
}
.sider-title {
  padding: 8px 12px 14px;
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}
.group-menu {
  display: flex;
  flex-direction: column;
  list-style: none;
  margin: 0;
  padding: 0;
}
.group-menu li {
  margin-bottom: 2px;
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  width: 100%;
  text-decoration: none;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: var(--radius-md);
  font-weight: 500;
  font-size: 13px;
  transition: all var(--transition-fast);
}
.menu-item :deep(.anticon) {
  font-size: 15px;
  opacity: 0.85;
  flex-shrink: 0;
}
.menu-item:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.menu-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

/* 右侧内容区 */
.settings-content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
  padding: var(--spacing-lg) var(--spacing-xl);
  background: var(--bg-base);
}
.settings-content::-webkit-scrollbar {
  width: 8px;
}
.settings-content::-webkit-scrollbar-thumb {
  background: var(--border-base);
  border-radius: 4px;
}

.pane {
  /* 占满内容区宽度 */
}
/* 外观面板：撑满内容区，让自定义编辑器（可视化/JSON）flex 撑满剩余空间 */
.pane.pane-appearance {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.pane-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: var(--spacing-md);
}

/* 二级 tab 导航（.mcp-sub-tabs 与 .sub-tabs 共享规则块，选择器保持原样不拆分） */
.sub-tabs,
.mcp-sub-tabs {
  display: flex;
  gap: 4px;
  margin-bottom: var(--spacing-md);
  border-bottom: 1px solid var(--border-light);
}
.sub-tab {
  padding: 8px 16px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition-fast);
  margin-bottom: -1px;
}
.sub-tab:hover {
  color: var(--text-primary);
}
.sub-tab.active {
  color: var(--color-primary);
  border-bottom-color: var(--color-primary);
  font-weight: 600;
}
</style>
