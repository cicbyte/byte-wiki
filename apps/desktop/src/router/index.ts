import { createRouter, createWebHashHistory } from "vue-router";

const routes = [
  {
    path: "/",
    name: "wiki",
    component: () => import("@/views/wiki/index.vue"),
    meta: { title: "仓库 Wiki", fullsize: true },
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
