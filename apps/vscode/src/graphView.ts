/**
 * 调用链图 webview —— vis-network（桌面端同款，本地 vendor 离线）。
 *
 * 两种形态：
 * - 模块全景：overview 的 module_edges 聚合边（模块级全景图）
 * - 节点子图：--focus <seed> 的 N 跳双向邻域（函数级）
 *
 * 交互（渲染器 media/graph.js）：节点单击 → 跳源码；双击 → 以该节点为
 * seed 重新取子图（宿主重查 --focus 后推数据）。
 */
import * as vscode from "vscode";
import { runBs } from "./cliClient";
import { callchainOverview, type Overview } from "./callchainClient";
import { workspaceRoot } from "./wikiTree";
import { openSourceAt } from "./sourceLocator";

export type GraphMode = "modules" | "focus";

interface FocusNode {
  id: string;
  qualname: string;
  name: string;
  file: string;
  lineno?: number;
  is_entry?: boolean;
  is_exported?: boolean;
}

interface FocusResult {
  nodes: FocusNode[];
  edges: { src: string; dst: string; line: number; kind: string }[];
  truncated: boolean;
  seed: string;
}

let panel: vscode.WebviewPanel | null = null;
/** 每次数据就绪后更新（ready 消息触发的是最新一次的推送，避免闭包旧数据） */
let currentPush: (() => void) | null = null;
/** 最新概览（模块全景点击模块 → 解析该模块下的入口节点做子图中心） */
let latestOv: Overview | null = null;

async function fetchFocus(wsPath: string, seed: string): Promise<FocusResult> {
  const out = await runBs(["--json", "callchain-query", wsPath, "--focus", seed]);
  return JSON.parse(out.trim()) as FocusResult;
}

function html(context: vscode.ExtensionContext, webview: vscode.Webview): string {
  const vendor = (f: string) => webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", "vendor", f));
  const res = (f: string) => webview.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, "media", f));
  const visJs = vendor("vis-network.min.js");
  const visCss = vendor("vis-network.min.css");
  const graphJs = res("graph.js");
  const css = res("preview.css");
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource} data:; script-src ${visJs} ${graphJs}; style-src ${visCss} ${css} 'unsafe-inline';">
<link rel="stylesheet" href="${visCss}">
<link rel="stylesheet" href="${css}">
</head>
<body class="graph-body">
<div id="mode"></div>
<div id="net"></div>
<script src="${visJs}"></script>
<script src="${graphJs}"></script>
</body>
</html>`;
}

/** 打开/复用图面板。mode=modules 画全景；mode=focus 以 seed 取子图。 */
export async function openGraphView(
  context: vscode.ExtensionContext,
  mode: GraphMode,
  seed?: string,
): Promise<void> {
  const ws = workspaceRoot();
  if (!ws) return;
  let ov: Overview;
  let focus: FocusResult | null = null;
  try {
    ov = await callchainOverview(ws);
    if (mode === "focus" && seed) focus = await fetchFocus(ws, seed);
  } catch (e) {
    void vscode.window.showErrorMessage(`调用链数据获取失败：${e instanceof Error ? e.message : e}`);
    return;
  }
  if (!panel) {
    panel = vscode.window.createWebviewPanel(
      "byteWiki.graph",
      "调用链图",
      vscode.ViewColumn.One,
      { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, "media")] },
    );
    panel.webview.html = html(context, panel.webview);
    panel.webview.onDidReceiveMessage((m: { type: string; id?: string; file?: string; start?: number }) => {
      if (m.type === "ready") currentPush?.();
      else if (m.type === "openNode" && m.id && m.file) {
        void openSourceAt(ws, { file: m.file, start: m.start });
      } else if (m.type === "refocus" && m.id) {
        void refocus(context, ws, m.id);
      }
    });
    panel.onDidDispose(() => {
      panel = null;
      currentPush = null;
    });
  } else {
    panel.reveal(vscode.ViewColumn.One);
  }
  panel.title = mode === "modules" ? "调用链图 · 模块全景" : `调用链图 · ${seed?.split("::").pop() ?? ""}`;
  const data = {
    type: "graph" as const,
    dark: vscode.window.activeColorTheme.kind === vscode.ColorThemeKind.Dark,
    mode,
    seed,
    truncated: focus?.truncated ?? false,
    nodes: focus?.nodes ?? [],
    edges: focus?.edges ?? [],
    modules: ov.modules,
    moduleEdges: ov.module_edges ?? [],
  };
  currentPush = () => panel?.webview.postMessage(data);
  latestOv = ov;
  currentPush();
}

/** refocus：节点 id 直接用；模块 id（无 ::）解析为该模块下的第一个入口节点 */
async function refocus(context: vscode.ExtensionContext, ws: string, id: string): Promise<void> {
  if (id.includes("::")) {
    await openGraphView(context, "focus", id);
    return;
  }
  const hit = latestOv?.entries.find((e) => e.id.startsWith(id + "::"));
  if (hit) {
    await openGraphView(context, "focus", hit.id);
  } else {
    void vscode.window.showInformationMessage(`模块「${id}」在入口清单里没有可聚焦的函数`);
  }
}
