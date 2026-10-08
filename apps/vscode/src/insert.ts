/**
 * 展开并插入 —— 移植自 apps/desktop/src/composables/useTemplateExpand.ts 的 expandById。
 *
 * 管线：
 *   snippet / 无变量 template → 直接插 body
 *   有变量 template → extractVariables → collectVariables（InputBox 填值）→ expandBody（Tera）→ 插
 *
 * 插入：editor.edit replace 当前选区（纯文本，不解析 $1 等 TextMate 语法 —— 避免模板正文里的
 * shell 变量等被误当 tabstop）。无活跃编辑器则复制剪贴板（对齐桌面端 deliver 兜底）。
 */
import * as vscode from "vscode";
import type { StashItem } from "./types";
import { extractVariables } from "./variables";
import { collectVariables } from "./varPicker";
import { expandBody } from "./client";
import { showBsError } from "./errors";

/** 把 code 插入活跃编辑器（替换当前选区；光标移到插入文本末尾）。返回是否成功插入。 */
async function insertCode(code: string): Promise<boolean> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    await vscode.env.clipboard.writeText(code);
    vscode.window.showInformationMessage("已复制到剪贴板（无活跃编辑器）");
    return true;
  }
  const start = editor.selection.start;
  const ok = await editor.edit((b) => b.replace(editor.selection, code));
  if (!ok) return false;
  // 光标移到插入文本末尾
  const lines = code.split(/\r\n|\r|\n/);
  const lineDelta = lines.length - 1;
  const lastLen = lines[lines.length - 1].length;
  const end = start.translate(lineDelta, lastLen);
  editor.selection = new vscode.Selection(end, end);
  return true;
}

/** 展开并插入一个 stash 项（片段或模板）。补全项选中、QuickPick 选中都收敛到此。 */
export async function expandAndInsert(item: StashItem): Promise<void> {
  try {
    if (item.type === "snippet") {
      await insertCode(item.body);
      return;
    }
    // template
    const vars = extractVariables(item.body);
    if (vars.length === 0) {
      await insertCode(item.body);
      return;
    }
    const filled = await collectVariables(vars, item.title);
    if (!filled) return; // 用户取消
    const code = await expandBody(item.body, filled);
    await insertCode(code);
  } catch (e) {
    showBsError(e);
  }
}
