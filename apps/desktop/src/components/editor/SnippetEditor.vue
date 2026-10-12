<script setup lang="ts">
/**
 * Monaco 编辑器封装（单实例直接 mount）
 *
 * - v-model 双向绑定代码内容
 * - language 切换时动态改 model language（经 normalizeLanguageId 规范化）
 * - autoDetect：language 为空时按代码内容启发式猜语言（Monaco 无内置自动检测）
 * - 主题跟随 themeStore.isDark（Monaco 不能读 CSS 变量，必须显式 setTheme）
 * - readonly 模式用于查看
 */
import { ref, computed, onMounted, onBeforeUnmount, watch } from "vue";
import { monaco } from "@/monaco/setup";
import { useThemeStore } from "@/stores/theme";
import { normalizeLanguageId, detectLanguage } from "./detect";
import { setActiveEditor, getActiveEditor } from "@/composables/useActiveEditor";

/** 自定义右键菜单项（注册为 Monaco action，出现在编辑器右键菜单里） */
export interface EditorContextMenuItem {
  id: string;
  label: string;
  /** Monaco keybinding，如 'ctrl+s'；可选 */
  keybindings?: number[];
  /** 点击回调 */
  run: () => void;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    language?: string;
    readonly?: boolean;
    /** language 为空时自动按内容检测语言（默认开） */
    autoDetect?: boolean;
    /** 自定义右键菜单项（追加到 Monaco 默认菜单后） */
    contextMenuItems?: EditorContextMenuItem[];
    /** 字号（默认 13；长文阅读场景可传 14/15） */
    fontSize?: number;
  }>(),
  {
    language: "",
    readonly: false,
    autoDetect: true,
    contextMenuItems: () => [],
    fontSize: 13,
  }
);
const emit = defineEmits<{
  "update:modelValue": [string];
  /** 自动检测到语言时通知父组件（便于回填表单 language 字段） */
  "detect-language": [string];
}>();

const themeStore = useThemeStore();
const el = ref<HTMLDivElement>();
let editor: monaco.editor.IStandaloneCodeEditor | null = null;
/** 已注册的自定义 action disposable（重新注册前需 dispose 旧的，避免重复） */
let registeredActions: monaco.IDisposable[] = [];

/** 最终生效的 Monaco language id */
const effectiveLang = computed(() => {
  const user = normalizeLanguageId(props.language);
  if (user !== "plaintext") return user;
  // plaintext + 开启自动检测 + 有内容 → 按内容猜
  // 防御 modelValue 偶发 undefined（后端 skip_serializing_if 省略字段时）
  const text = props.modelValue ?? "";
  if (props.autoDetect && text.trim()) {
    return detectLanguage(text);
  }
  return "plaintext";
});

function applyLanguage(lang: string) {
  if (!editor) return;
  const model = editor.getModel();
  if (model) monaco.editor.setModelLanguage(model, lang);
}

onMounted(() => {
  if (!el.value) return;
  editor = monaco.editor.create(el.value, {
    value: props.modelValue,
    language: effectiveLang.value,
    theme: themeStore.isDark ? "byte-wiki-dark" : "byte-wiki-light",
    readOnly: props.readonly,
    automaticLayout: true,
    fontSize: props.fontSize,
    fontFamily: "'JetBrains Mono', 'Cascadia Code', Consolas, monospace",
    lineHeight: 1.7 * props.fontSize,
    minimap: { enabled: !props.readonly },
    scrollBeyondLastLine: false,
    smoothScrolling: true,
    cursorBlinking: "smooth",
    renderWhitespace: "selection",
    bracketPairColorization: { enabled: true },
    padding: { top: 12, bottom: 12 },
  });
  editor.onDidChangeModelContent(() => {
    const v = editor!.getValue();
    emit("update:modelValue", v);
    // 编辑中若处于自动检测模式，尝试通知父组件检测到的语言
    if (props.autoDetect && !normalizeLanguageId(props.language).includes(props.language)) {
      const guessed = detectLanguage(v);
      if (guessed !== "plaintext") emit("detect-language", guessed);
    }
  });
  // 注册自定义右键菜单项（mount 时立即注册一次）
  registerContextMenu();
  // 聚焦时登记为活跃编辑器（模板展开插入目标）
  editor.onDidFocusEditorText(() => setActiveEditor(editorApi));
});

// contextMenuItems 变化时重新注册 Monaco action（deep 监听数组内容变化）
watch(
  () => props.contextMenuItems,
  () => registerContextMenu(),
  { deep: true }
);

/** 把 contextMenuItems 注册为 Monaco editor actions。
 * 重新注册前先 dispose 旧的，避免重复 action 堆积导致快捷键冲突或菜单项重复。 */
function registerContextMenu() {
  if (!editor) return;
  // 清理上一批
  for (const d of registeredActions) d.dispose();
  registeredActions = [];
  // 重新注册当前 props 的菜单项
  for (const item of props.contextMenuItems) {
    const d = editor.addAction({
      id: item.id,
      label: item.label,
      keybindings: item.keybindings,
      contextMenuGroupId: "byte-wiki",
      contextMenuOrder: 1,
      run: () => item.run(),
    });
    registeredActions.push(d);
  }
}

// 外部值变化时同步进编辑器（避免光标跳动：仅当值确实不同）
watch(
  () => props.modelValue,
  (v) => {
    if (editor && v !== editor.getValue()) {
      editor.setValue(v);
    }
  }
);

// 语言切换（含自动检测的响应式）
watch(effectiveLang, (lang) => applyLanguage(lang));

// 主题跟随
watch(
  () => themeStore.isDark,
  (dark) => {
    monaco.editor.setTheme(dark ? "byte-wiki-dark" : "byte-wiki-light");
  }
);

// readonly 跟随（全局 editMode 切换时响应）
watch(
  () => props.readonly,
  (ro) => {
    editor?.updateOptions({ readOnly: ro });
  }
);

/** 暴露给父组件 / 活跃编辑器单例：在光标处插入文本 + 聚焦 */
const editorApi = {
  insertText: (text: string) => {
    if (!editor) return;
    const pos = editor.getPosition();
    const range = pos
      ? new monaco.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column)
      : new monaco.Range(1, 1, 1, 1);
    editor.executeEdits("template-insert", [{ range, text, forceMoveMarkers: true }]);
    editor.focus();
  },
  focus: () => editor?.focus(),
  getEditor: () => editor,
};

defineExpose(editorApi);

onBeforeUnmount(() => {
  if (getActiveEditor() === editorApi) setActiveEditor(null);
  for (const d of registeredActions) d.dispose();
  registeredActions = [];
  editor?.dispose();
  editor = null;
});
</script>

<template>
  <div ref="el" class="snippet-editor" />
</template>

<style scoped>
.snippet-editor {
  width: 100%;
  height: 100%;
  min-height: 200px;
  background: var(--bg-input);
  border-radius: var(--radius-md);
  overflow: hidden;
}
</style>
