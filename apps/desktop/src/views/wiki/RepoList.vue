<script setup lang="ts">
/**
 * 项目列表页 /（全局层入口）
 *
 * 打开新仓库（输入/浏览）+ 项目卡片网格（最近仓库）。点击卡片进入项目空间
 * /repo/:id（左侧菜单切换为该项目专属菜单）。
 */
import { onMounted, ref } from "vue";
import { message } from "ant-design-vue";
import {
  FolderOpenOutlined,
  FolderOutlined,
  BranchesOutlined,
  FileOutlined,
  ClockCircleOutlined,
} from "@ant-design/icons-vue";
import { useRouter } from "vue-router";
import { useLayoutStore } from "@/stores/layout";
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";
import Panel from "@/components/shared/Panel.vue";
import EmptyState from "@/components/common/EmptyState.vue";

const router = useRouter();
const layout = useLayoutStore();
const wiki = useWikiStore();
const pathInput = ref("");

onMounted(() => {
  layout.setStatusItems([{ key: "view", text: "项目列表" }]);
  wiki.refreshRecent();
});

async function pickAndOpen() {
  try {
    const dir = await api.pickDirectory();
    if (dir) await doOpen(dir);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "选择目录失败");
  }
}

async function openFromInput() {
  const p = pathInput.value.trim();
  if (!p) return;
  await doOpen(p);
}

/** 打开成功 → 进入项目空间总览 */
async function doOpen(path: string) {
  try {
    const info = await wiki.openRepo(path);
    message.success(`已打开仓库：${info.name}`);
    pathInput.value = "";
    router.push(`/repo/${info.id}`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "打开仓库失败");
  }
}

function fmtTime(iso: string | null): string {
  if (!iso) return "未生成";
  return iso.replace("T", " ").replace("Z", "");
}
</script>

<template>
  <div class="repo-list-view">
    <Panel title="打开仓库" :icon="FolderOpenOutlined">
      <div class="open-body">
        <div class="open-row">
          <a-input
            v-model:value="pathInput"
            placeholder="输入本地 Git 仓库路径，如 D:\code\my-repo"
            class="path-input"
            @keydown.enter="openFromInput"
          />
          <button class="storage-btn primary" :disabled="wiki.opening || !pathInput.trim()" @click="openFromInput">
            {{ wiki.opening ? "打开中…" : "打开" }}
          </button>
          <button class="storage-btn" :disabled="wiki.opening" @click="pickAndOpen">
            <FolderOpenOutlined />
            浏览…
          </button>
        </div>
        <p class="hint">仅支持 Git 仓库（含 .git 目录）；代码默认不出本机（local-first）。</p>
      </div>
    </Panel>

    <Panel title="项目" :icon="FolderOutlined" :count="wiki.recentRepos.length">
      <EmptyState
        v-if="!wiki.recentRepos.length"
        :icon="FolderOutlined"
        text="暂无项目"
        hint="打开过的仓库会出现在这里，点击进入项目空间"
      />
      <div v-else class="repo-grid">
        <div
          v-for="repo in wiki.recentRepos"
          :key="repo.id"
          class="repo-card"
          @click="router.push(`/repo/${repo.id}`)"
        >
          <div class="repo-card-head">
            <FolderOutlined class="repo-icon" />
            <span class="repo-name">{{ repo.name }}</span>
            <span v-if="repo.branch" class="meta-chip"><BranchesOutlined /> {{ repo.branch }}</span>
          </div>
          <div class="repo-card-path" :title="repo.path">{{ repo.path }}</div>
          <div class="repo-card-meta">
            <span class="meta-chip"><FileOutlined /> {{ repo.file_count }} 文件</span>
            <span class="meta-chip"><ClockCircleOutlined /> {{ fmtTime(repo.last_generated_at) }}</span>
          </div>
        </div>
      </div>
    </Panel>
  </div>
</template>

<style scoped>
.repo-list-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--spacing-md);
  overflow-y: auto;
  gap: var(--spacing-md);
}

/* 打开入口 */
.open-body {
  padding: var(--spacing-md) var(--spacing-lg) var(--spacing-sm);
}
.open-row {
  display: flex;
  gap: var(--spacing-sm);
}
.path-input {
  flex: 1;
}
.hint {
  color: var(--text-tertiary);
  font-size: 12px;
  margin-top: 10px;
  line-height: 1.6;
}

/* 项目卡片网格 */
.repo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--spacing-md);
  padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-lg);
}
.repo-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: var(--spacing-md);
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.repo-card:hover {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
}
.repo-card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.repo-icon {
  font-size: 16px;
  color: var(--color-primary);
  flex-shrink: 0;
}
.repo-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.repo-card-path {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.repo-card-meta {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.meta-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 2px 8px;
  border-radius: var(--radius-xs);
  font-family: "JetBrains Mono", monospace;
  flex-shrink: 0;
}

/* 复用设置页按钮规范 */
.storage-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--border-base);
  background: var(--bg-surface);
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 13px;
  transition: all var(--transition-fast);
}
.storage-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}
.storage-btn.primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #fff;
}
.storage-btn.primary:hover:not(:disabled) {
  background: var(--color-primary-hover);
  border-color: var(--color-primary-hover);
  color: #fff;
}
.storage-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
