/**
 * 捕获 / 导入命令。
 *   ② captureSelection：编辑器选中代码 → 存片段/模板
 *   ③ captureFile：文件（explorer 右键 / 当前文件）→ 存片段/模板
 *   ④ importFolder：文件夹（explorer 右键 / 当前工作区）→ 导入为项目（bs add，带进度）
 */
import * as vscode from "vscode";
import { captureForm } from "./captureForm";
import { createSnippet, createTemplate, importProject, clearCache } from "./client";
import { showBsError } from "./errors";

/** ② 选中代码 → 存为片段/模板 */
export async function captureSelection(kind: "snippet" | "template"): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.selection.isEmpty) {
    vscode.window.showWarningMessage("请先选中一段代码");
    return;
  }
  const text = editor.document.getText(editor.selection);
  const meta = await captureForm({
    kind,
    defaultLanguage: editor.document.languageId,
    defaultTitle: baseName(editor.document.fileName),
  });
  if (!meta) return;
  try {
    if (kind === "snippet") {
      await createSnippet({
        title: meta.title, code: text, language: meta.language,
        tags: meta.tags, category: meta.category,
      });
    } else {
      await createTemplate({
        title: meta.title, body: text, language: meta.language, trigger: meta.trigger,
        tags: meta.tags, category: meta.category,
      });
      clearCache(); // 新模板立即可被补全/QuickPick 看到
    }
    vscode.window.showInformationMessage(`已存为${kind === "snippet" ? "片段" : "模板"}：${meta.title}`);
  } catch (e) {
    showBsError(e);
  }
}

/** ③ 文件 → 存为片段/模板（uri 来自 explorer 右键，缺省用当前文件） */
export async function captureFile(
  uri: vscode.Uri | undefined,
  kind: "snippet" | "template",
): Promise<void> {
  const target = uri ?? vscode.window.activeTextEditor?.document.uri;
  if (!target) {
    vscode.window.showWarningMessage("没有可用的文件");
    return;
  }
  let content: string;
  try {
    const buf = await vscode.workspace.fs.readFile(target);
    content = Buffer.from(buf).toString("utf8");
  } catch (e) {
    vscode.window.showErrorMessage(`读取文件失败：${(e as Error).message}`);
    return;
  }
  // 兜底：二进制文件（含 NUL 字节）不支持存为片段/模板
  if (content.includes("\0")) {
    vscode.window.showWarningMessage("Byte-Wiki：二进制文件不支持存为片段/模板");
    return;
  }
  const meta = await captureForm({
    kind,
    defaultLanguage: inferLang(target),
    defaultTitle: baseName(target.fsPath),
  });
  if (!meta) return;
  try {
    if (kind === "snippet") {
      await createSnippet({
        title: meta.title, code: content, language: meta.language,
        tags: meta.tags, category: meta.category,
      });
    } else {
      await createTemplate({
        title: meta.title, body: content, language: meta.language, trigger: meta.trigger,
        tags: meta.tags, category: meta.category,
      });
      clearCache();
    }
    vscode.window.showInformationMessage(`已存为${kind === "snippet" ? "片段" : "模板"}：${meta.title}`);
  } catch (e) {
    showBsError(e);
  }
}

/** ④ 文件夹/工作区 → 导入为项目（uri 来自 explorer 右键，缺省用工作区根） */
export async function importFolder(uri: vscode.Uri | undefined): Promise<void> {
  const target = uri ?? vscode.workspace.workspaceFolders?.[0]?.uri;
  if (!target) {
    vscode.window.showWarningMessage("没有可导入的文件夹");
    return;
  }
  const path = target.fsPath;
  const title = await vscode.window.showInputBox({
    prompt: "项目标题",
    value: baseName(path),
    ignoreFocusOut: true,
  });
  if (title === undefined) return; // 取消
  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "导入项目到 byte-wiki…",
      cancellable: false,
    },
    async () => {
      try {
        await importProject({ path, title: title.trim() || undefined });
        vscode.window.showInformationMessage(`已导入项目：${title || path}`);
      } catch (e) {
        showBsError(e);
      }
    },
  );
}

// ============ helpers ============

function baseName(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}

function inferLang(uri: vscode.Uri): string {
  const ext = uri.fsPath.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    rs: "rust", ts: "typescript", tsx: "typescriptreact",
    js: "javascript", jsx: "javascriptreact", py: "python", go: "go",
    java: "java", c: "c", cpp: "cpp", md: "markdown", json: "json",
  };
  return map[ext] ?? ext;
}
