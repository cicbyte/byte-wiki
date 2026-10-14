<script setup lang="ts">
/**
 * 生成进度面板：阶段条 + 进度 + 耗时 + 页面级清单
 *
 * 大纲确定后后端推全量页面骨架（pending），此后每页推单条增量
 * （generating/done/failed），父组件按 id 合并后传入分组清单。
 * LLM 输出期间后端每秒推「已输出 N 字」心跳，静默期由耗时计时器兜底。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  LoadingOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  ClockCircleOutlined,
  DownOutlined,
} from "@ant-design/icons-vue";
import type { GenerateEvent, PageProgress } from "@/api/generate";

const props = defineProps<{
  progress: GenerateEvent | null;
  groups: { section: string; items: PageProgress[] }[];
  panelOpen: boolean;
  /** 本次生成开始时间（ms 时间戳） */
  startedAt: number | null;
}>();

const emit = defineEmits<{ (e: "toggle-panel"): void }>();

const PHASE_TEXT: Record<string, string> = {
  indexing: "建立索引",
  checking: "检测变更",
  outlining: "设计大纲",
  generating: "生成页面文档",
  assembling: "组装写入",
  embedding: "重建向量索引",
  done: "完成",
  error: "失败",
};

const RUNNING_PHASES = new Set(["checking", "indexing", "outlining", "generating", "assembling", "embedding"]);

const running = computed(() => RUNNING_PHASES.has(props.progress?.phase || ""));

const doneCount = computed(() => props.groups.reduce((n, g) => n + g.items.filter((p) => p.status === "done" || p.status === "failed").length, 0));
const totalCount = computed(() => props.groups.reduce((n, g) => n + g.items.length, 0));

// 耗时计时：运行中每秒跳动，静默阶段也有「活着」的反馈
const elapsedSec = ref(0);
let timer: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  timer = setInterval(() => {
    if (running.value && props.startedAt) {
      elapsedSec.value = Math.floor((Date.now() - props.startedAt) / 1000);
    }
  }, 1000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
});
const elapsedText = computed(() => {
  if (!props.startedAt || !running.value) return "";
  const s = elapsedSec.value;
  const mm = String(Math.floor(s / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return `${mm}:${ss}`;
});
</script>

<template>
  <div v-if="progress" class="gen-progress">
    <div class="gen-phase">
      <span class="phase-chip" :class="{ active: running }">
        {{ PHASE_TEXT[progress.phase] || progress.phase }}
      </span>
      <span v-if="progress.total" class="phase-count">
        {{ Math.min(progress.current, progress.total) }} / {{ progress.total }}
      </span>
      <span v-if="totalCount && progress.phase === 'generating'" class="phase-count">
        （{{ doneCount }} 页完成）
      </span>
      <span v-if="elapsedText" class="phase-count elapsed">{{ elapsedText }}</span>
      <span class="phase-msg">{{ progress.message }}</span>
      <button
        v-if="groups.length"
        class="pages-toggle"
        :title="panelOpen ? '收起页面清单' : '展开页面清单'"
        @click="emit('toggle-panel')"
      >
        <DownOutlined :class="{ open: panelOpen }" />
        页面清单
      </button>
    </div>
    <!-- 有总量走精确进度；大纲等无总量阶段走不定态动画 -->
    <div v-if="progress.total" class="progress-track">
      <div
        class="progress-fill"
        :style="{ width: `${Math.round((Math.min(progress.current, progress.total) / progress.total) * 100)}%` }"
      ></div>
    </div>
    <div v-else-if="running" class="progress-track indeterminate">
      <div class="progress-fill indet"></div>
    </div>

    <!-- 页面级清单：大纲骨架即时渲染，状态实时刷新 -->
    <div v-if="panelOpen && groups.length" class="page-checklist">
      <div v-for="g in groups" :key="g.section || '__root'" class="page-group">
        <div v-if="g.section" class="page-group-title">{{ g.section }}</div>
        <div v-for="p in g.items" :key="p.id" class="page-row" :data-status="p.status">
          <LoadingOutlined v-if="p.status === 'generating'" class="st spin" />
          <CheckCircleOutlined v-else-if="p.status === 'done'" class="st ok" />
          <CloseCircleOutlined v-else-if="p.status === 'failed'" class="st bad" />
          <ClockCircleOutlined v-else class="st wait" />
          <span class="page-title">{{ p.title }}</span>
          <span v-if="p.importance === 'high'" class="imp high">核心</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
  white-space: nowrap;
}
.phase-msg {
  font-size: 12px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pages-toggle {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--text-tertiary);
  background: none;
  border: none;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: var(--radius-xs);
  flex-shrink: 0;
}
.pages-toggle:hover {
  color: var(--color-primary);
  background: var(--bg-input);
}
.pages-toggle :deep(.open) {
  transform: rotate(180deg);
}
.progress-track {
  height: 4px;
  background: var(--bg-input);
  border-radius: 2px;
  overflow: hidden;
  position: relative;
}
.progress-fill {
  height: 100%;
  background: var(--color-primary);
  border-radius: 2px;
  transition: width var(--transition-normal);
}
/* 无总量阶段（大纲设计等）：不定态滑动条 */
.progress-fill.indet {
  width: 32%;
  animation: bw-indet 1.4s ease-in-out infinite;
}
@keyframes bw-indet {
  0% {
    margin-left: -32%;
  }
  100% {
    margin-left: 100%;
  }
}
.elapsed {
  color: var(--text-tertiary);
}
.page-checklist {
  margin-top: 8px;
  max-height: 220px;
  overflow-y: auto;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  padding: 6px 8px;
  background: var(--bg-surface);
}
.page-group + .page-group {
  margin-top: 4px;
}
.page-group-title {
  font-size: 11px;
  font-weight: 700;
  color: var(--text-tertiary);
  padding: 3px 2px;
  letter-spacing: 0.03em;
}
.page-row {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 2px 2px 2px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  border-radius: var(--radius-xs);
}
.page-row[data-status="done"] {
  color: var(--text-primary);
}
.page-row[data-status="failed"] .page-title {
  color: var(--color-danger, #d06050);
  text-decoration: line-through;
}
.page-row .st {
  font-size: 12px;
  flex-shrink: 0;
}
.st.spin {
  color: var(--color-primary);
}
.st.ok {
  color: var(--color-success, #52c41a);
}
.st.bad {
  color: var(--color-danger, #d06050);
}
.st.wait {
  color: var(--text-disabled);
}
.page-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.imp {
  font-size: 10px;
  color: var(--color-warning);
  border: 1px solid currentColor;
  border-radius: var(--radius-xs);
  padding: 0 4px;
  flex-shrink: 0;
  line-height: 16px;
}
</style>
