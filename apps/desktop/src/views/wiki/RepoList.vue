<script setup lang="ts">
/**
 * 项目页 /（全局层入口）
 *
 * 居中欢迎区 + 单个「打开项目」主按钮；点击弹窗选择：
 * 最近项目（紧凑行列表，点击直达项目空间）或 输入/浏览路径打开。
 * 不再平铺项目卡片，避免挤压项目空间。
 */
import { onMounted, ref } from "vue";
import { message } from "ant-design-vue";
import {
  FolderOpenOutlined,
  FolderOutlined,
  BranchesOutlined,
  RightOutlined,
} from "@ant-design/icons-vue";
import { useRouter } from "vue-router";
import { useLayoutStore } from "@/stores/layout";
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";

const router = useRouter();
const layout = useLayoutStore();
const wiki = useWikiStore();
const pathInput = ref("");
const openModalVisible = ref(false);

onMounted(() => {
  layout.setStatusItems([{ key: "view", text: "项目" }]);
  wiki.refreshRecent();
});

function showOpen() {
  openModalVisible.value = true;
}

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
    openModalVisible.value = false;
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
  <div class="repo-home">
    <!-- 空状态：居中引导打开第一个项目 -->
    <div v-if="!wiki.recentRepos.length" class="home-hero">
      <div class="hero-icon"><FolderOpenOutlined /></div>
      <h1 class="hero-title">byte-wiki</h1>
      <p class="hero-sub">本地优先的仓库级知识引擎——打开一个 Git 仓库开始</p>
      <button class="btn primary hero-btn" @click="showOpen">
        <FolderOpenOutlined /> 打开项目
      </button>
      <p class="hero-hint">仅支持 Git 仓库（含 .git 目录）；代码默认不出本机（local-first）</p>
    </div>

    <!-- 有项目：后台页骨架（页头 + 操作）+ 卡片网格 -->
    <div v-else class="home-table">
      <div class="page-head">
        <div class="page-title">
          <h2>项目</h2>
          <span class="page-desc">{{ wiki.recentRepos.length }} 个仓库 · 点击卡片进入项目空间</span>
        </div>
        <button class="btn primary" @click="showOpen"><FolderOpenOutlined /> 打开项目</button>
      </div>

      <div class="card-grid">
        <div
          v-for="repo in wiki.recentRepos"
          :key="repo.id"
          class="repo-card"
          @click="router.push(`/repo/${repo.id}`)"
        >
          <div class="card-head">
            <FolderOutlined class="row-icon" />
            <span class="row-name">{{ repo.name }}</span>
            <span v-if="repo.branch" class="meta-chip"><BranchesOutlined /> {{ repo.branch }}</span>
            <RightOutlined class="go-icon" />
          </div>
          <div class="card-path" :title="repo.path">{{ repo.path }}</div>
          <div class="card-meta">
            <span>{{ repo.file_count }} 文件</span>
            <span class="dot">·</span>
            <span>{{ fmtTime(repo.last_generated_at) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 打开项目弹窗：按路径输入/浏览（最近项目已在首页常显） -->
    <a-modal
      v-model:open="openModalVisible"
      title="打开项目"
      :footer="null"
      width="560px"
    >
      <div class="open-block-modal">
        <div class="open-row">
          <a-input
            v-model:value="pathInput"
            placeholder="输入本地 Git 仓库路径，如 D:\code\my-repo"
            class="path-input"
            @keydown.enter="openFromInput"
          />
          <button class="btn primary" :disabled="wiki.opening || !pathInput.trim()" @click="openFromInput">
            {{ wiki.opening ? "打开中…" : "打开" }}
          </button>
          <button class="btn" :disabled="wiki.opening" @click="pickAndOpen">
            <FolderOpenOutlined />
            浏览…
          </button>
        </div>
        <p class="hint">仅支持 Git 仓库（含 .git 目录）；代码默认不出本机（local-first）。</p>
      </div>
    </a-modal>
  </div>
</template>

<style scoped>
.repo-home {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow-y: auto;
}
/* 居中欢迎区 */
.home-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 40px 24px;
  text-align: center;
}
.hero-icon {
  width: 64px;
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-lg, 16px);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-size: 28px;
  margin-bottom: 10px;
}
.hero-title {
  font-size: 22px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0;
  letter-spacing: 0.01em;
}
.hero-sub {
  font-size: 13px;
  color: var(--text-tertiary);
  margin: 0 0 18px;
}
.hero-btn {
  height: 38px;
  padding: 0 22px;
  font-size: 14px;
}
.hero-hint {
  font-size: 11.5px;
  color: var(--text-tertiary);
  margin: 14px 0 0;
}

/* 有项目：后台页骨架 + 卡片网格（左右窄边距）。
   repo-home 为空态 hero 居中而设 flex 居中——卡片模式必须显式全宽，否则被收缩到内容宽 */
.home-table {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 16px 20px;
  min-height: 0;
  gap: 14px;
}
/* 页头：标题 + 描述 + 主操作 */
.page-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  flex-shrink: 0;
}
.page-title {
  display: flex;
  align-items: baseline;
  gap: 10px;
}
.page-title h2 {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
}
.page-desc {
  font-size: 12px;
  color: var(--text-tertiary);
}

/* 卡片网格：占满剩余空间，滚动 */
.card-grid {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 14px;
  align-content: start;
}
.repo-card {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 14px 16px;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: border-color var(--transition-fast), background var(--transition-fast);
}
.repo-card:hover {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
}
.card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.row-icon {
  font-size: 17px;
  color: var(--color-primary);
  flex-shrink: 0;
}
.row-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.go-icon {
  font-size: 12px;
  color: var(--color-primary);
  margin-left: auto;
  opacity: 0;
  transform: translateX(-4px);
  transition: all var(--transition-fast);
  flex-shrink: 0;
}
.repo-card:hover .go-icon {
  opacity: 1;
  transform: translateX(0);
}
.card-path {
  font-family: "JetBrains Mono", monospace;
  font-size: 11.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.card-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  color: var(--text-secondary);
  font-family: "JetBrains Mono", monospace;
}
.card-meta .dot {
  color: var(--text-tertiary);
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

/* 弹窗内：路径打开 */
.open-block-modal .hint {
  color: var(--text-tertiary);
  font-size: 12px;
  margin: 10px 0 0;
  line-height: 1.6;
}
.open-row {
  display: flex;
  gap: var(--spacing-sm);
}
.path-input {
  flex: 1;
}
</style>
