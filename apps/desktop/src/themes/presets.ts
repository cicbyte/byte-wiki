/**
 * 主题色板：5 套内置预设 + 1 套自定义。
 *
 * 设计取向（写作应用，纸感优先）：
 * - 每套预设浅/深两态都是「精调专色板」——中性色（背景/文字/边框）带统一色相温度，
 *   低饱和、暖纸感，而非从单一主色机械派生的工具灰；
 * - 主色克制：纸墨用墨色（黑底白字主按钮，Linear 质感），其余四套用低饱和的中国传统色
 *   （青黛/青瓷/藕荷/茶褐），避免高饱和「管理后台蓝绿橙红」；
 * - 自定义主题仍可用 deriveBaseLayer 由底色自动派生协调色板（兜底工具）。
 *
 * CICBYTE 扩展：在 ax 基础 12 token 之外补充状态色（success/warning/danger/info）+
 * 禁用文字 + 主色三态 + 阴影，按明暗固定提供，不参与预设色调（按钮/消息始终可辨）。
 *
 * 区域语义层（REGION_DEFS）独立于预设：任何预设下都可叠加区域微调，跨预设保留。
 */
import type { ThemeColors, BuiltinPreset, ResolvedMode, RegionKey } from "./types";

/** 基础层 12 token → CSS 变量名（注入 :root 用） */
export const PALETTE_CSS_VARS: Record<string, string> = {
  primary: "--color-primary",
  primaryBg: "--color-primary-bg",
  colorHover: "--color-hover",
  bgBase: "--bg-base",
  bgSurface: "--bg-surface",
  bgInput: "--bg-input",
  bgElevated: "--bg-elevated",
  textPrimary: "--text-primary",
  textSecondary: "--text-secondary",
  textTertiary: "--text-tertiary",
  borderBase: "--border-base",
  borderLight: "--border-light",
};

/** 扩展层变量名（状态色 / 禁用文字 / 主色三态 / 阴影） */
export const EXT_CSS_VARS: Record<string, string> = {
  textDisabled: "--text-disabled",
  primaryHover: "--color-primary-hover",
  primaryActive: "--color-primary-active",
  primaryBorder: "--color-primary-border",
  success: "--color-success",
  warning: "--color-warning",
  danger: "--color-danger",
  info: "--color-info",
  successBg: "--success-bg",
  warningBg: "--warning-bg",
  dangerBg: "--danger-bg",
  infoBg: "--info-bg",
  shadowMd: "--shadow-md",
};

/** 预设专色板：单明暗态的完整基础层（主色 + 9 个中性 token），全部手工精调 */
export interface PresetPalette {
  primary: string;
  bgBase: string;
  bgSurface: string;
  bgInput: string;
  bgElevated: string;
  textPrimary: string;
  textSecondary: string;
  textTertiary: string;
  borderBase: string;
  borderLight: string;
}

/**
 * 5 套内置预设（浅/深各一套专色板）。
 *
 * 浅色共同语言：画布带轻微色相的暖/冷灰，纸面近白，文字同色相墨色，细线低饱和。
 * 深色共同语言：带色相的炭底（不死黑），亮度小步长递进，米白文字。
 */
export const PRESETS: Record<BuiltinPreset, { label: string; palette: Record<ResolvedMode, PresetPalette> }> = {
  // 纸墨（默认）：暖灰画布 + 白纸 + 墨色主色；深色为暖炭夜纸。
  // 主色即墨——浅色黑底白字主按钮、深色纸白主按钮，品牌色让位于文字本身。
  paper: {
    label: "纸墨",
    palette: {
      light: {
        primary: "#1b1c1f",
        bgBase: "#f1f1ee",
        bgSurface: "#ffffff",
        bgInput: "#f3f3f1",
        bgElevated: "#f8f8f7",
        textPrimary: "#1c1d21",
        textSecondary: "#5c6167",
        textTertiary: "#97999d",
        borderBase: "#dcdcd9",
        borderLight: "#e9e9e7",
      },
      dark: {
        primary: "#f2f2f0",
        bgBase: "#191a1c",
        bgSurface: "#232527",
        bgInput: "#2d2f31",
        bgElevated: "#282a2c",
        textPrimary: "#ececee",
        textSecondary: "#a3a6aa",
        textTertiary: "#74777c",
        borderBase: "#3a3c3f",
        borderLight: "#2d2f31",
      },
    },
  },
  // 青黛：雾靛蓝。蓝而不艳，替换旧「经典蓝」的 slate 工具感。
  indigo: {
    label: "青黛",
    palette: {
      light: {
        primary: "#46618f",
        bgBase: "#f1f2f5",
        bgSurface: "#ffffff",
        bgInput: "#f2f3f6",
        bgElevated: "#f7f8fa",
        textPrimary: "#1d2027",
        textSecondary: "#565d6b",
        textTertiary: "#949aa5",
        borderBase: "#d8dae0",
        borderLight: "#e7e8ed",
      },
      dark: {
        primary: "#8ca3cc",
        bgBase: "#15171d",
        bgSurface: "#1e2129",
        bgInput: "#282c36",
        bgElevated: "#232730",
        textPrimary: "#e9ebf0",
        textSecondary: "#a0a6b3",
        textTertiary: "#707583",
        borderBase: "#343945",
        borderLight: "#282c36",
      },
    },
  },
  // 青瓷：青瓷绿。雨过天青的低饱和绿，替换旧「翡翠绿」。
  celadon: {
    label: "青瓷",
    palette: {
      light: {
        primary: "#47796b",
        bgBase: "#eef2f0",
        bgSurface: "#ffffff",
        bgInput: "#f0f3f1",
        bgElevated: "#f5f8f6",
        textPrimary: "#1b211f",
        textSecondary: "#55605b",
        textTertiary: "#939c97",
        borderBase: "#d7ded9",
        borderLight: "#e6ebe8",
      },
      dark: {
        primary: "#82b0a0",
        bgBase: "#131a17",
        bgSurface: "#1b2420",
        bgInput: "#242e29",
        bgElevated: "#202925",
        textPrimary: "#e8ece9",
        textSecondary: "#9da8a1",
        textTertiary: "#6e7973",
        borderBase: "#303b35",
        borderLight: "#242e29",
      },
    },
  },
  // 藕荷：灰紫。莲藕花瓣的柔和紫，替换旧「紫罗兰」的高饱和 violet。
  plum: {
    label: "藕荷",
    palette: {
      light: {
        primary: "#6f6494",
        bgBase: "#f3f2f6",
        bgSurface: "#ffffff",
        bgInput: "#f4f3f7",
        bgElevated: "#f8f8fa",
        textPrimary: "#201e28",
        textSecondary: "#5a5768",
        textTertiary: "#96939f",
        borderBase: "#dad8e0",
        borderLight: "#e9e8ee",
      },
      dark: {
        primary: "#a79bd0",
        bgBase: "#17151d",
        bgSurface: "#201e29",
        bgInput: "#2a2735",
        bgElevated: "#252230",
        textPrimary: "#eae8ef",
        textSecondary: "#a5a1b4",
        textTertiary: "#746f82",
        borderBase: "#363244",
        borderLight: "#2a2735",
      },
    },
  },
  // 茶褐：焙茶棕。温润的茶汤色，替换旧「琥珀橙」的警示感橙。
  tea: {
    label: "茶褐",
    palette: {
      light: {
        primary: "#8a6242",
        bgBase: "#f6f3ed",
        bgSurface: "#fffdf9",
        bgInput: "#f6f4ee",
        bgElevated: "#faf8f3",
        textPrimary: "#262019",
        textSecondary: "#5f574c",
        textTertiary: "#9b9184",
        borderBase: "#e0d9cc",
        borderLight: "#ece6da",
      },
      dark: {
        primary: "#c49a72",
        bgBase: "#191612",
        bgSurface: "#221e19",
        bgInput: "#2c2721",
        bgElevated: "#27221d",
        textPrimary: "#ede8df",
        textSecondary: "#a89e8f",
        textTertiary: "#786e60",
        borderBase: "#393229",
        borderLight: "#2c2721",
      },
    },
  },
};

/** 预设列表（UI 用，固定顺序：纸墨为默认居首） */
export const PRESET_LIST: { key: BuiltinPreset; label: string }[] = [
  { key: "paper", label: "纸墨" },
  { key: "indigo", label: "青黛" },
  { key: "celadon", label: "青瓷" },
  { key: "plum", label: "藕荷" },
  { key: "tea", label: "茶褐" },
];

/** 扩展层固定补充（按明暗；不参与预设色调） */
const SUPPLEMENT: Record<ResolvedMode, Omit<ThemeColors, keyof PresetPalette | "primaryBg" | "colorHover">> = {
  dark: {
    textDisabled: "#475569",
    primaryHover: "", // 由 primary 派生（见 derivePrimaryStates）
    primaryActive: "",
    primaryBorder: "",
    success: "#22c55e",
    warning: "#f59e0b",
    danger: "#ef4444",
    info: "#06b6d4",
    successBg: "rgba(34,197,94,0.14)",
    warningBg: "rgba(245,158,11,0.14)",
    dangerBg: "rgba(239,68,68,0.14)",
    infoBg: "rgba(6,182,212,0.14)",
    shadowMd: "0 8px 24px rgba(0,0,0,0.4)",
  },
  light: {
    textDisabled: "#c9c7c2",
    primaryHover: "",
    primaryActive: "",
    primaryBorder: "",
    success: "#16a34a",
    warning: "#d97706",
    danger: "#dc2626",
    info: "#0891b2",
    successBg: "rgba(22,163,74,0.10)",
    warningBg: "rgba(217,119,6,0.10)",
    dangerBg: "rgba(220,38,38,0.10)",
    infoBg: "rgba(8,145,178,0.10)",
    shadowMd: "0 4px 12px rgba(28,29,33,0.08)",
  },
};

// ===== 色彩运算（移植自 ax，逐字对齐） =====

/** hex(#rgb|#rrggbb) → [r,g,b]；非法兜底靛蓝 */
export function hexToRgb(hex: string): [number, number, number] {
  let h = hex.replace("#", "").trim();
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const n = parseInt(h, 16);
  if (Number.isNaN(n) || h.length !== 6) return [110, 127, 158];
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** primary-bg：primary @ 15% alpha */
export function derivePrimaryBg(primary: string): string {
  const [r, g, b] = hexToRgb(primary);
  return `rgba(${r}, ${g}, ${b}, 0.15)`;
}

/** primary-border：primary @ 40% alpha */
export function derivePrimaryBorder(primary: string): string {
  const [r, g, b] = hexToRgb(primary);
  return `rgba(${r}, ${g}, ${b}, 0.4)`;
}

/** hover：dark = white@5%，light = black@4% */
export function deriveHover(mode: ResolvedMode): string {
  return mode === "dark" ? "rgba(255, 255, 255, 0.05)" : "rgba(28, 29, 33, 0.04)";
}

/** 主色 hover/active：hover 向白 12%（更亮），active 向黑 12%（更深） */
export function derivePrimaryStates(primary: string): { primaryHover: string; primaryActive: string } {
  return {
    primaryHover: mix(primary, "white", 0.12),
    primaryActive: mix(primary, "black", 0.12),
  };
}

/** hex 向白/黑混合 pct（0~1），返回 #rrggbb */
function mix(hex: string, target: "white" | "black", pct: number): string {
  const [r, g, b] = hexToRgb(hex);
  const t = target === "white" ? 255 : 0;
  const toHex = (n: number) => Math.round(n).toString(16).padStart(2, "0");
  return `#${toHex(r + (t - r) * pct)}${toHex(g + (t - g) * pct)}${toHex(b + (t - b) * pct)}`;
}

/** 基础层派生：由「页面底色 + 明暗」派生协调中性色（自定义主题「按底色自动协调」用） */
export function deriveBaseLayer(base: string, mode: ResolvedMode, primary: string) {
  const common = { primary, primaryBg: derivePrimaryBg(primary), bgBase: base, colorHover: deriveHover(mode) };
  if (mode === "dark") {
    return {
      ...common,
      bgSurface: mix(base, "white", 0.05),
      bgInput: mix(base, "white", 0.08),
      bgElevated: mix(base, "white", 0.12),
      borderBase: mix(base, "white", 0.22),
      borderLight: mix(base, "white", 0.12),
      textPrimary: "#ececee",
      textSecondary: "#a3a6aa",
      textTertiary: "#74777c",
    };
  }
  return {
    ...common,
    bgSurface: mix(base, "black", 0.03),
    bgInput: mix(base, "black", 0.05),
    bgElevated: mix(base, "black", 0.06),
    borderBase: mix(base, "black", 0.18),
    borderLight: mix(base, "black", 0.08),
    textPrimary: "#1c1d21",
    textSecondary: "#5c6167",
    textTertiary: "#97999d",
  };
}

/** 由预设 + 明暗组装完整 ThemeColors（专色板 + 扩展补充 + 主色三态） */
export function buildPresetColors(preset: BuiltinPreset, mode: ResolvedMode): ThemeColors {
  const base = PRESETS[preset].palette[mode];
  const sup = SUPPLEMENT[mode];
  const states = derivePrimaryStates(base.primary);
  return {
    ...base,
    primaryBg: derivePrimaryBg(base.primary),
    colorHover: deriveHover(mode),
    textDisabled: sup.textDisabled,
    primaryHover: states.primaryHover,
    primaryActive: states.primaryActive,
    primaryBorder: derivePrimaryBorder(base.primary),
    success: sup.success,
    warning: sup.warning,
    danger: sup.danger,
    info: sup.info,
    successBg: sup.successBg,
    warningBg: sup.warningBg,
    dangerBg: sup.dangerBg,
    infoBg: sup.infoBg,
    shadowMd: sup.shadowMd,
  };
}

/** 自定义默认色板：纸墨预设浅/深副本（切到自定义无视觉跳变） */
export function defaultCustom(): { light: ThemeColors; dark: ThemeColors } {
  return { light: buildPresetColors("paper", "light"), dark: buildPresetColors("paper", "dark") };
}

/** 用户可编辑的基础层 token（派生项与扩展层不开放） */
export const EDITABLE_TOKENS: { group: string; key: keyof ThemeColors; label: string }[] = [
  { group: "主色", key: "primary", label: "主题色" },
  { group: "背景", key: "bgBase", label: "页面底色" },
  { group: "背景", key: "bgSurface", label: "容器底色" },
  { group: "背景", key: "bgInput", label: "输入框背景" },
  { group: "背景", key: "bgElevated", label: "浮层背景" },
  { group: "文字", key: "textPrimary", label: "主文字" },
  { group: "文字", key: "textSecondary", label: "次要文字" },
  { group: "文字", key: "textTertiary", label: "弱化文字" },
  { group: "边框", key: "borderBase", label: "主边框" },
  { group: "边框", key: "borderLight", label: "弱边框" },
];

// ===== 区域语义层（命名：header/sider/footer/content/card） =====

export interface RegionEntry {
  bg: string;
  text: string;
  textSub: string;
}
export type RegionPalette = Record<RegionKey, RegionEntry>;

/** 区域 key → UI 中文名 + 3 个 CSS 变量名（设置页渲染 + apply 注入共用） */
export const REGION_DEFS: { key: RegionKey; label: string; vars: { bg: string; text: string; textSub: string } }[] = [
  { key: "header", label: "顶部栏", vars: { bg: "--bg-header", text: "--text-header", textSub: "--text-header-sub" } },
  { key: "sider", label: "侧边栏", vars: { bg: "--bg-sider", text: "--text-sider", textSub: "--text-sider-sub" } },
  { key: "footer", label: "底部栏", vars: { bg: "--bg-footer", text: "--text-footer", textSub: "--text-footer-sub" } },
  { key: "content", label: "内容区", vars: { bg: "--bg-content", text: "--text-content", textSub: "--text-content-sub" } },
  { key: "card", label: "卡片/面板", vars: { bg: "--bg-card", text: "--text-card", textSub: "--text-card-sub" } },
];

/** 由某 ThemeColors 派生默认区域配色：chrome 类（顶/侧/底栏）与内容区统一用 bgBase（整窗一张纸），卡片用 bgSurface */
export function defaultRegions(c: ThemeColors): RegionPalette {
  const chrome: RegionEntry = { bg: c.bgBase, text: c.textPrimary, textSub: c.textSecondary };
  return {
    header: { ...chrome },
    sider: { ...chrome },
    footer: { ...chrome },
    content: { bg: c.bgBase, text: c.textPrimary, textSub: c.textSecondary },
    card: { bg: c.bgSurface, text: c.textPrimary, textSub: c.textSecondary },
  };
}
