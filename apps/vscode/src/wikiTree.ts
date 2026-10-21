/**
 * Wiki 页面树 —— 直读工作区 `.bytewiki/branches/<当前分支>/` 产物（零后端依赖）。
 *
 * 分支用 `git rev-parse --abbrev-ref HEAD` 探测；目录不存在（未生成/克隆未带产物）
 * 时给出占位说明。文档点击 → byteWiki.openPage 进 webview 预览（wikiPreview.ts）。
 */
import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";

export function workspaceRoot(): string | undefined {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

export function gitBranch(root: string): string {
  try {
    const b = execSync("git rev-parse --abbrev-ref HEAD", {
      cwd: root,
      encoding: "utf8",
      windowsHide: true,
    }).trim();
    return b || "main";
  } catch {
    return "main";
  }
}

export function branchWikiRoot(root: string): string {
  return path.join(root, ".bytewiki", "branches", gitBranch(root));
}

export interface DocNode {
  kind: "dir" | "doc" | "hint";
  name: string;
  rel: string;
  children?: DocNode[];
}

export class WikiPagesProvider implements vscode.TreeDataProvider<DocNode> {
  private readonly _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;
  private root: DocNode | null = null;

  refresh(): void {
    this.root = null;
    this._onDidChange.fire();
  }

  getTreeItem(el: DocNode): vscode.TreeItem {
    if (el.kind === "hint") {
      return { label: el.name, iconPath: new vscode.ThemeIcon("warning"), collapsibleState: vscode.TreeItemCollapsibleState.None };
    }
    if (el.kind === "dir") {
      return {
        label: el.name,
        iconPath: new vscode.ThemeIcon(el.rel ? "folder" : "root-folder"),
        collapsibleState: vscode.TreeItemCollapsibleState.Expanded,
      };
    }
    return {
      label: el.name,
      description: el.rel.includes("/") ? el.rel.slice(0, el.rel.lastIndexOf("/")) : "",
      iconPath: new vscode.ThemeIcon("book"),
      tooltip: el.rel,
      command: { command: "byteWiki.openPage", title: "打开 Wiki 页面", arguments: [el.rel] },
    };
  }

  async getChildren(el?: DocNode): Promise<DocNode[]> {
    if (!el) {
      await this.ensure();
      return this.root?.children ?? [];
    }
    return el.children ?? [];
  }

  private async ensure(): Promise<void> {
    if (this.root) return;
    const ws = workspaceRoot();
    if (!ws) {
      this.root = { kind: "dir", name: "", rel: "", children: [{ kind: "hint", name: "未打开工作区", rel: "" }] };
      return;
    }
    const broot = branchWikiRoot(ws);
    if (!fs.existsSync(broot)) {
      this.root = {
        kind: "dir",
        name: "",
        rel: "",
        children: [{ kind: "hint", name: `分支「${gitBranch(ws)}」还没有 .bytewiki 产物（先在桌面端或 bs 生成）`, rel: "" }],
      };
      return;
    }
    const children: DocNode[] = [];
    for (const f of ["overview.md", "architecture.md"]) {
      if (fs.existsSync(path.join(broot, f))) {
        children.push({ kind: "doc", name: f.replace(/\.md$/, ""), rel: f });
      }
    }
    for (const dir of ["pages", "cards"]) {
      const sub = this.scanDir(path.join(broot, dir), dir);
      if (sub) children.push(sub);
    }
    const mmd = path.join(broot, "graphs", "dependency.mmd");
    if (fs.existsSync(mmd)) children.push({ kind: "doc", name: "模块依赖图谱", rel: "graphs/dependency.mmd" });
    if (!children.length) {
      this.root = { kind: "dir", name: "", rel: "", children: [{ kind: "hint", name: "产物为空", rel: "" }] };
      return;
    }
    this.root = { kind: "dir", name: "", rel: "", children };
  }

  /** 递归收集 md/mmd（文档在前、子目录在后，各自按名排序） */
  private scanDir(abs: string, rel: string): DocNode | null {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(abs, { withFileTypes: true });
    } catch {
      return null;
    }
    const docs: DocNode[] = [];
    const dirs: DocNode[] = [];
    for (const e of entries) {
      if (e.name.startsWith(".")) continue;
      const childRel = `${rel}/${e.name}`;
      if (e.isDirectory()) {
        const sub = this.scanDir(path.join(abs, e.name), childRel);
        if (sub) dirs.push(sub);
      } else if (/\.(md|mmd)$/i.test(e.name)) {
        docs.push({ kind: "doc", name: e.name.replace(/\.(md|mmd)$/i, ""), rel: childRel });
      }
    }
    if (!docs.length && !dirs.length) return null;
    const byName = (a: DocNode, b: DocNode) => a.name.localeCompare(b.name);
    return {
      kind: "dir",
      name: rel.split("/").pop() || rel,
      rel,
      children: [...docs.sort(byName), ...dirs.sort(byName)],
    };
  }
}
