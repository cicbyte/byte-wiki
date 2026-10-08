<script setup lang="ts">
/**
 * 仓库 Wiki 首页 /
 *
 * 未打开仓库：打开入口（选择目录 / 输入路径）+ 最近仓库列表。
 * 已打开仓库：总览（名称/路径/分支/文件数）+ 后续生成入口（F07 接入）。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import { message } from "ant-design-vue";
import {
  FolderOpenOutlined,
  FolderOutlined,
  BranchesOutlined,
  FileOutlined,
  ReloadOutlined,
  DatabaseOutlined,
  ThunderboltOutlined,
} from "@ant-design/icons-vue";
import { useLayoutStore } from "@/stores/layout";
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";
import type { IndexSummary } from "@/api/repo";
import type { GenerateEvent } from "@/api/generate";
import Panel from "@/components/shared/Panel.vue";
import EmptyState from "@/components/common/EmptyState.vue";
import WikiTree from "./components/WikiTree.vue";
import WikiViewer from "./components/WikiViewer.vue";
import ChatPanel from "./components/ChatPanel.vue";
import type { WikiDocEntry } from "@/api/generate";
import type { StaleReport } from "@/api/watch";
import { MessageOutlined, ClusterOutlined, SyncOutlined, FileAddOutlined } from "@ant-design/icons-vue";

const layout = useLayoutStore();
const wiki = useWikiStore();
const pathInput = ref("");
const indexing = ref(false);
const indexSummary = ref<IndexSummary | null>(null);
const generating = ref(false);
const genProgress = ref<GenerateEvent | null>(null);
const genSummary = ref<{ modules: number; prompt: number; completion: number; ms: number } | null>(null);

let unlisten: Awaited<ReturnType<typeof api.onGenerateEvent>> | null = null;
/** 已生成的文档清单与当前选中项（浏览模式） */
const wikiDocs = ref<WikiDocEntry[]>([]);
const currentDoc = ref("");
/** 问答面板开合 */
const chatOpen = ref(false);
/** 向量重建中 */
const rebuilding = ref(false);
/** 变更检测（滞后报告） */
const staleReport = ref<StaleReport | null>(null);
const updating = ref(false);
/** 滞后提示（前 10 条变更） */
const staleTooltip = computed(() => {
  if (!staleReport.value) return "";
  const lines = staleReport.value.changes
    .slice(0, 10)
    .map((c) => `${c.kind}: ${c.path}`);
  return lines.join("\n");
});

/** 新建知识卡弹窗 */
const cardModalOpen = ref(false);
const cardTitle = ref("");
const cardContent = ref("");
const creatingCard = ref(false);

async function refreshStaleness() {
  if (!wiki.currentRepo) {
    staleReport.value = null;
    return;
  }
  try {
    staleReport.value = await api.checkStaleness(wiki.currentRepo.path);
  } catch {
    staleReport.value = null;
  }
}

/** 增量更新（FR-06：手动触发，仅重生成受影响模块） */
async function runUpdate() {
  if (!wiki.currentRepo || updating.value) return;
  updating.value = true;
  genProgress.value = null;
  try {
    const s = await api.updateWiki(wiki.currentRepo.path);
    if (s.unchanged) {
      message.info("代码无变更，无需更新");
    } else {
      message.success(`已更新 ${s.regenerated_modules} 个模块文档（${s.changed_files} 个文件变更）`);
    }
    await wiki.openRepo(wiki.currentRepo.path);
    await refreshDocs();
    await refreshStaleness();
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "更新失败");
  } finally {
    updating.value = false;
  }
}

/** 新建知识卡 */
async function createCard() {
  if (!wiki.currentRepo || !cardTitle.value.trim()) return;
  creatingCard.value = true;
  try {
    const rel = await api.createCard(wiki.currentRepo.path, cardTitle.value.trim(), cardContent.value);
    cardModalOpen.value = false;
    cardTitle.value = "";
    cardContent.value = "";
    await refreshDocs();
    currentDoc.value = rel;
    message.success("知识卡已创建（生成/更新不会覆盖它）");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "创建失败");
  } finally {
    creatingCard.value = false;
  }
}

async function rebuildEmbeddings() {
  if (!wiki.currentRepo || rebuilding.value) return;
  rebuilding.value = true;
  try {
    const s = await api.rebuildEmbeddings(wiki.currentRepo.path);
    message.success(`向量索引完成：${s.embedded}/${s.chunked} 块`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "重建失败");
  } finally {
    rebuilding.value = false;
  }
}

/** 问答出处跳转：打开对应 wiki 文档 */
function onOpenDoc(docPath: string) {
  if (wikiDocs.value.some(([p]) => p === docPath)) {
    currentDoc.value = docPath;
  }
}

async function refreshDocs() {
  if (!wiki.currentRepo) {
    wikiDocs.value = [];
    return;
  }
  try {
    wikiDocs.value = await api.listWikiDocs(wiki.currentRepo.path);
    if (wikiDocs.value.length && !wikiDocs.value.some(([p]) => p === currentDoc.value)) {
      currentDoc.value = wikiDocs.value[0][0];
    }
  } catch {
    wikiDocs.value = [];
  }
}

onMounted(async () => {
  layout.setStatusItems([{ key: "view", text: "仓库 Wiki" }]);
  wiki.refreshRecent();
  refreshDocs();
  refreshStaleness();
  unlisten = await api.onGenerateEvent((e) => {
    genProgress.value = e;
  });
});
onUnmounted(() => {
  unlisten?.();
});

onMounted(() => {
  layout.setStatusItems([{ key: "view", text: "仓库 Wiki" }]);
  wiki.refreshRecent();
});

/** 选择目录打开 */
async function pickAndOpen() {
  try {
    const dir = await api.pickDirectory();
    if (dir) await doOpen(dir);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "选择目录失败");
  }
}

/** 输入路径打开 */
async function openFromInput() {
  const p = pathInput.value.trim();
  if (!p) return;
  await doOpen(p);
}

async function doOpen(path: string) {
  try {
    const info = await wiki.openRepo(path);
    message.success(`已打开仓库：${info.name}`);
    pathInput.value = "";
    indexSummary.value = null;
    // 打开即加载既有文档清单（含克隆场景扫描回退）与滞后状态
    await refreshDocs();
    await refreshStaleness();
    if (wikiDocs.value.length) {
      message.info(`已加载既有 Wiki：${wikiDocs.value.length} 篇文档`);
    }
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "打开仓库失败");
  }
}

/** 索引当前仓库 */
async function runIndex() {
  if (!wiki.currentRepo) return;
  indexing.value = true;
  try {
    indexSummary.value = await api.indexRepo(wiki.currentRepo.path);
    // 刷新仓库信息（last_indexed_at）
    await wiki.openRepo(wiki.currentRepo.path);
    message.success(`索引完成：${indexSummary.value.files_indexed} 个文件`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "索引失败");
  } finally {
    indexing.value = false;
  }
}

/** 一键生成 Wiki */
async function runGenerate() {
  if (!wiki.currentRepo) return;
  generating.value = true;
  genProgress.value = null;
  genSummary.value = null;
  try {
    const s = await api.generateWiki(wiki.currentRepo.path);
    genSummary.value = {
      modules: s.modules_generated,
      prompt: s.prompt_tokens,
      completion: s.completion_tokens,
      ms: s.elapsed_ms,
    };
    await wiki.openRepo(wiki.currentRepo.path);
    await refreshDocs();
    await refreshStaleness();
    message.success(`Wiki 已生成：${s.modules_generated} 个模块文档`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "生成失败");
  } finally {
    generating.value = false;
  }
}

/** 进度事件的阶段文案 */
const PHASE_TEXT: Record<string, string> = {
  indexing: "建立索引",
  checking: "检测变更",
  planning: "规划文档结构",
  generating: "生成模块文档",
  assembling: "组装写入",
  embedding: "重建向量索引",
  done: "完成",
  error: "失败",
};

function fmtTime(iso: string | null): string {
  if (!iso) return "—";
  return iso.replace("T", " ").replace("Z", "");
}
</script>

<template>
  <div class="wiki-view">
    <!-- 未打开仓库：打开入口 + 最近列表 -->
    <template v-if="!wiki.currentRepo">
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

      <Panel title="最近打开" :icon="ReloadOutlined">
        <EmptyState
          v-if="!wiki.recentRepos.length"
          :icon="FolderOutlined"
          text="暂无最近仓库"
          hint="打开过的仓库会出现在这里，点击即可重新打开"
        />
        <ul v-else class="recent-list">
          <li v-for="repo in wiki.recentRepos" :key="repo.id" class="recent-item" @click="doOpen(repo.path)">
            <FolderOutlined class="repo-icon" />
            <div class="repo-main">
              <span class="repo-name">{{ repo.name }}</span>
              <span class="repo-path">{{ repo.path }}</span>
            </div>
            <div class="repo-meta">
              <span class="meta-chip"><BranchesOutlined /> {{ repo.branch || "—" }}</span>
              <span class="meta-chip"><FileOutlined /> {{ repo.file_count }} 文件</span>
            </div>
          </li>
        </ul>
      </Panel>
    </template>

    <!-- 已打开仓库 -->
    <template v-else>
      <!-- 浏览模式：已生成文档 → 左树 + 右内容 -->
      <template v-if="wikiDocs.length">
        <div class="browse-toolbar">
          <span class="browse-title">
            <FolderOutlined /> {{ wiki.currentRepo.name }}
            <em v-if="wiki.currentRepo.branch">@{{ wiki.currentRepo.branch }}</em>
            <span
              v-if="staleReport && staleReport.stale"
              class="stale-badge"
              title="检测到代码变更，文档可能滞后"
            >
              文档滞后
            </span>
          </span>
          <div class="browse-actions">
            <button
              v-if="staleReport && staleReport.stale"
              class="storage-btn stale-btn"
              :disabled="updating || generating"
              :title="staleReport.changes.slice(0, 10).map((c) => `${c.kind}: ${c.path}`).join('\n')"
              @click="runUpdate"
            >
              <SyncOutlined :spin="updating" />
              {{ updating ? "更新中…" : `更新（${staleReport.changes.length} 变更）` }}
            </button>
            <button class="storage-btn" :disabled="generating" @click="runGenerate">
              <ThunderboltOutlined />
              {{ generating ? "生成中…" : "重新生成" }}
            </button>
            <button class="storage-btn" :disabled="rebuilding" @click="rebuildEmbeddings">
              <ClusterOutlined />
              {{ rebuilding ? "重建索引中…" : "重建索引" }}
            </button>
            <button
              class="storage-btn"
              :class="{ 'primary-soft': chatOpen }"
              @click="chatOpen = !chatOpen"
            >
              <MessageOutlined />
              问答
            </button>
            <button class="storage-btn" @click="cardModalOpen = true">
              <FileAddOutlined />
              知识卡
            </button>
            <button class="storage-btn" @click="wiki.closeRepo()">
              <FolderOpenOutlined />
              切换仓库
            </button>
          </div>
        </div>
        <div v-if="genProgress && generating" class="gen-progress browse-progress">
          <div class="gen-phase">
            <span class="phase-chip active">{{ PHASE_TEXT[genProgress.phase] || genProgress.phase }}</span>
            <span v-if="genProgress.total" class="phase-count">
              {{ Math.min(genProgress.current, genProgress.total) }} / {{ genProgress.total }}
            </span>
            <span class="phase-msg">{{ genProgress.message }}</span>
          </div>
          <div v-if="genProgress.total" class="progress-track">
            <div
              class="progress-fill"
              :style="{ width: `${Math.round((Math.min(genProgress.current, genProgress.total) / genProgress.total) * 100)}%` }"
            ></div>
          </div>
        </div>

        <div class="browse-layout">
          <aside class="tree-pane">
            <WikiTree :docs="wikiDocs" :current="currentDoc" @select="currentDoc = $event" />
          </aside>
          <section class="viewer-pane">
            <WikiViewer :repo-path="wiki.currentRepo.path" :doc-path="currentDoc" />
          </section>
          <aside v-if="chatOpen" class="chat-pane">
            <ChatPanel :repo-path="wiki.currentRepo.path" @open-doc="onOpenDoc" />
          </aside>
        </div>
      </template>

      <!-- 总览模式：尚未生成 -->
      <Panel v-else :title="`仓库总览 — ${wiki.currentRepo.name}`" :icon="FolderOutlined">
        <div class="overview">
          <div class="stat-grid">
            <div class="stat-cell">
              <span class="stat-label">分支</span>
              <span class="stat-value">{{ wiki.currentRepo.branch || "—" }}</span>
            </div>
            <div class="stat-cell">
              <span class="stat-label">文件数</span>
              <span class="stat-value">{{ wiki.currentRepo.file_count }}</span>
            </div>
            <div class="stat-cell">
              <span class="stat-label">上次索引</span>
              <span class="stat-value">{{ fmtTime(wiki.currentRepo.last_indexed_at) }}</span>
            </div>
            <div class="stat-cell">
              <span class="stat-label">上次生成</span>
              <span class="stat-value">{{ fmtTime(wiki.currentRepo.last_generated_at) }}</span>
            </div>
          </div>
          <div class="path-line">
            <span class="stat-label">路径</span>
            <code class="repo-path-code">{{ wiki.currentRepo.path }}</code>
          </div>
          <div class="overview-actions">
            <button class="storage-btn primary" :disabled="generating" @click="runGenerate">
              <ThunderboltOutlined />
              {{ generating ? "生成中…" : "一键生成 Wiki" }}
            </button>
            <button class="storage-btn" :disabled="indexing || generating" @click="runIndex">
              <DatabaseOutlined />
              {{ indexing ? "索引中…" : "仅索引" }}
            </button>
            <button class="storage-btn" @click="wiki.closeRepo()">
              <FolderOpenOutlined />
              切换仓库
            </button>
          </div>

          <!-- 生成进度 -->
          <div v-if="genProgress" class="gen-progress">
            <template v-if="generating || genProgress.phase === 'done'">
              <div class="gen-phase">
                <span class="phase-chip" :class="{ active: genProgress.phase === 'generating' }">
                  {{ PHASE_TEXT[genProgress.phase] || genProgress.phase }}
                </span>
                <span v-if="genProgress.total" class="phase-count">
                  {{ Math.min(genProgress.current, genProgress.total) }} / {{ genProgress.total }}
                </span>
                <span class="phase-msg">{{ genProgress.message }}</span>
              </div>
              <div v-if="genProgress.total" class="progress-track">
                <div
                  class="progress-fill"
                  :style="{ width: `${Math.round((Math.min(genProgress.current, genProgress.total) / genProgress.total) * 100)}%` }"
                ></div>
              </div>
            </template>
          </div>
          <p v-if="genSummary" class="hint">
            生成完成：{{ genSummary.modules }} 个模块文档 · prompt {{ genSummary.prompt }} /
            completion {{ genSummary.completion }} tokens · 耗时 {{ (genSummary.ms / 1000).toFixed(1) }}s
            （浏览文档的 WikiTree/Viewer 在下一版本接入 F09）
          </p>

          <EmptyState
            v-if="!indexSummary && !genSummary"
            text="尚未生成 Wiki"
            hint="一键生成 = 索引 → 规划 → 逐模块文档 → 组装写入 .bytewiki/"
          />
          <div v-if="indexSummary" class="index-summary">
            <p class="hint">
              索引完成：{{ indexSummary.files_indexed }} 个文件、
              {{ indexSummary.modules.length }} 个模块、{{ indexSummary.edges }} 条引用边。
            </p>
            <div class="module-chips">
              <span v-for="m in indexSummary.modules" :key="m.name" class="module-chip">
                {{ m.name }}
                <em>{{ m.file_count }} 文件 · {{ m.langs.join("/") || "—" }}</em>
              </span>
            </div>
          </div>
        </div>
      </Panel>
    </template>

    <!-- 新建知识卡弹窗（已打开仓库时可用） -->
    <a-modal
      v-model:open="cardModalOpen"
      title="新建知识卡"
      ok-text="创建"
      cancel-text="取消"
      :confirm-loading="creatingCard"
      :ok-button-props="{ disabled: !cardTitle.trim() }"
      @ok="createCard"
    >
      <p class="hint" style="margin: 0 0 12px">
        写入 .bytewiki/cards/（人工知识区）：重新生成与增量更新不会覆盖它，且可被问答检索。
      </p>
      <a-input v-model:value="cardTitle" placeholder="标题（必填）" style="margin-bottom: 10px" />
      <a-textarea
        v-model:value="cardContent"
        placeholder="Markdown 内容，如部署要点、踩坑记录、接口约定…"
        :rows="8"
      />
    </a-modal>
  </div>
</template>

<style scoped>
.wiki-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--spacing-md);
  overflow-y: auto;
  gap: var(--spacing-md);
}
.wiki-view :deep(.panel) {
  min-height: 160px;
}

/* 浏览模式（树 + 内容） */
.browse-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-md);
  flex-shrink: 0;
}
.browse-title {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.browse-title em {
  font-style: normal;
  font-size: 12px;
  font-weight: 400;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-tertiary);
}
.browse-actions {
  display: flex;
  gap: var(--spacing-sm);
}
.browse-layout {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--spacing-md);
}
.tree-pane {
  width: 240px;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
}
.viewer-pane {
  flex: 1;
  min-width: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
}
.browse-progress {
  padding: 0;
}
.stale-badge {
  font-size: 11px;
  font-weight: 600;
  color: #ea8a3c;
  background: color-mix(in oklch, #ea8a3c 14%, transparent);
  border-radius: var(--radius-xs);
  padding: 1px 8px;
}
.storage-btn.stale-btn {
  border-color: #ea8a3c;
  color: #ea8a3c;
}
.storage-btn.stale-btn:hover:not(:disabled) {
  border-color: #ea8a3c;
  color: #ea8a3c;
  background: color-mix(in oklch, #ea8a3c 10%, transparent);
}

.chat-pane {
  width: 360px;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
}
.storage-btn.primary-soft {
  border-color: var(--color-primary);
  color: var(--color-primary);
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

/* 最近列表 */
.recent-list {
  list-style: none;
  margin: 0;
  padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-md);
}
.recent-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--transition-fast);
}
.recent-item:hover {
  background: var(--color-primary-bg);
}
.recent-item + .recent-item {
  border-top: 1px solid var(--border-light);
}
.repo-icon {
  font-size: 18px;
  color: var(--color-primary);
  flex-shrink: 0;
}
.repo-main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}
.repo-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.repo-path {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.repo-meta {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
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
}

/* 总览 */
.overview {
  padding: var(--spacing-md) var(--spacing-lg);
  position: static;
}
.stat-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: var(--spacing-md);
  margin-bottom: var(--spacing-md);
}
.stat-cell {
  display: flex;
  flex-direction: column;
  gap: 4px;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  padding: 10px 14px;
}
.stat-label {
  font-size: 11px;
  color: var(--text-tertiary);
}
.stat-value {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: "JetBrains Mono", monospace;
}
.path-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: var(--spacing-md);
}
.repo-path-code {
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 2px 8px;
  border-radius: var(--radius-xs);
  word-break: break-all;
}
.overview-actions {
  display: flex;
  gap: var(--spacing-sm);
  margin-bottom: var(--spacing-md);
}

/* 索引结果 */
.index-summary {
  padding: var(--spacing-sm) 0;
}

/* 生成进度 */
.gen-progress {
  padding: var(--spacing-xs) 0 var(--spacing-sm);
}
.gen-phase {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 6px;
}
.phase-chip {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 2px 10px;
  border-radius: var(--radius-xs);
}
.phase-chip.active {
  color: var(--color-primary);
  background: var(--color-primary-bg);
}
.phase-count {
  font-size: 12px;
  color: var(--text-primary);
  font-family: "JetBrains Mono", monospace;
}
.phase-msg {
  font-size: 12px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.progress-track {
  height: 4px;
  background: var(--bg-input);
  border-radius: 2px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: var(--color-primary);
  border-radius: 2px;
  transition: width var(--transition-normal);
}
.module-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}
.module-chip {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--bg-input);
  padding: 3px 10px;
  border-radius: var(--radius-xs);
}
.module-chip em {
  font-style: normal;
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
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
