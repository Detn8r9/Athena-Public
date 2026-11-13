export type {
  AppearanceMode,
  BuiltInThemeId,
  ResolvedAppearance,
  ThemeChromeFlags,
  ThemeFxDefaults,
  ThemeLexiconId,
  ThemePackage,
  ThemePackageId,
  ThemePersonaId,
  ThemeTokenMap,
} from "./types";

export {
  experiencePresetOptions,
  getThemePackage,
  getThemePackageOrDefault,
  isBuiltInThemeId,
  listBuiltInThemes,
  resolveAppearanceMode,
  tokensForPackage,
  tacticalPackage,
  falloutPackage,
  fieldguidePackage,
  fruitPackage,
  modernPackage,
} from "./registry";

export {
  applyTheme,
  clearFruitTokenOverrides,
  FALLOUT_PACKAGE_ACCENT_RGB,
  FALLOUT_PACKAGE_BG_RGB,
} from "./applyTheme";

export { survivalManualChromeBodyClass } from "./fieldguide";

import "./tactical/tactical.css";
import "./fallout/fallout.css";
import "./fieldguide/fieldguide.css";
import "./fruit/fruit.css";
import "./modern/modern.css";
