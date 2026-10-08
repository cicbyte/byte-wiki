/**
 * 活跃编辑器单例：跟踪当前聚焦的 SnippetEditor 实例。
 *
 * 模板展开管线据此决定把渲染后的代码插入到哪个编辑器；
 * 无活跃编辑器（焦点不在 Monaco）时降级为复制到剪贴板。
 *
 * 多个 SnippetEditor 同屏时，谁聚焦谁就是 active；失焦/卸载时清除。
 */

export interface ActiveEditorApi {
  /** 在当前光标处插入文本（并聚焦编辑器） */
  insertText: (text: string) => void;
  /** 聚焦编辑器 */
  focus: () => void;
}

let active: ActiveEditorApi | null = null;

/** 编辑器聚焦时登记自己为活跃编辑器；传 null 清除 */
export function setActiveEditor(api: ActiveEditorApi | null) {
  active = api;
}

/** 取当前活跃编辑器（无则 null） */
export function getActiveEditor(): ActiveEditorApi | null {
  return active;
}
