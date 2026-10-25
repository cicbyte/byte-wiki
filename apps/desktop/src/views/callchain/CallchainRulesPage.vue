<script setup lang="ts">
/**
 * 调用链规则（抽屉表单形态；#733 后从独立设置页收进调用链画布 ⚙ 抽屉）
 *
 * 形态：a-tabs 分类 + 表格行内管理（入口排除 / 文件排除是 glob 列表，
 * 自定义入口正则为单值）——规则按项目独立保存。
 *
 * - 文件排除：命中的文件不进调用图（体积/噪声治理）
 * - 入口排除：文件仍在图中，但不标入口（默认治 build.rs/examples/benches/tests
 *   里的多 main 与装饰器命令噪声）
 * - 自定义入口正则：qualname 命中即标入口（叠加内置路由/约定识别）
 * 保存后对新生成的调用链生效（既有产物需重新生成）。
 */
import { onMounted, ref } from "vue";
import { DeleteOutlined, PlusOutlined } from "@ant-design/icons-vue";
import { message } from "ant-design-vue";
import { callchainApi } from "@/api/callchain";
import type { CallchainRules } from "@/api/callchain";

/** 归属仓库（规则按项目管理——各仓库语义决策互不相干） */
const props = defineProps<{ repoPath: string }>();

const loading = ref(false);
const saving = ref(false);
/** glob 列表（表格行内编辑；空行保存时过滤） */
const entryExcludes = ref<string[]>([]);
const excludes = ref<string[]>([]);
const entryRegex = ref("");

const globColumns = [
  { title: "模式（gitignore 风格 glob）", dataIndex: "pattern" },
  { title: "", key: "op", width: 56 },
];

/** 表格行源（key=索引即可，pattern 双向绑定到数组） */
const rowsOf = (arr: string[]) => arr.map((p, i) => ({ key: i, pattern: p }));
const countOf = (arr: string[]) => arr.filter(Boolean).length;

async function load() {
  loading.value = true;
  try {
    const r = await callchainApi.getRules(props.repoPath);
    excludes.value = [...r.exclude_globs];
    entryExcludes.value = [...r.entry_exclude_globs];
    entryRegex.value = r.entry_regex ?? "";
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "读取规则失败");
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  try {
    if (entryRegex.value.trim()) {
      new RegExp(entryRegex.value.trim());
    }
    const clean = (a: string[]) => a.map((s) => s.trim()).filter(Boolean);
    const rules: CallchainRules = {
      exclude_globs: clean(excludes.value),
      entry_exclude_globs: clean(entryExcludes.value),
      entry_regex: entryRegex.value.trim() || null,
    };
    await callchainApi.setRules(props.repoPath, rules);
    message.success("规则已保存，重新生成调用链后生效");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "保存失败");
  } finally {
    saving.value = false;
  }
}

function addRow(arr: string[]) {
  arr.push("");
}
function removeRow(arr: string[], i: number) {
  arr.splice(i, 1);
}

function resetDefaults() {
  excludes.value = [];
  entryExcludes.value = ["build.rs", "examples/**", "benches/**", "tests/**", "test/**"];
  entryRegex.value = "";
}

onMounted(load);
</script>

<template>
  <div class="rules-form">
    <div v-if="loading" class="rules-loading">读取规则中…</div>
    <template v-else>
      <p class="rules-intro">
        规则随项目独立保存；保存后对新生成的调用链生效（既有产物需重新生成）。
      </p>

      <a-tabs size="small" class="rules-tabs">
        <a-tab-pane key="entry" :tab="`入口排除${countOf(entryExcludes) ? `（${countOf(entryExcludes)}）` : ''}`">
          <p class="sec-desc">
            文件仍在图中，仅不标入口——治理「一堆 main / 测试装饰器命令」的入口噪声（build 脚本、示例、基准、测试目录默认不作为入口）。
          </p>
          <a-table
            :columns="globColumns"
            :data-source="rowsOf(entryExcludes)"
            :pagination="false"
            size="small"
            :locale="{ emptyText: '暂无排除项，点「添加」开始' }"
          >
            <template #bodyCell="{ column, index }">
              <template v-if="column.dataIndex === 'pattern'">
                <a-input
                  v-model:value="entryExcludes[index]"
                  size="small"
                  placeholder="build.rs / examples/**"
                />
              </template>
              <template v-else-if="column.key === 'op'">
                <button class="row-del" title="删除此行" @click="removeRow(entryExcludes, index)">
                  <DeleteOutlined />
                </button>
              </template>
            </template>
          </a-table>
          <button class="row-add" @click="addRow(entryExcludes)"><PlusOutlined /> 添加</button>
        </a-tab-pane>

        <a-tab-pane key="exclude" :tab="`文件排除${countOf(excludes) ? `（${countOf(excludes)}）` : ''}`">
          <p class="sec-desc">
            命中的文件不进调用图——体积与噪声治理（如 vendored 依赖、生成代码）。默认为空；目录级排除仍走索引排除规则。
          </p>
          <a-table
            :columns="globColumns"
            :data-source="rowsOf(excludes)"
            :pagination="false"
            size="small"
            :locale="{ emptyText: '暂无排除项，点「添加」开始' }"
          >
            <template #bodyCell="{ column, index }">
              <template v-if="column.dataIndex === 'pattern'">
                <a-input
                  v-model:value="excludes[index]"
                  size="small"
                  placeholder="vendor/** / **/*.generated.ts"
                />
              </template>
              <template v-else-if="column.key === 'op'">
                <button class="row-del" title="删除此行" @click="removeRow(excludes, index)">
                  <DeleteOutlined />
                </button>
              </template>
            </template>
          </a-table>
          <button class="row-add" @click="addRow(excludes)"><PlusOutlined /> 添加</button>
        </a-tab-pane>

        <a-tab-pane key="regex" tab="自定义入口正则">
          <p class="sec-desc">
            qualname 命中即标入口，叠加在内置识别（路由/约定入口/装饰器命令）之上。如 <code>^(run|start)_</code> 把 run_/start_ 前缀函数标为入口。
          </p>
          <a-input v-model:value="entryRegex" placeholder="^(run|start)_" allow-clear />
        </a-tab-pane>
      </a-tabs>
    </template>

    <!-- 吸底操作条 -->
    <div class="rules-foot">
      <button class="btn" :disabled="saving || loading" @click="resetDefaults">恢复默认</button>
      <button class="btn primary" :disabled="saving || loading" @click="save">
        {{ saving ? "保存中…" : "保存" }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.rules-form {
  display: flex;
  flex-direction: column;
  min-height: 100%;
}
.rules-loading {
  padding: 40px 0;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
}
.rules-intro {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.6;
}
.sec-desc {
  margin: 2px 0 10px;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
}
.sec-desc code {
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-xs);
  padding: 0 4px;
  font-size: 11px;
}
.row-add {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-top: 8px;
  padding: 4px 12px;
  border: 1px dashed var(--border-base);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}
.row-add:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}
.row-del {
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 12px;
  padding: 3px;
  border-radius: 4px;
  cursor: pointer;
}
.row-del:hover {
  color: var(--color-danger, #e5484d);
}
/* 吸底操作条（抽屉滚动时贴底常驻） */
.rules-foot {
  position: sticky;
  bottom: 0;
  margin-top: auto;
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 12px 0;
  background: var(--bg-elevated, var(--bg-card));
  border-top: 1px solid var(--border-light);
}
</style>
