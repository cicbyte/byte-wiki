/**
 * 捕获元数据表单 —— 串行 QuickInput 收集 title/language/tags/category/trigger。
 * 复用 varPicker 的 InputBox 串行模式；Esc 任一步取消整组（返回 null）。
 */
import * as vscode from "vscode";

export interface CaptureMeta {
  title: string;
  language?: string;
  tags?: string[];
  category?: string;
  trigger?: string; // 仅 template
}

export interface CaptureFormOptions {
  kind: "snippet" | "template";
  defaultLanguage?: string;
  defaultTitle?: string;
}

interface InputOpts {
  prompt: string;
  value?: string;
  placeHolder?: string;
  validate?: (v: string) => string | null;
}

async function inputBox(opts: InputOpts): Promise<string | undefined> {
  return vscode.window.showInputBox({
    prompt: opts.prompt,
    value: opts.value,
    placeHolder: opts.placeHolder,
    ignoreFocusOut: true,
    validateInput: opts.validate,
  });
}

/** 串行收集元数据；用户取消返回 null。 */
export async function captureForm(opts: CaptureFormOptions): Promise<CaptureMeta | null> {
  // 1. 标题（必填）
  const title = await inputBox({
    prompt: "标题",
    value: opts.defaultTitle ?? "",
    validate: (v) => (v.trim() ? null : "标题不能为空"),
  });
  if (title === undefined) return null;

  // 2. 语言
  const language = await inputBox({ prompt: "语言（可留空）", value: opts.defaultLanguage ?? "" });
  if (language === undefined) return null;

  // 3. 标签（逗号分隔）
  const tagsRaw = await inputBox({ prompt: "标签，逗号分隔（可留空）", value: "" });
  if (tagsRaw === undefined) return null;
  const tags = tagsRaw
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);

  // 4. 分类
  const category = await inputBox({ prompt: "分类（可留空）", value: "" });
  if (category === undefined) return null;

  // 5. 触发词（仅 template）
  let trigger: string | undefined;
  if (opts.kind === "template") {
    const tr = await inputBox({ prompt: "触发词（可留空，编辑器输入即提示）", value: "" });
    if (tr === undefined) return null;
    trigger = tr.trim() || undefined;
  }

  return {
    title: title.trim(),
    language: language.trim() || undefined,
    tags: tags.length ? tags : undefined,
    category: category.trim() || undefined,
    trigger,
  };
}
