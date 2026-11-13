import type {
  AppearanceMode,
  BuiltInThemeId,
  ResolvedAppearance,
  ThemePackage,
  ThemeTokenMap,
} from "./types";
import { tacticalPackage } from "./tactical";
import { falloutPackage } from "./fallout";
import { fieldguidePackage } from "./fieldguide";
import { fruitPackage } from "./fruit";
import { modernPackage } from "./modern";

const BUILTIN: ThemePackage[] = [
  tacticalPackage,
  falloutPackage,
  fieldguidePackage,
  fruitPackage,
  modernPackage,
];

const BY_ID = new Map<string, ThemePackage>(BUILTIN.map((p) => [p.id, p]));

export function listBuiltInThemes(): ThemePackage[] {
  return BUILTIN.slice();
}

export function getThemePackage(id: string | null | undefined): ThemePackage | null {
  if (!id) return null;
  return BY_ID.get(id) ?? null;
}

export function getThemePackageOrDefault(id: string | null | undefined): ThemePackage {
  return getThemePackage(id) ?? tacticalPackage;
}

export function isBuiltInThemeId(id: string): id is BuiltInThemeId {
  return BY_ID.has(id);
}

/** Settings / experience picker rows (replaces inline EXPERIENCE_PRESETS). */
export function experiencePresetOptions(): { id: BuiltInThemeId; title: string; description: string }[] {
  return BUILTIN.map((p) => ({
    id: p.id,
    title: p.label,
    description: p.description,
  }));
}

export function resolveAppearanceMode(
  mode: AppearanceMode,
  isNight: boolean,
): ResolvedAppearance {
  if (mode === "auto") return isNight ? "dark" : "light";
  return mode;
}

export function tokensForPackage(
  pkg: ThemePackage,
  appearance: ResolvedAppearance | null,
): ThemeTokenMap | null {
  if (pkg.supportsAppearanceModes && appearance && pkg.tokensByAppearance?.[appearance]) {
    return pkg.tokensByAppearance[appearance] ?? null;
  }
  return pkg.tokens ?? null;
}

export { tacticalPackage, falloutPackage, fieldguidePackage, fruitPackage, modernPackage };
