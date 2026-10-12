<script setup lang="ts">
/**
 * 问答面板（项目空间 → 问答 tab）——参考 byte-writer AiDock 重做。
 *
 * timeline 结构：user / assistant（含思考过程折叠块）。
 * 流式 Markdown 本地渲染（MdStreamPreview，120ms 节流）；思考过程独立通道
 * （wiki://chat-thinking，流式自动展开、结束折叠）；服务端真取消（chat_cancel，
 * token 边界中断 SSE）；出处芯片化可点跳文档。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
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
  RobotOutlined,
} from "@ant-design/icons-vue";
import { useThemeStore } from "@/stores/theme";
import MdStreamPreview from "@/components/editor/MdStreamPreview.vue";

const props = defineProps<{ repoPath: string }>();
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

  timeline.value.push({ kind: "user", text: q }, ai);
  asking.value = true;
  requestId.value = `chat-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  scrollToBottom();

  try {
    const answer = await invoke<{ text: string; sources: string[] }>("repo_chat", {
      path: props.repoPath,
      question: q,
      history,
      requestId: requestId.value,
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
          <div class="msg-role"><RobotOutlined class="msg-role-ico" />AI</div>
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
      <div class="composer-bar">
        <button class="btn sm" title="清空会话" :disabled="asking || !timeline.length" @click="clearAll">
          <ClearOutlined /> 清空
        </button>
        <span class="bar-spacer"></span>
        <button v-if="asking" class="btn sm danger" @click="stop"><StopOutlined /> 停止</button>
      </div>
      <div class="composer-row">
        <a-textarea
          v-model:value="input"
          :rows="2"
          :auto-size="{ minRows: 2, maxRows: 6 }"
          placeholder="问点什么… Enter 发送（Shift+Enter 换行）"
          :disabled="asking"
          @keydown="onComposerKey"
        />
        <button class="btn primary send-btn" :disabled="asking || !input.trim()" @click="ask">
          <SendOutlined />
        </button>
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
.dock-timeline {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 16px 18px;
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
  border-top: 1px solid var(--border-light);
  padding: 8px 12px 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex-shrink: 0;
}
.composer-bar {
  display: flex;
  align-items: center;
  gap: 6px;
}
.bar-spacer {
  flex: 1;
}
.composer-row {
  display: flex;
  gap: 8px;
  align-items: flex-end;
}
.composer-row :deep(.ant-input) {
  background: var(--bg-input);
}
.send-btn {
  height: 38px;
  width: 44px;
  justify-content: center;
  flex-shrink: 0;
}
</style>
