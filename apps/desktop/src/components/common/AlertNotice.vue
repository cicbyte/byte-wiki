<script setup lang="ts">
import { computed } from "vue";
import {
  WarningOutlined,
  CheckCircleOutlined,
  ExclamationCircleOutlined,
} from "@ant-design/icons-vue";
/**
 * 统一提示条组件（全局复用）
 *
 * 替代散落各页的 ⚠️ emoji + 裸文字 span（如 .warn-text），统一视觉规范：
 * 左侧语义线性图标 + 浅色背景 + 左边框，颜色复用 --color-warning/--warning-bg 等语义变量，
 * 深浅色模式自动适配。emoji ⚠️ 跨平台渲染不一致且与 Ant Design 线性图标风格冲突，故弃用。
 *
 * 正文用 --text-secondary（中性可读，避免语义色文字配同色浅底导致对比度不足），
 * 语义级别由图标 + 左边框 + 背景三重传达（与 antd Alert 一致）。
 *
 * Props:
 * - type: warning（默认）/ success / danger
 * - icon: 自定义图标组件（不传则按 type 自动选）
 *
 * 用法：
 *   <AlertNotice type="warning">当前无鉴权，绑定 <code>0.0.0.0</code> 时任何人都可访问。</AlertNotice>
 *   <AlertNotice type="success">鉴权已开启，调用需携带 API Key。</AlertNotice>
 */
const props = withDefaults(defineProps<{
  type?: "warning" | "success" | "danger";
  /** 自定义图标组件（不传则按 type 自动选） */
  icon?: any;
}>(), {
  type: "warning",
  icon: null,
});

const defaultIcon = computed(() => {
  switch (props.type) {
    case "success":
      return CheckCircleOutlined;
    case "danger":
      return ExclamationCircleOutlined;
    default:
      return WarningOutlined;
  }
});
</script>

<template>
  <div class="alert-notice" :class="type">
    <component :is="icon || defaultIcon" class="alert-icon" />
    <div class="alert-content"><slot /></div>
  </div>
</template>

<style scoped>
/* 通用结构：图标 + 内容横向排列，左边框 + 浅底 + 圆角。外边距由调用方按需控制（组件保持中性）。 */
.alert-notice {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 12px;
  border-left: 3px solid transparent;
  border-radius: var(--radius-sm);
}
.alert-icon {
  font-size: 14px;
  margin-top: 1px;
  flex-shrink: 0;
}
.alert-content {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
}
.alert-content :deep(code) {
  background: var(--bg-input);
  padding: 1px 5px;
  border-radius: 3px;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
}

/* warning（默认） */
.alert-notice.warning {
  background: var(--warning-bg);
  border-left-color: var(--color-warning);
}
.alert-notice.warning .alert-icon { color: var(--color-warning); }

/* success */
.alert-notice.success {
  background: var(--success-bg);
  border-left-color: var(--color-success);
}
.alert-notice.success .alert-icon { color: var(--color-success); }

/* danger */
.alert-notice.danger {
  background: var(--danger-bg);
  border-left-color: var(--color-danger);
}
.alert-notice.danger .alert-icon { color: var(--color-danger); }
</style>
