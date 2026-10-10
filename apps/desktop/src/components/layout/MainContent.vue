<script setup lang="ts">
/**
 * MainContent：纯内容区
 * 所有页面统一 fullsize（自管 padding + overflow），content-wrapper 只做 flex 撑满。
 *
 * RouterView 外包 CSS class <Transition>：路由切换时旧页快速淡出、新页柔和弹入
 * （动画语言见 useMotion.ts 的 motionDisabled）。
 */
import { computed } from "vue";
import { useRoute } from "vue-router";
import { motionDisabled } from "@/composables/useMotion";

const route = useRoute();
const routeTransitionName = computed(() => (motionDisabled() ? "route-instant" : "route"));
const routeKey = route.path;
</script>

<template>
  <main class="main-content">
    <div class="content-wrapper">
      <RouterView v-slot="{ Component }">
        <!-- CSS class 过渡（Vue 原生机制）。注意：不可用 mode="out-in"——
             与含 Drawer/异步挂载的组件树组合时，out-in 的"旧节点移除→新节点插入"
             串联有概率中断，导致主区永久空白（byte-writer 踩过的坑）。
             默认模式（进出并行）+ 离场元素脱离文档流达到相同视觉效果。 -->
        <Transition :name="routeTransitionName">
          <component :is="Component" :key="routeKey" />
        </Transition>
      </RouterView>
    </div>
  </main>
</template>

<style scoped>
.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--bg-content);
}
.content-wrapper {
  position: relative;
  flex: 1;
  min-height: 0;
  /* 所有页面均为 fullsize，统一 padding 0，由各页面自管 padding + overflow */
  padding: 0;
}
.route-enter-active {
  transition:
    opacity 0.25s ease,
    transform 0.25s ease;
}
.route-leave-active {
  /* 并行模式下离场元素脱离文档流，避免与新入场元素同时 in-flow 造成高度跳动 */
  position: absolute;
  inset: 0;
  transition:
    opacity 0.15s ease-in,
    transform 0.15s ease-in;
}
.route-enter-from {
  opacity: 0;
  transform: translateY(10px);
}
.route-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
/* 禁用动画：瞬时切换 */
.route-instant-enter-active,
.route-instant-leave-active {
  transition: none;
}
.route-instant-enter-from,
.route-instant-leave-to {
  opacity: 1;
}
</style>
