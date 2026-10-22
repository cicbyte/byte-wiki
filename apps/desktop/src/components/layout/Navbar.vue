<script setup lang="ts">
/**
 * Navbar 自绘标题栏（对齐 reference Navbar）
 *
 * 左：收缩按钮（紧贴 Sidebar 右侧） + 面包屑
 * 中：全局搜索框（保留布局位，Wiki 搜索在后续版本接入）
 * 右：AI 问答（项目空间）+ 明暗切换 + 设置 + 窗口控制
 */
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  MinusOutlined,
  BorderOutlined,
  CloseOutlined,
  SettingOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  BranchesOutlined,
} from "@ant-design/icons-vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useThemeStore } from "@/stores/theme";
import { useLayoutStore } from "@/stores/layout";
import { useAiStore } from "@/stores/ai";
import AiSparkIcon from "@/components/icons/AiSparkIcon.vue";
import { useWikiStore } from "@/stores/wiki";
import { callchainApi } from "@/api/callchain";

const route = useRoute();
const router = useRouter();
const themeStore = useThemeStore();
const layout = useLayoutStore();
const ai = useAiStore();

/** 项目空间才显示 AI 入口（对话需要仓库上下文） */
const inProjectSpace = computed(() => route.path.startsWith("/repo/"));
const wiki = useWikiStore();
const appWindow = getCurrentWindow();

// ===== 全局分支切换（右上角）：wiki/调用链/问答统一按查看分支工作 =====
interface BranchItem {
  name: string;
  current: boolean;
  is_default_bucket: boolean;
  has_wiki: boolean;
  has_callgraph: boolean;
}
const branchMenuOpen = ref(false);
const branches = ref<BranchItem[]>([]);

async function loadBranches() {
  if (!wiki.currentRepo) return;
  try {
    branches.value = await callchainApi.listBranches(wiki.currentRepo.path);
  } catch {
    branches.value = [];
  }
}
watch(() => wiki.currentRepo?.path, loadBranches, { immediate: true });

/** 选中的是当前分支 → 回正常模式（null）；其他分支 → 只读查看 */
function pickBranch(b: BranchItem) {
  branchMenuOpen.value = false;
  wiki.setViewBranch(b.current ? null : b.name);
  // 分支视图变了，当前页数据自会按 store 重取（各面板 watch viewBranch）
}
const branchLabel = computed(() => wiki.viewBranch ?? branches.value.find((b) => b.current)?.name ?? "当前");
const viewingOther = computed(() => !!wiki.viewBranch && !branches.value.find((b) => b.name === wiki.viewBranch)?.current);

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

    <!-- 中部留空（原搜索 Wiki 占位已移除，搜索功能后续再立项）；可拖拽区由右侧补足 -->
    <div class="navbar-center" data-tauri-drag-region></div>

    <div class="navbar-right" data-tauri-drag-region>
      <!-- 全局分支切换（项目空间）：wiki/调用链/问答统一按所选分支工作 -->
      <a-popover
        v-if="inProjectSpace"
        v-model:open="branchMenuOpen"
        trigger="click"
        placement="bottomRight"
        overlay-class-name="bw-branch-pop"
      >
        <button
          class="icon-btn branch-btn"
          :class="{ on: viewingOther }"
          :title="viewingOther ? `正在查看 ${wiki.viewBranch}（只读）· 点击切回当前分支` : '切换查看分支'"
        >
          <BranchesOutlined />
          <span class="branch-btn-name">{{ branchLabel }}</span>
        </button>
        <template #content>
          <div class="bw-branch-list">
            <div class="bw-branch-head">按分支工作（非当前分支为只读查看）</div>
            <button
              v-for="b in branches"
              :key="b.name"
              class="bw-branch-item"
              :class="{ cur: b.current, sel: b.current ? !wiki.viewBranch : wiki.viewBranch === b.name }"
              @click="pickBranch(b)"
            >
              <span class="bw-branch-name">{{ b.is_default_bucket ? "（detached/默认桶）" : b.name }}</span>
              <span v-if="b.current" class="bw-branch-tag">当前</span>
              <span class="bw-branch-dots">
                <em v-if="b.has_wiki" title="有 Wiki 产物">W</em>
                <em v-if="b.has_callgraph" title="有调用链产物">C</em>
              </span>
            </button>
            <div v-if="!branches.length" class="bw-branch-empty">未解析到分支</div>
          </div>
        </template>
      </a-popover>
      <button
        v-if="inProjectSpace && typeof route.meta.tab === 'string' && route.meta.tab !== 'chat'"
        class="icon-btn ai-toggle"
        :class="{ on: ai.dockOpen }"
        title="AI 对话侧栏"
        @click="ai.dockOpen = !ai.dockOpen"
      >
        <AiSparkIcon :size="16" />
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
  min-width: 0;
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

/* 分支切换按钮（scoped 部分） */
.branch-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 160px;
}
.branch-btn .branch-btn-name {
  font-size: 12px;
  max-width: 110px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.branch-btn.on {
  color: var(--color-primary);
  background: var(--color-primary-bg);
}
</style>

<style>
/* 分支下拉（a-popover 传送层，非 scoped） */
.bw-branch-pop .bw-branch-list {
  min-width: 240px;
  max-height: 320px;
  overflow-y: auto;
}
.bw-branch-pop .bw-branch-head {
  font-size: 11px;
  color: var(--text-tertiary);
  padding: 2px 4px 8px;
  border-bottom: 1px solid var(--border-light);
  margin-bottom: 4px;
}
.bw-branch-pop .bw-branch-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  background: none;
  border-radius: 6px;
  cursor: pointer;
  color: var(--text-primary);
  font-size: 12.5px;
  text-align: left;
}
.bw-branch-pop .bw-branch-item:hover {
  background: var(--bg-surface);
}
.bw-branch-pop .bw-branch-item.sel {
  color: var(--color-primary);
  font-weight: 600;
}
.bw-branch-pop .bw-branch-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono, monospace);
}
.bw-branch-pop .bw-branch-tag {
  font-size: 10px;
  color: var(--color-primary);
  background: var(--color-primary-bg);
  border-radius: 4px;
  padding: 0 5px;
}
.bw-branch-pop .bw-branch-dots {
  display: inline-flex;
  gap: 3px;
}
.bw-branch-pop .bw-branch-dots em {
  font-style: normal;
  font-size: 10px;
  color: var(--text-tertiary);
  border: 1px solid var(--border-light);
  border-radius: 4px;
  padding: 0 4px;
}
.bw-branch-pop .bw-branch-empty {
  padding: 10px 4px;
  font-size: 12px;
  color: var(--text-tertiary);
}
</style>
