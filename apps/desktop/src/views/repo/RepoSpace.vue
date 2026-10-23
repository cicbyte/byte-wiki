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
  BookOutlined,
  ArrowLeftOutlined,
  CloseOutlined,
  ExportOutlined,
} from "@ant-design/icons-vue";
import { useLayoutStore } from "@/stores/layout";
import { useAiStore } from "@/stores/ai";
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";
import type { IndexSummary } from "@/api/repo";
import type { WikiDocEntry, GenerateEvent, PageProgress, RepoOverview } from "@/api/generate";
import type { StaleReport } from "@/api/watch";
import Panel from "@/components/shared/Panel.vue";
import EmptyState from "@/components/common/EmptyState.vue";
import WikiTree from "@/views/wiki/components/WikiTree.vue";
import WikiViewer from "@/views/wiki/components/WikiViewer.vue";
import ChatPanel from "@/views/wiki/components/ChatPanel.vue";
import CallGraphPanel from "@/views/wiki/components/CallGraphPanel.vue";
import FileBrowse from "@/views/wiki/components/FileBrowse.vue";
import SourceDrawer from "@/views/wiki/components/SourceDrawer.vue";
import type { SourceTarget } from "@/views/wiki/components/SourceDrawer.vue";
import LlmLogPanel from "@/views/wiki/components/LlmLogPanel.vue";
import LlmStreamTerminal from "@/views/wiki/components/LlmStreamTerminal.vue";
import { useLlmStreamStore } from "@/stores/llmStream";

const route = useRoute();
const router = useRouter();
const layout = useLayoutStore();
const ai = useAiStore();
const wiki = useWikiStore();
const llmStream = useLlmStreamStore();

const repoId = computed(() => Number(route.params.id));
const tab = computed(() => (route.meta.tab as string) || "overview");

// ===== 共享状态 =====
const wikiDocs = ref<WikiDocEntry[]>([]);
/** 大纲既定顺序的页面路径（磁盘树按大纲序渲染；骨架树内部自取 genPages 序） */
const outlineOrder = ref<string[]>([]);
/** 只读查看分支（全局：Navbar 分支切换器写入 store，本地 computed 代理保持既有引用） */
const viewBranch = computed({
  get: () => wiki.viewBranch,
  set: (v: string | null) => wiki.setViewBranch(v),
});
/** 已生成且非当前的分支（空态直达） */
const switchableBranches = computed(() =>
  generatedBranchList.value.filter((b) => b !== (wiki.currentRepo?.branch || "")),
);
/** 已生成分支选项（branches/ 下有产物的目录） */
const generatedBranchList = ref<string[]>([]);

/** 是否处于只读查看（非当前分支） */
const readonlyBranch = computed(
  () => !!viewBranch.value && viewBranch.value !== wiki.currentRepo?.branch,
);
const currentDoc = ref("");
const staleReport = ref<StaleReport | null>(null);
const genProgress = ref<GenerateEvent | null>(null);
/** 页面级进度清单（大纲确定建骨架，此后按 id 合并增量） */
const genPages = ref<Map<string, PageProgress>>(new Map());
const genSummary = ref<{ pages: number; failed: number; prompt: number; completion: number; ms: number } | null>(null);
const indexSummary = ref<IndexSummary | null>(null);
/** 总览聚合（索引/Wiki/向量/活动） */
const overview = ref<RepoOverview | null>(null);
const generating = ref(false);
const updating = ref(false);
/** 外部运行中的任务（本组件未发起——重入水合或后台任务页并行） */
const externalRunning = ref(false);
const indexing = ref(false);
const rebuilding = ref(false);
const cardModalOpen = ref(false);
/** 知识卡 tab 内选中阅读的卡片路径 */
const currentCard = ref("");
const cardTitle = ref("");
const cardContent = ref("");
const creatingCard = ref(false);
const loadFailed = ref(false);

const cards = computed(() => wikiDocs.value.filter(([, kind]) => kind === "card"));

/** 本次生成/更新开始时间（耗时计时） */
const genStartedAt = ref<number | null>(null);

/**
 * 骨架页实时内容：不依赖 busy 态（失败后骨架保留，已完成页仍可回看）。
 * 未落盘路径（含 overview.md）返回空串占位——绝不读盘，避免「文档不存在」。
 */
const liveContent = computed(() => {
  const cur = currentDoc.value;
  if (!cur || !genPages.value.size || cur.startsWith("cards/")) return null;
  const hit = [...genPages.value.values()].find((g) => g.path === cur);
  if (hit) return hit.content || "";
  if (!docEntries.value.some(([d]) => d === cur)) return "";
  return null;
});

/** 当前页的骨架状态（徽标只在 generating 时亮——收尾期 done 页不误报） */
const liveStatus = computed(() => {
  const cur = currentDoc.value;
  if (!cur || !genPages.value.size) return null;
  return [...genPages.value.values()].find((g) => g.path === cur)?.status ?? null;
});

/** 树头部动作分发（原工具栏按钮收敛于此） */
function onTreeAction(kind: string) {
  if (kind === "update") runUpdate();
  else if (kind === "regenerate") runGenerate();
  else if (kind === "excludes") openExcludeModal();
  else rebuildEmbeddings();
}

// ===== 索引排除规则（隐藏目录已默认排除；这里叠加用户自定义 glob） =====
const excludeOpen = ref(false);
const excludeText = ref("");
const excludeSaving = ref(false);

async function openExcludeModal() {
  if (!wiki.currentRepo) return;
  excludeOpen.value = true;
  excludeText.value = "";
  try {
    const list = await api.getRepoExcludes(wiki.currentRepo.path);
    excludeText.value = list.join("\n");
  } catch {
    /* 读取失败留空，保存时覆盖 */
  }
}

async function saveExcludes() {
  if (!wiki.currentRepo) return;
  excludeSaving.value = true;
  try {
    const patterns = excludeText.value
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    await api.setRepoExcludes(wiki.currentRepo.path, patterns);
    excludeOpen.value = false;
    message.success(`已保存 ${patterns.length} 条排除规则，重新生成 Wiki 或重建索引后生效`);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "保存失败");
  } finally {
    excludeSaving.value = false;
  }
}

const PHASE_TEXT: Record<string, string> = {
  indexing: "建立索引",
  checking: "检测变更",
  outlining: "设计大纲",
  generating: "生成页面",
  assembling: "组装写入",
  embedding: "重建向量索引",
  done: "完成",
  error: "失败",
};

// 耗时计时（运行中每秒跳动）
const elapsedSec = ref(0);
let elapsedTimer: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  elapsedTimer = setInterval(() => {
    if (busy.value && genStartedAt.value) {
      elapsedSec.value = Math.floor((Date.now() - genStartedAt.value) / 1000);
    }
  }, 1000);
});
onUnmounted(() => {
  if (elapsedTimer) clearInterval(elapsedTimer);
});
const elapsedText = computed(() => {
  if (!busy.value || !genStartedAt.value) return "";
  const s = elapsedSec.value;
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
});

// ===== 事件订阅 =====
let unlisten: Awaited<ReturnType<typeof api.onGenerateEvent>> | null = null;
let unlistenStream: Awaited<ReturnType<typeof api.onLlmStream>> | null = null;
onMounted(async () => {
  unlistenStream = await api.onLlmStream((c) => {
    llmStream.append(c);
    // 页面正文按页累积（骨架点击实时查看）
    if (c.page_id && c.channel === "content") {
      const next = new Map(genPages.value);
      const hit = next.get(c.page_id);
      if (hit) {
        hit.content = (hit.content || "") + c.delta;
        genPages.value = next;
      }
    }
  });
  unlisten = await api.onGenerateEvent((e) => {
    // 按项目+分支过滤：其它项目/其它分支的任务事件不进本视图
    if (e.repo_id != null && wiki.currentRepo && e.repo_id !== wiki.currentRepo.id) return;
    if (e.branch != null && wiki.currentRepo?.branch && e.branch !== wiki.currentRepo.branch) return;
    const external = !generating.value && !updating.value; // 本组件未发起（其它页面/重入前的运行）
    genProgress.value = e;
    if (e.phase === "done" && external) {
      // 外部任务完成：刷新文档并清骨架（原发起页面可能已销毁）
      refreshDocs().then(() => {
        genPages.value = new Map();
        externalRunning.value = false;
      });
    }
    if (e.phase === "error" && external) {
      externalRunning.value = false;
    }
    if (e.pages?.length) {
      // 大纲确定（outlining 带骨架）重置清单；其余为单页增量合并
      const isSkeleton = e.phase === "outlining";
      if (isSkeleton) genPages.value = new Map();
      const next = isSkeleton ? new Map<string, PageProgress>() : new Map(genPages.value);
      for (const p of e.pages) next.set(p.id, p);
      genPages.value = next;
    }
  });
});
onUnmounted(() => {
  unlisten?.();
  unlistenStream?.();
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
  await hydrateExternalTask();
  await refreshOverview();
}

async function refreshOverview() {
  if (!wiki.currentRepo) {
    overview.value = null;
    return;
  }
  try {
    overview.value = await api.repoOverview(wiki.currentRepo.path);
  } catch {
    overview.value = null;
  }
}

/** 水合外部运行中的任务快照（页面销毁后重入：恢复进度树/计时） */
async function hydrateExternalTask() {
  try {
    const tasks = await api.listGenTasks();
    const t = tasks.find(
      (x) => !x.finished && x.repo_path === wiki.currentRepo?.path && x.branch === (wiki.currentRepo?.branch || ""),
    );
    if (!t) {
      externalRunning.value = false;
      return;
    }
    externalRunning.value = true;
    genStartedAt.value = t.started_ms;
    genPages.value = new Map(t.pages.map((p) => [p.id, { ...p }]));
    genProgress.value = {
      phase: t.phase,
      current: t.current,
      total: t.total,
      message: t.message,
      prompt_tokens: t.prompt_tokens,
      completion_tokens: t.completion_tokens,
    };
  } catch {
    /* 快照失败不影响页面 */
  }
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
/** 文档页默认选中：总览 → 页面 → 旧版模块，不落到知识卡/图谱
 * （清单按路径排序，cards/ 会排在前面，直接取 [0] 会默认选中卡片） */
function pickDefaultDoc(): string {
  for (const kind of ["architecture", "page", "module"]) {
    const hit = docEntries.value.find(([, k]) => k === kind);
    if (hit) return hit[0];
  }
  // 无文档类产物时保持空（右侧显示空态），不回退到非文档项
  return "";
}

/** 文档视图清单：不含知识卡（卡片有专门 tab/入口，不混入文档树） */
const docEntries = computed(() => wikiDocs.value.filter(([, kind]) => kind !== "card"));

async function refreshDocs() {
  if (!wiki.currentRepo) {
    wikiDocs.value = [];
    return;
  }
  // 分支态对齐实时 HEAD：currentRepo.branch 是打开时的快照，外部 checkout 后
  // 会过期——只读判定（viewBranch === 当前？）与后端读取口径必须一致，
  // 否则「查看的分支恰好等于过期缓存」时会按错误分支目录读（404）
  try {
    const live = await api.currentGitBranch(wiki.currentRepo.path);
    if (wiki.currentRepo.branch !== live) wiki.currentRepo.branch = live;
  } catch {
    /* 对齐失败按缓存值继续 */
  }
  // 可查看分支选项（当前分支不在目录列表时也补进去；先于只读 early-return）
  try {
    const list = await api.generatedBranches(wiki.currentRepo.path);
    const cur = wiki.currentRepo.branch || "";
    generatedBranchList.value = cur && !list.includes(cur) ? [cur, ...list] : list;
  } catch {
    generatedBranchList.value = [];
  }
  // 只读查看分支：清单走目标分支目录（无 db 登记依赖）
  if (readonlyBranch.value) {
    try {
      wikiDocs.value = await api.listWikiDocsFor(wiki.currentRepo.path, viewBranch.value!);
    } catch {
      wikiDocs.value = [];
    }
    currentDoc.value = pickDefaultDoc();
    outlineOrder.value = [];
    return;
  }
  try {
    wikiDocs.value = await api.listWikiDocs(wiki.currentRepo.path);
    if (wikiDocs.value.length && !wikiDocs.value.some(([p]) => p === currentDoc.value)) {
      currentDoc.value = pickDefaultDoc();
    }
  } catch {
    wikiDocs.value = [];
  }
  // 大纲顺序（best-effort：无 outline.json 时回退字母序）
  try {
    outlineOrder.value = (await api.wikiOutlinePaths(wiki.currentRepo.path)).map(([p]) => p);
  } catch {
    outlineOrder.value = [];
  }
}

/** 切换查看分支（只读；不切换 git，仅文档视图）。
 *  只设值——刷新统一由下方 watch(viewBranch) 响应（Navbar 全局切换器也走 store，
 *  两条入口共享同一条刷新路径，避免各自为政漏刷） */
function switchViewBranch(b: string) {
  viewBranch.value = b || null;
}

/** 回到当前分支（退出只读查看） */
function backToCurrentBranch() {
  viewBranch.value = null;
}

/** 全局查看分支变化（Navbar 图标 / WikiTree 入口）→ 文档树、选中态、
 *  滞后报告全部按新分支视角重载；树残留旧分支条目是「文档不存在」的直接来源 */
watch(
  () => wiki.viewBranch,
  async () => {
    currentDoc.value = "";
    staleReport.value = null;
    await refreshDocs();
    void refreshStaleness();
  },
);

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
/** 导出 Wiki 为自包含静态 HTML 站（VitePress 形态；dialog 选目录） */
const exporting = ref(false);
async function exportHtml() {
  if (!wiki.currentRepo || exporting.value) return;
  exporting.value = true;
  try {
    const sum = await api.exportWikiHtml(wiki.currentRepo.path, viewBranch.value);
    if (sum) message.success(`已导出 ${sum.pages} 个页面 → ${sum.out_dir}（打开 index.html 离线浏览）`);
  } catch (e) {
    message.error(typeof e === "string" ? e : "导出失败");
  } finally {
    exporting.value = false;
  }
}

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
    await refreshOverview();
    genPages.value = new Map(); // 树切回磁盘文档
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
    await refreshOverview();
    genPages.value = new Map();
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

/** 文档读取 404 自愈：检出切换后树残留他分支条目 / 登记与磁盘腐化——
 *  强制重扫 .bytewiki/ 重建当前分支登记，分支态对齐实时 HEAD，树刷新；
 *  该条目已不存在则清掉选中并提示（一次点击完成恢复，无需手动刷新） */
async function onDocReadFailed(err: string) {
  if (!err.includes("文档不存在") || !wiki.currentRepo) return;
  try {
    const [liveBranch] = await api.resyncWikiDocs(wiki.currentRepo.path);
    if (liveBranch && wiki.currentRepo.branch !== liveBranch) {
      wiki.currentRepo.branch = liveBranch;
    }
    await refreshDocs();
    if (currentDoc.value && !wikiDocs.value.some(([p]) => p === currentDoc.value)) {
      currentDoc.value = "";
      message.info("分支已切换或产物已更新，文档树已刷新");
    }
  } catch {
    /* 自愈失败保持原错误提示 */
  }
}

/** wiki 引用链接（bw-file://）→ 源码抽屉就地查看（不跳页、不动文件树） */
const sourceDrawer = ref<InstanceType<typeof SourceDrawer>>();
function openCiteDrawer(t: SourceTarget) {
  sourceDrawer.value?.open(t);
}

/** wiki 引用链接（bw-file://）→ 文件浏览页定位（抽屉内「在文件页打开」按钮的显式动作） */
function openFileAt(target: { path: string; startLine?: number; endLine?: number }) {
  wiki.revealFile(target.path, target.startLine, target.endLine);
  router.push(`/repo/${repoId.value}/files`);
}

const lineMax = computed(() => Math.max(1, ...(overview.value?.lang_lines.map(([, , n]) => n) ?? [1])));
function linePct(n: number): string {
  return `${Math.round((n / lineMax.value) * 100)}%`;
}
function fmtCount(n?: number): string {
  if (n == null) return "—";
  return n >= 10000 ? `${(n / 10000).toFixed(1)}万` : `${n}`;
}
function fmtSize(n: number): string {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`;
}
function shortPath(p: string): string {
  const parts = p.split("/");
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}

function fmtTime(iso: string | null): string {
  if (!iso) return "—";
  return iso.replace("T", " ").replace("Z", "");
}
const busy = computed(() => generating.value || updating.value || externalRunning.value);
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
                <span class="stat-label">代码规模</span>
                <span class="stat-value">{{ fmtCount(overview?.total_lines) }} 行</span>
                <span class="stat-sub">有效 {{ fmtCount(overview?.code_lines) }} · {{ fmtSize(overview?.size_bytes || 0) }}</span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">文件数</span>
                <span class="stat-value">{{ overview?.files_indexed ?? wiki.currentRepo.file_count }}</span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">模块 / 引用边</span>
                <span class="stat-value">{{ overview?.modules ?? "—" }} <em class="dim">/ {{ overview?.edges ?? "—" }}</em></span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">Wiki 章节 / 页面</span>
                <span class="stat-value">{{ overview?.wiki_sections ?? 0 }} <em class="dim">/ {{ overview?.wiki_pages ?? 0 }}</em></span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">向量块</span>
                <span class="stat-value">{{ overview?.chunks ?? 0 }}</span>
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
              <span class="stat-label">分支</span>
              <span class="stat-value">{{ wiki.currentRepo.branch || "—" }}</span>
              <span class="stat-label" style="margin-left: 14px">路径</span>
              <code class="repo-path-code">{{ wiki.currentRepo.path }}</code>
              <span
                v-if="staleReport && staleReport.stale"
                class="stale-badge"
                title="检测到代码变更，文档可能滞后"
              >
                文档滞后（{{ staleReport.changes.length }} 变更）
              </span>
            </div>

            <!-- 索引画像：语言分布 + 模块 TOP -->
            <div v-if="overview && (overview.lang_lines.length || overview.top_modules.length)" class="profile-grid">
              <div v-if="overview.lang_lines.length" class="profile-card">
                <div class="profile-title">语言分布（按行数）</div>
                <div class="lang-bars">
                  <div v-for="[lang, fileN, lineN] in overview.lang_lines" :key="lang" class="lang-row">
                    <span class="lang-name">{{ lang }}</span>
                    <div class="lang-track">
                      <div class="lang-fill" :style="{ width: linePct(lineN) }"></div>
                    </div>
                    <span class="lang-count" :title="`${fileN} 个文件`">{{ fmtCount(lineN) }} 行</span>
                  </div>
                </div>
              </div>
              <div v-if="overview.top_modules.length" class="profile-card">
                <div class="profile-title">模块 TOP</div>
                <div class="mod-list">
                  <div v-for="[m, n] in overview.top_modules" :key="m" class="mod-row">
                    <span class="mod-name" :title="m">{{ m }}</span>
                    <span class="mod-count">{{ n }} 文件</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- 变更与活动 -->
            <div v-if="staleReport && staleReport.stale" class="stale-files">
              <div class="profile-title">待更新变更（{{ staleReport.changes.length }}）</div>
              <div class="stale-chips">
                <span v-for="c in staleReport.changes.slice(0, 8)" :key="c.path" class="stale-chip" :title="c.path">
                  {{ c.kind === "added" ? "＋" : c.kind === "removed" ? "－" : "＊" }} {{ shortPath(c.path) }}
                </span>
                <span v-if="staleReport.changes.length > 8" class="stale-chip more">
                  … 另 {{ staleReport.changes.length - 8 }} 个
                </span>
              </div>
            </div>
            <div v-if="overview?.recent_activity?.length" class="activity">
              <div class="profile-title">最近生成活动</div>
              <ul class="activity-list">
                <li v-for="(a, i) in overview.recent_activity" :key="i" class="activity-item">
                  <span class="act-kind" :class="a[0]">{{ a[0] === "generate" ? "生成" : a[0] === "update" ? "更新" : a[0] }}</span>
                  <span class="act-detail" :title="a[1]">{{ a[1] }}</span>
                  <span class="act-time">{{ fmtTime(a[2]) }}</span>
                </li>
              </ul>
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
                浏览 Wiki（{{ wikiDocs.length }}）
              </button>
            </div>

            <!-- 轻量状态行：进度详情在左侧文档树 -->
            <div v-if="genProgress && (busy || genProgress.phase === 'done')" class="gen-inline">
              <span class="phase-chip" :class="{ active: busy }">
                {{ PHASE_TEXT[genProgress.phase] || genProgress.phase }}
              </span>
              <span v-if="genProgress.total" class="mono">
                {{ Math.min(genProgress.current, genProgress.total) }}/{{ genProgress.total }}
              </span>
              <span v-if="elapsedText" class="mono dim">{{ elapsedText }}</span>
              <span class="gen-msg">{{ genProgress.message }}</span>
            </div>
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
        <div class="browse-layout">
          <aside class="tree-pane">
            <WikiTree
              :docs="docEntries"
              :current="currentDoc"
              :order="outlineOrder"
              :gen-pages="[...genPages.values()]"
              :generating="busy"
              :gen-hint="genProgress?.message"
              :stale-count="!readonlyBranch && staleReport?.stale ? staleReport.changes.length : 0"
              :branch="wiki.currentRepo?.branch || undefined"
              :view-branch="viewBranch"
              :branch-options="generatedBranchList"
              :actions-disabled="busy || rebuilding"
              @select="currentDoc = $event"
              @action="onTreeAction"
              @switch-branch="switchViewBranch"
            />
          </aside>
          <section class="viewer-pane">
            <WikiViewer
              v-if="currentDoc"
              :repo-path="wiki.currentRepo.path"
              :doc-path="currentDoc"
              :live-content="liveContent"
              :live-status="liveStatus"
              :view-branch="readonlyBranch ? viewBranch : null"
              @open-doc="openDocInDocs"
              @open-file="openCiteDrawer"
              @read-failed="onDocReadFailed"
            />
            <!-- 空态：主内容区角色（比侧栏树空态更有分量：大标题+分支行+主行动按钮） -->
            <div v-else class="viewer-empty">
              <div class="ve-title">这个分支还没有 Wiki</div>
              <div class="ve-sub">
                <template v-if="readonlyBranch">
                  正在查看 <span class="ve-mono">{{ viewBranch }}</span> · 当前分支是
                  <span class="ve-mono">{{ wiki.currentRepo?.branch }}</span>
                </template>
                <template v-else-if="wiki.currentRepo?.branch">
                  当前分支 <span class="ve-mono">{{ wiki.currentRepo.branch }}</span>
                  <template v-if="switchableBranches.length">
                    · 已生成：<span class="ve-mono">{{ switchableBranches.join(" / ") }}</span>
                  </template>
                </template>
              </div>
              <div class="ve-actions">
                <button v-if="readonlyBranch" class="btn primary sm" @click="backToCurrentBranch()">
                  回 {{ wiki.currentRepo?.branch }} 生成
                </button>
                <template v-else>
                  <button class="btn primary sm" :disabled="busy || generating" @click="runGenerate()">
                    {{ generating ? "生成中…" : "生成 Wiki" }}
                  </button>
                  <button
                    v-for="b in switchableBranches"
                    :key="b"
                    class="btn sm"
                    @click="switchViewBranch(b)"
                  >
                    查看 {{ b }}
                  </button>
                </template>
              </div>
            </div>
          </section>
        </div>
      </div>

      <!-- ============ 文件浏览 ============ -->
      <div v-else-if="tab === 'files'" class="files-pane">
        <FileBrowse />
      </div>

      <!-- ============ 模型日志 ============ -->
      <div v-else-if="tab === 'llm-logs'" class="llm-logs-pane">
        <LlmLogPanel :repo-path="wiki.currentRepo.path" :busy="busy" />
      </div>

      <!-- ============ 调用链 ============ -->
      <div v-else-if="tab === 'callgraph'" class="callgraph-pane">
        <CallGraphPanel :repo-path="wiki.currentRepo.path" :branch="viewBranch" />
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
          <ChatPanel :repo-path="wiki.currentRepo.path" :branch="viewBranch" @open-doc="openDocInDocs" />
        </div>
      </div>

      <!-- ============ 知识卡：左列表 + 右就地阅读 ============ -->
      <div v-else-if="tab === 'cards'" class="cards-pane">
        <aside class="cards-list-pane">
          <div class="cards-list-head">
            <span>知识卡</span>
            <span class="cards-count">{{ cards.length }}</span>
            <button class="btn sm" title="新建知识卡" @click="cardModalOpen = true">
              <FileAddOutlined />
            </button>
          </div>
          <div class="cards-list-body">
            <div v-if="!cards.length" class="cards-empty">
              <div class="ce2-title">暂无节点</div>
              <div class="ce2-sub">记录部署要点、踩坑经验、接口约定</div>
            </div>
            <div
              v-for="[path, , title] in cards"
              :key="path"
              class="card-item"
              :class="{ active: currentCard === path }"
              @click="currentCard = path"
            >
              <BookOutlined class="card-icon" />
              <div class="card-text">
                <span class="card-title">{{ title }}</span>
                <code class="card-path">{{ path }}</code>
              </div>
            </div>
          </div>
        </aside>
        <section class="cards-viewer">
          <WikiViewer
            v-if="currentCard"
            :repo-path="wiki.currentRepo.path"
            :doc-path="currentCard"
            @open-file="openCiteDrawer"
            @read-failed="onDocReadFailed"
          />
          <div v-else class="viewer-empty">
            <div class="ve-title">选择左侧知识卡阅读</div>
            <div class="ve-sub">cards/ 为人工知识区：重新生成不会覆盖，且可被问答检索</div>
          </div>
        </section>
      </div>
        </template>

        <!-- 加载中 -->
        <div v-else class="loading-pane">
          <EmptyState text="加载中…" />
        </div>
      </div>

      <!-- 引用源码抽屉（文档页/知识卡页的 bw-file:// 链接就地查看） -->
      <SourceDrawer
        v-if="wiki.currentRepo"
        ref="sourceDrawer"
        :repo-path="wiki.currentRepo.path"
        :branch="readonlyBranch ? viewBranch : null"
        @locate="openFileAt"
      />

      <!-- AI 对话侧栏：内嵌布局右侧（VSCode 式挤压内容区，非浮层）；chat tab 本身即对话页不重复显示 -->
      <aside v-if="ai.dockOpen && tab !== 'chat' && wiki.currentRepo" class="rs-ai-dock">
        <div class="ai-dock-head">
          <span class="ai-dock-title">AI 对话</span>
          <button class="btn sm" title="收起侧栏" @click="ai.dockOpen = false">
            <CloseOutlined />
          </button>
        </div>
        <ChatPanel :repo-path="wiki.currentRepo.path" :branch="viewBranch" @open-doc="openDocInDocs" />
      </aside>
    </div>

    <!-- 模型流式终端（右下角浮窗，fixed 定位） -->
    <LlmStreamTerminal />

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

    <!-- 索引排除规则（gitignore 风格 glob，每行一条；重建索引后生效） -->
    <a-modal
      v-model:open="excludeOpen"
      title="索引排除规则"
      :confirm-loading="excludeSaving"
      ok-text="保存"
      cancel-text="取消"
      @ok="saveExcludes"
    >
      <p class="exclude-tip">
        每行一条 gitignore 风格 glob（如 <code>docs/**</code>、<code>*.min.js</code>、<code>vendor/</code>）。
        排除的文件不入索引、不进 Wiki 大纲与页面引用；<b>.gitignore 与隐藏目录（.github 除外）已默认排除</b>。保存后需重新生成 Wiki 或重建索引生效。
      </p>
      <a-textarea v-model:value="excludeText" :rows="8" placeholder="docs/**&#10;*.min.js&#10;vendor/" />
    </a-modal>
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
  /* 顶部与内容面板 padding 对齐：头部分隔线同水平线 */
  padding-top: var(--spacing-md);
}
.ai-dock-head {
  /* 统一面板头：与树头部 / 内容工具栏分隔线同高同水平线 */
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 38px;
  box-sizing: border-box;
  padding: 0 10px 0 14px;
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
.stat-sub {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
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

/* 索引画像 / 活动 */
.profile-grid {
  display: grid;
  grid-template-columns: 1.2fr 1fr;
  gap: var(--spacing-md);
  margin-bottom: var(--spacing-md);
}
.profile-card {
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  padding: 10px 14px;
}
.profile-title {
  font-size: 11px;
  font-weight: 700;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 8px;
}
.lang-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 0;
}
.lang-name {
  width: 64px;
  font-size: 12px;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.lang-track {
  flex: 1;
  height: 8px;
  background: var(--bg-input);
  border-radius: 4px;
  overflow: hidden;
}
.lang-fill {
  height: 100%;
  background: var(--color-primary);
  opacity: 0.75;
  border-radius: 4px;
}
.lang-count {
  width: 34px;
  text-align: right;
  font-size: 11px;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-tertiary);
}
.mod-list {
  display: flex;
  flex-direction: column;
}
.mod-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 3px 0;
  font-size: 12.5px;
}
.mod-name {
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: "JetBrains Mono", monospace;
}
.mod-count {
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.stat-value .dim {
  font-style: normal;
  color: var(--text-tertiary);
  font-weight: 400;
}
.stale-files {
  margin-bottom: var(--spacing-md);
}
.stale-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.stale-chip {
  font-size: 11.5px;
  font-family: "JetBrains Mono", monospace;
  color: var(--color-warning, #ea8a3c);
  background: color-mix(in oklch, #ea8a3c 10%, transparent);
  border-radius: var(--radius-xs);
  padding: 2px 8px;
}
.stale-chip.more {
  color: var(--text-tertiary);
  background: var(--bg-input);
}
.activity {
  margin-bottom: var(--spacing-sm);
}
.activity-list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.activity-item {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 3px 0;
  font-size: 12px;
}
.act-kind {
  flex-shrink: 0;
  font-size: 10.5px;
  font-weight: 600;
  color: var(--color-primary);
  background: var(--color-primary-bg);
  border-radius: var(--radius-xs);
  padding: 0 6px;
  line-height: 17px;
}
.act-kind.update {
  color: var(--color-warning, #ea8a3c);
  background: color-mix(in oklch, #ea8a3c 12%, transparent);
}
.act-detail {
  flex: 1;
  min-width: 0;
  color: var(--text-secondary);
  font-family: "JetBrains Mono", monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.act-time {
  flex-shrink: 0;
  color: var(--text-tertiary);
  font-size: 11px;
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
/* 查看器空态：主内容区角色——大标题 + 单行说明 + 按钮组（比侧栏树空态重一级） */
.viewer-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  text-align: center;
  padding: var(--spacing-lg);
}
.ve-title {
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
}
.ve-sub {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.7;
}
.ve-mono {
  font-family: "JetBrains Mono", monospace;
  color: var(--text-secondary);
}
.ve-actions {
  margin-top: 6px;
  display: flex;
  gap: var(--spacing-sm);
}
.cards-viewer {
  flex: 1;
  min-width: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  overflow: hidden;
  display: flex;
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

/* 知识卡：左列表 + 右阅读 */
.cards-pane {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--spacing-sm);
  padding: var(--spacing-md);
}
.cards-list-pane {
  width: 280px;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.cards-list-head {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 38px;
  box-sizing: border-box;
  padding: 0 12px;
  border-bottom: 1px solid var(--border-light);
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  flex-shrink: 0;
}
.cards-list-head .btn {
  margin-left: auto;
}
.cards-count {
  font-family: "JetBrains Mono", monospace;
  font-size: 10.5px;
  color: var(--text-tertiary);
  background: var(--bg-input);
  border-radius: var(--radius-xs);
  padding: 0 6px;
}
.cards-list-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 6px;
}
.cards-empty {
  flex: 1;
  min-height: 140px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 5px;
  text-align: center;
}
.ce2-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-secondary);
}
.ce2-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}
.card-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--transition-fast);
}
.card-item:hover {
  background: var(--bg-elevated);
}
.card-item.active {
  background: var(--color-primary-bg);
}
.card-icon {
  color: var(--color-primary);
  flex-shrink: 0;
}
.card-text {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.card-icon {
  color: var(--color-primary);
  flex-shrink: 0;
}
.card-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.card-path {
  font-family: "JetBrains Mono", monospace;
  font-size: 10.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 进度与徽标 */
.gen-inline {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 0 var(--spacing-sm);
  min-width: 0;
}
.gen-inline .phase-chip {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 2px 10px;
  border-radius: var(--radius-xs);
  white-space: nowrap;
}
.gen-inline .phase-chip.active {
  color: var(--color-primary);
  background: var(--color-primary-bg);
}
.gen-inline .mono {
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  color: var(--text-primary);
  white-space: nowrap;
}
.gen-inline .mono.dim {
  color: var(--text-tertiary);
}
.gen-inline .gen-msg {
  font-size: 12px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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


.exclude-tip {
  font-size: 12.5px;
  color: var(--text-secondary);
  line-height: 1.7;
  margin-bottom: 10px;
}
.exclude-tip code {
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-xs);
  padding: 0 4px;
  font-size: 11.5px;
}
</style>
