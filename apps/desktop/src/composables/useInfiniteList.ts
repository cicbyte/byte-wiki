/**
 * 无限滚动列表状态机（泛型，片段 / 笔记 / 项目三页共用）
 *
 * - `items`：已加载的全部项（逐页累加）
 * - `total`：满足过滤条件的总数（后端 count，仅用于 footer 展示）
 * - `hasMore`：是否还有下一页。判据 = 本次返回条数 < pageSize（可靠，不依赖 total）
 * - `loadMore()`：加载下一页（滚动触底时调用；并发/无更多时自动忽略）
 * - `reload()`：清空并加载第一页（切换分类 / 筛选时调用）
 *
 * 不耦合具体类型与数据源，由各页传入 `fetcher(offset, limit)`。
 *
 * 用 `epoch` 计数防止「切分类时上一页请求未完成」的竞态：reload 自增 epoch，
 * 途中的旧 loadMore 完成后若发现 epoch 已变即丢弃结果。
 *
 * `items` 用 shallowRef（列表项为对象，深层响应开销大且无必要），因此只对
 * `.value` 整体赋值才触发更新。调用方对本列表的增删改（unshift / patch / filter）
 * 也必须用整体替换生成新数组，**不能用索引赋值** `arr[i] = x`。
 */
import { ref, shallowRef } from "vue";

export function useInfiniteList<T>(
  fetcher: (offset: number, limit: number) => Promise<{ items: T[]; total: number }>,
  opts: { pageSize: number },
) {
  const items = shallowRef<T[]>([]);
  const total = ref(0);
  const hasMore = ref(true);
  const loading = ref(true); // 初始 true：避免挂载首帧（items 空 + loading 假）闪现「暂无 xx」空状态

  let page = 0; // 已加载页数（非响应式，仅内部控制）
  let epoch = 0; // 每次 reload 自增，用于丢弃过期请求结果

  async function loadMore() {
    if (loading.value || !hasMore.value) return;
    const myEpoch = epoch;
    loading.value = true;
    try {
      const offset = page * opts.pageSize;
      const res = await fetcher(offset, opts.pageSize);
      if (myEpoch !== epoch) return; // 期间发生了 reload，丢弃本次结果
      // 整体替换（shallowRef 仅对 .value 赋值触发响应）
      items.value = items.value.concat(res.items);
      total.value = res.total;
      page += 1;
      // 本次不足一页 → 没有更多了
      if (res.items.length < opts.pageSize) {
        hasMore.value = false;
      }
    } finally {
      if (myEpoch === epoch) loading.value = false;
    }
  }

  async function reload() {
    epoch += 1;
    page = 0;
    items.value = [];
    hasMore.value = true;
    loading.value = false;
    await loadMore();
  }

  return { items, total, hasMore, loading, loadMore, reload };
}
