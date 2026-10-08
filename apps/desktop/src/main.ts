import { createApp } from "vue";
import { createPinia } from "pinia";
import Antd from "ant-design-vue";
import "ant-design-vue/dist/reset.css";

import App from "./App.vue";
import router from "./router";
import "@/themes/derivation.css";
import "@/styles/global.css";
// Monaco 初始化（副作用：注册 MonacoEnvironment + 主题，必须在组件 mount 前）
import "@/monaco/setup";
// 全局动画指令（v-reveal / v-reveal-group），GSAP 入口
import { vReveal, vRevealGroup } from "@/composables/useMotion";
import { useThemeStore } from "@/stores/theme";

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(Antd);
app.directive("reveal", vReveal);
app.directive("reveal-group", vRevealGroup);
// 主题初始化（mount 前同步，配合 index.html FOUC 脚本首帧即正确主题）
useThemeStore().initialize();
app.mount("#app");
