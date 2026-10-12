/**
 * 全局层 + 项目空间菜单静态数据（SidebarNav 消费）
 *
 * 两层 IA（对齐 router/index.ts）：
 * - 全局层：项目（顶层项）+ 服务 / 系统 分组
 * - 项目层：repoMenu(repoId) 平铺项目专属菜单（总览/文档/调用链/对话/知识卡）
 *
 * 注意：label 支持 `() => string`（动态标签预留），统一经 labelOfItem 解析。
 */
import type { Component } from "vue";
import {
  ProjectOutlined,
  FolderOutlined,
  CloudServerOutlined,
  ClusterOutlined,
  ApiOutlined,
  SettingOutlined,
  AppstoreOutlined,
  FileTextOutlined,
  NodeIndexOutlined,
  MessageOutlined,
  BookOutlined,
} from "@ant-design/icons-vue";

/** 单个菜单项（key = 路由 path，高亮按精确匹配） */
export interface MenuItem {
  key: string;
  icon: Component;
  label: string | (() => string);
}

/** 分组菜单（折叠态 hover flyout / 展开态手风琴） */
export interface MenuGroup {
  key: string;
  label: string;
  icon: Component;
  children: MenuItem[];
}

/** 解析菜单项标签（兼容静态字符串与动态函数） */
export function labelOfItem(item: MenuItem): string {
  return typeof item.label === "function" ? item.label() : item.label;
}

/** 全局层：顶层独立项 */
export const topItems: MenuItem[] = [
  { key: "/", icon: ProjectOutlined, label: "项目" },
];

/** 全局层：分组菜单（服务 / 系统） */
export const menuGroups: MenuGroup[] = [
  {
    key: "service",
    label: "服务",
    icon: CloudServerOutlined,
    children: [
      { key: "/mcp-service", icon: ClusterOutlined, label: "MCP 服务" },
      { key: "/api-service", icon: ApiOutlined, label: "REST API" },
    ],
  },
  {
    key: "system",
    label: "系统",
    icon: SettingOutlined,
    children: [{ key: "/settings", icon: SettingOutlined, label: "设置" }],
  },
];

/**
 * 项目空间菜单：完全为项目设计，平铺五项（总览/文档/调用链/对话/知识卡），
 * 当前仓库信息展示在全局 footer（见 SidebarBottom）。
 */
export function repoMenu(repoId: string): { topItems: MenuItem[]; menuGroups: MenuGroup[] } {
  const base = `/repo/${repoId}`;
  return {
    topItems: [
      { key: base, icon: AppstoreOutlined, label: "总览" },
      { key: `${base}/docs`, icon: FileTextOutlined, label: "文档" },
      { key: `${base}/files`, icon: FolderOutlined, label: "文件" },
      { key: `${base}/callgraph`, icon: NodeIndexOutlined, label: "调用链" },
      { key: `${base}/chat`, icon: MessageOutlined, label: "对话" },
      { key: `${base}/cards`, icon: BookOutlined, label: "知识卡" },
    ],
    menuGroups: [],
  };
}
