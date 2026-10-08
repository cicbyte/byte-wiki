/** 统一错误类型（HTTP 与 CLI 两个 transport 共用）+ 统一错误提示。 */
import * as vscode from "vscode";

export type BsErrorKind = "conn" | "config" | "http" | "cli";

export class BsError extends Error {
  kind: BsErrorKind;
  constructor(message: string, kind: BsErrorKind) {
    super(message);
    this.name = "BsError";
    this.kind = kind;
  }
}

/** 统一错误提示：config/conn 类附「打开设置」按钮，其他普通报错。 */
export function showBsError(e: unknown): void {
  if (e instanceof BsError && (e.kind === "config" || e.kind === "conn")) {
    void vscode.window
      .showErrorMessage(`Byte-Wiki：${e.message}`, "打开设置")
      .then((action) => {
        if (action === "打开设置") {
          void vscode.commands.executeCommand("workbench.action.openSettings", "byteWiki");
        }
      });
  } else {
    void vscode.window.showErrorMessage(
      `Byte-Wiki：${e instanceof Error ? e.message : String(e)}`,
    );
  }
}
