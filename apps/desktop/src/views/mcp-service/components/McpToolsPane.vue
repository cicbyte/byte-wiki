<script setup lang="ts">
/**
 * 工具列表面板（mcp-service → 工具列表 tab）
 *
 * 自管只读视图 + 本地展开状态：
 * - 工具定义来自 ../lib/constants 的 readTools / writeTools
 * - expandedTools 仅为本面板交互状态，单一来源，无外部依赖
 */
import { ref } from "vue";
import { SearchOutlined, EditOutlined, CaretRightOutlined } from "@ant-design/icons-vue";
import { readTools, writeTools } from "../lib/constants";

/** 工具展开状态 */
const expandedTools = ref<Set<string>>(new Set());
function toggleTool(name: string) {
  const next = new Set(expandedTools.value);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  expandedTools.value = next;
}
</script>

<template>
  <div class="tool-table-head">
    <span class="col-expand"></span>
    <span class="col-name">工具名</span>
    <span class="col-desc">说明</span>
  </div>
  <div class="tool-group">
    <div class="tool-group-label"><SearchOutlined /> 读取（{{ readTools.length }}）</div>
    <template v-for="t in readTools" :key="t.name">
      <div class="tool-row" @click="toggleTool(t.name)">
        <span class="col-expand"><CaretRightOutlined class="expand-icon" :class="{ open: expandedTools.has(t.name) }" /></span>
        <code class="tool-name">{{ t.name }}</code>
        <span class="tool-desc">{{ t.desc }}</span>
      </div>
      <div v-if="expandedTools.has(t.name)" class="tool-detail">
        <div v-if="t.params.length" class="param-table">
          <div class="param-row param-head"><span>参数</span><span>类型</span><span>必填</span><span>说明</span></div>
          <div v-for="p in t.params" :key="p.name" class="param-row">
            <span class="param-name">{{ p.name }}</span>
            <span class="param-type" :class="'type-' + p.type">{{ p.type }}</span>
            <span class="param-required"><span :class="p.required ? 'req-yes' : 'req-no'">{{ p.required ? "是" : "否" }}</span></span>
            <span class="param-desc">{{ p.desc }}</span>
          </div>
        </div>
        <div v-else class="param-empty">无参数</div>
      </div>
    </template>
  </div>
  <div class="tool-group">
    <div class="tool-group-label write"><EditOutlined /> 修改（{{ writeTools.length }}）</div>
    <template v-for="t in writeTools" :key="t.name">
      <div class="tool-row" @click="toggleTool(t.name)">
        <span class="col-expand"><CaretRightOutlined class="expand-icon" :class="{ open: expandedTools.has(t.name) }" /></span>
        <code class="tool-name">{{ t.name }}</code>
        <span class="tool-desc">{{ t.desc }}</span>
      </div>
      <div v-if="expandedTools.has(t.name)" class="tool-detail">
        <div class="param-table">
          <div class="param-row param-head"><span>参数</span><span>类型</span><span>必填</span><span>说明</span></div>
          <div v-for="p in t.params" :key="p.name" class="param-row">
            <span class="param-name">{{ p.name }}</span>
            <span class="param-type" :class="'type-' + p.type">{{ p.type }}</span>
            <span class="param-required"><span :class="p.required ? 'req-yes' : 'req-no'">{{ p.required ? "是" : "否" }}</span></span>
            <span class="param-desc">{{ p.desc }}</span>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
/* 工具列表 */
.tool-table-head { display: grid; grid-template-columns: 28px 180px 1fr; gap: 10px; padding: 6px 10px; font-size: 11px; font-weight: 600; color: var(--text-tertiary); border-bottom: 1px solid var(--border-light); text-transform: uppercase; letter-spacing: 0.04em; }
.tool-group { margin-bottom: var(--spacing-md); }
.tool-group-label { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; color: var(--color-primary); margin: 10px 0 4px; padding-bottom: 4px; border-bottom: 1px solid var(--border-light); }
.tool-group-label.write { color: var(--color-warning); }
.tool-row { display: grid; grid-template-columns: 28px 180px 1fr; gap: 10px; padding: 8px 10px; align-items: center; border-radius: var(--radius-xs); cursor: pointer; transition: background var(--transition-fast); }
.tool-row:hover { background: var(--bg-elevated); }
.col-expand { display: flex; align-items: center; justify-content: center; }
.expand-icon { font-size: 12px; color: var(--text-tertiary); transition: transform var(--transition-fast); }
.expand-icon.open { transform: rotate(90deg); color: var(--color-primary); }
.tool-name { font-family: "JetBrains Mono", monospace; font-size: 12px; color: var(--color-primary); font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tool-desc { font-size: 12px; color: var(--text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tool-detail { padding: 4px 10px 10px 38px; margin-bottom: 2px; }
.param-table { border: 1px solid var(--border-light); border-radius: var(--radius-sm); overflow: hidden; }
.param-row { display: grid; grid-template-columns: 140px 70px 50px 1fr; gap: 10px; padding: 6px 10px; font-size: 12px; align-items: center; border-bottom: 1px solid var(--border-light); }
.param-row:last-child { border-bottom: none; }
.param-row.param-head { background: var(--bg-input); font-size: 11px; font-weight: 600; color: var(--text-tertiary); text-transform: uppercase; letter-spacing: 0.03em; }
.param-name { font-family: "JetBrains Mono", monospace; color: var(--text-primary); font-weight: 500; }
.param-type { font-family: "JetBrains Mono", monospace; font-size: 11px; padding: 1px 6px; border-radius: 4px; text-align: center; }
.type-string { color: var(--color-primary); background: var(--color-primary-bg); }
.type-number { color: var(--color-success); background: var(--success-bg); }
.type-boolean { color: var(--color-warning); background: var(--warning-bg); }
.param-required { text-align: center; }
.req-yes { color: var(--color-danger); font-weight: 600; font-size: 11px; }
.req-no { color: var(--text-disabled); font-size: 11px; }
.param-desc { color: var(--text-tertiary); }
.param-empty { font-size: 12px; color: var(--text-disabled); padding: 8px 0; }
</style>
