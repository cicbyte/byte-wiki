/**
 * 命令面板 / 右键菜单的 QuickPick 入口。
 *
 * list（带缓存，含正文）→ QuickPick 混排 snippet/template → 本地 filter → 选中 → expandAndInsert。
 * 不用 search 端点（hit 无正文，每选一次都要往返 get）；本地 filter 对 200 条规模够用。
 */
import * as vscode from "vscode";
import { listTemplates, listSnippets } from "./client";
import type { StashItem, Template, Snippet } from "./types";
import { expandAndInsert } from "./insert";

/**
 * QuickPick 项：StashItem + QuickPickItem 需要的 label/detail。
 * 不写成 `StashItem & QuickPickItem`：两者都有 description/kind 字段（语义不同），交叉会坍缩。
 * StashItem 用 `type`（非 `kind`）避开 QuickPickItem.kind 冲突；description 复用为 QuickPick
 * 副标题（显示 trigger/language），StashItem 的描述放 detail。
 */
interface PickItem extends StashItem {
  label: string;
  detail: string;
}

function fromTemplate(t: Template): PickItem {
  return {
    type: "template",
    id: t.short_id,
    title: t.title,
    description: `${t.trigger ? `⌖${t.trigger}  ` : ""}${t.language || "通用"}`,
    language: t.language,
    body: t.body,
    trigger: t.trigger,
    label: t.title,
    detail: `模板${t.description ? " · " + t.description : ""}`,
  };
}

function fromSnippet(s: Snippet): PickItem {
  return {
    type: "snippet",
    id: s.short_id,
    title: s.title,
    description: s.language || "通用",
    language: s.language,
    body: s.code,
    trigger: null,
    label: s.title,
    detail: `片段${s.description ? " · " + s.description : ""}`,
  };
}

/** 弹出 QuickPick 选择并插入。kindFilter 限定类型（右键菜单用）。 */
export async function pickAndInsert(kindFilter?: "snippet" | "template"): Promise<void> {
  const [templates, snippets] = await Promise.all([
    listTemplates().catch(() => [] as Template[]),
    listSnippets().catch(() => [] as Snippet[]),
  ]);

  const items: PickItem[] = [];
  if (kindFilter !== "snippet") templates.forEach((t) => items.push(fromTemplate(t)));
  if (kindFilter !== "template") snippets.forEach((s) => items.push(fromSnippet(s)));

  if (!items.length) {
    vscode.window.showWarningMessage(
      "Byte-Wiki：库为空，或无法连接 REST API（请在桌面端开启 REST API）",
    );
    return;
  }

  const qp = vscode.window.createQuickPick<PickItem>();
  qp.items = items;
  qp.placeholder =
    kindFilter === "snippet"
      ? "搜索片段…"
      : kindFilter === "template"
        ? "搜索模板…"
        : "搜索片段或模板…";
  qp.matchOnDescription = true;
  qp.matchOnDetail = true;
  qp.onDidAccept(() => {
    const sel = qp.selectedItems[0];
    qp.hide();
    if (sel) void expandAndInsert(sel);
  });
  qp.show();
}
