import { invoke } from "@tauri-apps/api/core";

export const wallpaperApi = {
  // ===== 壁纸 =====
  wallpaperUpload: (srcPath: string) => invoke<string>("upload_wallpaper", { srcPath }),
  wallpaperList: () => invoke<string[]>("list_wallpapers"),
  wallpaperDelete: (filename: string) => invoke<void>("delete_wallpaper", { filename }),
};
