import { invoke } from "@tauri-apps/api/core";

export const backupApi = {
  // ============ 备份 / 恢复 ============

  /** 导出备份（返回生成的备份文件/目录路径） */
  backup: (output?: string, noCompress = false) =>
    invoke<string>("backup", { output, noCompress }),

  /** 从备份恢复（覆盖当前数据，需重启生效） */
  restore: (path: string) => invoke<void>("restore", { path }),

  /** 设置数据目录指针（重启后生效） */
  setDataDir: (dir: string) => invoke<void>("set_data_dir", { dir }),

  /** 清除数据目录指针（恢复默认路径，重启后生效） */
  clearDataDir: () => invoke<void>("clear_data_dir"),
};
