import type { ResolvedAppearance, ThemePackage, ThemeTokenMap } from "../types";

/**
 * Modern — clean, elevated, high-contrast field UI.
 * Tokens live in `modern.css` (not inline) so night-vision and reduced-transparency
 * overrides can still win in CSS. These maps document the values and feed glass RGB.
 */
export const modernFont =
  '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Segoe UI Variable Text", "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';

export const modernLightTokens: ThemeTokenMap = {
  bg: "#eef0f3",
  surface: "rgba(255, 255, 255, 0.86)",
  surfaceSolid: "#ffffff",
  elevated: "#ffffff",
  primary: "#0a5fd1",
  secondary: "#0b4fae",
  text: "#0b0d10",
  textMuted: "#4b525c",
  border: "rgba(11, 13, 16, 0.14)",
  hairline: "rgba(11, 13, 16, 0.08)",
  danger: "#c8231b",
  success: "#0f7a55",
  userBubble: "rgba(10, 95, 209, 0.1)",
  hoverOverlay: "rgba(11, 13, 16, 0.05)",
  glow: "rgba(10, 95, 209, 0.22)",
  radius: "14px",
  shadow: "0 1px 2px rgba(11, 13, 16, 0.06), 0 8px 24px rgba(11, 13, 16, 0.08)",
  font: modernFont,
};

export const modernDarkTokens: ThemeTokenMap = {
  bg: "#07090c",
  surface: "rgba(20, 24, 30, 0.82)",
  surfaceSolid: "#13171d",
  elevated: "#1b2028",
  primary: "#4ea4ff",
  secondary: "#7bbcff",
  text: "#f3f5f7",
  textMuted: "#a2abb7",
  border: "rgba(255, 255, 255, 0.14)",
  hairline: "rgba(255, 255, 255, 0.08)",
  danger: "#ff5d52",
  success: "#3ddc97",
  userBubble: "rgba(78, 164, 255, 0.16)",
  hoverOverlay: "rgba(255, 255, 255, 0.06)",
  glow: "rgba(78, 164, 255, 0.28)",
  radius: "14px",
  shadow: "0 1px 0 rgba(255, 255, 255, 0.04) inset, 0 12px 32px rgba(0, 0, 0, 0.5)",
  font: modernFont,
};

/** `--theme-bg-rgb` / `--theme-accent-rgb` per appearance (glass tints, html background). */
export const modernRgbByAppearance: Record<ResolvedAppearance, { bg: string; accent: string }> = {
  light: { bg: "238, 240, 243", accent: "10, 95, 209" },
  dark: { bg: "7, 9, 12", accent: "78, 164, 255" },
};

export const modernPackage: ThemePackage = {
  id: "modern",
  packageKey: "modern",
  label: "Modern",
  description: "Clean, elevated, high-contrast — light for sun, dark for night, or auto by time of day.",
  personaId: "default",
  lexiconId: "survival",
  chrome: {
    bodyClass: "theme-modern",
    locksPalette: true,
    defaultPalette: "slate",
    rgbByAppearance: modernRgbByAppearance,
    cssOwnsTokens: true,
  },
  fxDefaults: {
    scanlines: false,
    vignette: false,
    grain: 0,
    sharpCorners: false,
  },
  supportsAppearanceModes: true,
  tokensByAppearance: {
    light: modernLightTokens,
    dark: modernDarkTokens,
  },
};
