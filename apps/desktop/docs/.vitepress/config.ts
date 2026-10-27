import { defineConfig } from 'vitepress'

// byte-wiki 文档站配置
// base 设为 /docs/：构建产物部署到桌面端 public/docs/，经 Tauri webview 同源 iframe 加载
export default defineConfig({
  title: 'byte-wiki',
  description: '本地优先的仓库级知识引擎',
  base: '/docs/',
  lang: 'zh-CN',
  lastUpdated: true,
  themeConfig: {
    nav: [
      { text: '指南', link: '/guide' },
      { text: '功能', link: '/features' },
    ],
    sidebar: [
      {
        text: '开始',
        items: [
          { text: '简介', link: '/' },
          { text: '快速开始', link: '/guide' },
        ],
      },
      {
        text: '参考',
        items: [{ text: '功能详解', link: '/features' }],
      },
    ],
    outline: { label: '本页目录' },
    docFooter: { prev: '上一页', next: '下一页' },
    darkModeSwitchLabel: '主题',
    sidebarMenuLabel: '菜单',
    returnToTopLabel: '回到顶部',
    langMenuLabel: '语言',
  },
})
