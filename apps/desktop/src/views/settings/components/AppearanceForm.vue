<script setup lang="ts">
/**
 * 外观设置：5 套预设 + 自定义色板（基础 token + 状态色）+ 区域覆盖 + 导入导出
 *
 * 对齐 ax 三层架构：
 * - 预设：每套浅/深两态手工精调（低饱和纸感），自定义由底色派生协调中性色
 * - 自定义：编辑基础 token + 状态色（深/浅各一套），primary 变化时自动重算派生项
 * - 区域覆盖：5 区域 × (背景/主文字/次文字)，跨预设保留
 * 改动即时生效（可视化拖动只更 CSS 变量；松开/JSON/导入时完整 apply + 持久化）。
 */
import { ref, computed, watch } from "vue";
import { message } from "ant-design-vue";
import { DownloadOutlined, ImportOutlined } from "@ant-design/icons-vue";
import { useThemeStore } from "@/stores/theme";
import ThemePreviewCard from "@/components/settings/ThemePreviewCard.vue";
import { PRESET_LIST, REGION_DEFS, buildPresetColors, deriveBaseLayer } from "@/themes/presets";
import { applyColorsToDOM } from "@/themes/token";
import type { ThemeColors, BuiltinPreset, PresetKey, ResolvedMode, RegionKey, RegionProp } from "@/themes/types";
import SnippetEditor from "@/components/editor/SnippetEditor.vue";

const themeStore = useThemeStore();

function onSelectPreset(key: BuiltinPreset | "custom") {
  themeStore.setPreset(key as PresetKey);
}

// ===== 自定义色板编辑（基础 token + 状态色；深/浅各一套本地副本） =====
const editTab = ref<ResolvedMode>(themeStore.isDark ? "dark" : "light");
const editDark = ref<ThemeColors>({ ...themeStore.custom.dark });
const editLight = ref<ThemeColors>({ ...themeStore.custom.light });

const editColors = computed(() => (editTab.value === "dark" ? editDark.value : editLight.value));

/** 即时视觉：拖动只更新 CSS 变量（轻量高频；不触发 antd/Monaco/持久化） */
function applyLive() {
  applyColorsToDOM(editColors.value, editTab.value, themeStore.regions[editTab.value]);
}
/** 完整应用：持久化两边 + antd + Monaco */
function applyCustom() {
  themeStore.setCustomColors("light", { ...editLight.value });
  themeStore.setCustomColors("dark", { ...editDark.value });
}

// 进入自定义时，用 store.custom 初始化本地副本（首次即创建）
watch(
  () => themeStore.preset,
  (p) => {
    if (p !== "custom") return;
    editDark.value = { ...themeStore.custom.dark };
    editLight.value = { ...themeStore.custom.light };
    editTab.value = themeStore.isDark ? "dark" : "light";
  },
  { immediate: true },
);

function setEditColor(field: keyof ThemeColors, value: string) {
  const target = editColors.value;
  (target as any)[field] = value;
}
function onColorInput(field: keyof ThemeColors, value: string) {
  setEditColor(field, value);
  applyLive();
}
function onColorCommit() {
  applyCustom();
}
function onHexCommit(field: keyof ThemeColors, value: string) {
  setEditColor(field, value);
  applyCustom();
}

function resetVariant() {
  editDark.value = buildPresetColors("paper", "dark");
  editLight.value = buildPresetColors("paper", "light");
  applyCustom();
  message.success("已重置为默认配色");
}
/** 基于当前编辑变体的页面底色自动派生协调色板（保留主色 + 状态色扩展） */
function onAutoFit() {
  const m = editTab.value;
  const cur = { ...editColors.value };
  const next = { ...cur, ...deriveBaseLayer(cur.bgBase, m, cur.primary) } as ThemeColors;
  if (m === "dark") editDark.value = next;
  else editLight.value = next;
  themeStore.setCustomColors(m, next);
  message.success("已按底色自动协调");
}

// ===== 区域覆盖 =====
const REGION_PROPS: { prop: RegionProp; label: string }[] = [
  { prop: "bg", label: "背景" },
  { prop: "text", label: "主文字" },
  { prop: "textSub", label: "次文字" },
];
function regionVal(key: RegionKey, prop: RegionProp): string {
  return themeStore.regionValue(editTab.value, key, prop);
}
function onRegionInput(key: RegionKey, prop: RegionProp, val: string) {
  themeStore.setRegionToken(editTab.value, key, prop, val);
}

// ===== 字段组（可视化） =====
const TOKEN_GROUPS: { label: string; fields: { key: keyof ThemeColors; label: string }[] }[] = [
  { label: "主色", fields: [{ key: "primary", label: "主题色" }] },
  { label: "背景", fields: [
    { key: "bgBase", label: "页面底色" },
    { key: "bgSurface", label: "容器底色" },
    { key: "bgInput", label: "输入框背景" },
    { key: "bgElevated", label: "浮层背景" },
  ]},
  { label: "文字", fields: [
    { key: "textPrimary", label: "主文字" },
    { key: "textSecondary", label: "次要文字" },
    { key: "textTertiary", label: "弱化文字" },
    { key: "textDisabled", label: "禁用文字" },
  ]},
  { label: "边框", fields: [
    { key: "borderBase", label: "主边框" },
    { key: "borderLight", label: "弱边框" },
  ]},
  { label: "主色状态", fields: [
    { key: "primaryHover", label: "主色 · 悬停" },
    { key: "primaryActive", label: "主色 · 按下" },
    { key: "primaryBorder", label: "主色 · 边框" },
  ]},
  { label: "状态色", fields: [
    { key: "success", label: "成功" },
    { key: "warning", label: "警告" },
    { key: "danger", label: "危险" },
    { key: "info", label: "信息" },
  ]},
];

// ===== JSON 代码编辑 + 导入导出 =====
const editorMode = ref<"visual" | "json">("visual");
const jsonText = ref("");

function serializeTheme(): string {
  return JSON.stringify(themeStore.exportState(), null, 2);
}
function enterJsonMode() {
  jsonText.value = serializeTheme();
  editorMode.value = "json";
}
function applyJson() {
  const r = themeStore.applyRawState(jsonText.value);
  if (r.ok) {
    editDark.value = { ...themeStore.custom.dark };
    editLight.value = { ...themeStore.custom.light };
    message.success("已应用 JSON 主题");
  } else {
    message.error(r.error);
  }
}
function exportToFile() {
  const blob = new Blob([serializeTheme()], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "byte-wiki-theme.json";
  a.click();
  URL.revokeObjectURL(url);
}
function onImportFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => {
    const r = themeStore.applyRawState(reader.result);
    if (r.ok) {
      editDark.value = { ...themeStore.custom.dark };
      editLight.value = { ...themeStore.custom.light };
      message.success("主题已导入");
    } else {
      message.error(r.error);
    }
  };
  reader.readAsText(file);
  (e.target as HTMLInputElement).value = "";
}
</script>

<template>
  <div class="appearance-form">
    <a-divider orientation="left" plain>配色方案</a-divider>
    <div class="theme-grid">
      <ThemePreviewCard
        v-for="p in PRESET_LIST"
        :key="p.key"
        :preset="p.key"
        :label="p.label"
        :is-active="themeStore.preset === p.key"
        @select="onSelectPreset"
      />
      <!-- 自定义卡片（选中后下方展开编辑面板） -->
      <div
        class="custom-preview-card"
        :class="{ active: themeStore.preset === 'custom' }"
        :style="{
          background: themeStore.custom.dark.bgSurface,
          borderColor: themeStore.preset === 'custom'
            ? themeStore.custom.dark.primary
            : themeStore.custom.dark.borderBase,
        }"
        @click="onSelectPreset('custom')"
      >
        <div class="cpc-swatch" :style="{ background: themeStore.custom.dark.primary }" />
        <div class="cpc-body">
          <div class="cpc-name" :style="{ color: themeStore.custom.dark.textPrimary }">自定义</div>
          <div class="cpc-lines">
            <div class="cpc-line w70" :style="{ background: themeStore.custom.dark.textTertiary }" />
            <div class="cpc-line w50" :style="{ background: themeStore.custom.dark.textDisabled }" />
            <div class="cpc-line w60" :style="{ background: themeStore.custom.dark.primaryBg }" />
          </div>
          <div class="cpc-dots">
            <span class="cpc-dot" :style="{ background: themeStore.custom.dark.success }" />
            <span class="cpc-dot" :style="{ background: themeStore.custom.dark.warning }" />
            <span class="cpc-dot" :style="{ background: themeStore.custom.dark.danger }" />
          </div>
        </div>
      </div>
    </div>

    <!-- 自定义编辑（选中自定义时内联展开，改动即时生效） -->
    <div v-if="themeStore.preset === 'custom'" class="custom-editor">
      <div class="editor-head">
        <span class="editor-title">自定义主题配色</span>
        <div class="head-actions">
          <a-button size="small" @click="exportToFile"><DownloadOutlined /> 导出</a-button>
          <a-button size="small">
            <ImportOutlined /> 导入
            <input class="import-input" type="file" accept="application/json" @change="onImportFile" />
          </a-button>
          <div class="mode-tabs">
            <button class="mode-tab" :class="{ active: editorMode === 'visual' }" @click="editorMode = 'visual'">可视化</button>
            <button class="mode-tab" :class="{ active: editorMode === 'json' }" @click="enterJsonMode">JSON 代码</button>
          </div>
        </div>
      </div>

      <!-- 可视化编辑 -->
      <template v-if="editorMode === 'visual'">
        <div class="visual-subhead">
          <div class="editor-tabs">
            <button class="editor-tab" :class="{ active: editTab === 'dark' }" @click="editTab = 'dark'">深色</button>
            <button class="editor-tab" :class="{ active: editTab === 'light' }" @click="editTab = 'light'">浅色</button>
          </div>
          <span class="editor-hint">改动即时生效</span>
          <div class="subhead-actions">
            <a-button size="small" @click="onAutoFit">按底色自动协调</a-button>
            <a-button size="small" @click="resetVariant">重置</a-button>
          </div>
        </div>

        <div class="editor-body">
          <div v-for="group in TOKEN_GROUPS" :key="group.label" class="color-group">
            <div class="color-group-label">{{ group.label }}</div>
            <div class="color-row" v-for="f in group.fields" :key="f.key">
              <input type="color" :value="(editColors as any)[f.key] || '#000000'" class="color-picker" @input="onColorInput(f.key, ($event.target as HTMLInputElement).value)" @change="onColorCommit" />
              <span class="color-label">{{ f.label }}</span>
              <input type="text" :value="(editColors as any)[f.key] || ''" class="color-hex" @change="onHexCommit(f.key, ($event.target as HTMLInputElement).value)" />
            </div>
          </div>

          <!-- 区域覆盖 -->
          <div class="color-group">
            <div class="color-group-label">区域覆盖（独立于预设，跨预设保留）</div>
            <div v-for="reg in REGION_DEFS" :key="reg.key" class="region-block">
              <div class="region-label">{{ reg.label }}</div>
              <div class="color-row" v-for="rp in REGION_PROPS" :key="rp.prop">
                <input type="color" :value="regionVal(reg.key, rp.prop) || '#000000'" class="color-picker" @input="onRegionInput(reg.key, rp.prop, ($event.target as HTMLInputElement).value)" />
                <span class="color-label">{{ rp.label }}</span>
                <input type="text" :value="regionVal(reg.key, rp.prop) || ''" class="color-hex" disabled />
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- JSON 代码编辑 -->
      <template v-else>
        <div class="json-wrap">
          <div class="json-editor-wrap">
            <SnippetEditor v-model="jsonText" language="json" :auto-detect="false" />
          </div>
          <div class="json-footer">
            <a-button size="small" @click="enterJsonMode">刷新</a-button>
            <a-button size="small" type="primary" @click="applyJson">应用</a-button>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.appearance-form { padding: 8px 4px; flex: 1; display: flex; flex-direction: column; min-height: 0; }
.theme-grid { display: flex; flex-wrap: wrap; gap: 12px; }

/* 自定义卡片（对齐 ThemePreviewCard 尺寸） */
.custom-preview-card {
  position: relative;
  display: flex;
  gap: 8px;
  padding: 10px;
  width: 140px;
  height: 90px;
  border: 1px solid;
  border-radius: 8px;
  cursor: pointer;
  transition: transform 150ms ease;
  overflow: hidden;
}
.custom-preview-card:hover { transform: translateY(-1px); }
.custom-preview-card.active { box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.1); }
.cpc-swatch { width: 4px; border-radius: 2px; flex-shrink: 0; }
.cpc-body { display: flex; flex-direction: column; gap: 4px; flex: 1; min-width: 0; }
.cpc-name { font-size: 12px; font-weight: 600; }
.cpc-lines { display: flex; flex-direction: column; gap: 3px; margin-top: 2px; }
.cpc-line { height: 4px; border-radius: 2px; }
.cpc-line.w70 { width: 70%; }
.cpc-line.w50 { width: 50%; }
.cpc-line.w60 { width: 60%; }
.cpc-dots { display: flex; gap: 4px; margin-top: auto; }
.cpc-dot { width: 8px; height: 8px; border-radius: 50%; }

/* ===== 自定义编辑面板 ===== */
.custom-editor {
  margin-top: 16px;
  padding: 16px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-light);
  border-radius: 8px;
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.editor-head { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; flex-wrap: wrap; }
.editor-title { font-size: 14px; font-weight: 600; color: var(--text-primary); }
.head-actions { display: flex; align-items: center; gap: 8px; margin-left: auto; }
.mode-tabs { display: flex; gap: 2px; }
.mode-tab {
  padding: 4px 12px; border: 1px solid var(--border-base); border-radius: 4px;
  background: transparent; color: var(--text-tertiary); font-size: 12px; cursor: pointer;
}
.mode-tab.active { background: var(--color-primary); color: #fff; border-color: var(--color-primary); }

.visual-subhead { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; flex-wrap: wrap; }
.editor-tabs { display: flex; gap: 2px; }
.editor-tab {
  padding: 4px 12px; border: 1px solid var(--border-base); border-radius: 4px;
  background: transparent; color: var(--text-tertiary); font-size: 12px; cursor: pointer;
}
.editor-tab.active { background: var(--color-primary); color: #fff; border-color: var(--color-primary); }
.editor-hint { font-size: 11px; color: var(--text-tertiary); }
.subhead-actions { display: flex; gap: 8px; margin-left: auto; }

.editor-body { display: flex; flex-direction: column; gap: 20px; flex: 1; overflow-y: auto; min-height: 0; padding-right: 4px; }
.color-group { display: flex; flex-direction: column; gap: 8px; }
.color-group-label { font-size: 11px; font-weight: 600; color: var(--text-tertiary); text-transform: uppercase; letter-spacing: 0.05em; }
.color-row { display: flex; align-items: center; gap: 10px; }
.color-picker { width: 32px; height: 32px; border: 1px solid var(--border-base); border-radius: 4px; cursor: pointer; background: none; padding: 2px; }
.color-picker::-webkit-color-swatch-wrapper { padding: 0; }
.color-picker::-webkit-color-swatch { border: none; border-radius: 3px; }
.color-label { flex: 1; font-size: 13px; color: var(--text-secondary); }
.color-hex { width: 110px; border: 1px solid var(--border-light); border-radius: 3px; padding: 3px 6px; font-size: 12px; font-family: "JetBrains Mono", monospace; background: var(--bg-input); color: var(--text-secondary); }
.color-hex:focus { outline: none; border-color: var(--color-primary); }
.color-hex:disabled { opacity: 0.6; }

.region-block { padding: 8px 0 0; border-top: 1px dashed var(--border-light); }
.region-block:first-of-type { border-top: none; }
.region-label { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin-bottom: 4px; }

.json-wrap { display: flex; flex-direction: column; gap: 12px; flex: 1; min-height: 0; }
.json-editor-wrap {
  flex: 1; min-height: 200px; display: flex; flex-direction: column;
  border: 1px solid var(--border-base); border-radius: 4px; overflow: hidden;
}
.json-editor-wrap :deep(.snippet-editor) { flex: 1; height: auto; }
.json-footer { display: flex; gap: 8px; justify-content: flex-end; }

.import-input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
</style>
