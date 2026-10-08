/**
 * 代码语言检测 + 规范化（供 Monaco 编辑器用）
 *
 * Monaco 没有"自动语言检测"能力（VS Code 靠文件扩展名推断，裸 Monaco 不带）。
 * 这里提供两路保险：
 *   1. normalizeLanguageId：把用户填的非标准名（txt/sh/bash/shell）规范成 Monaco 认的 id
 *   2. detectLanguage：按代码内容启发式猜（fn main → rust，def → python…）
 */

/**
 * 用户填的语言名 → Monaco language id 的映射。
 * 覆盖常见别名/大小写变体。未命中返回原值的 lowerCase（多数语言名本身就是 Monaco id）。
 */
export function normalizeLanguageId(input: string | undefined | null): string {
  if (!input) return "plaintext";
  const n = input.trim().toLowerCase();
  if (!n) return "plaintext";
  const map: Record<string, string> = {
    text: "plaintext",
    txt: "plaintext",
    plain: "plaintext",
    sh: "shell",
    bash: "shell",
    zsh: "shell",
    shellscript: "shell",
    rs: "rust",
    py: "python",
    js: "javascript",
    ts: "typescript",
    jsx: "javascript",
    tsx: "typescript",
    go: "go",
    golang: "go",
    yml: "yaml",
    md: "markdown",
    csharp: "csharp",
    "c#": "csharp",
    cs: "csharp",
    cpp: "cpp",
    "c++": "cpp",
    cxx: "cpp",
    cc: "cpp",
    ps: "powershell",
    ps1: "powershell",
    powershell: "powershell",
    dockerfile: "dockerfile",
    docker: "dockerfile",
    ini: "ini",
    conf: "ini",
    bat: "bat",
    batch: "bat",
    // Vue SFC 用 html tokenizer（Monaco html 已支持 template/script/style 块与注释）
    vue: "html",
    // SVG 源码复用 XML 高亮
    svg: "xml",
  };
  return map[n] ?? n;
}

/**
 * 按代码内容启发式检测语言。返回 Monaco language id，识别不出返回 "plaintext"。
 * 规则按"特征性强、误伤少"排序，命中即返回。
 */
export function detectLanguage(code: string): string {
  const c = code;
  if (!c.trim()) return "plaintext";

  // shell shebang 最强信号
  if (/^#!.*\b(bash|zsh|sh)\b/.test(c)) return "shell";
  if (/^#!\/usr\/bin\/env\s+python/.test(c) || /^#!.*python3?/.test(c)) return "python";
  if (/^#!.*\bnode\b/.test(c)) return "javascript";

  // 各语言强特征
  if (/\bfn\s+main\s*\(\)/.test(c) || /\blet\s+mut\s+|->\s*Result</.test(c)) return "rust";
  if (/^\s*(def|class)\s+\w+.*:\s*$/m.test(c) || /^\s*from\s+\w+\s+import\s/m.test(c) || /\bprint\s*\(/.test(c))
    return "python";
  if (/^\s*package\s+\w+/m.test(c) && /\bfunc\s+\w+\s*\(/.test(c)) return "go";
  if (/<template>|<\/template>|<script\s+setup/.test(c)) return "html";
  if (/^\s*<!DOCTYPE\s|<html/i.test(c)) return "html";
  if (/^\s*<\?xml/.test(c)) return "xml";
  if (/^\s*SELECT\s+.+\s+FROM\s+/im.test(c)) return "sql";
  if (/^\s*({|\[)\s*$/m.test(c) && /["']\w+["']\s*:/.test(c)) return "json";
  if (/^\s*(import|export)\s+.+\s+from\s+['"]/m.test(c) || /\bconst\s+\w+\s*=\s*\(/.test(c))
    return "typescript"; // ES module 倾向 ts
  if (/\bpublic\s+(static\s+)?(class|void)\s+|System\.out\.println/.test(c)) return "java";
  if (/^\s*#include\s+[<"]/.test(c)) return "cpp";
  if (/^\s*(FROM|RUN|CMD|ENTRYPOINT|WORKDIR|COPY|ARG|ENV)\s+/m.test(c)) return "dockerfile";
  if (/^\s*\[[a-z]+\]/m.test(c) && /^\s*\w+\s*=/m.test(c)) return "ini";
  if (/^\s*(apiVersion|kind|metadata|spec):\s*$/m.test(c)) return "yaml";
  if (/^\s*-{3}\s*$/m.test(c)) return "yaml"; // YAML front matter
  if (/^#{1,6}\s+\S/.test(c)) return "markdown";
  return "plaintext";
}
