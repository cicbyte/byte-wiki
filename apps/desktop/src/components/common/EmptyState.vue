<script setup lang="ts">
import { onMounted, ref } from "vue";
import gsap from "gsap";
import { motionDisabled } from "@/composables/useMotion";
/**
 * 统一空状态组件（全局复用）
 *
 * 替代散落各页的 panel-empty / empty-state / chart-empty / empty-list 等自定义实现，
 * 统一视觉规范：图标 28px、主文字 12px、副标题 11px、颜色 var(--text-disabled)。
 *
 * Props:
 * - text: 主文字（必填）
 * - hint: 副标题（可选，比主文字小、更淡）
 * - size: 'default'（面板级，居中撑满，absolute 定位） | 'small'（图表/侧栏内嵌，小图标）
 * - inline: true 时不用 absolute 定位，跟随父容器流式布局（用于列表项、表格内）
 *
 * 用法：
 *   <EmptyState :icon="FolderOutlined" text="暂无项目" />
 *   <EmptyState :icon="FolderOutlined" text="暂无项目" hint="点击右上角新建" />
 *   <EmptyState text="暂无语言数据" size="small" />   <!-- 图表内嵌，无图标 -->
 */
withDefaults(defineProps<{
  /** 图标组件（antd 图标，如 FolderOutlined）。不传则不显示图标 */
  icon?: any;
  /** 主文字 */
  text: string;
  /** 副标题（可选） */
  hint?: string;
  /** 尺寸：default=面板级（28px图标）；small=图表/内嵌（20px图标） */
  size?: "default" | "small";
  /** 内嵌模式：不用 absolute 撑满，跟随父容器（用于列表/表格内、chart 区） */
  inline?: boolean;
}>(), {
  icon: null,
  hint: "",
  size: "default",
  inline: false,
});

// 入场弹入：图标 scale 弹出 + 文字错峰淡入。只动子元素，不碰外层 transform
// （absolute 居中靠 translate(-50%,-50%)，动它会破坏居中）。
const rootRef = ref<HTMLElement>();
onMounted(() => {
  const el = rootRef.value;
  if (!el) return;
  if (motionDisabled()) return;
  const icon = el.querySelector<HTMLElement>(".empty-icon");
  if (icon) {
    gsap.fromTo(icon, { scale: 0.8, autoAlpha: 0 }, { scale: 1, autoAlpha: 1, clearProps: "all" });
  }
  const texts = el.querySelectorAll<HTMLElement>(".empty-text, .empty-hint");
  if (texts.length) {
    gsap.fromTo(
      texts,
      { y: 6, autoAlpha: 0 },
      { y: 0, autoAlpha: 1, stagger: 0.06, delay: 0.05, clearProps: "all" },
    );
  }
});
</script>

<template>
  <div
    ref="rootRef"
    class="empty-state"
    :class="[size, { inline, absolute: !inline }]"
  >
    <component :is="icon" v-if="icon" class="empty-icon" />
    <div class="empty-text">{{ text }}</div>
    <div v-if="hint" class="empty-hint">{{ hint }}</div>
  </div>
</template>

<style scoped>
/*
 * 定位策略：作为 panel（flex column）的直接子项，用 flex:1 占满 header 之外的
 * 剩余空间，内部 flex 居中。不依赖 absolute/relative/父容器高度计算，
 * 各列 panel 等高时空状态天然垂直对齐。
 *
 * 要求：EmptyState 是 panel 直接子级，且与 panel-body 互斥（v-if 控制）。
 * inline 模式用于列表/图表内的紧凑提示（跟随流式布局）。
 */

/* ============ 通用结构 ============ */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
}

/* absolute（默认）：面板级空状态。相对 panel（需 position:relative）中心居中。
   用 top/left:50% + transform 定位，不依赖容器高度计算，
   overflow:hidden 容器中也能正确显示（不像 inset:0 可能被裁剪）。
   各列 EmptyState 都在 panel 直接子级 + grid-template-rows 保证等高 → 中心对齐。 */
.empty-state.absolute {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  pointer-events: none;
}
/* inline：内嵌，跟随父容器流式布局（列表/图表内） */
.empty-state.inline {
  display: flex;
}

/* ============ 尺寸：default（面板级，28px 图标） ============ */
.empty-state.default .empty-icon {
  font-size: 28px;
  color: var(--text-disabled);
}
.empty-state.default .empty-text {
  font-size: 12px;
  color: var(--text-disabled);
}
.empty-state.default .empty-hint {
  font-size: 11px;
  color: var(--text-disabled);
  opacity: 0.85;
}

/* ============ 尺寸：small（图表/侧栏内嵌，20px 图标，更紧凑） ============ */
.empty-state.small {
  gap: 6px;
}
.empty-state.small .empty-icon {
  font-size: 20px;
  color: var(--text-disabled);
}
.empty-state.small .empty-text {
  font-size: 12px;
  color: var(--text-disabled);
}
.empty-state.small .empty-hint {
  font-size: 11px;
  color: var(--text-disabled);
  opacity: 0.85;
}
</style>
