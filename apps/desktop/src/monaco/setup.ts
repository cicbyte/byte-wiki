/**
 * Monaco Editor 初始化（必须在任何 editor.create 之前 import 一次）
 *
 * - 配置 MonacoEnvironment.getWorker
 * - 动态主题：applyMonacoTheme(bgColor, textColor, isDark) 直接接收色值
 */
import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case "json":
        return new jsonWorker();
      case "css":
      case "scss":
      case "less":
        return new cssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new htmlWorker();
      case "typescript":
      case "javascript":
        return new tsWorker();
      default:
        return new editorWorker();
    }
  },
};

/**
 * 动态定义并应用 Monaco 主题。
 * 直接接收色值字符串，不再依赖灰阶映射。
 */
export function applyMonacoTheme(bgColor: string, _textColor: string, isDark: boolean) {
  if (isDark) {
    monaco.editor.defineTheme("byte-wiki-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": bgColor,
        "editor.lineHighlightBackground": bgColor,
        "editor.lineHighlightBorder": "#00000000",
        "editorGutter.background": bgColor,
        "editorCursor.foreground": "#3B82F6",
      },
    });
    monaco.editor.setTheme("byte-wiki-dark");
  } else {
    monaco.editor.defineTheme("byte-wiki-light", {
      base: "vs",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": bgColor,
        "editor.lineHighlightBackground": bgColor,
        "editor.lineHighlightBorder": "#00000000",
        "editorGutter.background": bgColor,
        "editorCursor.foreground": "#3B82F6",
      },
    });
    monaco.editor.setTheme("byte-wiki-light");
  }
}

/** 初始化默认主题 */
applyMonacoTheme("#09090B", "#F4F4F5", true);

export { monaco };
