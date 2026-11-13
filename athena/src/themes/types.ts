/** Built-in theme package ids (compat: `default` is the tactical survival guide). */
export type BuiltInThemeId = "default" | "fallout" | "fieldguide" | "fruit" | "modern";

export type ThemePackageId = BuiltInThemeId | string;

/** Persona / AI voice ids owned by packages (custom themes may use the same set). */
export type ThemePersonaId = "default" | "fallout" | "fieldguide" | "fruit";

/** Lexicon axis used by LexiconProvider. */
export type ThemeLexiconId = "survival" | "fallout" | "fruit";

/** Fruit (and future dual-mode packages) appearance preference. */
export type AppearanceMode = "light" | "dark" | "auto";

/** Resolved appearance after Auto / time-of-day. */
export type ResolvedAppearance = "light" | "dark";

/** Semantic color + chrome tokens applied as CSS custom properties. */
export type ThemeTokenMap = {
  bg: string;
  surface: string;
  surfaceSolid: string;
  elevated: string;
  primary: string;
  secondary: string;
  text: string;
  textMuted: string;
  border: string;
  hairline: string;
  danger: string;
  success: string;
  userBubble: string;
  hoverOverlay: string;
  glow: string;
  radius: string;
  shadow: string;
  font: string;
};

export type ThemeFxDefaults = {
  scanlines: boolean;
  vignette: boolean;
  grain: number;
  sharpCorners: boolean;
};

export type ThemeChromeFlags = {
  /** Body class for CRT / vellum chrome (e.g. `theme-fallout`). */
  bodyClass?: string;
  /** When true, palette picker is locked (fixed look). */
  locksPalette?: boolean;
  /** Default palette id when selecting this experience (`blue`, `amber`, …). */
  defaultPalette?: string;
  /** Force fixed RGB triple for `--theme-bg-rgb` / accent (Fallout CRT). */
  forceThemeBgRgb?: string;
  forceThemeAccentRgb?: string;
  /** Appearance-aware `--theme-bg-rgb` / `--theme-accent-rgb` (packages that own their surfaces). */
  rgbByAppearance?: Partial<Record<ResolvedAppearance, { bg: string; accent: string }>>;
  /**
   * When true the package's CSS file defines its tokens (scoped by `data-athena-theme` +
   * `data-appearance`), so they are not written inline and night vision can still override them.
   */
  cssOwnsTokens?: boolean;
};

/**
 * One section of the codebase owns everything a theme can change.
 * Adding a theme = new package folder + register in `registry.ts`.
 */
export type ThemePackage = {
  id: BuiltInThemeId;
  /** Folder / registry key (tactical package still uses id `default`). */
  packageKey: string;
  label: string;
  description: string;
  personaId: ThemePersonaId;
  lexiconId: ThemeLexiconId;
  chrome: ThemeChromeFlags;
  fxDefaults: ThemeFxDefaults;
  /** When true, Settings shows Light / Dark / Auto. */
  supportsAppearanceModes: boolean;
  /** Tokens used when appearance modes are off (single look). */
  tokens?: ThemeTokenMap;
  tokensByAppearance?: Partial<Record<ResolvedAppearance, ThemeTokenMap>>;
};
