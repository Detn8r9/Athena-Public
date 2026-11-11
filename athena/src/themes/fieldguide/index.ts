import type { ThemePackage, ThemeTokenMap } from "../types";

/** Field guide / old survival manual experience (amber + fieldguide persona). */
export const fieldguideTokens: ThemeTokenMap = {
  bg: "#e8dcc8",
  surface: "color-mix(in srgb, #d9c9a8 88%, #1a120c 12%)",
  surfaceSolid: "#d9c9a8",
  elevated: "#efe6d4",
  primary: "#2a2018",
  secondary: "#1a120c",
  text: "#1a120c",
  textMuted: "#453828",
  border: "color-mix(in srgb, #1a120c 22%, transparent)",
  hairline: "color-mix(in srgb, #1a120c 14%, transparent)",
  danger: "#8b3a2a",
  success: "#3d5c3a",
  userBubble: "rgba(42, 32, 22, 0.12)",
  hoverOverlay: "rgba(42, 32, 22, 0.1)",
  glow: "rgba(42, 32, 22, 0.2)",
  radius: "0.35rem",
  shadow: "0 10px 26px rgba(16, 10, 6, 0.24)",
  font: '"Georgia", "Times New Roman", Times, serif',
};

export const fieldguidePackage: ThemePackage = {
  id: "fieldguide",
  packageKey: "fieldguide",
  label: "Old survival manual (Default)",
  description: "Paper field-guide look (warm paper tones).",
  personaId: "fieldguide",
  lexiconId: "survival",
  chrome: {
    locksPalette: true,
    defaultPalette: "amber",
  },
  fxDefaults: {
    scanlines: false,
    vignette: false,
    grain: 0,
    sharpCorners: false,
  },
  supportsAppearanceModes: false,
  tokens: fieldguideTokens,
};

/**
 * Dedicated vellum book chrome (`body.theme-survival-manual`).
 * Kept as a chrome variant owned by the fieldguide package folder.
 */
export const survivalManualChromeBodyClass = "theme-survival-manual";
