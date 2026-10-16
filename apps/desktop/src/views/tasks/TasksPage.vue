<script setup lang="ts">
/**
 * 后台任务页：生成/更新任务的全局视图
 *
 * 生成在 Rust 侧独立于页面运行（切页/关项目不中断，关应用才中断且可断点续跑）。
 * 本页展示任务注册表快照：运行中的实时刷新（事件 + 轮询兜底），
 * 已结束的保留结果直到清除；可一键回到对应项目。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  ReloadOutlined,
  ClearOutlined,
  ArrowRightOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
} from "@ant-design/icons-vue";
import { api } from "@/api";
import type { GenTask, PageProgress } from "@/api/generate";
import EmptyState from "@/components/common/EmptyState.vue";

const router = useRouter();

const tasks = ref<GenTask[]>([]);
const loading = ref(false);

const PHASE_TEXT: Record<string, string> = {
  starting: "启动中",
  indexing: "建立索引",
  checking: "检测变更",
  outlining: "设计大纲",
  generating: "生成页面",
  assembling: "组装写入",
  embedding: "重建向量索引",
  done: "已完成",
  error: "失败",
};

const running = computed(() => tasks.value.filter((t) => !t.finished));
const finished = computed(() => tasks.value.filter((t) => t.finished));

async function load() {
  loading.value = true;
  try {
    tasks.value = await api.listGenTasks();
  } finally {
    loading.value = false;
  }
}

async function clearFinished() {
  await api.clearGenTasks();
  await load();
}

function enterRepo(t: GenTask) {
  router.push(`/repo/${t.repo_id}`);
}

function elapsedOf(t: GenTask): string {
  const end = t.finished ? 0 : Date.now();
  const ms = (end || Date.now()) - t.started_ms;
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

const tick = ref(0); // 运行中耗时的秒级跳动
let timer: ReturnType<typeof setInterval> | null = null;
let poll: ReturnType<typeof setInterval> | null = null;
let unlisten: Awaited<ReturnType<typeof api.onGenerateEvent>> | null = null;

onMounted(async () => {
  await load();
  timer = setInterval(() => (tick.value++), 1000);
  // 事件实时更新 + 轮询兜底（事件丢失/外部启动时也能追上）
  unlisten = await api.onGenerateEvent((e) => {
    if (e.repo_id == null) return;
    const t = tasks.value.find((x) => x.repo_id === e.repo_id);
    if (t) {
      t.phase = e.phase;
      t.current = e.current;
      t.total = e.total;
      if (e.message) t.message = e.message;
      if (e.pages?.length) {
        for (const p of e.pages) {
          const slot = t.pages.find((x) => x.id === p.id);
          if (slot) Object.assign(slot, p);
          else t.pages.push(p);
        }
      }
      if (e.phase === "done") t.finished = true;
    } else {
      load();
    }
  });
  poll = setInterval(() => {
    if (running.value.length) load();
  }, 5000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
  if (poll) clearInterval(poll);
  unlisten?.();
});

function doneCount(t: GenTask): number {
  return t.pages.filter((p) => p.status === "done" || p.status === "failed").length;
}

function pageStat(p: PageProgress): string {
  switch (p.status) {
    case "generating":
      return "⟳";
    case "done":
      return "✓";
    case "failed":
      return "✗";
    default:
      return "·";
  }
}
</script>

<template>
  <div class="tasks-page">
    <div class="tasks-toolbar">
      <span class="tasks-title">
        后台任务
        <em v-if="running.length">{{ running.length }} 个进行中</em>
      </span>
      <div class="tasks-actions">
        <button class="btn sm" :disabled="loading" @click="load">
          <ReloadOutlined :spin="loading" /> 刷新
        </button>
        <button class="btn sm" :disabled="!finished.length" @click="clearFinished">
          <ClearOutlined /> 清除已完成
        </button>
      </div>
    </div>

    <div v-if="!tasks.length" class="tasks-empty">
      <EmptyState text="暂无任务" hint="在项目里点「一键生成」或「更新」，任务会在后台运行，离开页面也不会中断" />
    </div>

    <div class="task-list">
      <div v-for="t in tasks" :key="t.repo_id" class="task-card" :data-finished="t.finished">
        <div class="task-head">
          <LoadingOutlined v-if="!t.finished && t.phase !== 'error'" class="spin" />
          <CheckCircleOutlined v-else-if="t.phase === 'done'" class="ok" />
          <CloseCircleOutlined v-else class="bad" />
          <span class="task-repo">{{ t.repo_name }}</span>
          <span class="task-kind">{{ t.kind === "update" ? "增量更新" : "一键生成" }}</span>
          <span class="task-phase" :class="{ err: t.phase === 'error' }">
            {{ PHASE_TEXT[t.phase] || t.phase }}
          </span>
          <span v-if="t.total" class="task-count">{{ Math.min(t.current, t.total) }}/{{ t.total }}</span>
          <span class="task-time">{{ elapsedOf(t) }}<span v-if="!t.finished" style="opacity:.4">…</span></span>
          <button class="btn sm enter" @click="enterRepo(t)">
            进入项目 <ArrowRightOutlined />
          </button>
        </div>
        <div class="task-msg" :class="{ err: t.phase === 'error' }" :title="t.message">
          {{ t.error || t.message || "—" }}
        </div>
        <div v-if="t.total" class="task-track">
          <div
            class="task-fill"
            :style="{ width: `${Math.round((Math.min(t.current, t.total) / t.total) * 100)}%` }"
          ></div>
        </div>
        <div v-if="t.pages.length" class="task-pages">
          <span
            v-for="p in t.pages"
            :key="p.id"
            class="pp"
            :data-status="p.status"
            :title="`${p.status} · ${p.title}`"
          >{{ pageStat(p) }} {{ p.title }}</span>
        </div>
        <div v-if="t.phase === 'done'" class="task-done">
          完成 · prompt {{ t.prompt_tokens }} / completion {{ t.completion_tokens }} tokens
        </div>
      </div>
    </div>
    <span style="display: none">{{ tick }}</span>
  </div>
</template>

<style scoped>
.tasks-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--spacing-lg);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}
.tasks-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-md);
}
.tasks-title {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
  display: inline-flex;
  gap: 8px;
  align-items: baseline;
}
.tasks-title em {
  font-style: normal;
  font-size: 12px;
  color: var(--color-primary);
}
.tasks-actions {
  display: flex;
  gap: var(--spacing-sm);
}
.tasks-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.task-list {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}
.task-card {
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.task-card[data-finished="true"] {
  opacity: 0.75;
}
.task-head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.spin {
  color: var(--color-primary);
  animation: spin 1.2s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.ok {
  color: var(--color-success, #52c41a);
}
.bad {
  color: var(--color-danger, #d06050);
}
.task-repo {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.task-kind {
  font-size: 11px;
  color: var(--text-tertiary);
  background: var(--bg-input);
  border-radius: var(--radius-xs);
  padding: 1px 8px;
}
.task-phase {
  font-size: 11.5px;
  font-weight: 600;
  color: var(--color-primary);
  background: var(--color-primary-bg);
  border-radius: var(--radius-xs);
  padding: 1px 10px;
}
.task-phase.err {
  color: var(--color-danger, #d06050);
  background: color-mix(in oklch, var(--color-danger, #d06050) 12%, transparent);
}
.task-count,
.task-time {
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  color: var(--text-secondary);
}
.enter {
  margin-left: auto;
}
.task-msg {
  font-size: 12px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.task-msg.err {
  color: var(--color-danger, #d06050);
}
.task-track {
  height: 4px;
  background: var(--bg-input);
  border-radius: 2px;
  overflow: hidden;
}
.task-fill {
  height: 100%;
  background: var(--color-primary);
  border-radius: 2px;
  transition: width var(--transition-normal);
}
.task-pages {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  max-height: 96px;
  overflow-y: auto;
}
.pp {
  font-size: 11.5px;
  color: var(--text-disabled);
  white-space: nowrap;
}
.pp[data-status="generating"] {
  color: var(--color-primary);
  font-weight: 600;
}
.pp[data-status="done"] {
  color: var(--text-secondary);
}
.pp[data-status="failed"] {
  color: var(--color-danger, #d06050);
}
.task-done {
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
</style>
