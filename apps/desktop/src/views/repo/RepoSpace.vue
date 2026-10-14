<script setup lang="ts">
/**
 * 项目空间 /repo/:id（总览 | /docs | /chat | /cards）
 *
 * 左侧菜单在项目模式下切换为项目专属菜单（见 constants.repoMenu）。
 * 本组件持有跨 pane 共享状态（文档清单/滞后报告/生成进度），按路由 meta.tab 渲染：
 * - 总览：仓库概览 + 生成/更新/索引
 * - 文档：WikiTree + WikiViewer
 * - 对话：ChatPanel 全高
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
  CloseOutlined,
} from "@ant-design/icons-vue";
import { useLayoutStore } from "@/stores/layout";
import { useAiStore } from "@/stores/ai";
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";
import type { IndexSummary } from "@/api/repo";
import type { WikiDocEntry, GenerateEvent, PageProgress } from "@/api/generate";
import type { StaleReport } from "@/api/watch";
import Panel from "@/components/shared/Panel.vue";
import EmptyState from "@/components/common/EmptyState.vue";
import WikiTree from "@/views/wiki/components/WikiTree.vue";
import WikiViewer from "@/views/wiki/components/WikiViewer.vue";
import ChatPanel from "@/views/wiki/components/ChatPanel.vue";
import CallGraphPanel from "@/views/wiki/components/CallGraphPanel.vue";
import FileBrowse from "@/views/wiki/components/FileBrowse.vue";
import LlmLogPanel from "@/views/wiki/components/LlmLogPanel.vue";
import GenProgress from "@/views/wiki/components/GenProgress.vue";

const route = useRoute();
const router = useRouter();
const layout = useLayoutStore();
const ai = useAiStore();
const wiki = useWikiStore();

const repoId = computed(() => Number(route.params.id));
const tab = computed(() => (route.meta.tab as string) || "overview");

// ===== 共享状态 =====
const wikiDocs = ref<WikiDocEntry[]>([]);
const currentDoc = ref("");
const staleReport = ref<StaleReport | null>(null);
const genProgress = ref<GenerateEvent | null>(null);
/** 页面级进度清单（大纲确定建骨架，此后按 id 合并增量） */
const genPages = ref<Map<string, PageProgress>>(new Map());
const genSummary = ref<{ pages: number; failed: number; prompt: number; completion: number; ms: number } | null>(null);
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

const cards = computed(() => wikiDocs.value.filter(([, kind]) => kind === "card"));

/** 页面清单按章节分组（保持大纲顺序） */
const genPageGroups = computed(() => {
  const groups: { section: string; items: PageProgress[] }[] = [];
  for (const p of genPages.value.values()) {
    const last = groups[groups.length - 1];
    if (last && last.section === p.section) {
      last.items.push(p);
    } else {
      groups.push({ section: p.section, items: [p] });
    }
  }
  return groups;
});

/** 大纲骨架折叠态（页多时默认收起，只看进度条） */
const pagesPanelOpen = ref(false);
/** 本次生成/更新开始时间（进度面板耗时计时） */
const genStartedAt = ref<number | null>(null);

// ===== 事件订阅 =====
let unlisten: Awaited<ReturnType<typeof api.onGenerateEvent>> | null = null;
onMounted(async () => {
  unlisten = await api.onGenerateEvent((e) => {
    genProgress.value = e;
    if (e.pages?.length) {
      // 大纲确定（outlining 带骨架）重置清单；其余为单页增量合并
      const isSkeleton = e.phase === "outlining";
      if (isSkeleton) genPages.value = new Map();
      const next = isSkeleton ? new Map<string, PageProgress>() : new Map(genPages.value);
      for (const p of e.pages) next.set(p.id, p);
      genPages.value = next;
      if (isSkeleton) pagesPanelOpen.value = true;
    }
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
  genPages.value = new Map();
  genStartedAt.value = Date.now();
  try {
    const s = await api.generateWiki(wiki.currentRepo.path);
    genSummary.value = {
      pages: s.pages_generated,
      failed: s.pages_failed,
      prompt: s.prompt_tokens,
      completion: s.completion_tokens,
      ms: s.elapsed_ms,
    };
    await refreshRepoInfo();
    await refreshDocs();
    await refreshStaleness();
    const failNote = s.pages_failed ? `（${s.pages_failed} 页失败）` : "";
    message.success(`Wiki 已生成：${s.pages_generated} 个页面文档${failNote}`);
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
  genPages.value = new Map();
  genStartedAt.value = Date.now();
  try {
    const s = await api.updateWiki(wiki.currentRepo.path);
    if (s.unchanged) {
      message.info("代码无变更，无需更新");
    } else {
      message.success(`已更新 ${s.regenerated_pages} 个页面文档（${s.changed_files} 个文件变更）`);
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

/** wiki 引用链接（bw-file://）→ 文件浏览页定位 */
function openFileAt(target: { path: string; startLine?: number; endLine?: number }) {
  wiki.revealFile(target.path, target.startLine, target.endLine);
  router.push(`/repo/${repoId.value}/files`);
}

function fmtTime(iso: string | null): string {
  if (!iso) return "—";
  return iso.replace("T", " ").replace("Z", "");
}
const busy = computed(() => generating.value || updating.value);
</script>

<template>
  <div class="repo-space">
    <div class="rs-body">
      <div class="rs-main">
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
            </div>

            <!-- 生成进度（大纲骨架 + 页面级清单） -->
            <GenProgress
              v-if="genProgress && (busy || genProgress.phase === 'done')"
              :progress="genProgress"
              :groups="genPageGroups"
              :panel-open="pagesPanelOpen"
              :started-at="genStartedAt"
              @toggle-panel="pagesPanelOpen = !pagesPanelOpen"
            />
            <p v-if="genSummary" class="hint">
              生成完成：{{ genSummary.pages }} 个页面文档{{ genSummary.failed ? `（${genSummary.failed} 失败）` : "" }} ·
              prompt {{ genSummary.prompt }} / completion {{ genSummary.completion }} tokens · 耗时
              {{ (genSummary.ms / 1000).toFixed(1) }}s
            </p>

            <EmptyState
              v-if="!wikiDocs.length"
              banner
              :icon="BookOutlined"
              text="尚未生成 Wiki"
              hint="一键生成 = 索引 → LLM 设计大纲 → 逐页深度撰写（绑定真实源码）→ 组装写入 .bytewiki/"
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
              <MessageOutlined /> 对话
            </button>
            <button class="btn" @click="cardModalOpen = true">
              <FileAddOutlined /> 知识卡
            </button>
          </div>
        </div>
        <GenProgress
          v-if="genProgress && busy"
          class="browse-progress"
          :progress="genProgress"
          :groups="genPageGroups"
          :panel-open="pagesPanelOpen"
          :started-at="genStartedAt"
          @toggle-panel="pagesPanelOpen = !pagesPanelOpen"
        />

        <div class="browse-layout">
          <aside class="tree-pane">
            <WikiTree :docs="wikiDocs" :current="currentDoc" @select="currentDoc = $event" />
          </aside>
          <section class="viewer-pane">
            <WikiViewer
              v-if="currentDoc"
              :repo-path="wiki.currentRepo.path"
              :doc-path="currentDoc"
              @open-doc="openDocInDocs"
              @open-file="openFileAt"
            />
            <EmptyState v-else text="暂无文档" hint="左侧选择文档，或先生成 Wiki" />
          </section>
        </div>
      </div>

      <!-- ============ 文件浏览 ============ -->
      <div v-else-if="tab === 'files'" class="files-pane">
        <FileBrowse />
      </div>

      <!-- ============ 模型日志 ============ -->
      <div v-else-if="tab === 'llm-logs'" class="llm-logs-pane">
        <LlmLogPanel :repo-path="wiki.currentRepo.path" />
      </div>

      <!-- ============ 调用链 ============ -->
      <div v-else-if="tab === 'callgraph'" class="callgraph-pane">
        <CallGraphPanel :repo-path="wiki.currentRepo.path" />
      </div>

      <!-- ============ 对话 ============ -->
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
            <span class="hint">cards/ 为人工知识区：重新生成与增量更新不会覆盖，且可被问答检索</span>
          </div>
          <EmptyState
            v-if="!cards.length"
            banner
            :icon="BookOutlined"
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
      </div>

      <!-- AI 对话侧栏：内嵌布局右侧（VSCode 式挤压内容区，非浮层）；chat tab 本身即对话页不重复显示 -->
      <aside v-if="ai.dockOpen && tab !== 'chat' && wiki.currentRepo" class="rs-ai-dock">
        <div class="ai-dock-head">
          <span class="ai-dock-title">AI 对话</span>
          <button class="btn sm" title="收起侧栏" @click="ai.dockOpen = false">
            <CloseOutlined />
          </button>
        </div>
        <ChatPanel :repo-path="wiki.currentRepo.path" @open-doc="openDocInDocs" />
      </aside>
    </div>

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
/* 双栏：主内容 + AI 侧栏（内嵌挤压，非浮层） */
.rs-body {
  flex: 1;
  min-height: 0;
  display: flex;
}
.rs-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.rs-ai-dock {
  width: 400px;
  flex-shrink: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--border-light);
  background: var(--bg-card);
}
.ai-dock-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px 8px 14px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.ai-dock-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-secondary);
  letter-spacing: 0.04em;
}
.rs-ai-dock > :deep(.chat-dock) {
  flex: 1;
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

/* 模型日志 */
.llm-logs-pane {
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

/* 对话 */
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
.browse-progress {
  padding: 0;
  flex-shrink: 0;
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
