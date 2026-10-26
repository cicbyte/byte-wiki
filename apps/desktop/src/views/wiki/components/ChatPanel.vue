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
  PlusOutlined,
  HistoryOutlined,
  CaretRightOutlined,
  DeleteOutlined,
  EditOutlined,
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
import { useWikiStore } from "@/stores/wiki";
import { api } from "@/api";
import type { LlmProvider } from "@/api/llm";
import AiSparkIcon from "@/components/icons/AiSparkIcon.vue";
import MdStreamPreview from "@/components/editor/MdStreamPreview.vue";

const props = defineProps<{
  repoPath: string;
  /** 查看分支（null=当前 git 分支；问答检索按该分支的向量索引只读进行） */
  branch?: string | null;
  /** 紧凑形态（AI 侧栏窄容器）：不显示左侧会话栏，当前会话标题点击弹列表 */
  compact?: boolean;
}>();


const themeStore = useThemeStore();
const wiki = useWikiStore();
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
let ctxOutsideCloser: ((e: MouseEvent) => void) | null = null;

onMounted(async () => {
  restoreChat();
  document.addEventListener("click", closeCtxMenu);
  ctxOutsideCloser = (e: MouseEvent) => {
    // 菜单外右键：关菜单（列表项自身的 contextmenu 由 openCtxMenu 处理并开新菜单）
    if (!(e.target as HTMLElement).closest(".sess-item, .bw-sess-item")) closeCtxMenu();
  };
  document.addEventListener("contextmenu", ctxOutsideCloser);
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
  if (ctxOutsideCloser) document.removeEventListener("contextmenu", ctxOutsideCloser);
  document.removeEventListener("click", closeCtxMenu);
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
    persistChat();
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

// ===== 会话持久化（localStorage 按仓库维度，多会话；切页/重启后仍在）=====
const chatKey = computed(() => `bw:chat:${props.repoPath}`);
/** 存储结构 v2：{ v:2, sessions:[{id,title,ts,items}], activeId } */
interface StoredSession {
  id: string;
  title: string;
  ts: number;
  items: Array<{ k: string; t: string; s?: string[] }>;
}
interface StoredChats {
  v: 2;
  sessions: StoredSession[];
  activeId: string | null;
}
/** 会话上限：超出裁最旧（localStorage 配额保护） */
const MAX_SESSIONS = 30;

const chatSessions = ref<StoredSession[]>([]);
const activeChatId = ref<string | null>(null);
const historyOpen = ref(false);

function loadChats(): StoredChats {
  try {
    const raw = localStorage.getItem(chatKey.value);
    if (!raw) return { v: 2, sessions: [], activeId: null };
    const parsed = JSON.parse(raw) as StoredChats | Array<{ k: string; t: string; s?: string[] }>;
    if (Array.isArray(parsed)) {
      // v1（单会话数组）迁移为一条会话
      return parsed.length
        ? { v: 2, sessions: [{ id: `s-${Date.now()}`, title: parsed.find((x) => x.k === "u")?.t.slice(0, 40) || "历史对话", ts: Date.now(), items: parsed.slice(-400) }], activeId: null }
        : { v: 2, sessions: [], activeId: null };
    }
    return parsed;
  } catch {
    return { v: 2, sessions: [], activeId: null };
  }
}

function saveChats(store: StoredChats) {
  try {
    store.sessions = store.sessions.slice(-MAX_SESSIONS);
    localStorage.setItem(chatKey.value, JSON.stringify(store));
  } catch { /* 配额/序列化失败静默 */ }
}

/** 当前时间线 → 精简 items（thinking/流式态不入库；上限 400 条） */
function slimItems() {
  return timeline.value
    .filter((it) => it.kind === "user" || (it as AiItem).text)
    .map((it) => (it.kind === "user" ? { k: "u", t: it.text } : { k: "a", t: it.text, s: it.sources }))
    .slice(-400);
}

/** 问答收尾/切换时落盘当前会话 */
function persistChat() {
  if (activeChatId.value) {
    const store = loadChats();
    const hit = store.sessions.find((s) => s.id === activeChatId.value);
    const items = slimItems();
    if (hit && items.length) {
      hit.items = items;
      hit.ts = Date.now();
    } else if (hit) {
      store.sessions = store.sessions.filter((s) => s.id !== activeChatId.value); // 空会话不保留
    } else if (items.length) {
      store.sessions.push({ id: activeChatId.value, title: items.find((x) => x.k === "u")?.t.slice(0, 40) || "新对话", ts: Date.now(), items });
    }
    store.activeId = items.length ? activeChatId.value : null;
    saveChats(store);
    chatSessions.value = store.sessions;
  }
}

function itemsToTimeline(items: Array<{ k: string; t: string; s?: string[] }>) {
  return items.map((x) =>
    x.k === "u"
      ? { kind: "user" as const, text: x.t }
      : { kind: "assistant" as const, text: x.t, thinking: "", thinkOpen: false, streaming: false, sources: x.s ?? [] },
  );
}

function restoreChat() {
  const store = loadChats();
  chatSessions.value = store.sessions;
  const active = store.sessions.find((s) => s.id === store.activeId) ?? (store.sessions.length ? store.sessions[store.sessions.length - 1] : null);
  activeChatId.value = active?.id ?? null;
  timeline.value = active ? itemsToTimeline(active.items) : [];
  scrollToBottom();
}

/** 新对话：当前会话先落盘，切到空白（首条问答时 persistChat 建 id） */
function newChat() {
  if (asking.value) return;
  persistChat();
  activeChatId.value = `s-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
  timeline.value = [];
  historyOpen.value = false;
}

function switchChat(id: string) {
  if (asking.value || id === activeChatId.value) {
    historyOpen.value = false;
    return;
  }
  persistChat();
  const hit = chatSessions.value.find((s) => s.id === id);
  if (!hit) return;
  activeChatId.value = id;
  timeline.value = itemsToTimeline(hit.items);
  // 会话激活态落盘（下次打开直接回到它）
  const store = loadChats();
  store.activeId = id;
  saveChats(store);
  historyOpen.value = false;
  scrollToBottom();
}

// ===== 会话右键菜单（重命名 / 删除；两个形态共用）=====
const ctxMenu = ref<{ x: number; y: number; id: string; title: string } | null>(null);
function openCtxMenu(e: MouseEvent, s: StoredSession) {
  e.preventDefault();
  ctxMenu.value = { x: e.clientX, y: e.clientY, id: s.id, title: s.title };
}
function closeCtxMenu() {
  ctxMenu.value = null;
}

/** 重命名：行内编辑（Enter/失焦提交，Esc 取消） */
const renamingId = ref<string | null>(null);
const renameBuf = ref("");
function startRename(id: string, title: string) {
  closeCtxMenu();
  renamingId.value = id;
  renameBuf.value = title;
}
function commitRename() {
  const id = renamingId.value;
  const name = renameBuf.value.trim();
  renamingId.value = null;
  if (!id || !name) return;
  const store = loadChats();
  const hit = store.sessions.find((s) => s.id === id);
  if (hit && hit.title !== name) {
    hit.title = name.slice(0, 60);
    saveChats(store);
    chatSessions.value = store.sessions;
  }
}
function cancelRename() {
  renamingId.value = null;
}

function removeChat(id: string) {
  if (asking.value) return;
  const store = loadChats();
  store.sessions = store.sessions.filter((s) => s.id !== id);
  if (activeChatId.value === id) {
    activeChatId.value = null;
    timeline.value = [];
  }
  store.activeId = activeChatId.value;
  saveChats(store);
  chatSessions.value = store.sessions;
}

function fmtSessionTime(ts: number) {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

watch(chatKey, () => { timeline.value = []; restoreChat(); });

/** 历史列表（时间倒序）；当前标题（compact 形态显示用） */
const sortedSessions = computed(() => [...chatSessions.value].sort((a, b) => b.ts - a.ts));
const currentTitle = computed(
  () => chatSessions.value.find((s) => s.id === activeChatId.value)?.title ?? "新对话",
);

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
const thinkPopOpen = ref(false);
function pickThink(v: ThinkLevel) {
  thinkLevel.value = v;
  thinkPopOpen.value = false;
}

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

// ===== 出处抽屉：点击芯片就地渲染对应 Wiki 文档（按查看分支读取）=====
const srcOpen = ref(false);
const srcLoading = ref(false);
const srcDoc = ref("");
const srcTitle = ref("");
async function openSource(docPath: string) {
  if (!docPath.endsWith(".md")) {
    message.info(`机器产物出处（重建向量索引后将不再出现）：${docPath}`);
    return;
  }
  srcOpen.value = true;
  srcLoading.value = true;
  srcTitle.value = fmtSource(docPath);
  try {
    const branch = props.branch ?? wiki.currentRepo?.branch ?? "";
    srcDoc.value = await api.readWikiDocFor(props.repoPath, docPath, branch);
  } catch (e) {
    srcDoc.value = `> 读取失败：${typeof e === "string" ? e : "文档不存在"}`;
  } finally {
    srcLoading.value = false;
  }
}

function fmtSource(p: string) {
  return p.replace(/^.*?\.bytewiki\//, "").replace(/\.md$/, "");
}
</script>

<template>
  <div class="chat-dock">
    <!-- 左：会话栏（宽形态；AI 侧栏 compact 形态不渲染） -->
    <aside v-if="!compact" class="sess-rail">
      <button class="sess-new" title="开始新对话（当前对话自动存入历史）" :disabled="asking" @click="newChat">
        <PlusOutlined /> 新对话
      </button>
      <div class="sess-list">
        <div
          v-for="s in sortedSessions"
          :key="s.id"
          class="sess-item"
          :class="{ active: s.id === activeChatId }"
          @click="switchChat(s.id)"
          @contextmenu.prevent="openCtxMenu($event, s)"
          @dblclick="startRename(s.id, s.title)"
        >
          <input
            v-if="renamingId === s.id"
            class="si-rename"
            v-model="renameBuf"
            autofocus
            @keydown.enter.prevent="commitRename"
            @keydown.esc.prevent="cancelRename"
            @blur="commitRename"
            @click.stop
          />
          <template v-else>
            <span class="si-title">{{ s.title }}</span>
            <span class="si-time">{{ fmtSessionTime(s.ts) }}</span>
            <button class="si-del" title="删除此对话" @click.stop="removeChat(s.id)">
              <DeleteOutlined />
            </button>
          </template>
        </div>
        <div v-if="!sortedSessions.length" class="sess-empty">暂无历史对话<br />提问后自动保存</div>
      </div>
    </aside>

    <!-- 右：对话主区 -->
    <div class="chat-main">
      <!-- compact（AI 侧栏）：当前会话标题，点击弹会话列表 -->
      <div v-if="compact" class="sess-cur-row">
        <a-popover v-model:open="historyOpen" trigger="click" placement="bottomLeft" overlay-class-name="bw-sess-pop">
          <button class="sess-cur" :title="currentTitle + ' · 点击切换对话'">
            <span class="sc-title">{{ currentTitle }}</span>
            <CaretRightOutlined class="sc-caret" :class="{ open: historyOpen }" />
          </button>
          <template #content>
            <div class="bw-sess-list">
              <button class="bw-sess-new" :disabled="asking" @click="newChat">
                <PlusOutlined /> 新对话
              </button>
              <div
                v-for="s in sortedSessions"
                :key="s.id"
                class="bw-sess-item"
                :class="{ active: s.id === activeChatId }"
                @click="switchChat(s.id)"
                @contextmenu.prevent="openCtxMenu($event, s)"
                @dblclick="startRename(s.id, s.title)"
              >
                <input
                  v-if="renamingId === s.id"
                  class="si-rename"
                  v-model="renameBuf"
                  autofocus
                  @keydown.enter.prevent="commitRename"
                  @keydown.esc.prevent="cancelRename"
                  @blur="commitRename"
                  @click.stop
                />
                <template v-else>
                  <span class="bs-title">{{ s.title }}</span>
                  <span class="bs-time">{{ fmtSessionTime(s.ts) }}</span>
                  <button class="bs-del" title="删除此对话" @click.stop="removeChat(s.id)">
                    <DeleteOutlined />
                  </button>
                </template>
              </div>
              <div v-if="!sortedSessions.length" class="bw-sess-empty">暂无历史对话</div>
            </div>
          </template>
        </a-popover>
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
                </div>
              </template>
            </a-popover>

            <a-popover v-model:open="thinkPopOpen" trigger="click" placement="topLeft">
              <button class="chip ghost" title="思考级别">
                <BulbOutlined class="chip-ico" />{{ thinkLabel }}<span class="chip-caret">▾</span>
              </button>
              <template #content>
                <div class="mp-list">
                  <div
                    v-for="t in THINKS"
                    :key="t.v"
                    class="mp-item"
                    :class="{ cur: thinkLevel === t.v }"
                    @click="pickThink(t.v)"
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
    </div><!-- /chat-main -->

    <!-- 会话右键菜单（fixed 定位；两形态共用） -->
    <div
      v-if="ctxMenu"
      class="sess-ctx-menu"
      :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
      @click.stop
      @contextmenu.prevent
    >
      <button class="scx-item" @click="startRename(ctxMenu.id, ctxMenu.title)">
        <EditOutlined /> 重命名
      </button>
      <button class="scx-item danger" @click="closeCtxMenu(); removeChat(ctxMenu.id)">
        <DeleteOutlined /> 删除对话
      </button>
    </div>

    <!-- 出处抽屉：点击出处芯片就地渲染对应 Wiki 文档（与源码抽屉同交互） -->
    <a-drawer
      v-model:open="srcOpen"
      :title="srcTitle"
      width="56%"
      placement="right"
      :body-style="{ padding: '16px 20px', overflow: 'auto' }"
    >
      <div v-if="srcLoading" class="src-loading">读取文档…</div>
      <MdStreamPreview v-else :source="srcDoc" :streaming="false" :theme="mdTheme" />
    </a-drawer>
  </div>
</template>


<style scoped>
.chat-dock {
  height: 100%;
  display: flex;
  background: var(--bg-card);
}
/* 右侧对话主区（列：标题行?/时间线/输入） */
.chat-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
}

/* ===== 左侧会话栏（宽形态） ===== */
.sess-rail {
  width: 216px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--border-light);
  padding: 10px 8px;
  gap: 8px;
}
.sess-new {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 7px 0;
  border: 1px dashed var(--color-primary-border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-primary);
  font-size: 12.5px;
  cursor: pointer;
  transition: background var(--transition-fast);
  flex-shrink: 0;
}
.sess-new:hover:not(:disabled) {
  background: var(--color-primary-bg);
}
.sess-new:disabled {
  opacity: 0.5;
  cursor: default;
}
.sess-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.sess-item {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.sess-item:hover {
  background: var(--bg-surface);
}
.sess-item.active {
  background: var(--color-primary-bg);
}
.si-title {
  font-size: 12.5px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding-right: 20px;
}
.sess-item.active .si-title {
  color: var(--color-primary);
  font-weight: 600;
}
.si-time {
  font-size: 10.5px;
  color: var(--text-tertiary);
  font-family: var(--font-mono, monospace);
}
.si-del {
  position: absolute;
  top: 50%;
  right: 6px;
  transform: translateY(-50%);
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 11px;
  padding: 3px;
  border-radius: 4px;
  cursor: pointer;
  opacity: 0;
}
.sess-item:hover .si-del {
  opacity: 1;
}
.si-del:hover {
  color: var(--color-danger, #e5484d);
  background: color-mix(in srgb, var(--color-danger, #e5484d) 10%, transparent);
}
.sess-empty {
  padding: 18px 6px;
  text-align: center;
  font-size: 11.5px;
  color: var(--text-tertiary);
  line-height: 1.8;
}

/* 会话右键菜单 + 行内重命名（两形态共用） */
.si-rename {
  width: 100%;
  font-size: 12.5px;
  padding: 2px 6px;
  border: 1px solid var(--color-primary-border);
  border-radius: var(--radius-xs);
  background: var(--bg-input);
  color: var(--text-primary);
  outline: none;
}
.sess-ctx-menu {
  position: fixed;
  z-index: 1200;
  min-width: 132px;
  background: var(--bg-elevated, var(--bg-card));
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.scx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 12.5px;
  padding: 6px 10px;
  border-radius: var(--radius-xs);
  cursor: pointer;
  text-align: left;
}
.scx-item:hover {
  background: var(--bg-surface);
}
.scx-item.danger {
  color: var(--color-danger, #e5484d);
}
.scx-item.danger:hover {
  background: color-mix(in srgb, var(--color-danger, #e5484d) 8%, transparent);
}

/* ===== compact（AI 侧栏）：当前会话标题行 ===== */
.sess-cur-row {
  flex-shrink: 0;
  padding: 6px 10px 0;
}
.sess-cur {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 100%;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  padding: 3px 8px;
  border-radius: var(--radius-xs);
  cursor: pointer;
}
.sess-cur:hover {
  background: var(--bg-surface);
}
.sc-title {
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sc-caret {
  font-size: 9px;
  transition: transform var(--transition-fast);
}
.sc-caret.open {
  transform: rotate(90deg);
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

/* 出处抽屉加载态 */
.src-loading {
  color: var(--text-tertiary);
  font-size: 13px;
  padding: 24px 0;
  text-align: center;
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

<style>
/* compact 会话弹层（a-popover 传送层，非 scoped） */
.bw-sess-pop .bw-sess-list { width: 280px; max-height: 340px; overflow-y: auto; }
.bw-sess-pop .bw-sess-new {
  display: flex; align-items: center; justify-content: center; gap: 6px;
  width: 100%; padding: 6px 0; margin-bottom: 4px;
  border: 1px dashed var(--color-primary-border); border-radius: var(--radius-sm);
  background: transparent; color: var(--color-primary); font-size: 12px; cursor: pointer;
}
.bw-sess-pop .bw-sess-new:hover:not(:disabled) { background: var(--color-primary-bg); }
.bw-sess-pop .bw-sess-new:disabled { opacity: 0.5; cursor: default; }
.bw-sess-pop .bw-sess-item { position: relative; display: flex; flex-direction: column; gap: 1px; padding: 6px 10px; border-radius: var(--radius-xs); cursor: pointer; }
.bw-sess-pop .bw-sess-item:hover { background: var(--bg-surface); }
.bw-sess-pop .bw-sess-item.active { background: var(--color-primary-bg); }
.bw-sess-pop .bs-title { font-size: 12px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding-right: 18px; }
.bw-sess-pop .bw-sess-item.active .bs-title { color: var(--color-primary); font-weight: 600; }
.bw-sess-pop .bs-time { font-size: 10px; color: var(--text-tertiary); font-family: var(--font-mono, monospace); }
.bw-sess-pop .bs-del { position: absolute; top: 50%; right: 5px; transform: translateY(-50%); border: none; background: transparent; color: var(--text-tertiary); font-size: 10.5px; padding: 3px; border-radius: 4px; cursor: pointer; opacity: 0; }
.bw-sess-pop .bw-sess-item:hover .bs-del { opacity: 1; }
.bw-sess-pop .bs-del:hover { color: var(--color-danger, #e5484d); }
.bw-sess-pop .bw-sess-empty { padding: 14px; text-align: center; font-size: 11.5px; color: var(--text-tertiary); }
</style>
