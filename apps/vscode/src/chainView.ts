/**
 * 走读链 webview —— `bs callchain-query --chain` 数据（服务端 DFS：先序、
 * 主路径贪心最深）。大纲列表点击块 → 编辑器定位函数区间；调用链接点击 →
 * 跳到链内对应块。渲染器在 media/chain.js。
 */
import * as vscode from "vscode";
import { callchainChain, type Chain } from "./callchainClient";
import { workspaceRoot } from "./wikiTree";
import { openSourceAt } from "./sourceLocator";

export async function openChainView(context: vscode.ExtensionContext, rootId: string, title: string): Promise<void> {
  const ws = workspaceRoot();
  if (!ws) return;
  let data: Chain;
  try {
    data = await callchainChain(ws, rootId, 120);
  } catch (e) {
    void vscode.window.showErrorMessage(`走读链查询失败：${e instanceof Error ? e.message : e}`);
    return;
  }
  const panel = vscode.window.createWebviewPanel(
    "byteWiki.chain",
    `走读链：${title}`,
    vscode.ViewColumn.Beside,
    {
      enableScripts: true,
      localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, "media")],
    },
  );
  const res = (f: string) => panel.webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", f));
  const js = res("chain.js");
  const css = res("preview.css");
  panel.webview.html = `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src ${js}; style-src ${css} 'unsafe-inline';">
<link rel="stylesheet" href="${css}">
</head>
<body class="chain-body">
<div id="app"></div>
<script src="${js}"></script>
</body>
</html>`;
  const byId = new Map(data.entries.map((e) => [e.id, e]));
  panel.webview.onDidReceiveMessage((m: { type: string; file?: string; start?: number; end?: number; id?: string }) => {
    if (m.type === "ready") {
      panel.webview.postMessage({ type: "chain", chain: data });
    } else if (m.type === "openFunction" && m.file) {
      void openSourceAt(ws, { file: m.file, start: m.start, end: m.end });
    } else if (m.type === "focusNode" && m.id) {
      const hit = byId.get(m.id);
      if (hit) void openSourceAt(ws, { file: hit.node.file, start: hit.node.lineno, end: hit.node.end_lineno });
    }
  });
}
