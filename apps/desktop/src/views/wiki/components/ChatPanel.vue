<script setup lang="ts">
/**
 * AI 对话面板（项目空间 → 对话 tab / 右侧内嵌侧栏）——参考 byte-writer AiDock 重做。
 *
 * timeline 结构：user / assistant（含思考过程折叠块）。
 * 流式 Markdown 本地渲染（MdStreamPreview，120ms 节流）；思考过程独立通道
 * （wiki://chat-thinking，流式自动展开、结束折叠）；服务端真取消（chat_cancel，
 * token 边界中断 SSE）；出处芯片化可点跳文档。
 * composer 底栏（byte-writer 式）：模型选择（provider 覆盖，经 repo_chat 的
 * provider_id 路由）+ 思考级别（prompt 指令拼接，对不支持思考参数的模型也生效）。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message } from "ant-design-vue";
import {
  SendOutlined,
  StopOutlined,
  ClearOutlined,
  BulbOutlined,
  DownOutlined,
  RightOutlined,
  SearchOutlined,
  ApiOutlined,
  AppstoreOutlined,
  CheckOutlined,
} from "@ant-design/icons-vue";
import { useThemeStore } from "@/stores/theme";
import { useAiStore, type AiSelectionCtx } from "@/stores/ai";
import { api } from "@/api";
import type { LlmProvider } from "@/api/llm";
import AiSparkIcon from "@/components/icons/AiSparkIcon.vue";
import MdStreamPreview from "@/components/editor/MdStreamPreview.vue";

const props = defineProps<{
  repoPath: string;
  /** 查看分支（null=当前 git 分支；问答检索按该分支的向量索引只读进行） */
  branch?: string | null;
}>();
const emit = defineEmits<{ (e: "open-doc", docPath: string): void }>();

const themeStore = useThemeStore();
const mdTheme = computed(() => (themeStore.isDark ? "dark" : "light"));

interface UserItem {
  kind: "user";
  text: string;
}
interface AiItem {
  kind: "assistant";
  text: string;
  thinking: string;
  thinkOpen: boolean;
  streaming: boolean;
  sources: string[];
}
type TimelineItem = UserItem | AiItem;

const timeline = ref<TimelineItem[]>([]);
const input = ref("");
const asking = ref(false);
const listEl = ref<HTMLElement | null>(null);
/** 本轮 request_id（取消配对） */
const requestId = ref("");

function scrollToBottom() {
  nextTick(() => {
    if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
  });
}

let unDelta: (() => void) | null = null;
let unThink: (() => void) | null = null;

onMounted(async () => {
  // 模型选择数据源（启用且配置了对话模型的供应商）
  try {
    const c = await api.getLlmConfig();
    chatProviders.value = c.providers.filter((p) => p.enabled && p.chat_model);
    // 记忆的供应商已不可用时回落默认
    if (modelPick.value && !chatProviders.value.some((p) => p.id === modelPick.value)) {
      pickModel(null);
    }
  } catch { /* 配置读取失败：chip 显示「默认」仍可用 */ }
  unDelta = await listen<string>("wiki://chat-delta", (ev) => {
    const last = timeline.value[timeline.value.length - 1];
    if (last?.kind === "assistant" && last.streaming) {
      last.text += ev.payload;
      scrollToBottom();
    }
  });
  unThink = await listen<string>("wiki://chat-thinking", (ev) => {
    const last = timeline.value[timeline.value.length - 1];
    if (last?.kind === "assistant" && last.streaming) {
      last.thinking += ev.payload;
      scrollToBottom();
    }
  });
});
onBeforeUnmount(() => {
  unDelta?.();
  unThink?.();
});

async function ask() {
  const q = input.value.trim();
  if (!q || asking.value) return;
  input.value = "";
  const ai: AiItem = {
    kind: "assistant",
    text: "",
    thinking: "",
    thinkOpen: false,
    streaming: true,
    sources: [],
  };
  const history: [string, string][] = timeline.value
    .slice(-8)
    .map((it) =>
      it.kind === "user"
        ? (["user", it.text] as [string, string])
        : (["assistant", it.text] as [string, string]),
    )
    .filter(([, t]) => t.length > 0);

  // 思考级别指令拼进发送内容（对不支持思考参数的模型也生效）；timeline 显示原始问题
  const directive = THINK_DIRECTIVES[thinkLevel.value as Exclude<ThinkLevel, "auto">];
  const wire = directive ? `${q}${directive}` : q;

  timeline.value.push({ kind: "user", text: q }, ai);
  asking.value = true;
  requestId.value = `chat-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  scrollToBottom();

  try {
    const answer = await invoke<{ text: string; sources: string[] }>("repo_chat", {
      path: props.repoPath,
      question: wire,
      history,
      requestId: requestId.value,
      providerId: modelPick.value ?? undefined,
      branch: props.branch ?? null,
    });
    ai.sources = answer.sources ?? [];
    if (!ai.text && answer.text) ai.text = answer.text;
  } catch (e) {
    const msg = typeof e === "string" ? e : String(e);
    if (msg.includes("__CHAT_CANCELLED__")) {
      ai.text += ai.text ? "\n\n（已停止）" : "（已停止）";
    } else {
      ai.text = `回答失败：${msg}`;
      message.error(msg.slice(0, 120));
    }
  } finally {
    ai.streaming = false;
    ai.thinkOpen = false;
    asking.value = false;
    requestId.value = "";
    scrollToBottom();
  }
}

async function stop() {
  if (!requestId.value) return;
  try {
    await invoke("chat_cancel", { requestId: requestId.value });
  } catch {
    /* 注册表无此 id（已完成）静默 */
  }
}

function clearAll() {
  if (asking.value) return;
  timeline.value = [];
}

// ===== composer 底栏：模型选择 / 思考级别（参考 byte-writer AiDock）=====

/** 模型选择：null = 跟随设置中的默认供应商；否则按供应商覆盖路由 */
const PROVIDER_KEY = "bw-ai-chat-provider";
const modelPick = ref<string | null>(localStorage.getItem(PROVIDER_KEY) || null);
function pickModel(id: string | null) {
  modelPick.value = id;
  if (id) localStorage.setItem(PROVIDER_KEY, id);
  else localStorage.removeItem(PROVIDER_KEY);
  modelPopOpen.value = false;
}
const modelPopOpen = ref(false);

const chatProviders = ref<LlmProvider[]>([]);
const modelChipLabel = computed(() => {
  const p = chatProviders.value.find((x) => x.id === modelPick.value);
  return p ? p.name : "默认";
});

const THINKS = [
  { v: "auto", label: "自动", desc: "跟随模型默认" },
  { v: "off", label: "关", desc: "直接给结论，不展开推理" },
  { v: "medium", label: "中", desc: "先简要分析再回答" },
  { v: "high", label: "深", desc: "完整推演方案后回答" },
] as const;
type ThinkLevel = (typeof THINKS)[number]["v"];
const THINK_KEY = "bw-ai-chat-think";
const thinkLevel = ref<ThinkLevel>((localStorage.getItem(THINK_KEY) as ThinkLevel) || "auto");
watch(thinkLevel, (v) => localStorage.setItem(THINK_KEY, v));
const thinkLabel = computed(() => THINKS.find((t) => t.v === thinkLevel.value)?.label ?? "自动");

/** 思考级别的提示词指令（wire 级参数之外，对不支持思考参数的模型仍生效） */
const THINK_DIRECTIVES: Record<Exclude<ThinkLevel, "auto">, string> = {
  off: "\n\n[思考级别:关] 直接给出结论与回答，不展开长推理。",
  medium: "\n\n[思考级别:中] 先简要分析问题，再给出回答。",
  high: "\n\n[思考级别:深] 先在内部完整推演（问题结构、边界情况），再给出最终回答。",
};

// ===== 编辑器选中注入（右键「解释这段代码 / 添加到 AI 对话」，ai store 总线）=====
const ai = useAiStore();
const taEl = ref<{ focus: () => void } | null>(null);

const EXT_LANG: Record<string, string> = {
  py: "python", pyi: "python",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript",
  go: "go", java: "java", rs: "rust",
  json: "json", yml: "yaml", yaml: "yaml", toml: "toml", md: "markdown", sql: "sql",
};
const langOf = (file: string) => EXT_LANG[file.split(".").pop() ?? ""] ?? "";
const ctxBlock = (p: AiSelectionCtx) => `\`\`\`${langOf(p.file)}\n${p.code}\n\`\`\``;

watch(
  () => ai.pending?.seq,
  () => {
    const p = ai.consumePending();
    if (!p) return;
    if (p.mode === "explain") {
      input.value = `请解释这段代码（${p.file}:${p.startLine}-${p.endLine}）的作用与逻辑：\n\n${ctxBlock(p)}`;
      ask();
    } else {
      const ref = `（来自 ${p.file}:${p.startLine}-${p.endLine}）\n${ctxBlock(p)}`;
      input.value = input.value ? `${input.value}\n\n${ref}` : ref;
      nextTick(() => taEl.value?.focus());
    }
  },
  // immediate：注入触发侧栏从关到开时，ChatPanel 挂载晚于 store 写入，需挂载即消费
  { immediate: true },
);

function onComposerKey(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    ask();
  }
}

function openSource(docPath: string) {
  if (docPath.endsWith(".md")) emit("open-doc", docPath);
  else message.info(`代码出处：${docPath}`);
}

function fmtSource(p: string) {
  return p.replace(/^.*?\.bytewiki\//, "").replace(/\.md$/, "");
}
</script>

<template>
  <div class="chat-dock">
    <!-- 顶部：清空会话 -->
    <div v-show="timeline.length" class="dock-top">
      <button class="btn sm" title="清空会话" :disabled="asking" @click="clearAll">
        <ClearOutlined /> 清空
      </button>
    </div>

    <div ref="listEl" class="dock-timeline">
      <div v-if="!timeline.length" class="dock-hint">
        <p>基于本仓库 Wiki 与代码的 RAG 问答——先「一键生成」建立索引。</p>
        <ul>
          <li>「这个仓库的模块结构是什么？」</li>
          <li>「issue_token 的调用链经过哪些函数？」</li>
          <li>「配置读取是在哪里做的？」</li>
        </ul>
      </div>

      <template v-for="(item, i) in timeline" :key="i">
        <div v-if="item.kind === 'user'" class="msg user">
          <div class="msg-role">你</div>
          <div class="msg-text">{{ item.text }}</div>
        </div>

        <div v-else class="msg ai">
          <div class="msg-role"><AiSparkIcon class="msg-role-ico" :size="12" />AI</div>
          <div v-if="item.thinking" class="think-block" @click.stop="item.thinkOpen = !item.thinkOpen">
            <div class="think-head">
              <BulbOutlined class="think-ico" />
              <span>{{ item.streaming && !item.text ? "思考中…" : "思考过程" }}</span>
              <DownOutlined v-if="item.thinkOpen || (item.streaming && !item.text)" class="think-caret" />
              <RightOutlined v-else class="think-caret" />
            </div>
            <pre v-if="item.thinkOpen || (item.streaming && !item.text)" class="think-body">{{ item.thinking }}</pre>
          </div>
          <div class="msg-text">
            <MdStreamPreview :source="item.text" :streaming="item.streaming" :theme="mdTheme" />
            <span v-if="item.streaming && item.text" class="stream-cursor"></span>
          </div>
          <div v-if="item.sources.length" class="msg-sources">
            <span class="src-label"><SearchOutlined /> 出处</span>
            <button v-for="s in item.sources" :key="s" class="src-chip" :title="s" @click="openSource(s)">
              <ApiOutlined class="src-ico" />
              {{ fmtSource(s) }}
            </button>
          </div>
        </div>
      </template>
    </div>

    <div class="dock-foot">
      <!-- 输入容器：textarea + 内嵌工具行（模型 / 思考级别 / 发送） -->
      <div class="composer-box">
        <a-textarea
          ref="taEl"
          v-model:value="input"
          class="cb-input"
          :bordered="false"
          :rows="2"
          :auto-size="{ minRows: 2, maxRows: 6 }"
          placeholder="问点什么… Enter 发送（Shift+Enter 换行）"
          :disabled="asking"
          @keydown="onComposerKey"
        />
        <div class="cb-tools">
          <!-- 模型选择：默认 = 设置中的供应商；按启用供应商覆盖 -->
          <a-popover v-model:open="modelPopOpen" trigger="click" placement="topLeft">
            <button class="chip ghost" title="模型">
              <AppstoreOutlined class="chip-ico" />{{ modelChipLabel }}<span class="chip-caret">▾</span>
            </button>
            <template #content>
              <div class="mp-list">
                <div class="mp-item" :class="{ cur: !modelPick }" @click="pickModel(null)">
                  <span>默认（设置中的供应商）</span>
                  <CheckOutlined v-if="!modelPick" class="mp-check" />
                </div>
                <div
                  v-for="p in chatProviders"
                  :key="p.id"
                  class="mp-item"
                  :class="{ cur: modelPick === p.id }"
                  @click="pickModel(p.id)"
                >
                  <span class="mp-item-main">{{ p.name }}<em class="mp-model">{{ p.chat_model }}</em></span>
                  <CheckOutlined v-if="modelPick === p.id" class="mp-check" />
                </div>
                <div v-if="!chatProviders.length" class="mp-item dim">暂无可用供应商，请在设置中添加</div>
              </div>
            </template>
          </a-popover>

          <!-- 思考级别 -->
          <a-popover trigger="click" placement="top">
            <button class="chip ghost" title="思考级别">
              <BulbOutlined class="chip-ico" />思考·{{ thinkLabel }}<span class="chip-caret">▾</span>
            </button>
            <template #content>
              <div class="mp-list narrow">
                <div
                  v-for="t in THINKS"
                  :key="t.v"
                  class="mp-item stack"
                  :class="{ cur: thinkLevel === t.v }"
                  @click="thinkLevel = t.v"
                >
                  <div class="mp-item-line">
                    <span>思考·{{ t.label }}</span>
                    <CheckOutlined v-if="thinkLevel === t.v" class="mp-check" />
                  </div>
                  <div class="mp-item-desc">{{ t.desc }}</div>
                </div>
              </div>
            </template>
          </a-popover>

          <span class="bar-spacer"></span>
          <button v-if="asking" class="btn sm danger" title="停止" @click="stop"><StopOutlined /></button>
          <button v-else class="btn sm primary cb-send" :disabled="!input.trim()" title="发送" @click="ask">
            <SendOutlined />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-dock {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-card);
}
/* 顶部清空行 */
.dock-top {
  display: flex;
  justify-content: flex-end;
  padding: 6px 10px 0;
  flex-shrink: 0;
}
.dock-timeline {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 18px 16px;
}
.dock-hint {
  color: var(--text-tertiary);
  font-size: 13px;
  padding: 40px 12px;
  text-align: center;
}
.dock-hint p {
  margin-bottom: 10px;
  color: var(--text-secondary);
}
.dock-hint ul {
  list-style: none;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.dock-hint li {
  font-size: 12.5px;
  opacity: 0.85;
}

/* 消息（去气泡设计：AI 无框；用户轻底右对齐） */
.msg {
  margin-bottom: 16px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.msg-role {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.msg-role-ico {
  font-size: 11px;
  color: var(--color-primary);
}
.msg.user .msg-role {
  align-self: flex-end;
}
.msg.user .msg-text {
  font-size: 13.5px;
  color: var(--text-primary);
  white-space: pre-wrap;
  padding: 8px 12px;
  background: var(--bg-input);
  border-radius: var(--radius-md);
  align-self: flex-end;
  max-width: 88%;
  word-break: break-word;
}
.msg.ai .msg-text {
  font-size: 13px;
  min-width: 0;
}

/* 流式光标 */
.stream-cursor {
  display: inline-block;
  width: 7px;
  height: 15px;
  margin-left: 2px;
  vertical-align: text-bottom;
  background: var(--color-primary);
  border-radius: 1px;
  animation: blink 0.9s steps(2) infinite;
}
@keyframes blink {
  50% { opacity: 0; }
}

/* 思考过程折叠块 */
.think-block {
  border: 1px dashed var(--border-base);
  border-radius: var(--radius-md);
  overflow: hidden;
  cursor: pointer;
  max-width: 100%;
}
.think-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  font-size: 11.5px;
  color: var(--text-tertiary);
  user-select: none;
}
.think-ico {
  font-size: 11px;
  color: #c98a1e;
}
.think-caret {
  font-size: 9px;
  margin-left: auto;
}
.think-body {
  margin: 0;
  padding: 8px 12px;
  font-size: 11.5px;
  line-height: 1.65;
  color: var(--text-tertiary);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 260px;
  overflow-y: auto;
  border-top: 1px dashed var(--border-base);
  font-family: inherit;
}

/* 出处芯片 */
.msg-sources {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  align-items: center;
  margin-top: 3px;
}
.src-label {
  font-size: 11px;
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.src-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 1px solid var(--border-base);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 11.5px;
  font-family: "JetBrains Mono", monospace;
  padding: 1px 8px;
  border-radius: 999px;
  cursor: pointer;
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: all var(--transition-fast);
}
.src-chip:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}
.src-ico {
  font-size: 10px;
  color: var(--text-tertiary);
}

/* 底部输入区 */
.dock-foot {
  padding: 8px 12px 10px;
  flex-shrink: 0;
}
/* 输入容器：textarea + 内嵌工具行 */
.composer-box {
  border: 1px solid var(--border-base);
  border-radius: var(--radius-md);
  background: var(--bg-input);
  display: flex;
  flex-direction: column;
  transition: border-color var(--transition-fast);
}
.composer-box:focus-within {
  border-color: var(--color-primary-border);
}
.composer-box :deep(.cb-input) {
  background: transparent;
  padding: 8px 12px 4px;
}
.composer-box :deep(.cb-input:focus) {
  box-shadow: none;
}
.cb-tools {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px 6px 8px;
}
.bar-spacer {
  flex: 1;
}
/* chip 工具按钮（模型 / 思考级别） */
.chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 1px solid var(--border-base);
  background: var(--bg-input);
  color: var(--text-secondary);
  border-radius: 999px;
  font-size: 11.5px;
  padding: 1px 9px;
  line-height: 1.7;
  cursor: pointer;
  white-space: nowrap;
  transition: color var(--transition-fast), border-color var(--transition-fast), background var(--transition-fast);
}
/* 内嵌输入框的 ghost 变体：无边框透明底 */
.chip.ghost {
  border-color: transparent;
  background: transparent;
  color: var(--text-tertiary);
}
.chip.ghost:hover {
  background: var(--bg-surface);
  color: var(--text-primary);
  border-color: transparent;
}
.chip-ico {
  font-size: 11px;
}
.chip-caret {
  font-size: 9px;
  opacity: 0.55;
}
/* 发送：方形小按钮贴工具行右端 */
.cb-send {
  height: 24px;
  width: 28px;
  justify-content: center;
  padding: 0;
  flex-shrink: 0;
}
</style>

<style>
/* popover 内容经 teleport 挂 body，scoped 不命中——mp-* 前缀防泄漏 */
.mp-list {
  min-width: 230px;
  max-height: 320px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.mp-list.narrow {
  min-width: 200px;
}
.mp-item {
  padding: 7px 10px;
  font-size: 12.5px;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  border-radius: var(--radius-sm);
}
.mp-item:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.mp-item.cur {
  color: var(--color-primary);
  font-weight: 600;
}
.mp-item.dim {
  color: var(--text-tertiary);
  cursor: default;
}
.mp-item.stack {
  flex-direction: column;
  align-items: stretch;
  gap: 2px;
}
.mp-item-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.mp-item-main {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
}
.mp-model {
  font-style: normal;
  font-size: 10.5px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mp-item-desc {
  font-size: 11px;
  color: var(--text-tertiary);
  font-weight: 400;
}
.mp-check {
  font-size: 11px;
  color: var(--color-primary);
  flex-shrink: 0;
}
</style>
