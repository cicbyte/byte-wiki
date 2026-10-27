// 将 VitePress 构建产物（docs/.vitepress/dist）拷贝到 public/docs/，
// 以便 vite build 把文档随前端一起打包到 dist/docs/，供 Tauri webview 同源 iframe 加载。
import { cpSync, rmSync } from 'node:fs'

const src = 'docs/.vitepress/dist'
const dst = 'public/docs'

rmSync(dst, { recursive: true, force: true })
cpSync(src, dst, { recursive: true })
console.log(`✓ 文档已拷贝到 ${dst}`)
