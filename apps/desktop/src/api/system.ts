/**
 * 系统级 command 包装：数据目录、系统文件管理器联动
 */
import { invoke } from "@tauri-apps/api/core";

export const systemApi = {
  /** 当前数据目录（vault 所在根目录） */
  dataDir: () => invoke<string>("data_dir"),

  /** 在系统文件管理器中打开指定路径（目录直接打开，文件则定位并选中） */
  openInFolder: (path: string) => invoke<void>("open_in_folder", { path }),

  /** 图表导出落盘（kind: svg=文本 / png=data URL；path 来自系统保存对话框） */
  saveDiagram: (path: string, kind: "svg" | "png", data: string) =>
    invoke<void>("save_diagram", { path, kind, data }),
};
