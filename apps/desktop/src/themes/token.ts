/**
 * 主题注入（对齐 ax 三层架构 + CICBYTE 变量层半透明）
 *
 * - applyColorsToDOM()：基础 bg 注入 `--bg-*-orig`（原始色），由 :root 的
 *   `--bg-*: color-mix(var(--bg-*-orig), transparent var(--wp-panel-alpha,0%))`
 *   统一派生半透明（壁纸时透出，无壁纸等价纯色）。非 bg 基础 + 扩展层 + 区域文字直接注入；
 *   区域 bg 也走 -orig（覆盖值 ?? 默认派生）。
 * - buildAntdToken()：生成 Ant Design ConfigProvider token，读同一份 ThemeColors。
 */
import { theme as antdTheme } from "ant-design-vue";
import type { ThemeColors, Appearance, RegionOverrides, RegionOverrideKey } from "./types";
import { EXT_CSS_VARS, REGION_DEFS, defaultRegions, hexToRgb } from "./presets";

/** 主色是否偏亮（相对亮度 > 0.7）——亮主色（如纸墨暗色的纸白，亮度 ~0.9）需要深色反白文字；
 *  其余预设暗色主色亮度 ~0.63，保持白字观感不变 */
function isLightPrimary(hex: string): boolean {
  const [r, g, b] = hexToRgb(hex);
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 > 0.7;
}

/** 基础 bg → -orig（供 :root 派生 --bg-*） */
const BASE_BG: Record<string, string> = {
  bgBase: "--bg-base-orig",
  bgSurface: "--bg-surface-orig",
  bgElevated: "--bg-elevated-orig",
  bgInput: "--bg-input-orig",
};
/** 基础非 bg → 直接注入（不需半透明） */
const BASE_DIRECT: Record<string, string> = {
  primary: "--color-primary",
  primaryBg: "--color-primary-bg",
  colorHover: "--color-hover",
  textPrimary: "--text-primary",
  textSecondary: "--text-secondary",
  textTertiary: "--text-tertiary",
  borderBase: "--border-base",
  borderLight: "--border-light",
};

/** 把一份 ThemeColors 注入 DOM：基础 bg→-orig + 非bg直接 + 扩展 + 区域 + 明暗属性 */
export function applyColorsToDOM(
  colors: ThemeColors,
  appearance: Appearance,
  regions?: RegionOverrides,
): void {
  const root = document.documentElement;

  // 基础 bg → -orig（:root 派生 --bg-*）
  for (const key in BASE_BG) {
    root.style.setProperty(BASE_BG[key], colors[key as keyof ThemeColors] ?? "");
  }
  // 基础非 bg + 扩展层 → 直接
  for (const key in BASE_DIRECT) {
    root.style.setProperty(BASE_DIRECT[key], colors[key as keyof ThemeColors] ?? "");
  }
  for (const key in EXT_CSS_VARS) {
    root.style.setProperty(EXT_CSS_VARS[key], colors[key as keyof ThemeColors] ?? "");
  }
  root.style.setProperty("--accent", colors.primary);
  // 主题色 RGB 三元组（"r, g, b"）：供 rgba(var(--color-primary-rgb), α) 透明度梯度使用
  const [pr, pg, pb] = hexToRgb(colors.primary);
  root.style.setProperty("--color-primary-rgb", `${pr}, ${pg}, ${pb}`);
  // hero CTA：主题色底上的文字色（亮主色如纸墨暗色的纸白需墨字）
  root.style.setProperty(
    "--hero-cta-text",
    isLightPrimary(colors.primary) ? "#1b1c1f" : "#ffffff",
  );

  // 区域语义层：bg → -orig（用户覆盖值 ?? 默认派生，统一走半透明）；文字 → 直接（覆盖 set / 未覆盖 remove）
  const defs = defaultRegions(colors);
  REGION_DEFS.forEach((d) => {
    const bgOverride = regions ? regions[`${d.key}.bg` as RegionOverrideKey] : undefined;
    root.style.setProperty(`${d.vars.bg}-orig`, bgOverride ?? defs[d.key].bg);
    (["text", "textSub"] as const).forEach((prop) => {
      const v = regions ? regions[`${d.key}.${prop}` as RegionOverrideKey] : undefined;
      if (v != null) root.style.setProperty(d.vars[prop], v);
      else root.style.removeProperty(d.vars[prop]);
    });
  });

  // hero 暗色覆盖（derivation.css 只有浅色兜底，暗色由 JS 注入）：
  // 渐变随主题色板（bgBase→bgElevated，纸墨暗色即炭色系，不再写死 slate 偏蓝亮块）；
  // 纸白主色下白晕过亮 → 暗色移除光斑
  if (appearance === "dark") {
    root.style.setProperty("--hero-from", colors.bgBase);
    root.style.setProperty("--hero-to", colors.bgElevated);
    root.style.setProperty("--hero-glow-alpha", "0");
  }
  root.setAttribute("data-theme", appearance);
}

/** antd 轨：生成 ConfigProvider token，直接读 ThemeColors，不让 antd algorithm 自派生 */
export function buildAntdToken(colors: ThemeColors, appearance: Appearance) {
  const isDark = appearance === "dark";
  return {
    algorithm: isDark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
    token: {
      colorPrimary: colors.primary,
      colorBgBase: colors.bgBase,
      colorBgContainer: colors.bgSurface,
      colorBgElevated: colors.bgElevated,
      colorBgSpotlight: colors.bgElevated,
      colorBgLayout: colors.bgBase,
      colorTextBase: colors.textPrimary,
      colorText: colors.textPrimary,
      colorTextSecondary: colors.textSecondary,
      colorTextTertiary: colors.textTertiary,
      colorTextQuaternary: colors.textDisabled,
      colorBorder: colors.borderBase,
      colorBorderSecondary: colors.borderLight,
      colorSuccess: colors.success,
      colorWarning: colors.warning,
      colorError: colors.danger,
      colorInfo: colors.info,
      borderRadius: 6,
      fontFamily: "'Inter','PingFang SC','Microsoft YaHei',sans-serif",
      fontSize: 14,
      // 反白文字（主按钮/选中态等）：主色偏亮（纸墨暗色的纸白）时用墨字，避免白上白
      colorTextLightSolid: isLightPrimary(colors.primary) ? "#1b1c1f" : "#ffffff",
    },
    components: {
      Layout: {
        headerBg: colors.bgBase,
        siderBg: colors.bgBase,
        bodyBg: colors.bgBase,
      },
      Menu: {
        itemSelectedBg: colors.primaryBg,
        itemSelectedColor: colors.primary,
      },
      Card: {
        colorBgContainer: colors.bgSurface,
        borderRadiusLG: 8,
      },
      Modal: {
        contentBg: colors.bgSurface,
        headerBg: colors.bgSurface,
        titleColor: colors.textPrimary,
      },
      Dropdown: {
        colorBgElevated: colors.bgElevated,
      },
      Popover: {
        colorBgElevated: colors.bgElevated,
      },
      Input: {
        colorBgContainer: colors.bgInput,
      },
      Button: {
        // 暗色下默认按钮若用 bgSurface，与页面底色亮度差过小（轮廓不清）；
        // 提到 bgElevated 一档，浅色下维持 bgSurface 白底即可
        defaultBg: isDark ? colors.bgElevated : colors.bgSurface,
        defaultBorderColor: colors.borderBase,
        defaultColor: colors.textPrimary,
      },
    },
  };
}
