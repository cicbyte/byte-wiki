/**
 * 侧栏分组菜单展开状态（Sidebar 持有，经 props 下发 SidebarNav）
 *
 * expanded 为整体替换的 Set（Vue 对 Set 原地变更不触发响应，重建触发）；
 * 默认展开首个分组，其余手风琴式点按切换。
 */
import { ref } from "vue";

export function useMenuExpansion(initial: string[] = ["service"]) {
  const expanded = ref<Set<string>>(new Set(initial));

  function toggleGroup(key: string) {
    const next = new Set(expanded.value);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expanded.value = next;
  }

  return { expanded, toggleGroup };
}
