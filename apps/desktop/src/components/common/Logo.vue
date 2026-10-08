<script setup lang="ts">
/**
 * byte-wiki Logo（层叠卡片 · 透明背景 · 动态着色）
 *
 * 三张层叠卡片错落旋转，靠 opacity 区分层次；前卡片用 mask 挖空两条标题行，
 * 在小尺寸下也有辨识度。
 *
 * 着色：SVG 用 fill="currentColor"，颜色由 CSS color 控制——
 *   - 不传 color：继承父元素文字色（浅主题自动深、深主题自动浅）
 *   - 传 color：用指定色（如主色蓝 var(--color-primary)）
 *
 * 本体透明无背景方块，因此不会有黑边/底色问题，任意背景上都能融入。
 */
import { useId } from "vue";

withDefaults(defineProps<{
  /** 图标尺寸（px），正方形 */
  size?: number;
  /** 颜色（CSS color 值），不传则继承父元素 currentColor */
  color?: string;
}>(), {
  size: 26,
  color: "currentColor",
});

// 每个实例唯一 mask id，避免多实例 mask 引用错乱
const maskId = `logo-mask-${useId()}`;
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 64 64"
    :style="{ color }"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-label="byte-wiki"
    role="img"
  >
    <defs>
      <!-- 前卡片标题行镂空 mask：白=保留，黑=挖空（透出背景） -->
      <mask :id="maskId">
        <rect width="64" height="64" fill="white" />
        <rect x="25" y="23" width="16" height="2.4" rx="1.2" fill="black" />
        <rect x="25" y="29" width="11" height="2.4" rx="1.2" fill="black" />
      </mask>
    </defs>
    <!-- 后卡片（旋转 -12°，最淡） -->
    <rect x="18" y="15" width="28" height="34" rx="4" fill="currentColor" opacity="0.28" transform="rotate(-12 32 32)" />
    <!-- 中卡片（旋转 -3°，中等） -->
    <rect x="18" y="15" width="28" height="34" rx="4" fill="currentColor" opacity="0.5" transform="rotate(-3 32 32)" />
    <!-- 前卡片（不旋转，实色 + 标题行镂空） -->
    <rect x="18" y="15" width="28" height="34" rx="4" fill="currentColor" :mask="`url(#${maskId})`" />
  </svg>
</template>
