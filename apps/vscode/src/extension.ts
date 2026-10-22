/**
 * byte-wiki VSCode 插件入口。
 *
 * 注册：
 * - prefix 补全 provider（编辑器输入 trigger/title 即提示）
 * - 内部命令 byteWiki._expandItem（补全项选中后展开插入）
 * - 命令 byteWiki.insertAny / insertSnippet / insertTemplate（命令面板 + 右键菜单）
 * - 配置变更 → 清缓存（下次读取拉新）
 * - #581：活动栏 Byte Wiki 容器——Wiki 页面树（直读 .bytewiki）+ 调用链入口树
 *   （bs callchain-query），页面 webview 预览（markdown+mermaid，引用跳编辑器行）、
 *   走读链大纲、生成调用链、节点搜索
 */
import * as vscode from "vscode";
import { registerCompletionProvider } from "./completionProvider";
import { pickAndInsert } from "./quickPick";
import { expandAndInsert } from "./insert";
import { captureSelection, captureFile, importFolder } from "./capture";
import { clearCache, resetTransport } from "./client";
import type { StashItem } from "./types";
import { WikiPagesProvider, workspaceRoot } from "./wikiTree";
import { CallchainProvider } from "./callchainTree";
import { openWikiPage } from "./wikiPreview";
import { openChainView } from "./chainView";
import { openSourceAt } from "./sourceLocator";
import { callchainGenerate, callchainSearch, type OverviewEntry } from "./callchainClient";
import { openGraphView } from "./graphView";
import * as fs from "fs";
import * as path from "path";

export function activate(context: vscode.ExtensionContext): void {
  registerCompletionProvider(context);

  // ============ #581：Wiki / 调用链视图 ============
  const pagesProvider = new WikiPagesProvider();
  const callchainProvider = new CallchainProvider();
  const ws = () => workspaceRoot();

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("byteWiki.pages", pagesProvider),
    vscode.window.registerTreeDataProvider("byteWiki.callchain", callchainProvider),

    vscode.commands.registerCommand("byteWiki.openPage", (docRel: string) => {
      openWikiPage(context, docRel);
    }),
    vscode.commands.registerCommand("byteWiki.refreshPages", () => pagesProvider.refresh()),
    vscode.commands.registerCommand("byteWiki.refreshCallchain", () => callchainProvider.refresh()),

    // 调用链图：树条目 → 该节点 N 跳子图；裸调（视图标题栏/命令面板）→ 模块全景
    vscode.commands.registerCommand("byteWiki.showGraph", (e?: OverviewEntry) => {
      void openGraphView(context, e ? "focus" : "modules", e?.id);
    }),

    // 设置 bw CLI 路径：本仓库构建产物快捷项 / 浏览文件 / 打开设置项
    vscode.commands.registerCommand("byteWiki.setBsPath", async () => {
      const cfg = vscode.workspace.getConfiguration("byteWiki");
      const cur = cfg.get<string>("cliPath") ?? "bw";
      const picks: (vscode.QuickPickItem & { target?: string })[] = [];
      const w = ws();
      if (w) {
        const candidate = path.join(w, "target", "release", "bw.exe");
        if (fs.existsSync(candidate)) {
          picks.push({ label: "$(check) 使用本仓库构建产物", description: candidate, target: candidate });
        }
      }
      picks.push(
        { label: "$(file) 浏览文件…", target: "__browse" },
        { label: `$(settings-gear) 打开设置项 byteWiki.cliPath（当前：${cur}）`, target: "__settings" },
      );
      const pick = await vscode.window.showQuickPick(picks, { placeHolder: "设置 bw CLI 路径" });
      if (!pick) return;
      if (pick.target === "__settings") {
        void vscode.commands.executeCommand("workbench.action.openSettings", "byteWiki.cliPath");
        return;
      }
      let target = pick.target;
      if (target === "__browse") {
        const uris = await vscode.window.showOpenDialog({
          title: "选择 bw 可执行文件",
          openLabel: "选择",
          canSelectMany: false,
          filters:
            process.platform === "win32"
              ? { 可执行文件: ["exe"], 所有文件: ["*"] }
              : { 所有文件: ["*"] },
        });
        if (!uris?.length) return;
        target = uris[0].fsPath;
      }
      await cfg.update("cliPath", target, vscode.ConfigurationTarget.Global);
      void vscode.window.showInformationMessage(`bw CLI 路径已设置：${target}`);
      callchainProvider.refresh();
    }),

    vscode.commands.registerCommand("byteWiki.openFunction", (e?: OverviewEntry) => {
      const w = ws();
      if (!w || !e) return;
      void openSourceAt(w, { file: e.file, start: e.lineno, end: e.end_lineno });
    }),
    vscode.commands.registerCommand("byteWiki.openChain", (e?: OverviewEntry) => {
      const w = ws();
      if (!w) return;
      if (e) {
        void openChainView(context, e.id, e.qualname);
        return;
      }
      // 命令面板裸调用：从入口清单挑一个
      const all = callchainProvider.allEntries();
      if (!all.length) {
        void vscode.window.showInformationMessage("调用链数据尚未加载（先刷新或生成）");
        return;
      }
      void vscode.window
        .showQuickPick(
          all.map((x) => ({ label: x.qualname, description: x.file, id: x.id })),
          { placeHolder: "选择走读链起点" },
        )
        .then((pick) => {
          if (pick) void openChainView(context, pick.id, pick.label);
        });
    }),

    vscode.commands.registerCommand("byteWiki.generateCallchain", async () => {
      const w = ws();
      if (!w) return;
      await vscode.window.withProgress(
        { location: vscode.ProgressLocation.Notification, title: "Byte Wiki：生成调用链…", cancellable: false },
        async () => {
          try {
            await callchainGenerate(w);
            callchainProvider.refresh();
            void vscode.window.showInformationMessage("调用链已生成");
          } catch (e) {
            void vscode.window.showErrorMessage(`生成失败：${e instanceof Error ? e.message : e}`);
          }
        },
      );
    }),

    vscode.commands.registerCommand("byteWiki.searchNode", async () => {
      const w = ws();
      if (!w) return;
      const pat = await vscode.window.showInputBox({ placeHolder: "搜索 qualname / 文件（包含匹配）", prompt: "如 Query、cmd/query.rs" });
      if (!pat) return;
      try {
        const hits = await callchainSearch(w, pat, 30);
        if (!hits.length) {
          void vscode.window.showInformationMessage(`无命中：${pat}`);
          return;
        }
        const pick = await vscode.window.showQuickPick(
          hits.map((x) => ({ label: x.qualname, description: `${x.file}${x.is_entry ? " · 入口" : ""}`, id: x.id, entry: x })),
          { placeHolder: `命中 ${hits.length} 个，选择要打开的函数` },
        );
        if (pick) void openSourceAt(w, { file: pick.entry.file, start: pick.entry.lineno, end: pick.entry.end_lineno });
      } catch (e) {
        void vscode.window.showErrorMessage(`搜索失败：${e instanceof Error ? e.message : e}`);
      }
    }),
  );

  // 内部命令：补全项选中后展开插入
  context.subscriptions.push(
    vscode.commands.registerCommand("byteWiki._expandItem", (item: StashItem) => {
      void expandAndInsert(item);
    }),
  );

  // 命令面板 / 右键菜单
  context.subscriptions.push(
    vscode.commands.registerCommand("byteWiki.insertAny", () => {
      void pickAndInsert();
    }),
    vscode.commands.registerCommand("byteWiki.insertSnippet", () => {
      void pickAndInsert("snippet");
    }),
    vscode.commands.registerCommand("byteWiki.insertTemplate", () => {
      void pickAndInsert("template");
    }),
  );

  // 捕获 / 导入（capture / import）
  context.subscriptions.push(
    vscode.commands.registerCommand("byteWiki.captureSelectionSnippet", () => {
      void captureSelection("snippet");
    }),
    vscode.commands.registerCommand("byteWiki.captureSelectionTemplate", () => {
      void captureSelection("template");
    }),
    vscode.commands.registerCommand("byteWiki.captureFileSnippet", (uri?: vscode.Uri) => {
      void captureFile(uri, "snippet");
    }),
    vscode.commands.registerCommand("byteWiki.captureFileTemplate", (uri?: vscode.Uri) => {
      void captureFile(uri, "template");
    }),
    vscode.commands.registerCommand("byteWiki.importFolder", (uri?: vscode.Uri) => {
      void importFolder(uri);
    }),
  );

  // 配置变更 → 清缓存 + 重探测 transport（mode/cliPath 变了）+ 调用链树重探 bs
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("byteWiki")) {
        clearCache();
        resetTransport();
        callchainProvider.refresh();
      }
    }),
  );
}

export function deactivate(): void {
  // no-op
}
