<script setup lang="ts">
/**
 * 模型日志页：LLM 调用全量留痕查看（大纲/页面/总览/对话）
 *
 * 数据来自 {data_dir}/llm-logs/{repo_id}.jsonl：列表走瘦身项全量拉取
 * （不带 prompt/响应全文，避免整份文件过 IPC），点击行按行号懒加载完整
 * 详情进右侧抽屉。支持分支过滤（chat/管线日志均带归属分支）、阶段过滤、
 * 只看失败、刷新与清空。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import { message } from "ant-design-vue";
import {
  ReloadOutlined,
  ClearOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
} from "@ant-design/icons-vue";
import { api } from "@/api";
import type { LlmLogEntry, LlmLogListItem } from "@/api/llm-logs";
import EmptyState from "@/components/common/EmptyState.vue";

const props = defineProps<{ repoPath: string; busy?: boolean }>();

const logs = ref<LlmLogListItem[]>([]);
const size = ref(0);
const cacheStats = ref<{ count: number; bytes: number }>({ count: 0, bytes: 0 });
const loading = ref(false);
const stageFilter = ref("all");
const branchFilter = ref("__all__");
const onlyFailed = ref(false);

// 详情抽屉：列表项 → 行号懒加载全文（JSONL 行不可变，抽屉打开期间无需随轮询刷新）
const drawerOpen = ref(false);
const detailLoading = ref(false);
const detailError = ref("");
const detail = ref<LlmLogEntry | null>(null);
const detailItem = ref<LlmLogListItem | null>(null);
// Tab 分区（System/User/响应）单段占满剩余高度；默认落在最常读的响应，
// 选择跨抽屉保留（不随打开重置）
const detailTab = ref("response");

const STAGES: { key: string; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "outline", label: "大纲" },
  { key: "page", label: "页面" },
  { key: "overview", label: "总览" },
  { key: "chat", label: "对话" },
];

const STAGE_TEXT: Record<string, string> = {
  outline: "大纲",
  page: "页面",
  overview: "总览",
  chat: "对话",
};

/** 分支下拉选项（按日志出现顺序；空串 = 分支维度引入前的旧日志） */
const branchOptions = computed(() => {
  const seen: string[] = [];
  for (const l of logs.value) {
    if (!seen.includes(l.branch)) seen.push(l.branch);
  }
  return [
    { value: "__all__", label: "全部分支" },
    ...seen.map((b) => ({ value: b, label: b || "—（旧日志）" })),
  ];
});

/** 新的在前 */
const filtered = computed(() => {
  let list = [...logs.value].reverse();
  if (branchFilter.value !== "__all__") {
    list = list.filter((l) => l.branch === branchFilter.value);
  }
  if (stageFilter.value !== "all") {
    list = list.filter((l) => l.stage === stageFilter.value);
  }
  if (onlyFailed.value) {
    list = list.filter((l) => !l.ok);
  }
  return list;
});

async function load() {
  loading.value = true;
  try {
    const [entries, bytes] = await api.listLlmLogs(props.repoPath);
    logs.value = entries;
    size.value = bytes;
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "读取日志失败");
  } finally {
    loading.value = false;
  }
}

async function openDetail(item: LlmLogListItem) {
  detailItem.value = item;
  detail.value = null;
  detailError.value = "";
  detailLoading.value = true;
  drawerOpen.value = true;
  try {
    detail.value = await api.llmLogDetail(props.repoPath, item.line);
  } catch (e: any) {
    detailError.value = typeof e === "string" ? e : "读取详情失败";
  } finally {
    detailLoading.value = false;
  }
}

async function loadCache() {
  try {
    const [count, bytes] = await api.llmCacheStats(props.repoPath);
    cacheStats.value = { count, bytes };
  } catch {
    cacheStats.value = { count: 0, bytes: 0 };
  }
}

async function clearCache() {
  try {
    await api.clearLlmCache(props.repoPath);
    message.success("缓存已清空（下次生成将全部真实请求）");
    await loadCache();
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "清空失败");
  }
}

async function clearAll() {
  try {
    await api.clearLlmLogs(props.repoPath);
    logs.value = [];
    size.value = 0;
    drawerOpen.value = false;
    detail.value = null;
    detailItem.value = null;
    message.success("日志已清空");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "清空失败");
  }
}

function fmtTs(ts: string): string {
  return ts.replace("T", " ").replace("Z", "").slice(5, 19);
}

function fmtDur(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`;
}

function fmtSize(n: number): string {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`;
}

function fmtTokens(prompt: number, completion: number): string {
  if (!prompt && !completion) return "—";
  return `${prompt}/${completion}`;
}

// 生成进行中每 4s 自动刷新列表（日志在调用完成后写入，能看到逐条落盘；
// 瘦身项开销小，抽屉详情不随之刷新——JSONL 行不可变）
let pollTimer: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  load();
  loadCache();
  pollTimer = setInterval(() => {
    if (props.busy) {
      load();
      loadCache();
    }
  }, 4000);
});
onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<template>
  <div class="llm-logs">
    <div class="logs-toolbar">
      <span class="logs-title">
        模型日志
        <em v-if="logs.length">{{ logs.length }} 条 · {{ fmtSize(size) }}</em>
      </span>
      <div class="logs-actions">
        <a-select
          v-if="branchOptions.length > 2"
          v-model:value="branchFilter"
          size="small"
          class="branch-select"
          :options="branchOptions"
        />
        <span class="stage-chips">
          <button
            v-for="s in STAGES"
            :key="s.key"
            class="chip"
            :class="{ active: stageFilter === s.key }"
            @click="stageFilter = s.key"
          >
            {{ s.label }}
          </button>
          <button class="chip" :class="{ active: onlyFailed }" @click="onlyFailed = !onlyFailed">
            失败
          </button>
        </span>
        <button class="btn sm" :disabled="loading" @click="load">
          <ReloadOutlined :spin="loading" /> 刷新
        </button>
        <span class="cache-info" title="同一 prompt 不重复请求（键=供应商+模型+prompt 哈希）；开发期反复重跑的未变更页面直接回放">
          缓存 {{ cacheStats.count }} 条 · {{ fmtSize(cacheStats.bytes) }}
        </span>
        <button class="btn sm" :disabled="!cacheStats.count" @click="clearCache">
          <ClearOutlined /> 清缓存
        </button>
        <button class="btn sm danger" :disabled="!logs.length" @click="clearAll">
          <ClearOutlined /> 清日志
        </button>
      </div>
    </div>

    <div class="logs-list">
      <div class="list-head">
        <span>时间</span>
        <span>分支</span>
        <span>阶段</span>
        <span class="col-title">标题</span>
        <span>模型</span>
        <span>耗时</span>
        <span>tokens</span>
        <span>状态</span>
      </div>
      <div class="list-body">
        <div v-if="loading && !logs.length" class="list-state"><EmptyState text="读取中…" /></div>
        <div v-else-if="!filtered.length" class="list-state">
          <EmptyState
            text="暂无日志"
            hint="每次模型调用的完整输入输出都会记录在此（按项目存 JSONL，调用完成后写入）；正在进行的调用请看总览页进度面板"
          />
        </div>
        <div
          v-for="l in filtered"
          :key="l.line"
          class="row"
          :class="{ failed: !l.ok }"
          @click="openDetail(l)"
        >
          <span class="col-ts">{{ fmtTs(l.ts) }}</span>
          <span class="col-branch" :class="{ legacy: !l.branch }" :title="l.branch || '分支维度引入前的日志'">
            {{ l.branch || "—" }}
          </span>
          <span class="col-stage">{{ STAGE_TEXT[l.stage] || l.stage }}</span>
          <span class="col-title" :title="l.title">{{ l.title }}</span>
          <span class="col-model" :title="l.provider">{{ l.model }}</span>
          <span class="col-dur">{{ fmtDur(l.duration_ms) }}</span>
          <span class="col-tok">{{ fmtTokens(l.prompt_tokens, l.completion_tokens) }}</span>
          <span class="col-ok">
            <em v-if="l.cached" class="cached">缓存</em>
            <template v-else>
              <CheckCircleOutlined v-if="l.ok" class="ok" />
              <CloseCircleOutlined v-else class="bad" />
            </template>
            <em v-if="l.attempt > 1">重试{{ l.attempt }}</em>
          </span>
        </div>
      </div>
    </div>

    <!-- 详情抽屉：按行号懒加载完整 system/user/response -->
    <a-drawer
      v-model:open="drawerOpen"
      :title="detailItem?.title || '调用详情'"
      placement="right"
      width="720px"
      class="llm-log-drawer"
      :body-style="{ padding: '0', display: 'flex', flexDirection: 'column' }"
    >
      <div class="detail-wrap">
        <div v-if="detailLoading" class="detail-state">读取详情…</div>
        <div v-else-if="detailError" class="detail-state err">{{ detailError }}</div>
        <template v-else-if="detail">
          <div class="detail-head">
            <div class="detail-meta">
              <span class="stage-tag">{{ STAGE_TEXT[detail.stage] || detail.stage }}</span>
              <span class="branch-tag" :class="{ legacy: !detail.branch }">{{ detail.branch || "—" }}</span>
              <span>{{ fmtTs(detail.ts) }}</span>
              <span :title="detail.provider">{{ detail.provider }} · {{ detail.model }}</span>
              <span>{{ fmtDur(detail.duration_ms) }}</span>
              <span>prompt {{ detail.prompt_tokens }} / completion {{ detail.completion_tokens }}</span>
              <em v-if="detail.cached" class="cached-tag">缓存回放（未真实请求）</em>
              <em v-if="detail.attempt > 1" class="retry">第 {{ detail.attempt }} 次尝试</em>
            </div>
          </div>
          <div v-if="detail.error" class="detail-error">{{ detail.error }}</div>
          <div class="detail-body">
            <a-tabs v-model:activeKey="detailTab" class="detail-tabs" size="small">
              <a-tab-pane key="system" :tab="`System（${detail.system.length} 字）`">
                <pre>{{ detail.system }}</pre>
              </a-tab-pane>
              <a-tab-pane key="user" :tab="`User（${detail.user.length} 字）`">
                <pre>{{ detail.user }}</pre>
              </a-tab-pane>
              <a-tab-pane key="response" :tab="`响应（${detail.response.length} 字）`">
                <pre>{{ detail.response }}</pre>
              </a-tab-pane>
            </a-tabs>
          </div>
        </template>
        <div v-else class="detail-state">选择一条日志查看详情</div>
      </div>
    </a-drawer>
  </div>
</template>

<style scoped>
.llm-logs {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}
.logs-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-md);
  flex-shrink: 0;
  flex-wrap: wrap;
}
.logs-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  display: inline-flex;
  gap: 8px;
  align-items: baseline;
}
.logs-title em {
  font-style: normal;
  font-size: 11.5px;
  font-weight: 400;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
.logs-actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  flex-wrap: wrap;
}
.branch-select {
  width: 150px;
  font-size: 12px;
}
.stage-chips {
  display: inline-flex;
  gap: 4px;
  background: var(--bg-input);
  border-radius: var(--radius-sm);
  padding: 2px;
}
.chip {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  padding: 2px 10px;
  border-radius: var(--radius-xs);
  cursor: pointer;
}
.chip:hover {
  color: var(--text-primary);
}
.chip.active {
  background: var(--bg-card);
  color: var(--color-primary);
  font-weight: 600;
  box-shadow: 0 0 0 1px var(--border-light);
}
.btn.danger:hover {
  color: var(--color-danger, #d06050);
  border-color: var(--color-danger, #d06050);
}
.cache-info {
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  white-space: nowrap;
}

.logs-list {
  flex: 1;
  min-height: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.list-head,
.row {
  display: grid;
  grid-template-columns: 110px 72px 48px minmax(160px, 1.6fr) minmax(90px, 1fr) 56px 84px 76px;
  gap: 6px;
  align-items: center;
  padding: 5px 12px;
  font-size: 12px;
}
.list-head {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 600;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.list-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
.list-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.row {
  color: var(--text-secondary);
  cursor: pointer;
  border-bottom: 1px solid var(--border-light);
}
.row:hover {
  background: var(--bg-elevated);
}
.row.failed .col-title {
  color: var(--color-danger, #d06050);
}
.col-ts,
.col-dur,
.col-tok {
  font-family: "JetBrains Mono", monospace;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.col-branch {
  font-family: "JetBrains Mono", monospace;
  font-size: 11.5px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.col-branch.legacy {
  color: var(--text-tertiary);
}
.col-title,
.col-model {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.5;
}
.col-model {
  font-family: "JetBrains Mono", monospace;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.col-ok {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.col-ok .ok {
  color: var(--color-success, #52c41a);
}
.col-ok .bad {
  color: var(--color-danger, #d06050);
}
.col-ok em {
  font-style: normal;
  font-size: 10px;
  color: var(--color-warning);
}
.col-ok em.cached {
  color: var(--color-primary);
  border: 1px solid currentColor;
  border-radius: var(--radius-xs);
  padding: 0 4px;
  line-height: 14px;
}

/* 详情抽屉 */
.detail-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.detail-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  font-size: 13px;
}
.detail-state.err {
  color: var(--color-danger, #d06050);
  padding: 0 24px;
  text-align: center;
}
.detail-head {
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.detail-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
.stage-tag,
.branch-tag {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-xs);
  padding: 0 6px;
  line-height: 18px;
  color: var(--text-secondary);
  font-size: 11px;
}
.branch-tag.legacy {
  color: var(--text-tertiary);
}
.detail-meta .cached-tag {
  font-style: normal;
  color: var(--color-primary);
}
.detail-meta .retry {
  font-style: normal;
  color: var(--color-warning);
}
.detail-error {
  margin: 10px 16px 0;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: color-mix(in oklch, var(--color-danger, #d06050) 10%, transparent);
  color: var(--color-danger, #d06050);
  font-size: 12px;
  font-family: "JetBrains Mono", monospace;
  word-break: break-all;
}
.detail-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 0 16px 16px;
}
/* Tab 分区：单段占满抽屉剩余高度，pre 内部滚动 */
.detail-tabs {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.detail-tabs :deep(.ant-tabs-nav) {
  margin: 0 0 8px;
}
.detail-tabs :deep(.ant-tabs-content-holder) {
  flex: 1;
  min-height: 0;
  display: flex;
}
.detail-tabs :deep(.ant-tabs-content) {
  flex: 1;
  min-height: 0;
  display: flex;
}
.detail-tabs :deep(.ant-tabs-tabpane) {
  flex: 1;
  min-height: 0;
  display: flex;
}
.detail-tabs pre {
  flex: 1;
  min-height: 0;
  margin: 0;
  padding: 10px;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  font-size: 11.5px;
  line-height: 1.6;
  font-family: "JetBrains Mono", monospace;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-y: auto;
  color: var(--text-primary);
}
</style>
