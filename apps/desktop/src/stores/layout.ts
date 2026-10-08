/**
 * 布局 store（对齐 byte-wiki-桌面布局设计.md §8）
 * 只管纯布局：sidebar 折叠态 + statusbar 状态项。
 */
import { defineStore } from "pinia";
import { ref } from "vue";

export interface StatusItem {
  key: string;
  text: string;
}

export const useLayoutStore = defineStore("layout", () => {
  const sidebarCollapsed = ref(false);
  const statusItems = ref<StatusItem[]>([]);

  /** 全局编辑/阅读模式（footer 切换，所有编辑器共用） */
  const editMode = ref(false);

  function toggleSidebar() {
    sidebarCollapsed.value = !sidebarCollapsed.value;
  }

  function setStatusItems(items: StatusItem[]) {
    statusItems.value = items;
  }

  function toggleEditMode() {
    editMode.value = !editMode.value;
  }

  return { sidebarCollapsed, statusItems, editMode, toggleSidebar, setStatusItems, toggleEditMode };
});
