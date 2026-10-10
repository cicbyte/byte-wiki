<script setup lang="ts">
/**
 * 偏好分区（设置 → 偏好）
 *
 * 导入时排除的目录：a-select(tags 模式) + 保存 / 重置为默认。
 * 状态在本组件内部创建（沿用既有子组件约定）。
 */
import { ref, onMounted } from "vue";
import { message } from "ant-design-vue";
import { api } from "@/api";
import { DEFAULT_EXCLUDE_DIRS } from "../lib/constants";

const excludeDirs = ref<string[]>([]);

onMounted(async () => {
  try {
    excludeDirs.value = await api.getExcludeDirs();
  } catch {
    excludeDirs.value = [...DEFAULT_EXCLUDE_DIRS];
  }
});

/** 保存排除目录偏好 */
async function saveExcludeDirs() {
  try {
    await api.setExcludeDirs(excludeDirs.value);
    message.success("偏好已保存");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "保存失败");
  }
}

/** 重置为预置默认排除列表 */
function resetExcludeDirs() {
  excludeDirs.value = [...DEFAULT_EXCLUDE_DIRS];
}
</script>

<template>
  <section class="pane">
    <h2 class="pane-title">偏好</h2>

    <!-- 导入排除目录 -->
    <div class="storage-section">
      <div class="section-head">导入时排除的目录</div>
      <a-select
        v-model:value="excludeDirs"
        mode="tags"
        :token-separators="[',']"
        placeholder="输入目录名后回车添加，如 node_modules"
        style="width: 100%"
        :open="false"
      />
      <p class="hint">
        导入项目或往项目内导入文件夹时，会按目录名精确匹配并递归跳过这些目录（含其全部内容）。
        已预置常见依赖、构建产物与 IDE 配置目录。CLI 可用 <code>--exclude</code> 临时覆盖。
      </p>
      <div class="backup-actions">
        <button class="btn primary" @click="saveExcludeDirs">
          保存
        </button>
        <button class="btn" @click="resetExcludeDirs">
          重置为默认
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.pane-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: var(--spacing-md);
}

.storage-section {
  padding: var(--spacing-md) 0;
  border-bottom: 1px solid var(--border-light);
}
.storage-section:last-child {
  border-bottom: none;
}
.section-head {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 10px;
}
.backup-actions {
  display: flex;
  gap: var(--spacing-sm);
}

.hint {
  color: var(--text-tertiary);
  font-size: 12px;
  margin-top: 8px;
  line-height: 1.6;
}
.hint code {
  background: var(--bg-input);
  padding: 1px 5px;
  border-radius: 3px;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
}
</style>
