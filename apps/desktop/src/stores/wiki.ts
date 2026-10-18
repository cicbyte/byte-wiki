/**
 * Wiki 仓库状态：当前项目 + 项目列表
 *
 * 两层 IA：项目列表（/）→ 项目空间（/repo/:id）。进入项目由 ensureRepo 定位。
 */
import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/api";
import type { RepoInfo } from "@/api/repo";

/** 文件定位请求（wiki 引用跳转 → 文件浏览页展开并滚到行） */
export interface RevealTarget {
  seq: number;
  path: string;
  startLine?: number;
  endLine?: number;
}

export const useWikiStore = defineStore("wiki", () => {
  const currentRepo = ref<RepoInfo | null>(null);
  const recentRepos = ref<RepoInfo[]>([]);
  const opening = ref(false);
  const revealTarget = ref<RevealTarget | null>(null);

  /** 请求在文件浏览页定位到某文件（可选行区间）；seq 保证同目标可重复触发 */
  function revealFile(path: string, startLine?: number, endLine?: number) {
    revealTarget.value = {
      seq: (revealTarget.value?.seq ?? 0) + 1,
      path,
      startLine,
      endLine,
    };
  }

  async function refreshRecent(limit = 50) {
    try {
      recentRepos.value = await api.recentRepos(limit);
    } catch {
      recentRepos.value = [];
    }
  }

  /** 打开仓库（校验 + 登记），失败抛出可读错误 */
  async function openRepo(path: string): Promise<RepoInfo> {
    opening.value = true;
    try {
      const info = await api.openRepo(path);
      currentRepo.value = info;
      await refreshRecent();
      return info;
    } finally {
      opening.value = false;
    }
  }

  /**
   * 项目空间进入：按 id 定位仓库并设为当前（列表缺失时刷新；仓库已失效返回 null）。
   * 已是当前仓库时也重开一次——刷新 git 分支（切分支后文档/索引跟随新分支）。
   */
  async function ensureRepo(id: number): Promise<RepoInfo | null> {
    if (currentRepo.value?.id === id) {
      try {
        return await openRepo(currentRepo.value.path);
      } catch {
        return null;
      }
    }
    if (!recentRepos.value.some((r) => r.id === id)) {
      await refreshRecent();
    }
    const hit = recentRepos.value.find((r) => r.id === id);
    if (!hit) return null;
    try {
      return await openRepo(hit.path);
    } catch {
      return null;
    }
  }

  function closeRepo() {
    currentRepo.value = null;
  }

  return { currentRepo, recentRepos, opening, revealTarget, revealFile, openRepo, ensureRepo, closeRepo, refreshRecent };
});
