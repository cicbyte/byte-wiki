/**
 * 调用链入口树 —— `bw callchain-query --overview` 数据（#570 概览：入口清单，
 * 零入口库为公共 API 面 top30）。点击打开函数源码；行内按钮进走读链。
 *
 * 未检测到 bw CLI 时展示「设置路径」空状态项（不走错误文案）。
 */
import * as vscode from "vscode";
import { isCliAvailable } from "./cliClient";
import { callchainOverview, type Overview, type OverviewEntry } from "./callchainClient";
import { workspaceRoot } from "./wikiTree";

type ChainTreeItem =
  | { kind: "info"; text: string }
  | { kind: "setup" }
  | { kind: "entry"; entry: OverviewEntry };

export class CallchainProvider implements vscode.TreeDataProvider<ChainTreeItem> {
  private readonly _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;
  private overview: Overview | null = null;
  private error = "";
  /** bw 可用性探测缓存（refresh/配置变更时失效重探） */
  private cliOk: boolean | null = null;
  /** 供走读链/搜索快速取节点 */
  private byId = new Map<string, OverviewEntry>();

  refresh(): void {
    this.overview = null;
    this.error = "";
    this.cliOk = null;
    this._onDidChange.fire();
  }

  getTreeItem(el: ChainTreeItem): vscode.TreeItem {
    if (el.kind === "setup") {
      return {
        label: "未检测到 bw CLI（byte-wiki）",
        description: "点击设置路径",
        iconPath: new vscode.ThemeIcon("settings-gear"),
        tooltip: new vscode.MarkdownString(
          "调用链视图需要 `bw` CLI（byte-wiki 的 CLI，区别于 byte-stash 的 bs）。\n\n默认从 PATH 解析；可在设置 `byteWiki.cliPath` 指定绝对路径，或点击本项选择文件。",
        ),
        command: { command: "byteWiki.setBsPath", title: "设置 bs CLI 路径" },
      };
    }
    if (el.kind === "info") {
      return { label: el.text, iconPath: new vscode.ThemeIcon("info"), collapsibleState: vscode.TreeItemCollapsibleState.None };
    }
    const e = el.entry;
    return {
      label: e.qualname,
      description: e.file,
      tooltip: new vscode.MarkdownString(
        `\`${e.id}\`\n\n${e.file}${e.routes?.length ? " · " + e.routes.join(" / ") : ""}`,
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
    if (this.cliOk === null) {
      this.cliOk = isCliAvailable();
    }
    if (!this.cliOk) return [{ kind: "setup" }];
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
