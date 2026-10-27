import { createRouter, createWebHashHistory } from "vue-router";

/**
 * 两层 IA：
 * - 全局层：项目（/）、服务、系统
 * - 项目层：/repo/:id（总览）、/docs 文档、/callgraph 调用链、/walkthrough 链路走读、/chat 问答、/cards 知识卡
 *   —— 进入项目后 Sidebar 切换为项目专属菜单（components/layout/lib/constants.repoMenu）
 */
const routes = [
  {
    path: "/",
    name: "repo-list",
    component: () => import("@/views/wiki/RepoList.vue"),
    meta: { title: "项目", fullsize: true },
  },
  {
    path: "/repo/:id",
    name: "repo-overview",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "总览", tab: "overview", fullsize: true },
  },
  {
    path: "/repo/:id/docs",
    name: "repo-docs",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "Wiki", tab: "docs", fullsize: true },
  },
  {
    path: "/repo/:id/files",
    name: "repo-files",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "源代码", tab: "files", fullsize: true },
  },
  {
    path: "/repo/:id/callgraph",
    name: "repo-callgraph",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "调用链", tab: "callgraph", fullsize: true },
  },
  {
    path: "/repo/:id/graph",
    name: "repo-graph",
    component: () => import("@/views/graph/ProjectGraph.vue"),
    meta: { title: "项目图谱", fullsize: true },
  },
  {
    path: "/docs",
    name: "docs",
    component: () => import("@/views/docs/index.vue"),
    meta: { title: "文档", fullsize: true },
  },
  {
    path: "/repo/:id/walkthrough",
    name: "repo-walkthrough",
    component: () => import("@/views/wiki/Walkthrough.vue"),
    meta: { title: "链路走读", fullsize: true },
  },
  {
    path: "/repo/:id/chat",
    name: "repo-chat",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "对话", tab: "chat", fullsize: true },
  },
  {
    path: "/repo/:id/cards",
    name: "repo-cards",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "知识卡", tab: "cards", fullsize: true },
  },
  {
    path: "/repo/:id/llm-logs",
    name: "repo-llm-logs",
    component: () => import("@/views/repo/RepoSpace.vue"),
    meta: { title: "模型日志", tab: "llm-logs", fullsize: true },
  },
  {
    path: "/tasks",
    name: "gen-tasks",
    component: () => import("@/views/tasks/TasksPage.vue"),
    meta: { title: "后台任务", fullsize: true },
  },
  {
    path: "/mcp-service",
    name: "mcp-service",
    component: () => import("@/views/mcp-service/index.vue"),
    meta: { title: "MCP 服务", group: "服务", fullsize: true },
  },
  {
    path: "/api-service",
    name: "api-service",
    component: () => import("@/views/api-service/index.vue"),
    meta: { title: "REST API", group: "服务", fullsize: true },
  },
  {
    path: "/settings",
    name: "settings",
    component: () => import("@/views/settings/index.vue"),
    meta: { title: "设置", group: "系统", fullsize: true },
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
