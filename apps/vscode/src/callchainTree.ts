/**
 * 调用链入口树 —— `bs callchain-query --overview` 数据（#570 概览：入口清单，
 * 零入口库为公共 API 面 top30）。点击打开函数源码；行内按钮进走读链。
 */
import * as vscode from "vscode";
import { callchainOverview, type Overview, type OverviewEntry } from "./callchainClient";
import { workspaceRoot } from "./wikiTree";

type ChainTreeItem =
  | { kind: "info"; text: string }
  | { kind: "entry"; entry: OverviewEntry };

export class CallchainProvider implements vscode.TreeDataProvider<ChainTreeItem> {
  private readonly _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;
  private overview: Overview | null = null;
  private error = "";
  /** 供走读链/搜索快速取节点 */
  private byId = new Map<string, OverviewEntry>();

  refresh(): void {
    this.overview = null;
    this.error = "";
    this._onDidChange.fire();
  }

  getTreeItem(el: ChainTreeItem): vscode.TreeItem {
    if (el.kind === "info") {
      return { label: el.text, iconPath: new vscode.ThemeIcon("info"), collapsibleState: vscode.TreeItemCollapsibleState.None };
    }
    const e = el.entry;
    return {
      label: e.qualname,
      description: e.file,
      tooltip: new vscode.MarkdownString(
        `\`${e.id}\`\n\n${e.file}:${e.routes?.length ? " " + e.routes.join(" / ") : ""}`,
      ),
      iconPath: new vscode.ThemeIcon(e.is_entry ? "debug-start" : "symbol-method"),
      contextValue: "entry",
      command: { command: "byteWiki.openFunction", title: "打开函数源码", arguments: [e] },
    };
  }

  async getChildren(el?: ChainTreeItem): Promise<ChainTreeItem[]> {
    if (el) return [];
    const ws = workspaceRoot();
    if (!ws) return [{ kind: "info", text: "未打开工作区" }];
    if (this.error) return [{ kind: "info", text: this.error }];
    if (!this.overview) {
      try {
        this.overview = await callchainOverview(ws);
        this.byId = new Map(this.overview.entries.map((e) => [e.id, e]));
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        this.error = msg.includes("尚未生成") ? "尚未生成调用链（点上方 ▶ 生成）" : msg;
        return [{ kind: "info", text: this.error }];
      }
    }
    const ov = this.overview;
    const head =
      `函数 ${ov.total_nodes} · 边 ${ov.total_edges} · ` +
      (ov.entries_are_api ? `公共 API 面（无框架入口）` : `入口 ${ov.total_entries}`);
    return [{ kind: "info", text: head }, ...ov.entries.map((e) => ({ kind: "entry" as const, entry: e }))];
  }

  entryById(id: string): OverviewEntry | undefined {
    return this.byId.get(id);
  }

  allEntries(): OverviewEntry[] {
    return this.overview?.entries ?? [];
  }
}
