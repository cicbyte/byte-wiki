<script setup lang="ts">
/**
 * 仓库问答面板（FR-05 / FR-11）：RAG 检索注入 + 流式输出 + 出处跳转
 *
 * 按现有样式规范自研（气泡列表 + 输入框），嵌入浏览布局右侧。
 */
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { message } from "ant-design-vue";
import { SendOutlined, UserOutlined, RobotOutlined } from "@ant-design/icons-vue";
import { api } from "@/api";

interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  sources?: string[];
}

const props = defineProps<{
  repoPath: string;
}>();

const emit = defineEmits<{ (e: "open-doc", docPath: string): void }>();

const messages = ref<ChatMessage[]>([]);
const input = ref("");
const asking = ref(false);
const listRef = ref<HTMLElement | null>(null);
/** 本轮回答的流式缓冲（避免每个 delta 触发整列表重渲） */
let streamingIdx = -1;

let unlisten: Awaited<ReturnType<typeof listenDelta>> | null = null;
type UnlistenFn = () => void;
async function listenDelta(): Promise<UnlistenFn> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<string>("wiki://chat-delta", (ev) => {
    if (streamingIdx >= 0 && messages.value[streamingIdx]) {
      messages.value[streamingIdx].content += ev.payload;
      scrollToBottom();
    }
  });
}

onMounted(async () => {
  unlisten = await listenDelta();
});
onUnmounted(() => {
  unlisten?.();
});

function scrollToBottom() {
  nextTick(() => {
    listRef.value?.scrollTo({ top: listRef.value.scrollHeight });
  });
}

async function ask() {
  const q = input.value.trim();
  if (!q || asking.value) return;
  input.value = "";
  asking.value = true;

  // 历史（最近 8 轮）供多轮上下文
  const history: [string, string][] = messages.value
    .slice(-8)
    .map((m) => [m.role, m.content] as [string, string]);

  messages.value.push({ role: "user", content: q });
  messages.value.push({ role: "assistant", content: "" });
  streamingIdx = messages.value.length - 1;
  scrollToBottom();

  try {
    const answer = await api.repoChat(props.repoPath, q, history);
    // 用最终结果覆盖流式缓冲（保证与 sources 一致）
    messages.value[streamingIdx].content = answer.text;
    messages.value[streamingIdx].sources = answer.sources;
  } catch (e: any) {
    messages.value[streamingIdx].content =
      "⚠️ " + (typeof e === "string" ? e : "问答失败");
  } finally {
    streamingIdx = -1;
    asking.value = false;
    scrollToBottom();
  }
}

/** 出处点击：wiki md → 打开对应文档；代码文件 → 提示路径 */
function onSource(docPath: string) {
  const rel = docPath.replace(/^\.bytewiki\//, "");
  if (docPath.startsWith(".bytewiki/")) {
    emit("open-doc", rel);
  } else {
    message.info(`代码出处：${docPath}`);
  }
}
</script>

<template>
  <div class="chat-panel">
    <div class="chat-head">
      <RobotOutlined />
      <span>仓库问答</span>
      <span class="chat-hint">基于已生成的 Wiki 与代码索引</span>
    </div>

    <div ref="listRef" class="chat-list">
      <div v-if="!messages.length" class="chat-empty">
        问点什么吧，例如：<br />
        「这个仓库的整体架构是什么？」<br />
        「X 模块负责什么？」
      </div>
      <div
        v-for="(m, i) in messages"
        :key="i"
        class="bubble-row"
        :class="m.role"
      >
        <span class="bubble-avatar">
          <UserOutlined v-if="m.role === 'user'" />
          <RobotOutlined v-else />
        </span>
        <div class="bubble">
          <div class="bubble-text">{{ m.content || (asking && i === messages.length - 1 ? "…" : "") }}</div>
          <div v-if="m.sources?.length" class="bubble-sources">
            <span class="src-label">出处</span>
            <button
              v-for="s in m.sources"
              :key="s"
              class="src-chip"
              :title="s"
              @click="onSource(s)"
            >
              {{ s.replace(/^\.bytewiki\//, "") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="chat-input-row">
      <a-input
        v-model:value="input"
        placeholder="输入问题，回车发送"
        :disabled="asking"
        @keydown.enter="ask"
      />
      <button class="send-btn" :disabled="asking || !input.trim()" @click="ask">
        <SendOutlined />
      </button>
    </div>
  </div>
</template>

<style scoped>
.chat-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.chat-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border-light);
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  flex-shrink: 0;
}
.chat-hint {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
}
.chat-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--spacing-md);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}
.chat-empty {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 2;
  text-align: center;
  margin: auto;
}
.bubble-row {
  display: flex;
  gap: 8px;
}
.bubble-row.user {
  flex-direction: row-reverse;
}
.bubble-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: var(--bg-input);
  color: var(--text-secondary);
  font-size: 13px;
  flex-shrink: 0;
}
.bubble {
  max-width: 82%;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  padding: 8px 12px;
  font-size: 13px;
  line-height: 1.7;
  color: var(--text-primary);
  overflow-wrap: break-word;
}
.bubble-row.user .bubble {
  background: var(--color-primary-bg);
  border-color: transparent;
}
.bubble-text {
  white-space: pre-wrap;
}
.bubble-sources {
  margin-top: 8px;
  padding-top: 6px;
  border-top: 1px dashed var(--border-light);
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
}
.src-label {
  font-size: 10px;
  color: var(--text-tertiary);
}
.src-chip {
  font-size: 11px;
  font-family: "JetBrains Mono", monospace;
  color: var(--color-primary);
  background: var(--bg-input);
  border: none;
  border-radius: var(--radius-xs);
  padding: 1px 8px;
  cursor: pointer;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.src-chip:hover {
  background: var(--color-primary-bg);
}
.chat-input-row {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  border-top: 1px solid var(--border-light);
  flex-shrink: 0;
}
.send-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 32px;
  border: 1px solid var(--color-primary);
  background: var(--color-primary);
  color: #fff;
  border-radius: var(--radius-sm);
  cursor: pointer;
  flex-shrink: 0;
}
.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
