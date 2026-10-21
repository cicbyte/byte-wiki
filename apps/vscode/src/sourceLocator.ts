/**
 * 源码定位（共享）：打开文件 + 选中行区间 + 临时装饰高亮。
 * 编辑器原生的 reveal/selection——没有 webview 里那些布局时序坑。
 */
import * as path from "path";
import * as vscode from "vscode";

let decor: vscode.TextEditorDecorationType | null = null;

export interface SourceTarget {
  file: string;
  start?: number;
  end?: number;
}

export async function openSourceAt(wsPath: string, t: SourceTarget): Promise<void> {
  // 调用链/引用里的 file 一律是仓库相对路径（/ 分隔）
  const abs = path.join(wsPath, ...t.file.split("/"));
  let doc: vscode.TextDocument;
  try {
    doc = await vscode.workspace.openTextDocument(vscode.Uri.file(abs));
  } catch {
    void vscode.window.showWarningMessage(`文件不存在：${t.file}`);
    return;
  }
  const start = Math.min(Math.max(1, t.start ?? 1), doc.lineCount);
  const end = Math.min(Math.max(start, t.end ?? start), doc.lineCount);
  const range = new vscode.Range(start - 1, 0, end - 1, 0);
  const editor = await vscode.window.showTextDocument(doc, {
    selection: range,
    preview: true,
  });
  decor?.dispose();
  decor = vscode.window.createTextEditorDecorationType({
    backgroundColor: new vscode.ThemeColor("editor.rangeHighlightBackground"),
    border: "1px solid",
    borderColor: new vscode.ThemeColor("editor.findMatchBorder"),
  });
  editor.setDecorations(decor, [new vscode.Range(start - 1, 0, end - 1, 2 ** 31 - 1)]);
}
