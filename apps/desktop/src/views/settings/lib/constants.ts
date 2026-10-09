/**
 * 设置页静态数据：预置默认排除目录（与 core `default_exclude_dirs` 保持一致）
 */
export const DEFAULT_EXCLUDE_DIRS: string[] = [
  "node_modules",
  ".git",
  "target",
  "dist",
  "build",
  ".next",
  ".nuxt",
  "out",
  ".output",
  "__pycache__",
  ".venv",
  "venv",
  ".idea",
  ".vscode",
  ".DS_Store",
];
