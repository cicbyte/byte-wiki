<script setup lang="ts">
import { storeToRefs } from "pinia";
import { ConfigProvider } from "ant-design-vue";
import zhCN from "ant-design-vue/es/locale/zh_CN";
import { useThemeStore } from "@/stores/theme";
import AppLayout from "@/components/layout/AppLayout.vue";

const themeStore = useThemeStore();
const { antdTheme } = storeToRefs(themeStore);
</script>

<template>
  <ConfigProvider :theme="(antdTheme as any)" :locale="zhCN">
    <AppLayout />
  </ConfigProvider>
</template>
