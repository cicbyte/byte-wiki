/**
 * CLI transport —— 通过 spawn `bs` 进程访问本地库（无需桌面端开 REST）。
 *
 * bs 的 `--json` 全局开关把结构化结果输出到 stdout，日志走 stderr，二者分离。
 * 变量渲染走 `bs template expand --stdin`（stdin 喂 {body, variables} JSON，避免命令行转义/长度）。
 *
 * 后端命令对照（apps/cli）：
 *   list   → bs --json template/snippet list --limit 200
 *   get    → bs --json template/snippet get <id>
 *   expand → bs --json template expand --stdin  (stdin: {body, variables})
 */
import { spawn, spawnSync } from "child_process";
import * as vscode from "vscode";
import type { Snippet, Template, CreateSnippetInput, CreateTemplateInput, ImportProjectInput } from "./types";
import { BsError } from "./errors";

const TIMEOUT_MS = 5000;

function cliPath(): string {
  return vscode.workspace.getConfiguration("byteWiki").get<string>("cliPath") || "bs";
}

/** spawn bs，收集 stdout（可选写 stdin），超时 kill。返回 stdout 字符串。
 *  （wiki/调用链视图等扩展内其他模块复用同一传输通道） */
export function runBs(args: string[], stdin?: string, timeoutMs = TIMEOUT_MS): Promise<string> {
  return new Promise((resolve, reject) => {
    const bin = cliPath();
    let child;
    try {
      child = spawn(bin, args, { windowsHide: true });
    } catch (e) {
      reject(new BsError(`无法启动 bs CLI：${(e as Error).message}`, "conn"));
      return;
    }
    const stdoutChunks: Buffer[] = [];
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill();
      reject(new BsError(`bs 命令超时（${timeoutMs}ms）`, "cli"));
    }, timeoutMs);

    child.stdout.on("data", (d) => stdoutChunks.push(d));
    child.stderr.on("data", (d) => {
      stderr += d.toString();
    });
    child.on("error", (e) => {
      clearTimeout(timer);
      // ENOENT = bs 不在 PATH
      const msg = (e as NodeJS.ErrnoException).code === "ENOENT"
        ? `找不到 bs CLI（不在 PATH）。请在设置 byteWiki.cliPath 指定路径，或改用 HTTP 模式。`
        : `启动 bs CLI 失败：${e.message}`;
      reject(new BsError(msg, "conn"));
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      const out = Buffer.concat(stdoutChunks).toString("utf8");
      if (code !== 0) {
        reject(new BsError(`bs 命令失败（exit ${code}）${stderr ? "：" + stderr.trim() : ""}`, "cli"));
        return;
      }
      resolve(out);
    });
    if (stdin !== undefined) {
      child.stdin.end(stdin);
    } else {
      child.stdin.end();
    }
  });
}

function parseJson<T>(out: string): T {
  try {
    return JSON.parse(out.trim()) as T;
  } catch (e) {
    throw new BsError(`解析 bs 输出失败：${(e as Error).message}`, "cli");
  }
}

export async function listTemplates(): Promise<Template[]> {
  return parseJson<Template[]>(await runBs(["--json", "template", "list", "--limit", "200"]));
}

export async function listSnippets(): Promise<Snippet[]> {
  return parseJson<Snippet[]>(await runBs(["--json", "snippet", "list", "--limit", "200"]));
}

export async function getTemplate(id: string): Promise<Template> {
  return parseJson<Template>(await runBs(["--json", "template", "get", id]));
}

export async function getSnippet(id: string): Promise<Snippet> {
  return parseJson<Snippet>(await runBs(["--json", "snippet", "get", id]));
}

export async function expandBody(body: string, variables: Record<string, string>): Promise<string> {
  const stdin = JSON.stringify({ body, variables });
  const out = await runBs(["--json", "template", "expand", "--stdin"], stdin);
  return parseJson<{ code: string }>(out).code;
}

/** 检测 bs 是否可用（auto 模式探测用，结果由调用方缓存）。 */
export function isCliAvailable(): boolean {
  try {
    const r = spawnSync(cliPath(), ["--version"], { windowsHide: true, timeout: 3000 });
    return r.status === 0;
  } catch {
    return false;
  }
}

// ============ 创建 / 导入（capture 用） ============

export async function createSnippet(input: CreateSnippetInput): Promise<Snippet> {
  const args = ["--json", "snippet", "add", "--title", input.title];
  if (input.language) args.push("--lang", input.language);
  if (input.description) args.push("--description", input.description);
  if (input.category) args.push("--category", input.category);
  for (const t of input.tags || []) args.push("--tag", t);
  return parseJson<Snippet>(await runBs(args, input.code)); // code 走 stdin（不传 --code）
}

export async function createTemplate(input: CreateTemplateInput): Promise<Template> {
  const args = ["--json", "template", "add", "--title", input.title];
  if (input.language) args.push("--lang", input.language);
  if (input.trigger) args.push("--trigger", input.trigger);
  if (input.description) args.push("--description", input.description);
  if (input.category) args.push("--category", input.category);
  for (const t of input.tags || []) args.push("--tag", t);
  return parseJson<Template>(await runBs(args, input.body)); // body 走 stdin
}

export async function importProject(input: ImportProjectInput): Promise<unknown> {
  const args = ["--json", "add", input.path];
  if (input.title) args.push("--title", input.title);
  if (input.description) args.push("--description", input.description);
  if (input.category) args.push("--category", input.category);
  for (const t of input.tags || []) args.push("--tag", t);
  for (const l of input.languages || []) args.push("--lang", l);
  const out = await runBs(args, undefined, 180000); // 导入大目录放宽到 3 分钟
  try {
    return JSON.parse(out.trim());
  } catch {
    return undefined; // bs add 非 JSON 输出无所谓，成功与否看 exit code
  }
}
