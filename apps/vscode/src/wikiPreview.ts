/**
 * Wiki 页面预览 webview —— markdown + mermaid（本地 vendor，离线可用）。
 *
 * 内容由宿主 postMessage 推送（不内联注入，CSP 无需 unsafe-inline）；
 * `bw-file://` 引用链接与相对 .md 链接拦截后回传宿主处理。
 * 渲染器在 media/render.js（webview 内运行，不进 esbuild bundle）。
 */
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { branchWikiRoot, workspaceRoot } from "./wikiTree";
import { openSourceAt } from "./sourceLocator";

let panel: vscode.WebviewPanel | null = null;
let currentDoc: string | null = null;
let themeListener: vscode.Disposable | null = null;

/** 知识卡 frontmatter：直接渲染会把 --- 变分割线且标题重复 */
function stripFrontmatter(raw: string): string {
  return raw.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "").replace(/^\s+/, "");
}

function readDoc(docRel: string): string {
  const ws = workspaceRoot();
  if (!ws) throw new Error("未打开工作区");
  const abs = path.join(branchWikiRoot(ws), ...docRel.split("/"));
  const raw = fs.readFileSync(abs, "utf8");
  return /\.mmd$/i.test(docRel) ? "```mermaid\n" + raw + "\n```" : stripFrontmatter(raw);
}

function html(context: vscode.ExtensionContext, webview: vscode.Webview): string {
  const vendor = (f: string) => webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", "vendor", f));
  const res = (f: string) => webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", f));
  const mdJs = vendor("markdown-it.umd.min.js");
  const mermaidJs = vendor("mermaid.min.js");
  const renderJs = res("render.js");
  const css = res("preview.css");
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource} data:; script-src ${mdJs} ${mermaidJs} ${renderJs}; style-src ${css} 'unsafe-inline'; font-src ${webview.cspSource} data:;">
<link rel="stylesheet" href="${css}">
</head>
<body>
<div id="app"></div>
<script src="${mdJs}"></script>
<script src="${mermaidJs}"></script>
<script src="${renderJs}"></script>
</body>
</html>`;
}

async function pushDoc(): Promise<void> {
  if (!panel || !currentDoc) return;
  try {
    panel.webview.postMessage({
      type: "doc",
      path: currentDoc,
      content: readDoc(currentDoc),
      dark: vscode.window.activeColorTheme.kind === vscode.ColorThemeKind.Dark,
    });
  } catch (e) {
    void vscode.window.showErrorMessage(`读取文档失败：${e instanceof Error ? e.message : e}`);
  }
}

export function openWikiPage(context: vscode.ExtensionContext, docRel: string): void {
  if (!panel) {
    panel = vscode.window.createWebviewPanel(
      "byteWiki.preview",
      "Wiki",
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, "media")],
      },
    );
    panel.webview.html = html(context, panel.webview);
    panel.webview.onDidReceiveMessage((m: { type: string; path?: string; href?: string; start?: number; end?: number }) => {
      if (m.type === "ready") void pushDoc();
      else if (m.type === "openFile" && m.path) {
        const ws = workspaceRoot();
        if (ws) void openSourceAt(ws, { file: m.path, start: m.start, end: m.end });
      } else if (m.type === "openDoc" && m.href) {
        openWikiPage(context, resolveDocHref(currentDoc, m.href));
      }
    });
    panel.onDidDispose(() => {
      panel = null;
      currentDoc = null;
      themeListener?.dispose();
      themeListener = null;
    });
    themeListener = vscode.window.onDidChangeActiveColorTheme(() => void pushDoc());
  } else {
    panel.reveal(vscode.ViewColumn.One);
  }
  currentDoc = docRel;
  panel.title = path.basename(docRel).replace(/\.(md|mmd)$/i, "");
  void pushDoc();
}

/** 相对 .md 链接按当前文档目录解析（与桌面端 WikiViewer 同语义） */
function resolveDocHref(baseDoc: string | null, href: string): string {
  const parts = (baseDoc ?? "").split("/").slice(0, -1);
  const file = href.split("#")[0];
  for (const p of file.split("/")) {
    if (p === "..") parts.pop();
    else if (p && p !== ".") parts.push(p);
  }
  return parts.join("/");
}
