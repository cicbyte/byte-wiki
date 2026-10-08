/**
 * prefix 智能补全 —— 移植自 apps/desktop/src/monaco/templateCompletion.ts。
 *
 * 注册 {scheme:'file'} 全语言 provider，按当前文档 languageId 过滤候选：
 * 只推 language 匹配（或 language 为空=通用）的 snippet/template。
 * 候选用 trigger / title 做前缀/包含匹配，label = `trigger → title`。
 * 候选 detail 标注 `byte-wiki` 品牌，便于在混合补全列表（如 Vue/Volar）中一眼区分。
 *
 * 关键技术：候选 insertText="" + command(byteWiki._expandItem)。
 * 选中补全项 → 清空触发词 → 执行命令 → expandAndInsert（先填变量再插）。
 * 不能用纯 insertText，因为模板可能需要先弹变量框。
 */
import * as vscode from "vscode";
import { listTemplates, listSnippets } from "./client";
import type { StashItem, Template, Snippet } from "./types";

const TRIGGER_CHARS = "abcdefghijklmnopqrstuvwxyz".split("");

function templateToItem(t: Template): StashItem {
  return {
    type: "template", id: t.short_id, title: t.title, description: t.description,
    language: t.language, body: t.body, trigger: t.trigger,
  };
}
function snippetToItem(s: Snippet): StashItem {
  return {
    type: "snippet", id: s.short_id, title: s.title, description: s.description,
    language: s.language, body: s.code, trigger: null,
  };
}

/** 语言匹配：模板 language 空=通用；否则与当前 doc languageId 相等或常见别名 */
function langMatch(itemLang: string, docLangId: string): boolean {
  if (!itemLang) return true;
  const d = docLangId.toLowerCase();
  // 文本类文件（markdown / plaintext）不限模板语言——常用于代码块、文档片段
  if (d === "markdown" || d === "plaintext") return true;
  const langs = itemLang.split(",").map((x) => x.trim().toLowerCase());
  if (langs.includes(d)) return true;
  // 常见缩写别名
  const alias: Record<string, string[]> = {
    typescript: ["ts", "typescriptreact", "tsx"],
    javascript: ["js", "javascriptreact", "jsx"],
    rust: ["rs"],
    python: ["py"],
    golang: ["go"],
    vue: ["vue"],
  };
  return !!alias[d]?.some((a) => langs.includes(a));
}

export function registerCompletionProvider(context: vscode.ExtensionContext): void {
  const provider: vscode.CompletionItemProvider = {
    async provideCompletionItems(doc, position) {
      const cfg = vscode.workspace.getConfiguration("byteWiki");
      const enabled = cfg.get<boolean>("enableTriggers", true);
      if (!enabled) return [];
      const matchLang = cfg.get<boolean>("matchLanguage", true);

      const wordRange =
        doc.getWordRangeAtPosition(position) ?? new vscode.Range(position, position);
      const typed = doc.getText(wordRange).toLowerCase();
      if (!typed) return [];

      const [templates, snippets] = await Promise.all([
        listTemplates().catch(() => [] as Template[]),
        listSnippets().catch(() => [] as Snippet[]),
      ]);
      const items: StashItem[] = [
        ...templates.map(templateToItem),
        ...snippets.map(snippetToItem),
      ];
      const docLang = doc.languageId;

      // trigger / title 子串匹配（按当前语言过滤）
      const matched = items.filter((it) => {
        if (matchLang && !langMatch(it.language, docLang)) return false;
        const tr = (it.trigger ?? "").toLowerCase();
        const title = it.title.toLowerCase();
        return (tr && (tr.startsWith(typed) || tr.includes(typed))) || title.includes(typed);
      });

      return matched.map((it) => {
        const label = it.trigger ? `${it.trigger} → ${it.title}` : it.title;
        const ci = new vscode.CompletionItem(label, vscode.CompletionItemKind.Snippet);
        // detail 带 byte-wiki 品牌，混合补全列表中一眼区分
        ci.detail = `byte-wiki · ${it.type === "template" ? "模板" : "片段"} · ${it.language || "通用"}`;
        ci.documentation = it.body.slice(0, 300);
        ci.insertText = "";
        ci.range = wordRange;
        ci.command = {
          command: "byteWiki._expandItem",
          title: "展开并插入",
          arguments: [it],
        };
        ci.sortText = it.trigger ? `0_${it.trigger}` : `1_${it.title}`;
        return ci;
      });
    },
  };

  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider({ scheme: "file" }, provider, ...TRIGGER_CHARS),
  );
}
