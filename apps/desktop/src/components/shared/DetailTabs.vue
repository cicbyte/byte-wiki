<script setup lang="ts">
/**
 * DetailTabs —— 详情区 tab 切换条
 *
 * v-model(modelValue) 控制当前 tab；tabs 提供 key/label/可选 count。
 */
defineProps<{
  tabs: { key: string; label: string; count?: string | number }[];
  modelValue: string;
}>();
defineEmits<{ (e: "update:modelValue", key: string): void }>();
</script>

<template>
  <div class="detail-tabs">
    <button
      v-for="t in tabs"
      :key="t.key"
      class="detail-tab"
      :class="{ active: modelValue === t.key }"
      @click="$emit('update:modelValue', t.key)"
    >
      {{ t.label }}
      <span v-if="t.count !== undefined" class="tab-count">{{ t.count }}</span>
    </button>
  </div>
</template>

<style scoped>
.detail-tabs {
  display: flex;
  gap: 2px;
  padding: 0 var(--spacing-md);
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.detail-tab {
  padding: 8px 14px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 13px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: all var(--transition-fast);
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.detail-tab:hover {
  color: var(--text-primary);
}
.detail-tab.active {
  color: var(--color-primary);
  border-bottom-color: var(--color-primary);
  font-weight: 600;
}
.tab-count {
  font-size: 11px;
  font-family: "JetBrains Mono", monospace;
  background: var(--bg-input);
  padding: 0 5px;
  border-radius: 8px;
}
</style>
