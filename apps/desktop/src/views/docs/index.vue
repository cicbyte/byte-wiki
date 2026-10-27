<template>
  <div class="docs-view">
    <iframe
      v-if="ready"
      ref="frameEl"
      :src="frameSrc"
      class="docs-frame"
      title="byte-wiki 文档"
      @load="syncTheme"
    />
    <div v-else class="docs-empty">
      <EmptyState :text="hint" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from "vue";
import EmptyState from "@/components/common/EmptyState.vue";
import { useThemeStore } from "@/stores/theme";

// VitePress 构建产物部署在 public/docs/（vite build 拷到 dist/docs/），同源加载。
// base 配置为 /docs/，iframe 指向 /docs/ 即加载 index.html。
const ready = ref(false);
const frameSrc = ref("about:blank");
const hint = ref("文档加载中…");
const frameEl = ref<HTMLIFrameElement>();
const theme = useThemeStore();

/** 主题同步：文档站跟随应用明暗——同源 iframe 直接改 documentElement 的
 * dark 类，并写 VitePress 记忆键（'dark'/'light'，冷启动也保持一致） */
function syncTheme() {
  const frame = frameEl.value;
  const win = frame?.contentWindow;
  if (!frame || !win) return;
  try {
    frame.contentDocument?.documentElement.classList.toggle("dark", theme.isDark);
    win.localStorage.setItem("vitepress-theme-appearance", theme.isDark ? "dark" : "light");
  } catch {
    // 跨源兜底：不打断展示（同源部署理论上不走这里）
  }
}

// 应用切主题 → 文档跟着切（iframe 内 VitePress 自己的开关仍可用，下次同步会覆盖）
watch(() => theme.isDark, () => syncTheme());

onMounted(async () => {
  // 探测 /docs/index.html 是否为 VitePress 产物。dev 模式下产物缺失时
  // vite 会 SPA fallback 返回应用自身的 index.html（res.ok 恒 true），
  // 若只判状态码会把应用塞进 iframe 造成无限套娃——必须校验内容。
  try {
    const res = await fetch("/docs/index.html");
    const text = res.ok ? await res.text() : "";
    if (text.includes("VITEPRESS") || text.includes("vitepress")) {
      frameSrc.value = "/docs/index.html";
      ready.value = true;
    } else {
      hint.value = "文档未构建：请在 apps/desktop 下运行 npm run docs:build";
    }
  } catch {
    hint.value = "文档加载失败：请先运行 npm run docs:build";
  }
});
</script>

<style scoped>
.docs-view {
  height: 100%;
  width: 100%;
}
.docs-frame {
  width: 100%;
  height: 100%;
  border: none;
  /* 加载中/空白期的底色走主题（暗色下不刺眼），iframe 载入后即被覆盖 */
  background: var(--bg-card, #fff);
}
.docs-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}
</style>
