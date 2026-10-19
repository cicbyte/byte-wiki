/**
 * mermaid 与 md-editor-v3 的接线（共享单例）
 *
 * - 本地打包实例替代 md-editor 默认的 CDN 动态加载（桌面端离线可用）；
 * - 关闭内置的就地图表缩放（窄文档栏裁切 + 劫持滚轮），放大查看由
 *   WikiViewer 的全屏 DiagramLightbox 承载；
 * - 关键补丁：md-editor 在「同步 instance」模式下 onMounted 早退，
 *   mermaid.initialize 永远不会被调用（CDN 路径靠 script onload 触发），
 *   首屏会以出厂默认淡紫主题渲染。这里按 md-editor 内置的同款主题配置
 *   预初始化，渲染 mermaid 的组件需随应用主题调用 applyMermaidTheme。
 */
import { config as mdConfig } from "md-editor-v3";
import mermaid from "mermaid";

let wired = false;

/** 接线本地 mermaid 实例（幂等，任意组件 import 后调用一次即可） */
export function setupMermaid() {
  if (wired) return;
  wired = true;
  mdConfig({
    editorExtensions: {
      mermaid: { instance: mermaid, enableZoom: false },
    },
  });
}

/** 按应用主题初始化 mermaid（配置与 md-editor-v3 内置值保持一致） */
export function applyMermaidTheme(dark: boolean) {
  mermaid.initialize(
    dark
      ? { startOnLoad: false, theme: "dark" }
      : {
          startOnLoad: false,
          theme: "base",
          themeVariables: {
            background: "#ffffff",
            primaryColor: "#ffffff",
            primaryTextColor: "#1f2329",
            primaryBorderColor: "#b7c0cc",
            secondaryColor: "#f7f8fa",
            tertiaryColor: "#f7f8fa",
            lineColor: "#596273",
            edgeLabelBackground: "#ffffff",
            clusterBkg: "#ffffff",
            clusterBorder: "#b7c0cc",
          },
        },
  );
}
