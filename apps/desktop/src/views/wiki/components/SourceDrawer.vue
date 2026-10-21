<script setup lang="ts">
/**
 * 源码抽屉：Wiki 引用链接（bw-file://）就地查看——不跳页、不动文件树，
 * 右侧抽屉 Monaco 只读展示目标文件，滚动并高亮引用行区间。
 * 与 CallGraphPanel 的「打开函数 / 打开整个文件」抽屉同款交互。
 */
import { nextTick, reactive, ref } from "vue";
import { message } from "ant-design-vue";
import { FolderOpenOutlined } from "@ant-design/icons-vue";
import SnippetEditor from "@/components/editor/SnippetEditor.vue";
import { api } from "@/api";

export interface SourceTarget {
  path: string;
  startLine?: number;
  endLine?: number;
}

const props = defineProps<{ repoPath: string }>();
const emit = defineEmits<{ (e: "locate", t: SourceTarget): void }>();

const EXT_LANG: Record<string, string> = {
  py: "python", pyi: "python",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  go: "go", java: "java", rs: "rust",
};

const state = reactive({
  open: false,
  loading: false,
  path: "",
  label: "",
  lang: "plaintext",
  startLine: 0,
  endLine: 0,
  content: "",
});
const editorEl = ref<InstanceType<typeof SnippetEditor>>();
/** 行高亮装饰集合（每次打开重建） */
let decorations: { clear(): void } | null = null;

async function open(t: SourceTarget) {
  state.open = true;
  state.loading = true;
  state.path = t.path;
  const rangeLabel = t.startLine
    ? `:${t.startLine}${t.endLine && t.endLine !== t.startLine ? `-${t.endLine}` : ""}`
    : "";
  state.label = `${t.path}${rangeLabel}`;
  state.lang = EXT_LANG[t.path.split(".").pop() ?? ""] ?? "plaintext";
  state.startLine = t.startLine ?? 0;
  state.endLine = t.endLine ?? t.startLine ?? 0;
  state.content = "";
  try {
    const text = await api.readRepoFile(props.repoPath, t.path);
    state.content = text;
    // 先解除 v-show 隐藏再定位：Monaco 在 display:none 容器上是 0×0 布局，
    // 隐藏态 revealLineInCenter 会被夹成空操作（automaticLayout 的重排发生在
    // reveal 之后，不会补发滚动）——行号靠前的目标看不出来，靠下必现
    state.loading = false;
    await nextTick();
    await nextTick();
    const ed = editorEl.value?.getEditor();
    if (ed) {
      ed.layout();
      ed.updateOptions({ lineNumbers: "on" });
      decorations?.clear();
      if (state.startLine > 0) {
        ed.revealLineInCenter(state.startLine);
        ed.setPosition({ lineNumber: state.startLine, column: 1 });
        decorations = ed.createDecorationsCollection([
          {
            range: { startLineNumber: state.startLine, endLineNumber: state.endLine, startColumn: 1, endColumn: 1 },
            options: {
              isWholeLine: true,
              className: "cite-src-highlight",
              linesDecorationsClassName: "cite-src-mark",
            },
          },
        ]);
      } else {
        decorations = null;
        ed.revealLine(1);
      }
    }
  } catch (e) {
    message.error(typeof e === "string" ? e : "读取源码失败");
    state.open = false;
  } finally {
    state.loading = false;
  }
}

/** 需要文件树上下文时再跳文件页（保留旧定位链路作为显式动作） */
function locateInTree() {
  emit("locate", {
    path: state.path,
    startLine: state.startLine || undefined,
    endLine: state.endLine || undefined,
  });
}

defineExpose({ open });
</script>

<template>
  <a-drawer
    v-model:open="state.open"
    :title="state.label"
    width="62%"
    placement="right"
    :body-style="{ padding: '0', display: 'flex', flexDirection: 'column' }"
  >
    <div class="cite-drawer-body">
      <div v-if="state.loading" class="cite-drawer-loading">读取源码…</div>
      <SnippetEditor
        v-show="!state.loading"
        ref="editorEl"
        v-model="state.content"
        :language="state.lang"
        :readonly="true"
        :auto-detect="false"
      />
      <div class="cite-drawer-foot">
        <button class="btn sm" @click="locateInTree">
          <FolderOpenOutlined /> 在文件页打开（展开路径）
        </button>
      </div>
    </div>
  </a-drawer>
</template>

<style scoped>
.cite-drawer-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.cite-drawer-loading {
  padding: 24px;
  color: var(--text-tertiary, #888);
}
.cite-drawer-foot {
  display: flex;
  justify-content: flex-end;
  padding: 8px 12px;
  border-top: 1px solid var(--border-light, #eee);
}
/* 引用区间高亮（Monaco 装饰类是编辑器内部 DOM，经 :deep 命中；与调用链抽屉同色系） */
:deep(.cite-src-highlight) {
  background: color-mix(in oklch, var(--color-primary) 13%, transparent);
}
:deep(.cite-src-mark) {
  width: 3px !important;
  margin-left: 3px;
  background: var(--color-primary, #6366f1);
}
</style>
