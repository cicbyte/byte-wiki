<script setup lang="ts">
/**
 * 调用链规则管理：入口噪声治理与自定义识别
 *
 * - 文件排除：命中的文件不进调用图（体积/噪声治理）
 * - 入口排除：文件仍在图中，但不标入口（默认治 build.rs/examples/benches/tests
 *   里的多 main 与装饰器命令噪声）
 * - 自定义入口正则：qualname 命中即标入口（叠加内置路由/约定识别）
 * 保存后对新生成的调用链生效（既有产物需重新生成）。
 */
import { onMounted, ref } from "vue";

/** 归属仓库（规则按项目管理——各仓库语义决策互不相干） */
const props = defineProps<{ repoPath: string }>();
import { message } from "ant-design-vue";
import { callchainApi } from "@/api/callchain";
import type { CallchainRules } from "@/api/callchain";

const loading = ref(false);
const saving = ref(false);
const excludeText = ref("");
const entryExcludeText = ref("");
const entryRegex = ref("");

async function load() {
  loading.value = true;
  try {
    const r = await callchainApi.getRules(props.repoPath);
    excludeText.value = r.exclude_globs.join("\n");
    entryExcludeText.value = r.entry_exclude_globs.join("\n");
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
    const rules: CallchainRules = {
      exclude_globs: split(excludeText.value),
      entry_exclude_globs: split(entryExcludeText.value),
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

function split(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

function resetDefaults() {
  excludeText.value = "";
  entryExcludeText.value = ["build.rs", "examples/**", "benches/**", "tests/**", "test/**"].join("\n");
  entryRegex.value = "";
}

onMounted(load);
</script>

<template>
  <div class="cg-rules">
    <div class="rules-head">
      <h2>调用链规则</h2>
      <p class="sub">入口噪声治理与自定义识别 · 保存后对新生成的调用链生效（既有产物需重新生成）</p>
    </div>

    <section class="rule-card" v-if="!loading">
      <h3>入口排除 <span class="hint">文件仍在图中，仅不标入口</span></h3>
      <p class="desc">
        治理「一堆 main / 测试装饰器命令」的入口噪声：build 脚本、示例、基准、测试目录默认不作为入口。
        每行一条 gitignore 风格 glob。
      </p>
      <a-textarea v-model:value="entryExcludeText" :rows="5" placeholder="build.rs&#10;examples/**&#10;tests/**" />

      <h3 style="margin-top: 20px">文件排除 <span class="hint">命中的文件不进调用图</span></h3>
      <p class="desc">体积与噪声治理（如 vendored 依赖、生成代码）。默认为空；目录级排除仍走索引排除规则。</p>
      <a-textarea v-model:value="excludeText" :rows="4" placeholder="vendor/**&#10;**/*.generated.ts" />

      <h3 style="margin-top: 20px">自定义入口正则 <span class="hint">qualname 命中即标入口</span></h3>
      <p class="desc">叠加在内置识别（路由/约定入口/装饰器命令）之上。如 <code>^(run|start)_</code> 把 run_/start_ 前缀函数标为入口。</p>
      <a-input v-model:value="entryRegex" placeholder="^(run|start)_" allow-clear />

      <div class="actions">
        <button class="btn" @click="resetDefaults">恢复默认</button>
        <button class="btn primary" :disabled="saving" @click="save">{{ saving ? "保存中…" : "保存规则" }}</button>
      </div>
    </section>
    <p v-else class="loading">读取规则中…</p>
  </div>
</template>

<style scoped>
.cg-rules {
  max-width: 760px;
  margin: 0 auto;
  padding: 28px 24px;
}
.rules-head h2 {
  margin: 0 0 6px;
  font-size: 18px;
  color: var(--text-primary);
}
.rules-head .sub {
  margin: 0 0 20px;
  font-size: 12.5px;
  color: var(--text-tertiary);
}
.rule-card {
  background: var(--bg-card);
  border: 1px solid var(--border-card);
  border-radius: var(--radius-lg);
  padding: 20px 22px;
}
.rule-card h3 {
  margin: 0 0 6px;
  font-size: 14px;
  color: var(--text-primary);
}
.rule-card h3 .hint {
  font-size: 11.5px;
  font-weight: 400;
  color: var(--text-tertiary);
  margin-left: 8px;
}
.desc {
  font-size: 12.5px;
  color: var(--text-secondary);
  line-height: 1.7;
  margin: 0 0 10px;
}
.desc code {
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-surface);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-xs);
  padding: 0 4px;
  font-size: 11.5px;
}
.actions {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
  margin-top: 22px;
}
.loading {
  text-align: center;
  color: var(--text-tertiary);
  padding: 40px 0;
}
</style>
