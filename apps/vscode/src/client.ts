/**
 * 数据访问统一入口 —— 双 transport：CLI（bs 进程，免桌面端）+ HTTP（REST API）。
 *
 * - mode（byteWiki.mode）：auto | cli | http。auto 检测 bs 可用 → CLI，否则 HTTP。
 * - 缓存：listXxx 带 3s TTL（transport 无关）；失败回退旧缓存。
 * - 错误：统一 BsError（conn/config/http/cli），上层按 kind 针对性提示。
 *
 * CLI 实现见 cliClient.ts；HTTP 实现内联于本文件。
 */
import * as vscode from "vscode";
import type {
  Snippet,
  Template,
  CreateSnippetInput,
  CreateTemplateInput,
  ImportProjectInput,
} from "./types";
import { BsError } from "./errors";
export { BsError } from "./errors"; // re-export，保持 insert.ts 等的 `from "./client"` 导入不变
import * as cli from "./cliClient";

type Transport = "cli" | "http";

const CACHE_TTL = 3000;
const TIMEOUT_MS = 5000;

interface CacheEntry<T> {
  ts: number;
  data: T;
}
const cache = new Map<string, CacheEntry<unknown>>();

export function clearCache(): void {
  cache.clear();
}

// ============ transport 选择 ============

let transportCache: Transport | null = null;

function configuredMode(): "auto" | "cli" | "http" {
  return (
    vscode.workspace.getConfiguration("byteWiki").get<"auto" | "cli" | "http">("mode") ||
    "auto"
  );
}

function resolveTransport(): Transport {
  if (transportCache) return transportCache;
  const mode = configuredMode();
  let t: Transport;
  if (mode === "cli") t = "cli";
  else if (mode === "http") t = "http";
  else t = cli.isCliAvailable() ? "cli" : "http"; // auto
  transportCache = t;
  return t;
}

/** 配置变更（mode/cliPath）后重探测 transport。 */
export function resetTransport(): void {
  transportCache = null;
}

// ============ HTTP transport ============

function httpCfg(): { baseUrl: string; apiKey: string } {
  const c = vscode.workspace.getConfiguration("byteWiki");
  const baseUrl = (c.get<string>("baseUrl") || "").trim().replace(/\/+$/, "");
  const apiKey = (c.get<string>("apiKey") || "").trim();
  return { baseUrl, apiKey };
}

async function httpReq<T>(path: string, init?: RequestInit, timeoutMs = TIMEOUT_MS): Promise<T> {
  const { baseUrl, apiKey } = httpCfg();
  if (!baseUrl) {
    throw new BsError(
      "未配置 byteWiki.baseUrl（HTTP 模式需填写桌面端 REST API 地址；或改用 CLI/auto 模式）",
      "config",
    );
  }
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...((init?.headers as Record<string, string>) || {}),
  };
  if (apiKey) headers["X-API-Key"] = apiKey;

  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const res = await fetch(`${baseUrl}${path}`, { ...init, headers, signal: ctrl.signal });
    if (!res.ok) {
      const text = await res.text().catch(() => "");
      throw new BsError(`REST ${res.status}${text ? `: ${text}` : ` ${res.statusText}`}`, "http");
    }
    return (await res.json()) as T;
  } catch (e) {
    if (e instanceof BsError) throw e;
    throw new BsError(
      "无法连接 byte-wiki REST API，请确认桌面端已开启 REST API（或改用 CLI/auto 模式）",
      "conn",
    );
  } finally {
    clearTimeout(timer);
  }
}

// ============ 缓存辅助 ============

function cached<T>(key: string, loader: () => Promise<T>): Promise<T> {
  const e = cache.get(key) as CacheEntry<T> | undefined;
  if (e && Date.now() - e.ts < CACHE_TTL) return Promise.resolve(e.data);
  return loader()
    .then((data) => {
      cache.set(key, { ts: Date.now(), data });
      return data;
    })
    .catch((err) => {
      const old = cache.get(key) as CacheEntry<T> | undefined;
      if (old) return old.data; // 失败回退旧缓存
      throw err;
    });
}

// ============ 统一 API ============

export function listTemplates(): Promise<Template[]> {
  return cached("templates", async () =>
    resolveTransport() === "cli"
      ? cli.listTemplates()
      : (await httpReq<{ items: Template[] }>(`/templates?limit=200`)).items,
  );
}

export function listSnippets(): Promise<Snippet[]> {
  return cached("snippets", async () =>
    resolveTransport() === "cli"
      ? cli.listSnippets()
      : (await httpReq<{ items: Snippet[] }>(`/snippets?limit=200`)).items,
  );
}

export function getTemplate(id: string): Promise<Template> {
  return resolveTransport() === "cli"
    ? cli.getTemplate(id)
    : httpReq<Template>(`/templates/${encodeURIComponent(id)}`);
}

export function getSnippet(id: string): Promise<Snippet> {
  return resolveTransport() === "cli"
    ? cli.getSnippet(id)
    : httpReq<Snippet>(`/snippets/${encodeURIComponent(id)}`);
}

export function expandBody(body: string, variables: Record<string, string>): Promise<string> {
  return resolveTransport() === "cli"
    ? cli.expandBody(body, variables)
    : httpReq<{ code: string }>(`/templates/expand-body`, {
        method: "POST",
        body: JSON.stringify({ body, variables }),
      }).then((r) => r.code);
}

// ============ 创建 / 导入（capture 用） ============

export function createSnippet(input: CreateSnippetInput): Promise<Snippet> {
  return resolveTransport() === "cli"
    ? cli.createSnippet(input)
    : httpReq<Snippet>(`/snippets`, {
        method: "POST",
        body: JSON.stringify({
          title: input.title,
          code: input.code,
          language: input.language,
          description: input.description,
          tags: input.tags,
          category: input.category,
        }),
      });
}

export function createTemplate(input: CreateTemplateInput): Promise<Template> {
  return resolveTransport() === "cli"
    ? cli.createTemplate(input)
    : httpReq<Template>(`/templates`, {
        method: "POST",
        body: JSON.stringify({
          title: input.title,
          body: input.body,
          language: input.language,
          trigger: input.trigger,
          description: input.description,
          tags: input.tags,
          category: input.category,
        }),
      });
}

export function importProject(input: ImportProjectInput): Promise<unknown> {
  return resolveTransport() === "cli"
    ? cli.importProject(input)
    : httpReq<unknown>(
        `/projects`,
        {
          method: "POST",
          body: JSON.stringify({
            path: input.path,
            title: input.title,
            description: input.description,
            tags: input.tags,
            languages: input.languages,
            category: input.category,
          }),
        },
        180000, // 导入大目录放宽
      );
}
