import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

export const dialogApi = {
  ping: () => invoke<string>("ping"),

  /** 让用户选一个目录（dialog 插件），返回绝对路径或 null */
  pickDirectory: async (): Promise<string | null> => {
    const selected = await open({ directory: true, multiple: false });
    if (Array.isArray(selected)) return selected[0] ?? null;
    return selected ?? null;
  },

  /** 让用户选单个文件，返回绝对路径或 null */
  pickFile: async (): Promise<string | null> => {
    const selected = await open({ directory: false, multiple: false });
    if (Array.isArray(selected)) return selected[0] ?? null;
    return selected ?? null;
  },

  /** 保存对话框：让用户选择导出路径（默认带日期戳的 zip 文件名），返回绝对路径或 null（取消） */
  pickSavePath: async (): Promise<string | null> => {
    const d = new Date();
    const stamp = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const path = await save({
      defaultPath: `byte-wiki-backup-${stamp}.zip`,
      filters: [{ name: "ZIP 备份", extensions: ["zip"] }],
    });
    if (!path) return null;
    // 兜底：用户清空扩展名时补回 .zip（Rust 端按 .zip 扩展名判定为文件路径）
    return path.endsWith(".zip") ? path : `${path}.zip`;
  },
};
