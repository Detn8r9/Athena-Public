import type { ThemePackage, ThemeTokenMap } from "../types";

/** Fallout CRT — phosphor green defaults; palette accents still apply via App.css. */
export const falloutTokens: ThemeTokenMap = {
  bg: "#0a1408",
  surface: "rgba(12, 28, 10, 0.9)",
  surfaceSolid: "#0c1c0a",
  elevated: "#102410",
  primary: "#33ff66",
  secondary: "#1aff4d",
  text: "#b8ffc8",
  textMuted: "color-mix(in srgb, #b8ffc8 65%, #0a1408)",
  border: "rgba(51, 255, 102, 0.28)",
  hairline: "rgba(51, 255, 102, 0.16)",
  danger: "#ff6b6b",
  success: "#33ff66",
  userBubble: "rgba(51, 255, 102, 0.12)",
  hoverOverlay: "rgba(51, 255, 102, 0.08)",
  glow: "rgba(51, 255, 102, 0.25)",
  radius: "0.25rem",
  shadow: "0 0 24px rgba(51, 255, 102, 0.12)",
  font: '"SF Mono", ui-monospace, Menlo, Monaco, "Courier New", monospace',
};

export const falloutPackage: ThemePackage = {
  id: "fallout",
  packageKey: "fallout",
  label: "Fallout terminal (Default)",
  description: "CRT / Pip-Boy style; uses your color palette below.",
  personaId: "fallout",
  lexiconId: "fallout",
  chrome: {
    bodyClass: "theme-fallout",
    locksPalette: false,
    defaultPalette: "green",
    forceThemeBgRgb: "6, 14, 10",
    forceThemeAccentRgb: "52, 211, 153",
  },
  fxDefaults: {
    scanlines: true,
    vignette: true,
    grain: 0.12,
    sharpCorners: true,
  },
  supportsAppearanceModes: false,
  tokens: falloutTokens,
};
