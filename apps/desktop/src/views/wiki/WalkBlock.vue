<script setup lang="ts">
/**
 * 走读页代码块：懒挂载 Monaco（IntersectionObserver，滚动到视口附近才创建编辑器），
 * 行号偏移为真实文件行号；调用行高亮 + 点击跳转目标块（标注走读执行流）。
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { CaretRightOutlined } from "@ant-design/icons-vue";
import SnippetEditor from "@/components/editor/SnippetEditor.vue";

export interface WalkCallLink {
  targetId: string;
  targetNum: string;
  targetName: string;
  line: number; // 源文件行号（1-based）
  kind: string;
}

const props = defineProps<{
  num: string;
  qualname: string;
  file: string;
  startLine: number;
  endLine: number;
  lang: string;
  code: string;
  truncated: boolean;
  routes: string[];
  callLinks: WalkCallLink[];
  expanded: boolean;
  index: number;
}>();

const emit = defineEmits<{
  (e: "toggle"): void;
  (e: "jump", targetId: string): void;
  (e: "visible"): void;
}>();

const rootEl = ref<HTMLElement>();
const editorEl = ref<InstanceType<typeof SnippetEditor>>();
/** 编辑器真正挂载（expanded 且接近视口二者皆满足） */
const mounted = ref(false);
let observer: IntersectionObserver | null = null;
let near = false;

const EXT_LANG: Record<string, string> = {
  py: "python", pyi: "python",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  go: "go", java: "java", rs: "rust",
};

/** 相对行号（1-based within block）→ 该行发起的调用 */
function linkAt(relLine: number): WalkCallLink | undefined {
  return props.callLinks.find((l) => l.line - props.startLine + 1 === relLine);
}

async function ensureEditor() {
  if (mounted.value || !props.expanded) return;
  mounted.value = true;
  await nextTick();
  const ed = editorEl.value?.getEditor();
  if (!ed) return;
  // 行号偏移：块内第 1 行显示为真实文件行号
  ed.updateOptions({
    lineNumbers: (l: number) => String(props.startLine + l - 1),
    scrollbar: { alwaysConsumeMouseWheel: false },
    scrollBeyondLastLine: false,
  });
  // 滚轮穿透：Monaco 的 wheel 监听总会 preventDefault（实测 alwaysConsumeMouseWheel
  // 对已建实例无效），走读块按内容定高后自身无需滚动——在容器捕获阶段截停事件
  // （stopPropagation 不 preventDefault），浏览器原生滚动落到页面滚动容器
  const wrap = rootEl.value?.querySelector(".wk-code") as HTMLElement | null;
  wrap?.addEventListener(
    "wheel",
    (e) => {
      e.stopPropagation();
    },
    { capture: true },
  );
  // 容器按内容高度撑开（编辑器默认 min-height 200px 会让长函数在小框内滚动——走读要全展开）。
  // Monaco 布局是异步的：一次性测量会拿到未完成布局的高度（曾致某块卡在 98px），
  // 持续监听 layout 变化校正直至稳定（readonly 静态内容事件很少）
  const syncHeight = () => {
    const wrap = rootEl.value?.querySelector(".wk-code") as HTMLElement | null;
    if (wrap) wrap.style.height = `${ed.getContentHeight() + 2}px`;
  };
  requestAnimationFrame(syncHeight);
  ed.onDidLayoutChange(syncHeight);
  // 调用行高亮 + 点击跳转（走读的核心标注）
  if (props.callLinks.length) {
    ed.createDecorationsCollection(
      props.callLinks.map((l) => ({
        range: { startLineNumber: l.line - props.startLine + 1, startColumn: 1, endLineNumber: l.line - props.startLine + 1, endColumn: 1 },
        options: {
          isWholeLine: true,
          className: "wk-call-line",
          linesDecorationsClassName: "wk-call-glyph",
          hoverMessage: { value: `→ ${l.targetNum} ${l.targetName}` },
        },
      })),
    );
    ed.onMouseDown((p: any) => {
      const ln = p?.target?.position?.lineNumber;
      if (ln) {
        const link = linkAt(ln);
        if (link) emit("jump", link.targetId);
      }
    });
  }
}

watch(
  () => props.expanded,
  (v) => {
    if (v) ensureEditor();
  },
);

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      for (const en of entries) {
        if (en.isIntersecting) {
          emit("visible");
          if (props.expanded) ensureEditor();
          observer?.disconnect();
        }
      }
    },
    { rootMargin: "300px 0px" },
  );
  if (rootEl.value) observer.observe(rootEl.value);
});

onBeforeUnmount(() => observer?.disconnect());

const ext = () => props.file.split(".").pop() ?? "";
</script>

<template>
  <section ref="rootEl" class="wk-block" :data-node-id="`${index}`">
    <header class="wk-block-head">
      <button class="wk-toggle" :title="expanded ? '折叠代码' : '展开代码'" @click="emit('toggle')">
        <CaretRightOutlined :class="{ open: expanded }" />
      </button>
      <span class="wk-num">{{ num }}</span>
      <span class="wk-name" :title="qualname">{{ qualname }}</span>
      <span class="wk-loc">{{ file }}:{{ startLine }}-{{ endLine }}</span>
      <span v-for="r in routes" :key="r" class="wk-route">{{ r }}</span>
    </header>

    <!-- 本块发起的调用（点击跳目标块；等价于代码内行标注的列表形态） -->
    <div v-if="callLinks.length" class="wk-links">
      <button
        v-for="l in callLinks"
        :key="l.targetId + l.line"
        class="wk-link"
        @click="emit('jump', l.targetId)"
      >
        <span class="wk-link-line">L{{ l.line }}</span> → {{ l.targetNum }} {{ l.targetName }}
        <em v-if="l.kind === 'heuristic'">~</em>
      </button>
    </div>

    <div v-if="expanded" class="wk-code">
      <SnippetEditor
        v-if="mounted"
        ref="editorEl"
        :model-value="code"
        :language="EXT_LANG[ext()] ?? lang"
        :readonly="true"
        :auto-detect="false"
        :font-size="14"
      />
      <div v-else class="wk-code-loading">加载编辑器…</div>
    </div>
    <div v-else class="wk-collapsed">
      已折叠 · {{ endLine - startLine + 1 }} 行（点左侧展开）
    </div>
    <div v-if="truncated" class="wk-truncated">函数超 1200 行已截断</div>
  </section>
</template>

<style scoped>
.wk-block {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  background: var(--bg-surface);
  margin-bottom: 14px;
  overflow: hidden;
}
.wk-block-head {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border-light);
  cursor: default;
}
.wk-toggle {
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 2px;
  display: inline-flex;
}
.wk-toggle :deep(.anticon) {
  font-size: 10px;
  transition: transform var(--transition-fast);
}
.wk-toggle :deep(.anticon.open) {
  transform: rotate(90deg);
}
.wk-num {
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  font-weight: 700;
  color: var(--color-primary);
  flex-shrink: 0;
}
.wk-name {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wk-loc {
  font-family: "JetBrains Mono", monospace;
  font-size: 11.5px;
  color: var(--text-tertiary);
  flex-shrink: 0;
  margin-left: auto;
}
.wk-route {
  font-family: "JetBrains Mono", monospace;
  font-size: 10.5px;
  color: #3a9fae;
  background: rgba(125, 216, 232, 0.12);
  border: 1px solid rgba(125, 216, 232, 0.45);
  border-radius: var(--radius-xs);
  padding: 0 5px;
  flex-shrink: 0;
}
.wk-links {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  padding: 6px 12px;
  border-bottom: 1px dashed var(--border-light);
  background: var(--bg-input);
}
.wk-link {
  border: 1px solid var(--border-base);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 11.5px;
  border-radius: 999px;
  padding: 1px 8px;
  cursor: pointer;
  font-family: "JetBrains Mono", monospace;
  transition: all var(--transition-fast);
}
.wk-link:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}
.wk-link em {
  font-style: normal;
  color: var(--text-tertiary);
}
.wk-link-line {
  color: var(--text-tertiary);
}
.wk-code {
  position: relative;
  min-height: 60px;
}
.wk-code :deep(.snippet-editor) {
  border-radius: 0;
  min-height: 0;
  height: 100%;
}
.wk-code-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 60px;
  color: var(--text-tertiary);
  font-size: 12.5px;
}
.wk-collapsed {
  padding: 8px 12px;
  font-size: 12px;
  color: var(--text-tertiary);
}
.wk-truncated {
  padding: 4px 12px;
  font-size: 11.5px;
  color: var(--color-warning);
  border-top: 1px dashed var(--border-light);
}
/* 调用行高亮（Monaco 内部 DOM，经 :deep 命中） */
.wk-code :deep(.wk-call-line) {
  background: color-mix(in srgb, var(--color-primary) 9%, transparent);
  cursor: pointer;
}
.wk-code :deep(.wk-call-glyph) {
  border-left: 3px solid var(--color-primary);
  margin-left: 2px;
}
</style>
