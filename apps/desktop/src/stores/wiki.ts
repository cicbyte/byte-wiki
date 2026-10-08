/**
 * Wiki 仓库状态：当前打开的仓库 + 最近列表
 *
 * 遵循现有 Pinia store 模式；当前仓库不持久化（每次启动重新打开/从最近列表恢复）。
 */
import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/api";
import type { RepoInfo } from "@/api/repo";

export const useWikiStore = defineStore("wiki", () => {
  const currentRepo = ref<RepoInfo | null>(null);
  const recentRepos = ref<RepoInfo[]>([]);
  const opening = ref(false);

  async function refreshRecent() {
    try {
      recentRepos.value = await api.recentRepos(10);
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

  function closeRepo() {
    currentRepo.value = null;
  }

  return { currentRepo, recentRepos, opening, openRepo, closeRepo, refreshRecent };
});
