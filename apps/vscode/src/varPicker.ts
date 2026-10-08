/**
 * 变量值收集 —— 对应桌面端 TemplateExpandDialog 的填值环节。
 *
 * MVP 用串行 InputBox 逐个收集标量变量（预填空，Esc 取消整组）。
 * 不处理数组/循环（后端 expand 也只吃 HashMap<String,String>）。
 */
import * as vscode from "vscode";

/**
 * 逐个收集变量值。
 * @param vars 变量名列表（已去重保序）
 * @param title 模板标题（用于 InputBox 标题）
 * @returns 填好的 values，或 null（用户中途取消）
 */
export async function collectVariables(
  vars: string[],
  title?: string,
): Promise<Record<string, string> | null> {
  const values: Record<string, string> = {};
  for (let i = 0; i < vars.length; i++) {
    const name = vars[i];
    const val = await vscode.window.showInputBox({
      prompt: `填写变量：${name}`,
      placeHolder: name,
      value: "",
      title: `${title ?? "模板变量"}（${i + 1}/${vars.length}）`,
      ignoreFocusOut: true, // 失焦不关，避免误点丢失进度
    });
    if (val === undefined) return null; // Esc / 取消
    values[name] = val;
  }
  return values;
}
