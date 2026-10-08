/**
 * 窄屏检测（< 768px）
 *
 * 用 matchMedia 监听，比 resize 事件高效，且与 theme.ts 既有用法一致。
 * 模块级单例 isNarrow + 引用计数：多个视图组件共享同一个响应式状态，
 * 避免重复挂载监听器。最后一个组件卸载时才移除监听。
 *
 * 用于片段/笔记/项目页的窄屏单栏切换：列表 ↔ 详情。
 */
import { ref, onMounted, onUnmounted } from "vue";

/**
 * 窄屏检测（窗口内容宽 < 1100px 触发单栏模式）
 *
 * 阈值定 1100 而非常见的 768：Tauri 窗口 minWidth=900，物理上到不了 768；
 * 1100 以下三栏/四栏布局已经拥挤，切单栏体验更好。1200 以上才保持多栏。
 *
 * 同时挂两条监听保险：
 * 1. window resize 事件（标准 Web API）
 * 2. Tauri onResized（v2 的窗口 resize 在 WebView2 上可能不触发 resize 事件，兜底）
 */
const NARROW_BREAKPOINT = 1100;
const isNarrow = ref(typeof window !== "undefined" ? window.innerWidth < NARROW_BREAKPOINT : false);

let resizeTimer: ReturnType<typeof setTimeout> | null = null;
function refresh() {
  const w = typeof window !== "undefined" ? window.innerWidth : 0;
  isNarrow.value = w < NARROW_BREAKPOINT;
}

function onResize() {
  // 防抖：拖动窗口时高频触发，避免布局抖动
  if (resizeTimer) clearTimeout(resizeTimer);
  resizeTimer = setTimeout(refresh, 80);
}

let refCount = 0;
let unlistenTauriResize: (() => void) | null = null;

export function useNarrowScreen() {
  onMounted(async () => {
    if (refCount === 0) {
      window.addEventListener("resize", onResize);
      // Tauri v2：原生窗口 resize 事件（WebView resize 不触发时兜底）
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const appWindow = getCurrentWindow();
        const unlisten = await appWindow.onResized(() => onResize());
        unlistenTauriResize = unlisten;
      } catch {
        // 非 Tauri 环境（如纯浏览器调试）忽略
      }
      refresh();
    }
    refCount++;
  });
  onUnmounted(() => {
    refCount--;
    if (refCount === 0) {
      window.removeEventListener("resize", onResize);
      if (unlistenTauriResize) {
        unlistenTauriResize();
        unlistenTauriResize = null;
      }
      if (resizeTimer) clearTimeout(resizeTimer);
    }
  });
  return { isNarrow };
}


