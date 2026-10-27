---
layout: home

hero:
  name: byte-wiki
  text: 本地优先的仓库级知识引擎
  tagline: 把 Git 仓库变成可浏览、可追问、可分享的结构化知识，代码不出本机。
  actions:
    - theme: brand
      text: 快速开始
      link: /guide
    - theme: alt
      text: 功能详解
      link: /features

features:
  - title: 一键生成 Wiki
    details: 索引 → 规划 → 逐模块文档（拓扑序）→ 顶层架构，写入仓库内 .bytewiki/，全程本地。
  - title: 函数级调用链
    details: tree-sitter 多语言提取 + SQLite 毫秒查询；增量生成、事件合成边、断链边界报告、LSP 精确化。
  - title: RAG 问答
    details: 基于 Wiki 与知识卡的向量检索，流式回答、出处直达源码、多会话管理。
  - title: 分支感知
    details: Wiki / 调用链 / 问答按 git 分支隔离；滞后检测、同 commit 免重算、跨分支读 git 快照。
  - title: 可视化
    details: 调用链画布三种布局 + 项目图谱沉浸展示页。
  - title: 多端形态
    details: Tauri 桌面端、bw CLI、MCP 服务、VSCode 扩展，共享同一核心。
---
