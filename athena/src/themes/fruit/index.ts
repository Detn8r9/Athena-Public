import type { ThemePackage, ThemeTokenMap } from "../types";

const fruitFont =
  '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Helvetica Neue", Helvetica, Arial, sans-serif';

export const fruitLightTokens: ThemeTokenMap = {
  bg: "#f5f5f7",
  surface: "rgba(255, 255, 255, 0.72)",
  surfaceSolid: "#ffffff",
  elevated: "#ffffff",
  primary: "#0071e3",
  secondary: "#0077ed",
  text: "#1d1d1f",
  textMuted: "#6e6e73",
  border: "rgba(0, 0, 0, 0.08)",
  hairline: "rgba(0, 0, 0, 0.06)",
  danger: "#ff3b30",
  success: "#34c759",
  userBubble: "rgba(0, 113, 227, 0.1)",
  hoverOverlay: "rgba(0, 0, 0, 0.04)",
  glow: "rgba(0, 113, 227, 0.18)",
  radius: "12px",
  shadow: "0 4px 24px rgba(0, 0, 0, 0.08)",
  font: fruitFont,
};

export const fruitDarkTokens: ThemeTokenMap = {
  bg: "#000000",
  surface: "rgba(28, 28, 30, 0.72)",
  surfaceSolid: "#1c1c1e",
  elevated: "#2c2c2e",
  primary: "#0a84ff",
  secondary: "#409cff",
  text: "#f5f5f7",
  textMuted: "#98989d",
  border: "rgba(255, 255, 255, 0.1)",
  hairline: "rgba(255, 255, 255, 0.08)",
  danger: "#ff453a",
  success: "#30d158",
  userBubble: "rgba(10, 132, 255, 0.16)",
  hoverOverlay: "rgba(255, 255, 255, 0.06)",
  glow: "rgba(10, 132, 255, 0.22)",
  radius: "12px",
  shadow: "0 8px 32px rgba(0, 0, 0, 0.45)",
  font: fruitFont,
};

export const fruitPackage: ThemePackage = {
  id: "fruit",
  packageKey: "fruit",
  label: "Fruit",
  description: "Clean system look — light, dark, or auto by time of day.",
  personaId: "fruit",
  lexiconId: "fruit",
  chrome: {
    bodyClass: "theme-fruit",
    locksPalette: true,
    defaultPalette: "silver",
  },
  fxDefaults: {
    scanlines: false,
    vignette: false,
    grain: 0,
    sharpCorners: false,
  },
  supportsAppearanceModes: true,
  tokensByAppearance: {
    light: fruitLightTokens,
    dark: fruitDarkTokens,
  },
};
