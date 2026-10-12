<script setup lang="ts">
/**
 * 项目空间 /repo/:id（总览 | /docs | /chat | /cards）
 *
 * 左侧菜单在项目模式下切换为项目专属菜单（见 constants.repoMenu）。
 * 本组件持有跨 pane 共享状态（文档清单/滞后报告/生成进度），按路由 meta.tab 渲染：
 * - 总览：仓库概览 + 生成/更新/索引
 * - 文档：WikiTree + WikiViewer
 * - 问答：ChatPanel 全高
 * - 知识卡：卡片列表 + 新建
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { message } from "ant-design-vue";
import {
  ThunderboltOutlined,
  DatabaseOutlined,
  SyncOutlined,
  ClusterOutlined,
  FileAddOutlined,
  FileTextOutlined,
  MessageOutlined,
  BookOutlined,
  ArrowLeftOutlined,
} from "@ant-design/icons-vue";
import { useLayoutStore } from "@/stores/layout";
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";
import type { IndexSummary } from "@/api/repo";
import type { WikiDocEntry, GenerateEvent } from "@/api/generate";
import type { StaleReport } from "@/api/watch";
import Panel from "@/components/shared/Panel.vue";
import EmptyState from "@/components/common/EmptyState.vue";
import WikiTree from "@/views/wiki/components/WikiTree.vue";
import WikiViewer from "@/views/wiki/components/WikiViewer.vue";
import ChatPanel from "@/views/wiki/components/ChatPanel.vue";
import CallGraphPanel from "@/views/wiki/components/CallGraphPanel.vue";
import FileBrowse from "@/views/wiki/components/FileBrowse.vue";

const route = useRoute();
const router = useRouter();
const layout = useLayoutStore();
const wiki = useWikiStore();

const repoId = computed(() => Number(route.params.id));
const tab = computed(() => (route.meta.tab as string) || "overview");

// ===== 共享状态 =====
const wikiDocs = ref<WikiDocEntry[]>([]);
const currentDoc = ref("");
const staleReport = ref<StaleReport | null>(null);
const genProgress = ref<GenerateEvent | null>(null);
const genSummary = ref<{ modules: number; prompt: number; completion: number; ms: number } | null>(null);
const indexSummary = ref<IndexSummary | null>(null);
const generating = ref(false);
const updating = ref(false);
const indexing = ref(false);
const rebuilding = ref(false);
const cardModalOpen = ref(false);
const cardTitle = ref("");
const cardContent = ref("");
const creatingCard = ref(false);
const loadFailed = ref(false);


function openDocFromAi(docPath: string) {
  layout.aiDockOpen = false;
  openDocInDocs(docPath);
}

const cards = computed(() => wikiDocs.value.filter(([, kind]) => kind === "card"));

// ===== 事件订阅 =====
let unlisten: Awaited<ReturnType<typeof api.onGenerateEvent>> | null = null;
onMounted(async () => {
  unlisten = await api.onGenerateEvent((e) => {
    genProgress.value = e;
  });
});
onUnmounted(() => {
  unlisten?.();
});

// ===== 进入/切换项目 =====
async function enterRepo() {
  loadFailed.value = false;
  layout.setStatusItems([{ key: "tab", text: tabLabel() }]);
  const info = await wiki.ensureRepo(repoId.value);
  if (!info) {
    loadFailed.value = true;
    return;
  }
  await refreshDocs();
  await refreshStaleness();
}

function tabLabel(): string {
  return (route.meta.title as string) || "总览";
}

watch(
  () => [route.params.id, route.meta.tab],
  () => {
    if (route.path.startsWith("/repo/")) {
      layout.setStatusItems([{ key: "tab", text: tabLabel() }]);
    }
  },
);

onMounted(enterRepo);

// ===== 数据加载 =====
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

async function refreshRepoInfo() {
  if (wiki.currentRepo) {
    await wiki.openRepo(wiki.currentRepo.path);
  }
}

// ===== 生成 / 更新 / 索引 =====
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
    await refreshRepoInfo();
    await refreshDocs();
    await refreshStaleness();
    message.success(`Wiki 已生成：${s.modules_generated} 个模块文档`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "生成失败");
  } finally {
    generating.value = false;
  }
}

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
    await refreshRepoInfo();
    await refreshDocs();
    await refreshStaleness();
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "更新失败");
  } finally {
    updating.value = false;
  }
}

async function runIndex() {
  if (!wiki.currentRepo) return;
  indexing.value = true;
  try {
    indexSummary.value = await api.indexRepo(wiki.currentRepo.path);
    await refreshRepoInfo();
    message.success(`索引完成：${indexSummary.value.files_indexed} 个文件`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "索引失败");
  } finally {
    indexing.value = false;
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

// ===== 知识卡 =====
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

/** 卡片/出处跳转到文档视图 */
function openDocInDocs(docPath: string) {
  if (wikiDocs.value.some(([p]) => p === docPath)) {
    currentDoc.value = docPath;
    router.push(`/repo/${repoId.value}/docs`);
  }
}

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
const busy = computed(() => generating.value || updating.value);
</script>

<template>
  <div class="repo-space">
    <!-- 仓库失效 -->
    <Panel v-if="loadFailed" class="failed-panel">
      <EmptyState
        text="仓库不可用"
        hint="路径可能已被移动或删除，请返回项目列表重新打开"
      />
      <div class="failed-actions">
        <button class="btn" @click="router.push('/')">
          <ArrowLeftOutlined /> 返回项目列表
        </button>
      </div>
    </Panel>

    <template v-else-if="wiki.currentRepo">
      <!-- ============ 总览 ============ -->
      <div v-if="tab === 'overview'" class="pane-scroll">
        <Panel :title="`仓库总览 — ${wiki.currentRepo.name}`">
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
              <span
                v-if="staleReport && staleReport.stale"
                class="stale-badge"
                title="检测到代码变更，文档可能滞后"
              >
                文档滞后（{{ staleReport.changes.length }} 变更）
              </span>
            </div>

            <div class="overview-actions">
              <button
                v-if="staleReport && staleReport.stale"
                class="btn warn"
                :disabled="busy"
                @click="runUpdate"
              >
                <SyncOutlined :spin="updating" />
                {{ updating ? "更新中…" : `更新（${staleReport.changes.length} 变更）` }}
              </button>
              <button class="btn primary" :disabled="busy" @click="runGenerate">
                <ThunderboltOutlined />
                {{ generating ? "生成中…" : "一键生成 Wiki" }}
              </button>
              <button class="btn" :disabled="indexing || busy" @click="runIndex">
                <DatabaseOutlined />
                {{ indexing ? "索引中…" : "仅索引" }}
              </button>
              <button class="btn" :disabled="rebuilding" @click="rebuildEmbeddings">
                <ClusterOutlined />
                {{ rebuilding ? "重建索引中…" : "重建向量索引" }}
              </button>
              <button v-if="wikiDocs.length" class="btn" @click="router.push(`/repo/${repoId}/docs`)">
                浏览文档（{{ wikiDocs.length }}）
              </button>
              <button class="btn" @click="router.push('/')">
                <ArrowLeftOutlined /> 项目列表
              </button>
            </div>

            <!-- 生成进度 -->
            <div v-if="genProgress && (busy || genProgress.phase === 'done')" class="gen-progress">
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
            </div>
            <p v-if="genSummary" class="hint">
              生成完成：{{ genSummary.modules }} 个模块文档 · prompt {{ genSummary.prompt }} /
              completion {{ genSummary.completion }} tokens · 耗时 {{ (genSummary.ms / 1000).toFixed(1) }}s
            </p>

            <EmptyState
              v-if="!wikiDocs.length"
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
      </div>

      <!-- ============ 文档 ============ -->
      <div v-else-if="tab === 'docs'" class="docs-pane">
        <div class="browse-toolbar">
          <span class="browse-title">
            {{ wiki.currentRepo.name }}
            <em v-if="wiki.currentRepo.branch">@{{ wiki.currentRepo.branch }}</em>
            <span v-if="staleReport && staleReport.stale" class="stale-badge">文档滞后</span>
          </span>
          <div class="browse-actions">
            <button
              v-if="staleReport && staleReport.stale"
              class="btn warn"
              :disabled="busy"
              @click="runUpdate"
            >
              <SyncOutlined :spin="updating" />
              {{ updating ? "更新中…" : `更新（${staleReport.changes.length}）` }}
            </button>
            <button class="btn" :disabled="busy" @click="runGenerate">
              <ThunderboltOutlined />
              {{ generating ? "生成中…" : "重新生成" }}
            </button>
            <button class="btn" :disabled="rebuilding" @click="rebuildEmbeddings">
              <ClusterOutlined />
              {{ rebuilding ? "重建索引中…" : "重建索引" }}
            </button>
            <button class="btn" @click="router.push(`/repo/${repoId}/chat`)">
              <MessageOutlined /> 问答
            </button>
            <button class="btn" @click="cardModalOpen = true">
              <FileAddOutlined /> 知识卡
            </button>
            <button class="btn" @click="router.push('/')">
              <ArrowLeftOutlined /> 项目列表
            </button>
          </div>
        </div>
        <div v-if="genProgress && busy" class="gen-progress browse-progress">
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
            <WikiViewer v-if="currentDoc" :repo-path="wiki.currentRepo.path" :doc-path="currentDoc" />
            <EmptyState v-else text="暂无文档" hint="左侧选择文档，或先生成 Wiki" />
          </section>
        </div>
      </div>

      <!-- ============ 文件浏览 ============ -->
      <div v-else-if="tab === 'files'" class="files-pane">
        <FileBrowse />
      </div>

      <!-- ============ 调用链 ============ -->
      <div v-else-if="tab === 'callgraph'" class="callgraph-pane">
        <CallGraphPanel :repo-path="wiki.currentRepo.path" />
      </div>

      <!-- ============ 问答 ============ -->
      <div v-else-if="tab === 'chat'" class="chat-wrap">
        <div class="chat-toolbar">
          <span class="browse-title">
            {{ wiki.currentRepo.name }}
            <em v-if="wiki.currentRepo.branch">@{{ wiki.currentRepo.branch }}</em>
          </span>
          <div class="browse-actions">
            <button class="btn" @click="router.push(`/repo/${repoId}/docs`)">
              <FileTextOutlined /> 文档
            </button>
            <button class="btn" @click="router.push('/')">
              <ArrowLeftOutlined /> 项目列表
            </button>
          </div>
        </div>
        <div class="chat-tab">
          <ChatPanel :repo-path="wiki.currentRepo.path" @open-doc="openDocInDocs" />
        </div>
      </div>

      <!-- ============ 知识卡 ============ -->
      <div v-else-if="tab === 'cards'" class="pane-scroll">
        <Panel title="知识卡" :icon="BookOutlined" :count="cards.length">
          <div class="cards-actions">
            <button class="btn primary" @click="cardModalOpen = true">
              <FileAddOutlined /> 新建知识卡
            </button>
            <button class="btn" @click="router.push('/')">
              <ArrowLeftOutlined /> 项目列表
            </button>
            <span class="hint">cards/ 为人工知识区：重新生成与增量更新不会覆盖，且可被问答检索</span>
          </div>
          <EmptyState
            v-if="!cards.length"
            text="暂无知识卡"
            hint="记录部署要点、踩坑经验、接口约定等高密度知识"
          />
          <ul v-else class="card-list">
            <li v-for="[path, , title] in cards" :key="path" class="card-item" @click="openDocInDocs(path)">
              <BookOutlined class="card-icon" />
              <span class="card-title">{{ title }}</span>
              <code class="card-path">{{ path }}</code>
            </li>
          </ul>
        </Panel>
      </div>
    </template>

    <!-- 加载中 -->
    <div v-else class="loading-pane">
      <EmptyState text="加载中…" />
    </div>

    <!-- AI 对话抽屉：顶栏按钮触发（layout.aiDockOpen 全局态） -->
    <a-drawer
      v-model:open="layout.aiDockOpen"
      title="AI 问答"
      placement="right"
      :width="440"
      :body-style="{ padding: '0', display: 'flex', flexDirection: 'column' }"
      :header-style="{ padding: '10px 16px' }"
    >
      <ChatPanel :repo-path="wiki.currentRepo!.path" @open-doc="openDocFromAi" />
    </a-drawer>

    <!-- 新建知识卡弹窗 -->
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
.repo-space {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.pane-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--spacing-md);
}
.loading-pane,
.failed-panel {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.failed-actions {
  display: flex;
  justify-content: center;
  margin-top: 8px;
}

/* 总览 */
.overview {
  padding: var(--spacing-md) var(--spacing-lg);
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
  flex-wrap: wrap;
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
  flex-wrap: wrap;
}
.index-summary {
  padding: var(--spacing-sm) 0;
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

/* 文档 */
.docs-pane {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: var(--spacing-md);
  gap: var(--spacing-md);
}
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
  flex-wrap: wrap;
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
  display: flex;
}

/* 文件浏览 */
.files-pane {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: var(--spacing-md);
}

/* 调用链 */
.callgraph-pane {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: var(--spacing-md);
}

/* 问答 */
.chat-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: var(--spacing-md);
  gap: var(--spacing-md);
}
.chat-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-md);
  flex-shrink: 0;
}
.chat-tab {
  flex: 1;
  min-height: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
}

/* 知识卡 */
.cards-actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-sm);
}
.card-list {
  list-style: none;
  margin: 0;
  padding: var(--spacing-sm) var(--spacing-lg) var(--spacing-lg);
  display: flex;
  flex-direction: column;
}
.card-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--transition-fast);
}
.card-item:hover {
  background: var(--color-primary-bg);
}
.card-item + .card-item {
  border-top: 1px solid var(--border-light);
}
.card-icon {
  color: var(--color-primary);
  flex-shrink: 0;
}
.card-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.card-path {
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 进度与徽标 */
.gen-progress {
  padding: var(--spacing-xs) 0 var(--spacing-sm);
}
.browse-progress {
  padding: 0;
  flex-shrink: 0;
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
.stale-badge {
  font-size: 11px;
  font-weight: 600;
  color: #ea8a3c;
  background: color-mix(in oklch, #ea8a3c 14%, transparent);
  border-radius: var(--radius-xs);
  padding: 1px 8px;
}
.hint {
  color: var(--text-tertiary);
  font-size: 12px;
  line-height: 1.6;
}

</style>
