<script setup lang="ts">
/**
 * 流式 Markdown 预览（markdown-it 本地渲染，120ms 节流）。
 *
 * 不用 md-editor-v3 的 MdPreview：它按需从 CDN（unpkg）拉高亮/Katex 资源，
 * 被 CSP 拦截后内部 MutationObserver 会对空节点 observe 抛错（每实例一次），
 * 且云供应商高速流下每 token 全量重解析开销大。本地 markdown-it 无网络
 * 依赖、完全可控；html:false 使 AI 输出的 HTML 标签字面显示不执行。
 */
import { computed, onBeforeUnmount, ref, watch } from "vue";
import MarkdownIt from "markdown-it";

const props = defineProps<{
  source: string;
  streaming?: boolean;
  theme?: "light" | "dark";
}>();

const md = new MarkdownIt({ html: false, breaks: true, linkify: true });
// 链接交系统浏览器打开（避免 webview 内导航离开应用）
const defaultLinkOpen =
  md.renderer.rules.link_open ?? ((t, i, o, e, s) => s.renderToken(t, i, o));
md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
  tokens[idx].attrSet("target", "_blank");
  return defaultLinkOpen(tokens, idx, options, env, self);
};

const shown = ref(props.source);
let timer: ReturnType<typeof setTimeout> | null = null;

watch(
  () => props.source,
  (v) => {
    if (!props.streaming) {
      shown.value = v;
      return;
    }
    if (timer) return;
    timer = setTimeout(() => {
      timer = null;
      shown.value = props.source;
    }, 120);
  },
);

watch(
  () => props.streaming,
  (s) => {
    if (!s) {
      if (timer) clearTimeout(timer);
      timer = null;
      shown.value = props.source;
    }
  },
);

onBeforeUnmount(() => {
  if (timer) clearTimeout(timer);
});

const html = computed(() => md.render(shown.value || ""));
</script>

<template>
  <div class="md-lite" :class="{ dark: theme === 'dark' }" v-html="html"></div>
</template>

<style scoped>
.md-lite {
  font-size: 13px;
  line-height: 1.7;
  color: var(--text-primary);
  word-break: break-word;
}
.md-lite :deep(:first-child) {
  margin-top: 0;
}
.md-lite :deep(:last-child) {
  margin-bottom: 0;
}
.md-lite :deep(p) {
  margin-block: 6px;
}
.md-lite :deep(h1),
.md-lite :deep(h2),
.md-lite :deep(h3),
.md-lite :deep(h4) {
  margin: 10px 0 6px;
  font-weight: 650;
  line-height: 1.4;
}
.md-lite :deep(h1) {
  font-size: 17px;
}
.md-lite :deep(h2) {
  font-size: 15.5px;
}
.md-lite :deep(h3) {
  font-size: 14px;
}
.md-lite :deep(h4) {
  font-size: 13.5px;
}
.md-lite :deep(ul),
.md-lite :deep(ol) {
  margin: 6px 0;
  padding-left: 20px;
}
.md-lite :deep(li) {
  margin: 2px 0;
}
.md-lite :deep(code) {
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: 12px;
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--bg-input, rgba(127, 127, 127, 0.14));
}
.md-lite :deep(pre) {
  margin: 8px 0;
  padding: 10px 12px;
  border-radius: 8px;
  overflow-x: auto;
  background: var(--bg-input, rgba(127, 127, 127, 0.12));
  border: 1px solid var(--border-light);
}
.md-lite :deep(pre code) {
  padding: 0;
  background: none;
  font-size: 12px;
  line-height: 1.6;
}
.md-lite :deep(blockquote) {
  margin: 6px 0;
  padding: 2px 10px;
  border-left: 3px solid var(--border-light);
  color: var(--text-secondary);
}
.md-lite :deep(a) {
  color: var(--accent, #1677ff);
  text-decoration: none;
}
.md-lite :deep(a:hover) {
  text-decoration: underline;
}
.md-lite :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 12.5px;
}
.md-lite :deep(th),
.md-lite :deep(td) {
  border: 1px solid var(--border-light);
  padding: 4px 8px;
}
.md-lite :deep(th) {
  background: var(--bg-input, rgba(127, 127, 127, 0.1));
}
.md-lite :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-light);
  margin: 10px 0;
}
.md-lite :deep(img) {
  max-width: 100%;
  border-radius: 6px;
}
</style>
