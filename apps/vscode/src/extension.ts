/**
 * byte-wiki VSCode 插件入口。
 *
 * 注册：
 * - prefix 补全 provider（编辑器输入 trigger/title 即提示）
 * - 内部命令 byteWiki._expandItem（补全项选中后展开插入）
 * - 命令 byteWiki.insertAny / insertSnippet / insertTemplate（命令面板 + 右键菜单）
 * - 配置变更 → 清缓存（下次读取拉新）
 */
import * as vscode from "vscode";
import { registerCompletionProvider } from "./completionProvider";
import { pickAndInsert } from "./quickPick";
import { expandAndInsert } from "./insert";
import { captureSelection, captureFile, importFolder } from "./capture";
import { clearCache, resetTransport } from "./client";
import type { StashItem } from "./types";

export function activate(context: vscode.ExtensionContext): void {
  registerCompletionProvider(context);

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

  // 配置变更 → 清缓存 + 重探测 transport（mode/cliPath 变了）
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("byteWiki")) {
        clearCache();
        resetTransport();
      }
    }),
  );
}

export function deactivate(): void {
  // no-op
}
