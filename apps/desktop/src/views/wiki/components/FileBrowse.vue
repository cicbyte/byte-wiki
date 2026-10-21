<script setup lang="ts">
/**
 * 文件浏览页（项目空间 → 文件 tab）
 *
 * 参考 git-mate 工作区浏览（views/browse/workspace）：纯只读浏览——
 * 「被迫开 IDE 只为看一眼代码」的痛点。左：懒加载目录树 + 文件名搜索；
 * 右：Monaco 只读查看，Markdown 走 MdPreview（渲染/源码切换）。
 * 改代码指挥 AI，看代码用本页。
 */
import { computed, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { message } from "ant-design-vue";
import { MdPreview } from "md-editor-v3";
import "md-editor-v3/lib/style.css";
import {
  FolderFilled,
  FileOutlined,
  CaretRightOutlined,
  SearchOutlined,
  EyeOutlined,
  CodeOutlined,
} from "@ant-design/icons-vue";
import { api } from "@/api";
import type { DirEntryInfo } from "@/api/repo";
import { useWikiStore } from "@/stores/wiki";
import type { RevealTarget } from "@/stores/wiki";
import { useThemeStore } from "@/stores/theme";
import { useAiStore } from "@/stores/ai";
import type { EditorContextMenuItem } from "@/components/editor/SnippetEditor.vue";
import SnippetEditor from "@/components/editor/SnippetEditor.vue";
import EmptyState from "@/components/common/EmptyState.vue";

const route = useRoute();
const wiki = useWikiStore();
const themeStore = useThemeStore();
const ai = useAiStore();

// ===== 树状态（懒加载：点开目录才列子层） =====
const tree = ref<Map<string, DirEntryInfo[]>>(new Map()); // dirPath -> children
const loadingDir = ref(new Set<string>());
const expanded = ref<Set<string>>(new Set());
const selected = ref("");

// ===== 搜索 =====
const query = ref("");
let searchTimer: ReturnType<typeof setTimeout> | null = null;

// ===== 文件内容 =====
const content = ref("");
const fileLoading = ref(false);
const fileInfo = ref<{ path: string; size: number; lines: number } | null>(null);
const mdSourceMode = ref(false);

const isMarkdown = computed(() => /\.md$/i.test(selected.value));

const codeEditorEl = ref<InstanceType<typeof SnippetEditor>>();
watch(codeEditorEl, async (ed) => {
  if (!ed) return;
  await new Promise(r => setTimeout(r, 80));
  ed.getEditor()?.updateOptions({ padding: { top: 14, bottom: 14 } });
  applyReveal();
});

// ===== wiki 引用跳转定位（bw-file:// 链接 → 展开祖先 + 打开 + 滚到行）=====
const pendingReveal = ref<RevealTarget | null>(null);

watch(
  () => wiki.revealTarget?.seq,
  async () => {
    const t = wiki.revealTarget;
    if (!t || !wiki.currentRepo) return;
    pendingReveal.value = t;
    // 展开祖先目录并加载各层
    const parts = t.path.split("/");
    const next = new Set(expanded.value);
    next.add("");
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join("/");
      next.add(dir);
      await loadDir(dir);
    }
    expanded.value = next;
    await openByPath(t.path);
    applyReveal();
  },
);

/** 按路径打开文件（不经过目录树节点；体积尽力从目录清单取） */
async function openByPath(path: string) {
  if (!wiki.currentRepo) return;
  selected.value = path;
  fileLoading.value = true;
  content.value = "";
  fileInfo.value = null;
  mdSourceMode.value = false;
  try {
    const text = await api.readRepoFile(wiki.currentRepo.path, path);
    const parent = path.split("/").slice(0, -1).join("/");
    const name = path.split("/").pop() || path;
    const entry = (tree.value.get(parent) || []).find(e => e.name === name);
    content.value = text;
    fileInfo.value = { path, size: entry?.size ?? 0, lines: text.split("\n").length };
  } catch (e) {
    message.error(typeof e === "string" ? e : "读取失败");
    selected.value = "";
  } finally {
    fileLoading.value = false;
  }
}

/** 应用行定位：markdown 切源码模式；编辑器就绪后滚动 + 整行高亮 */
async function applyReveal() {
  const t = pendingReveal.value;
  if (!t || selected.value !== t.path) return;
  if (isMarkdown.value) {
    mdSourceMode.value = true; // 源码视图才能定位行
    await new Promise(r => setTimeout(r, 60));
  }
  const ed = codeEditorEl.value?.getEditor();
  if (!ed) {
    // 编辑器尚在挂载（SnippetEditor 内部有重试初始化），轮询等待
    for (let i = 0; i < 10; i++) {
      await new Promise(r => setTimeout(r, 150));
      const e2 = codeEditorEl.value?.getEditor();
      if (e2) {
        doReveal(e2, t);
        return;
      }
    }
    return;
  }
  doReveal(ed, t);
}

function doReveal(ed: any, t: RevealTarget) {
  pendingReveal.value = null;
  const start = t.startLine;
  if (!start) return;
  const end = t.endLine ?? start;
  ed.revealLineInCenter(start);
  ed.setPosition({ lineNumber: start, column: 1 });
  try {
    ed.createDecorationsCollection([
      {
        range: { startLineNumber: start, endLineNumber: end, startColumn: 1, endColumn: 1 },
        options: { isWholeLine: true, className: "bw-reveal-line", linesDecorationsClassName: "bw-reveal-gutter" },
      },
    ]);
  } catch {
    /* decorations 为增强体验，失败可忽略 */
  }
}

// ===== AI 侧栏联动：选中代码右键「解释 / 添加到 AI 对话」=====
function sendSelection(mode: "explain" | "append") {
  const ed = codeEditorEl.value?.getEditor();
  const sel = ed?.getSelection();
  const model = ed?.getModel();
  if (!ed || !sel || !model || sel.isEmpty()) {
    message.warning("请先选中一段代码");
    return;
  }
  const code = model.getValueInRange(sel).trim();
  if (!code) return;
  const ctx = { code, file: selected.value, startLine: sel.startLineNumber, endLine: sel.endLineNumber };
  if (mode === "explain") ai.explainSelection(ctx);
  else ai.appendSelection(ctx);
}
const aiMenuItems = computed<EditorContextMenuItem[]>(() => [
  { id: "ai-explain-selection", label: "解释这段代码", run: () => sendSelection("explain") },
  { id: "ai-append-selection", label: "添加到 AI 对话", run: () => sendSelection("append") },
]);

const EXT_LANG: Record<string, string> = {
  py: "python", pyi: "python",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  go: "go", java: "java", rs: "rust",
  json: "json", yaml: "yaml", yml: "yaml", toml: "ini", ini: "ini",
  css: "css", scss: "scss", less: "less", html: "html", vue: "html",
  sh: "shell", bat: "bat", sql: "sql",
};
const lang = computed(() => EXT_LANG[selected.value.split(".").pop() ?? ""] ?? "plaintext");

async function loadDir(dir: string) {
  if (!wiki.currentRepo || loadingDir.value.has(dir) || tree.value.has(dir)) return;
  const next = new Set(loadingDir.value);
  next.add(dir);
  loadingDir.value = next;
  try {
    const entries = await api.listRepoDir(wiki.currentRepo.path, dir);
    tree.value = new Map(tree.value).set(dir, entries);
  } catch (e) {
    message.error(typeof e === "string" ? e : "读取目录失败");
  } finally {
    const n2 = new Set(loadingDir.value);
    n2.delete(dir);
    loadingDir.value = n2;
  }
}

function toggleDir(node: DirEntryInfo) {
  const path = node.path;
  const next = new Set(expanded.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
    loadDir(path);
  }
  expanded.value = next;
}

async function openFile(node: DirEntryInfo) {
  if (node.is_dir || !wiki.currentRepo) return;
  selected.value = node.path;
  fileLoading.value = true;
  content.value = "";
  fileInfo.value = null;
  mdSourceMode.value = false;
  try {
    const text = await api.readRepoFile(wiki.currentRepo.path, node.path);
    content.value = text;
    fileInfo.value = { path: node.path, size: node.size, lines: text.split("\n").length };
  } catch (e) {
    message.error(typeof e === "string" ? e : "读取失败");
    selected.value = "";
  } finally {
    fileLoading.value = false;
  }
}

/** 搜索：命中后展开祖先并选第一个 */
async function runSearch() {
  const q = query.value.trim();
  if (!wiki.currentRepo || !q) return;
  try {
    const hits = await api.searchRepoFiles(wiki.currentRepo.path, q);
    if (!hits.length) return;
    const first = hits[0];
    // 展开全部祖先
    const next = new Set(expanded.value);
    const parts = first.split("/");
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join("/");
      next.add(dir);
      loadDir(dir);
    }
    expanded.value = next;
  } catch {
    /* 静默 */
  }
}
watch(query, () => {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = searchTimer && null;
  searchTimer = setTimeout(runSearch, 300);
});

onMounted(async () => {
  await wiki.ensureRepo(Number(route.params.id));
  loadDir("");
});

const fmtSize = (n: number) => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`);

/** 可见节点扁平表：递归展开当前 expanded 集合内的子树（无限层级），
 *  单列表渲染天然支持省略号，替代嵌套模板（曾因 flex-wrap 导致长文件名换行炸布局） */
interface VisibleNode {
  node: DirEntryInfo;
  depth: number;
}
const visibleNodes = computed<VisibleNode[]>(() => {
  const out: VisibleNode[] = [];
  const walk = (dir: string, depth: number) => {
    for (const n of tree.value.get(dir) ?? []) {
      out.push({ node: n, depth });
      if (n.is_dir && expanded.value.has(n.path)) walk(n.path, depth + 1);
    }
  };
  walk("", 0);
  return out;
});
</script>

<template>
  <div class="fb-page">
    <!-- 左：文件树 -->
    <aside class="fb-tree">
      <div class="fb-tree-head">
        <SearchOutlined class="fb-search-icon" />
        <input
          v-model="query"
          class="fb-search"
          placeholder="搜索文件名，回车定位"
          @keyup.enter="runSearch"
        />
      </div>
      <div class="fb-tree-body">
        <div
          v-for="vn in visibleNodes"
          :key="vn.node.path"
          class="fb-node"
          :style="{ paddingLeft: 8 + vn.depth * 14 + 'px' }"
          :class="{ dir: vn.node.is_dir, active: vn.node.path === selected }"
          :title="vn.node.path"
          @click="vn.node.is_dir ? toggleDir(vn.node) : openFile(vn.node)"
        >
          <CaretRightOutlined v-if="vn.node.is_dir" class="fb-caret" :class="{ open: expanded.has(vn.node.path) }" />
          <span v-else class="fb-caret-spacer"></span>
          <FolderFilled v-if="vn.node.is_dir" class="fb-icon dir" />
          <FileOutlined v-else class="fb-icon" />
          <span class="fb-name">{{ vn.node.name }}</span>
        </div>
        <div v-if="loadingDir.has('')" class="fb-tree-loading">加载中…</div>
        <div v-else-if="!visibleNodes.length" class="fb-tree-empty">
          <div class="fbte-title">暂无节点</div>
          <div class="fbte-sub">仓库为空或被过滤</div>
        </div>
      </div>
    </aside>

    <!-- 右：内容查看 -->
    <section class="fb-viewer">
      <div v-if="fileInfo" class="fb-viewer-head">
        <span class="fb-path">{{ fileInfo.path }}</span>
        <span class="fb-meta">{{ lang }} · {{ fileInfo.lines }} 行 · {{ fmtSize(fileInfo.size) }}</span>
        <button
          v-if="isMarkdown"
          class="btn sm"
          :title="mdSourceMode ? '切换为渲染' : '切换为源码'"
          @click="mdSourceMode = !mdSourceMode"
        >
          <EyeOutlined v-if="mdSourceMode" />
          <CodeOutlined v-else />
          {{ mdSourceMode ? "渲染" : "源码" }}
        </button>
      </div>
      <div class="fb-viewer-body">
        <div v-if="fileLoading" class="fb-state"><EmptyState text="读取文件…" /></div>
        <div v-else-if="!selected" class="fb-state">
          <EmptyState text="选择左侧文件查看" hint="纯只读浏览——改代码指挥 AI，看代码用本页" />
        </div>
        <template v-else>
          <MdPreview
            v-if="isMarkdown && !mdSourceMode"
            :model-value="content"
            :theme="themeStore.isDark ? 'dark' : 'light'"
            class="fb-md"
          />
          <div v-else class="fb-code">
            <SnippetEditor
              ref="codeEditorEl"
              :model-value="content"
              :language="lang"
              :readonly="true"
              :auto-detect="false"
              :context-menu-items="aiMenuItems"
            />
          </div>
        </template>
      </div>
    </section>
  </div>
</template>

<style scoped>
.fb-page {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--spacing-sm);
}
.fb-tree {
  width: 260px;
  flex-shrink: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.fb-tree-head {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 9px 10px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.fb-search-icon {
  color: var(--text-tertiary);
  font-size: 12px;
}
.fb-search {
  flex: 1;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 12.5px;
  outline: none;
}
.fb-search::placeholder {
  color: var(--text-disabled);
}
.fb-tree-body {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}
.fb-tree-loading {
  padding: 12px;
  font-size: 12px;
  color: var(--text-tertiary);
}
.fb-tree-empty {
  flex: 1;
  min-height: 140px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 5px;
  text-align: center;
}
.fbte-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-secondary);
}
.fbte-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}
.fb-node {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  padding-right: 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-secondary);
  font-size: 13px;
  transition: background var(--transition-fast);
}
.fb-node:hover {
  background: var(--bg-elevated);
}
.fb-node.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
}
.fb-caret {
  font-size: 9px;
  color: var(--text-tertiary);
  transition: transform var(--transition-fast);
  flex-shrink: 0;
}
.fb-caret.open {
  transform: rotate(90deg);
}
.fb-caret-spacer {
  width: 9px;
  flex-shrink: 0;
}
.fb-icon {
  font-size: 13px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.fb-icon.dir {
  color: var(--color-warning);
}
.fb-name {
  flex: 1;
  min-width: 0;
  line-height: 1.5;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fb-viewer {
  flex: 1;
  min-width: 0;
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.fb-viewer-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.fb-path {
  font-family: "JetBrains Mono", monospace;
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.fb-meta {
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  flex-shrink: 0;
  margin-left: auto;
}
.fb-viewer-body {
  flex: 1;
  min-height: 0;
  display: flex;
  position: relative;
}
.fb-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.fb-code {
  flex: 1;
  min-width: 0;
}
.fb-code :deep(.snippet-editor) {
  border-radius: 0;
  height: 100%;
}
.fb-md {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}
</style>

<style>
/* wiki 引用定位高亮（Monaco 装饰类，必须全局） */
.bw-reveal-line {
  background: color-mix(in oklch, var(--color-primary) 15%, transparent) !important;
}
.bw-reveal-gutter {
  background: var(--color-primary) !important;
  margin-left: 3px;
  width: 3px !important;
}
</style>
