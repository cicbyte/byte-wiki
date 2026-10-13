<script setup lang="ts">
/**
 * 生成进度面板：阶段条 + 进度 + 页面级清单
 *
 * 大纲确定后后端推全量页面骨架（pending），此后每页推单条增量
 * （generating/done/failed），父组件按 id 合并后传入分组清单。
 */
import { computed } from "vue";
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

const doneCount = computed(() => props.groups.reduce((n, g) => n + g.items.filter((p) => p.status === "done" || p.status === "failed").length, 0));
const totalCount = computed(() => props.groups.reduce((n, g) => n + g.items.length, 0));
</script>

<template>
  <div v-if="progress" class="gen-progress">
    <div class="gen-phase">
      <span class="phase-chip" :class="{ active: progress.phase === 'generating' || progress.phase === 'outlining' }">
        {{ PHASE_TEXT[progress.phase] || progress.phase }}
      </span>
      <span v-if="progress.total" class="phase-count">
        {{ Math.min(progress.current, progress.total) }} / {{ progress.total }}
      </span>
      <span v-if="totalCount && progress.phase === 'generating'" class="phase-count">
        （{{ doneCount }} 页完成）
      </span>
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
    <div v-if="progress.total" class="progress-track">
      <div
        class="progress-fill"
        :style="{ width: `${Math.round((Math.min(progress.current, progress.total) / progress.total) * 100)}%` }"
      ></div>
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
}
.progress-fill {
  height: 100%;
  background: var(--color-primary);
  border-radius: 2px;
  transition: width var(--transition-normal);
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
