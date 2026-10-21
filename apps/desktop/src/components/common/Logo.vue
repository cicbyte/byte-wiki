<script setup lang="ts">
/**
 * byte-wiki Logo（翻开的书 · 透明背景 · 动态着色）
 *
 * 摊开的双页 + 中缝留隙 + 前页文字行镂空，小尺寸下书形轮廓仍可辨。
 * （旧版层叠卡片为 byte-stash 的 stash 语义，fork 继承——已换为 wiki 书本）
 *
 * 着色：SVG 用 fill="currentColor"，颜色由 CSS color 控制——
 *   - 不传 color：继承父元素文字色（浅主题自动深、深主题自动浅）
 *   - 传 color：用指定色（如 var(--color-primary)）
 *
 * 本体透明无背景方块，任意背景上都能融入。
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
      <!-- 右页文字行镂空 mask：白=保留，黑=挖空（透出背景） -->
      <mask :id="maskId">
        <rect width="64" height="64" fill="white" />
        <rect x="37" y="24" width="10" height="2.2" rx="1.1" fill="black" />
        <rect x="40" y="29" width="7" height="2.2" rx="1.1" fill="black" />
      </mask>
    </defs>
    <!-- 左页（书页自然摊开：外沿高、内沿低、内底沿书脊下沉；opacity 区分双页） -->
    <path
      d="M12 16 L31 19 L31 44 Q31 47 28.5 46.4 L13 42.4 Q12 42.1 12 41 Z"
      fill="currentColor"
      opacity="0.55"
    />
    <!-- 右页（镜像；文字行经 mask 镂空） -->
    <path
      d="M52 16 L33 19 L33 44 Q33 47 35.5 46.4 L51 42.4 Q52 42.1 52 41 Z"
      fill="currentColor"
      :mask="`url(#${maskId})`"
    />
  </svg>
</template>
