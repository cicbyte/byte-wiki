/**
 * 模板变量提取 —— 移植自 apps/desktop/src/utils/templateEngine.ts。
 *
 * 仅提取顶层简单变量 {{ name }}，去重保序；不处理嵌套 / 控制结构（{% for %} 等）。
 * 与桌面端 extractVariables 行为一致，保证「桌面端能填的变量，插件也能填」。
 */

const VAR_RE = /\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)/g;

/** 从模板 body 提取变量名（去重、保序） */
export function extractVariables(body: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  VAR_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = VAR_RE.exec(body)) !== null) {
    const name = m[1];
    if (!seen.has(name)) {
      seen.add(name);
      out.push(name);
    }
  }
  return out;
}
