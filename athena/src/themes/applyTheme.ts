import {
  getThemePackage,
  resolveAppearanceMode,
  tokensForPackage,
} from "./registry";
import { survivalManualChromeBodyClass } from "./fieldguide";
import type { AppearanceMode, ResolvedAppearance, ThemeTokenMap } from "./types";

const KNOWN_BODY_CLASSES = [
  "theme-fallout",
  "theme-survival-manual",
  "theme-fruit",
  "theme-modern",
] as const;

const TOKEN_VAR_MAP: Record<keyof ThemeTokenMap, string> = {
  bg: "--bg",
  surface: "--surface",
  surfaceSolid: "--surface-solid",
  elevated: "--theme-elevated",
  primary: "--primary",
  secondary: "--secondary",
  text: "--text",
  textMuted: "--text-muted",
  border: "--border",
  hairline: "--theme-hairline",
  danger: "--theme-danger",
  success: "--theme-success",
  userBubble: "--user-bubble",
  hoverOverlay: "--hover-overlay",
  glow: "--glow",
  radius: "--theme-radius",
  shadow: "--theme-shadow",
  font: "--font",
};

/** Match historical Fallout CRT `--theme-bg-rgb` / accent from App.tsx. */
export const FALLOUT_PACKAGE_BG_RGB = "6, 14, 10";
export const FALLOUT_PACKAGE_ACCENT_RGB = "52, 211, 153";

export type ApplyThemeOptions = {
  themeId: string;
  appearanceMode?: AppearanceMode;
  isNight?: boolean;
  survivalManualChrome?: boolean;
  forceFalloutChrome?: boolean;
  appEl?: HTMLElement | null;
};

function applyTokenMap(el: HTMLElement, tokens: ThemeTokenMap | null, owned: boolean) {
  for (const key of Object.keys(TOKEN_VAR_MAP) as (keyof ThemeTokenMap)[]) {
    const cssVar = TOKEN_VAR_MAP[key];
    if (owned && tokens) {
      el.style.setProperty(cssVar, tokens[key]);
    } else if (owned) {
      el.style.removeProperty(cssVar);
    }
  }
}

/**
 * Single apply path for theme packages: DOM attrs + Fruit token CSS vars + body chrome.
 * Palette `--theme-bg-rgb` for non-Fruit themes remains the caller's responsibility
 * except Fallout CRT fixed RGB when Fallout chrome is active.
 */
export function applyTheme(opts: ApplyThemeOptions): {
  resolvedAppearance: ResolvedAppearance | null;
  isFruit: boolean;
  isFalloutChrome: boolean;
  /** True when the package set `--theme-bg-rgb` / accent itself (callers skip palette RGB). */
  ownsThemeRgb: boolean;
} {
  const pkg = getThemePackage(opts.themeId);
  const isFruit = pkg?.id === "fruit";
  const body = document.body;
  const root = document.documentElement;
  const app =
    opts.appEl ??
    (document.querySelector(".app") as HTMLElement | null);

  for (const cls of KNOWN_BODY_CLASSES) {
    body.classList.remove(cls);
  }

  const isFalloutChrome =
    Boolean(opts.forceFalloutChrome) || opts.themeId === "fallout";

  const isSurvivalManual =
    Boolean(opts.survivalManualChrome) || opts.themeId === "survival-manual";

  if (isFalloutChrome) {
    body.classList.add("theme-fallout");
  }
  if (isSurvivalManual) {
    body.classList.add(survivalManualChromeBodyClass);
  }
  if (pkg?.supportsAppearanceModes && pkg.chrome.bodyClass) {
    body.classList.add(pkg.chrome.bodyClass);
  }

  let resolvedAppearance: ResolvedAppearance | null = null;
  if (pkg?.supportsAppearanceModes) {
    resolvedAppearance = resolveAppearanceMode(
      opts.appearanceMode ?? "auto",
      Boolean(opts.isNight),
    );
  }

  const themeAttrId =
    opts.themeId === "survival-manual"
      ? "fieldguide"
      : pkg?.id ?? (opts.themeId || "default");

  body.setAttribute("data-athena-theme", themeAttrId);
  if (resolvedAppearance) {
    body.setAttribute("data-appearance", resolvedAppearance);
  } else {
    body.removeAttribute("data-appearance");
  }

  if (app) {
    app.setAttribute("data-athena-theme", themeAttrId);
    if (resolvedAppearance) {
      app.setAttribute("data-appearance", resolvedAppearance);
    } else {
      app.removeAttribute("data-appearance");
    }

    const tokens =
      pkg?.supportsAppearanceModes && !pkg.chrome.cssOwnsTokens && resolvedAppearance
        ? tokensForPackage(pkg, resolvedAppearance)
        : null;
    applyTokenMap(app, tokens, true);
  }

  if (isFalloutChrome) {
    root.style.setProperty("--theme-bg-rgb", FALLOUT_PACKAGE_BG_RGB);
    root.style.setProperty("--theme-accent-rgb", FALLOUT_PACKAGE_ACCENT_RGB);
  }

  let ownsThemeRgb = false;
  const pkgRgb = resolvedAppearance ? pkg?.chrome.rgbByAppearance?.[resolvedAppearance] : undefined;
  if (pkgRgb && !isFalloutChrome) {
    root.style.setProperty("--theme-bg-rgb", pkgRgb.bg);
    root.style.setProperty("--theme-accent-rgb", pkgRgb.accent);
    ownsThemeRgb = true;
  }

  return { resolvedAppearance, isFruit, isFalloutChrome, ownsThemeRgb };
}

export function clearFruitTokenOverrides(appEl?: HTMLElement | null) {
  const app = appEl ?? (document.querySelector(".app") as HTMLElement | null);
  if (!app) return;
  applyTokenMap(app, null, true);
}
