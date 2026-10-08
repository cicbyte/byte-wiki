<script setup lang="ts">
/**
 * MainContent：纯内容区
 * 所有页面统一 fullsize（自管 padding + overflow），content-wrapper 只做 flex 撑满。
 *
 * RouterView 外包 <Transition>（GSAP JS hooks，:css="false"）：路由切换时
 * 旧页快速淡出、新页柔和弹入（统一动画语言见 useMotion.ts）。
 */
import { useRoute } from "vue-router";
import { useRouteTransition } from "@/composables/useMotion";

const route = useRoute();
const { onEnter, onLeave } = useRouteTransition();
</script>

<template>
  <main class="main-content">
    <div class="content-wrapper">
      <RouterView v-slot="{ Component }">
        <Transition mode="out-in" :css="false" @enter="onEnter" @leave="onLeave">
          <component :is="Component" :key="route.path" />
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
  flex: 1;
  min-height: 0;
  /* 所有页面均为 fullsize，统一 padding 0，由各页面自管 padding + overflow */
  padding: 0;
}
</style>
