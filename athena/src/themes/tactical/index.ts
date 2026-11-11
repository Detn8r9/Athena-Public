import type { ThemePackage, ThemeTokenMap } from "../types";

/** Tactical / Survival Guide — current default look (zero intentional visual change). */
export const tacticalTokens: ThemeTokenMap = {
  bg: "#0c1222",
  surface: "rgba(18, 28, 45, 0.85)",
  surfaceSolid: "#121c2d",
  elevated: "#162033",
  primary: "#38bdf8",
  secondary: "#0ea5e9",
  text: "#e8ecf4",
  textMuted: "color-mix(in srgb, #e8ecf4 65%, #0c1222)",
  border: "rgba(56, 189, 248, 0.25)",
  hairline: "rgba(56, 189, 248, 0.14)",
  danger: "#f87171",
  success: "#34d399",
  userBubble: "rgba(56, 189, 248, 0.12)",
  hoverOverlay: "rgba(56, 189, 248, 0.08)",
  glow: "rgba(56, 189, 248, 0.2)",
  radius: "0.75rem",
  shadow: "0 10px 15px -3px rgba(0, 0, 0, 0.5)",
  font: 'system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
};

export const tacticalPackage: ThemePackage = {
  id: "default",
  packageKey: "tactical",
  label: "Survival Guide (Default)",
  description: "Calm, practical instructor.",
  personaId: "default",
  lexiconId: "survival",
  chrome: {
    locksPalette: false,
    defaultPalette: "blue",
  },
  fxDefaults: {
    scanlines: false,
    vignette: false,
    grain: 0,
    sharpCorners: false,
  },
  supportsAppearanceModes: false,
  tokens: tacticalTokens,
};
