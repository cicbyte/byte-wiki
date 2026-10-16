<script setup lang="ts">
/**
 * 模型日志页：LLM 调用全量留痕查看（大纲/页面/总览/对话）
 *
 * 数据来自 {data_dir}/llm-logs/{repo_id}.jsonl（尾部 100 条）。
 * 左列表（时间/阶段/标题/耗时/状态）+ 右详情（完整 prompt 与响应），
 * 支持阶段过滤、只看失败、刷新与清空。
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
import type { LlmLogEntry } from "@/api/llm-logs";
import EmptyState from "@/components/common/EmptyState.vue";

const props = defineProps<{ repoPath: string; busy?: boolean }>();

const logs = ref<LlmLogEntry[]>([]);
const size = ref(0);
const cacheStats = ref<{ count: number; bytes: number }>({ count: 0, bytes: 0 });
const loading = ref(false);
const selected = ref<LlmLogEntry | null>(null);
const stageFilter = ref("all");
const onlyFailed = ref(false);

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

/** 新的在前 */
const filtered = computed(() => {
  let list = [...logs.value].reverse();
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
    if (selected.value) {
      // 详情随刷新同步（按时间戳+阶段定位最新版本）
      const hit = entries.find(
        (e) => e.ts === selected.value!.ts && e.stage === selected.value!.stage && e.title === selected.value!.title,
      );
      selected.value = hit ?? (entries.length ? entries[entries.length - 1] : null);
    }
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "读取日志失败");
  } finally {
    loading.value = false;
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
    selected.value = null;
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

function fmtTokens(e: LlmLogEntry): string {
  if (!e.prompt_tokens && !e.completion_tokens) return "—";
  return `${e.prompt_tokens}/${e.completion_tokens}`;
}

// 生成进行中每 4s 自动刷新（日志在调用完成后写入，能看到逐条落盘）
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

    <div class="logs-layout">
      <div class="logs-list">
        <div class="list-head">
          <span>时间</span>
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
            v-for="(l, i) in filtered"
            :key="`${l.ts}-${i}`"
            class="row"
            :class="{ active: selected === l, failed: !l.ok }"
            @click="selected = l"
          >
            <span class="col-ts">{{ fmtTs(l.ts) }}</span>
            <span class="col-stage">{{ STAGE_TEXT[l.stage] || l.stage }}</span>
            <span class="col-title" :title="l.title">{{ l.title }}</span>
            <span class="col-model" :title="l.provider">{{ l.model }}</span>
            <span class="col-dur">{{ fmtDur(l.duration_ms) }}</span>
            <span class="col-tok">{{ fmtTokens(l) }}</span>
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

      <div class="log-detail">
        <template v-if="selected">
          <div class="detail-head">
            <span class="detail-title">{{ selected.title || "（无标题）" }}</span>
            <span class="detail-meta">
              {{ STAGE_TEXT[selected.stage] || selected.stage }} ·
              {{ fmtTs(selected.ts) }} · {{ selected.provider }} · {{ selected.model }}
              · {{ fmtDur(selected.duration_ms) }}
              · prompt {{ selected.prompt_tokens }} / completion {{ selected.completion_tokens }}
              <em v-if="selected.cached" class="cached-tag">缓存回放（未真实请求）</em>
              <em v-if="selected.attempt > 1" class="retry">第 {{ selected.attempt }} 次尝试</em>
            </span>
          </div>
          <div v-if="selected.error" class="detail-error">{{ selected.error }}</div>
          <div class="detail-body">
            <details open>
              <summary>System Prompt（{{ selected.system.length }} 字）</summary>
              <pre>{{ selected.system }}</pre>
            </details>
            <details open>
              <summary>User Prompt（{{ selected.user.length }} 字）</summary>
              <pre>{{ selected.user }}</pre>
            </details>
            <details open>
              <summary>响应（{{ selected.response.length }} 字）</summary>
              <pre>{{ selected.response }}</pre>
            </details>
          </div>
        </template>
        <div v-else class="list-state"><EmptyState text="选择一条日志查看详情" /></div>
      </div>
    </div>
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

.logs-layout {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--spacing-sm);
}
.logs-list {
  flex: 1.6;
  min-width: 0;
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
  grid-template-columns: 110px 48px minmax(120px, 1.4fr) minmax(90px, 1fr) 56px 84px 70px;
  gap: 6px;
  align-items: center;
  padding: 5px 10px;
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
.row.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
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
.row.active .col-ts,
.row.active .col-dur,
.row.active .col-tok {
  color: inherit;
}
.col-title,
.col-model {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
.cache-info {
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  white-space: nowrap;
}
.detail-meta .cached-tag {
  font-style: normal;
  color: var(--color-primary);
  margin-left: 6px;
}

.log-detail {
  flex: 1;
  min-width: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.detail-head {
  padding: 10px 14px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.detail-title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
  word-break: break-all;
}
.detail-meta {
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
.detail-meta .retry {
  font-style: normal;
  color: var(--color-warning);
  margin-left: 6px;
}
.detail-error {
  margin: 8px 14px 0;
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
  overflow-y: auto;
  padding: 10px 14px 14px;
}
.detail-body details {
  margin-bottom: 10px;
}
.detail-body summary {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 4px 0;
  user-select: none;
}
.detail-body pre {
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
  max-height: 320px;
  overflow-y: auto;
  color: var(--text-primary);
}
</style>
