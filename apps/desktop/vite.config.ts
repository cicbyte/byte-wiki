import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";

// Tauri 期望固定端口 1420，且禁用自动打开浏览器
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri 使用固定端口，Vite HMR 通过 websockets
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 不监听 src-tauri（Rust 端改动由 tauri dev 自己处理）
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  // Monaco 的 ts.worker 含 top-level await，IIFE（Vite 默认）不支持，必须用 es
  worker: {
    format: "es",
  },
  build: {
    // Tauri webview 支持现代 esbuild
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
}));
