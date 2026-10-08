/**
 * 壁纸 store（对齐 ax 面板级毛玻璃机制）
 *
 * 分工：
 * - 壁纸【文件】由后端管（data/wallpapers/，见 lib.rs 的 upload/list/delete_wallpaper）
 * - 壁纸【设置】（开关/选中/模糊/遮罩/面板透明度）持久化在 localStorage `bs-wallpaper`，
 *   纯 UI 偏好，不走 config.toml——避免死字段。
 *
 * apply() 在 :root 注入两个变量驱动整窗面板毛玻璃半透明：
 * - `--wp-panel-alpha`：区域背景半透明度（组件用 color-mix 消费，0%=不透明）
 * - `--wp-blur`：面板 backdrop-filter 模糊量
 * 并设 `<html data-wallpaper="on|off">`。壁纸层/遮罩层在 AppLayout.vue 渲染，遮罩色
 * `maskColor` 跟随 theme.resolvedMode（深色压暗黑、浅色压亮白）。
 */
import { defineStore } from "pinia";
import { ref, computed, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "@/api";
import { useThemeStore } from "@/stores/theme";

const STORAGE_KEY = "bs-wallpaper";
// 旧多 key（迁移用）
const LS_CURRENT = "bs-wallpaper-current";
const LS_OPACITY = "bs-wallpaper-opacity";
const LS_BLUR = "bs-wallpaper-blur";
const LS_MASK = "bs-wallpaper-mask";
const LS_ENABLED = "bs-wallpaper-enabled";

interface WallpaperState {
  version: 1;
  enabled: boolean;
  activeName: string | null;
  blur: number; // 0~40 px
  mask: number; // 0~0.8（遮罩 alpha）
  panelAlpha: number; // 0~0.7（面板背景 alpha）
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

export const useWallpaperStore = defineStore("wallpaper", () => {
  const enabled = ref(false);
  const activeName = ref<string | null>(null);
  const blur = ref(0);
  const mask = ref(0.3);
  const panelAlpha = ref(0.4);
  const dataDir = ref("");
  const list = ref<string[]>([]);

  const theme = useThemeStore();

  /** 某壁纸的 asset URL */
  function thumbUrl(name: string): string {
    if (!dataDir.value) return "";
    return convertFileSrc(`${dataDir.value}/wallpapers/${name}`);
  }
  /** 当前激活壁纸 URL（enabled && activeName && dataDir 齐备时） */
  const activeUrl = computed(() => {
    if (!enabled.value || !activeName.value || !dataDir.value) return null;
    return thumbUrl(activeName.value);
  });
  const hasWallpaper = computed(() => !!activeUrl.value);
  /** 兼容旧消费方（AppLayout 等用 url） */
  const url = computed(() => activeUrl.value ?? "");

  /** 遮罩层背景色：深色压暗（黑）、浅色压亮（白），跟随当前明暗 */
  const maskColor = computed(() => {
    const rgb = theme.resolvedMode === "dark" ? "0,0,0" : "255,255,255";
    return `rgba(${rgb}, ${mask.value})`;
  });

  /** 注入 CSS 变量 + data-wallpaper；禁用或无激活项时清零（组件自动恢复不透明） */
  function apply() {
    const root = document.documentElement;
    const on = enabled.value && !!activeName.value && !!dataDir.value;
    if (on) {
      root.style.setProperty("--wp-blur", `${blur.value}px`);
      root.style.setProperty("--wp-panel-alpha", `${Math.round(panelAlpha.value * 100)}%`);
      root.setAttribute("data-wallpaper", "on");
    } else {
      root.style.setProperty("--wp-blur", "0px");
      root.style.setProperty("--wp-panel-alpha", "0%");
      root.setAttribute("data-wallpaper", "off");
    }
  }

  function persist() {
    const s: WallpaperState = {
      version: 1,
      enabled: enabled.value,
      activeName: activeName.value,
      blur: blur.value,
      mask: mask.value,
      panelAlpha: panelAlpha.value,
    };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
  }

  /** 读设置：优先新单 key，否则迁移旧 5 个 key */
  function load() {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      try {
        const s = JSON.parse(raw) as Partial<WallpaperState>;
        enabled.value = !!s.enabled;
        activeName.value = s.activeName ?? null;
        blur.value = clamp(Number(s.blur ?? 0), 0, 40);
        mask.value = clamp(Number(s.mask ?? 0.3), 0, 0.8);
        panelAlpha.value = clamp(Number(s.panelAlpha ?? 0.4), 0, 0.7);
        return;
      } catch {
        /* 损坏则走迁移 */
      }
    }
    if (localStorage.getItem(LS_CURRENT) !== null) {
      enabled.value = localStorage.getItem(LS_ENABLED) !== "0";
      activeName.value = localStorage.getItem(LS_CURRENT) || null;
      // 旧 blur 0~1（AppLayout 曾 *20px）→ 新 px
      blur.value = clamp(Number(localStorage.getItem(LS_BLUR) ?? 0) * 20, 0, 40);
      mask.value = clamp(Number(localStorage.getItem(LS_MASK) ?? 0.35), 0, 0.8);
      // 旧 opacity 越大越透出壁纸 = 新 panelAlpha 越大越透明（语义一致，上限 0.7）
      panelAlpha.value = clamp(Number(localStorage.getItem(LS_OPACITY) ?? 0.85), 0, 0.7);
      persist();
      // 清旧 key（已迁入单 key）
      [LS_CURRENT, LS_OPACITY, LS_BLUR, LS_MASK, LS_ENABLED].forEach((k) => localStorage.removeItem(k));
    }
  }

  async function refreshList() {
    try {
      list.value = await api.wallpaperList();
    } catch {
      list.value = [];
    }
  }

  /** 初始化：load 设置 → 拉数据目录与列表 → 校验激活项仍存在 → apply */
  async function init() {
    load();
    try {
      dataDir.value = await api.dataDir();
    } catch {
      /* 后端不可用时降级：无壁纸 */
    }
    await refreshList();
    if (activeName.value && !list.value.includes(activeName.value)) {
      activeName.value = null;
    }
    apply();
  }

  // ===== setters =====
  function setEnabled(v: boolean) {
    enabled.value = v;
  }
  function setActive(name: string | null) {
    activeName.value = name;
    if (name) enabled.value = true;
  }
  function setBlur(v: number) {
    blur.value = clamp(v, 0, 40);
  }
  function setMask(v: number) {
    mask.value = clamp(v, 0, 0.8);
  }
  function setPanelAlpha(v: number) {
    panelAlpha.value = clamp(v, 0, 0.7);
  }
  function clear() {
    activeName.value = null;
  }

  // 任一相关 state 变化 → apply；拖动 slider 时 persist 防抖
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  watch([enabled, activeName, blur, mask, panelAlpha], () => {
    apply();
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => persist(), 300);
  });

  return {
    enabled,
    activeName,
    blur,
    mask,
    panelAlpha,
    dataDir,
    list,
    activeUrl,
    hasWallpaper,
    url,
    maskColor,
    thumbUrl,
    apply,
    persist,
    refreshList,
    init,
    setEnabled,
    setActive,
    setBlur,
    setMask,
    setPanelAlpha,
    clear,
  };
});
