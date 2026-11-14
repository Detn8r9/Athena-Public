import type React from "react";

type ThemeOrCustom = string;
type MasterThemeProfile = "tactical" | "fallout" | "night-vision" | "modern";
type AppearanceMode = "light" | "dark" | "auto";

type PaletteOption = { id: string; label: string };

export function ColorsAndThemes({
  theme,
  customBg,
  customPrimary,
  customText,
  masterThemeProfile,
  paletteOptions,
  onChangeMasterThemeProfile,
  appearance,
  onChangeAppearance,
  handlePresetChange,
  handleCustomColorChange,
}: {
  theme: ThemeOrCustom;
  customBg: string;
  customPrimary: string;
  customText: string;
  masterThemeProfile: MasterThemeProfile;
  paletteOptions: PaletteOption[];
  onChangeMasterThemeProfile: (next: MasterThemeProfile) => void;
  /** Light / Dark / Auto for themes that own their surfaces (Modern). */
  appearance?: AppearanceMode;
  onChangeAppearance?: (next: AppearanceMode) => void;
  handlePresetChange: (e: React.ChangeEvent<HTMLSelectElement>) => void;
  handleCustomColorChange: (which: "bg" | "primary" | "text", value: string) => void;
}) {
  return (
    <section className="library-section ui-colors-section" aria-label="Colors and themes">
      <h3>Master theme profile</h3>
      <div className="color-preset-wrap" style={{ marginTop: "0.5rem" }}>
        <label htmlFor="master-theme-profile-select">Master Theme Profile</label>
        <select
          id="master-theme-profile-select"
          className="theme-select"
          value={masterThemeProfile}
          onChange={(e) => onChangeMasterThemeProfile(e.target.value as MasterThemeProfile)}
          aria-label="Master theme profile"
        >
          <option value="tactical">Tactical (Default)</option>
          <option value="modern">Modern</option>
          <option value="fallout">Fallout</option>
          <option value="night-vision">Night Vision</option>
        </select>
      </div>

      {masterThemeProfile === "modern" ? (
        <>
          <p className="text-muted small" style={{ marginTop: "0.5rem" }}>
            Clean, high-contrast look with large touch targets. Uses its own colors, so the palette is not used.
          </p>
          <div className="color-preset-wrap" style={{ marginTop: "0.75rem" }}>
            <label htmlFor="modern-appearance-select">Appearance</label>
            <select
              id="modern-appearance-select"
              className="theme-select"
              value={appearance ?? "auto"}
              onChange={(e) => onChangeAppearance?.(e.target.value as AppearanceMode)}
              aria-label="Light or dark appearance"
            >
              <option value="light">Light (bright sun)</option>
              <option value="dark">Dark (night)</option>
              <option value="auto">Auto (time of day)</option>
            </select>
          </div>
          <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
            Auto follows local sunrise and sunset. Night vision (red) still takes over when it is on.
          </p>
        </>
      ) : null}

      {masterThemeProfile === "modern" ? null : (
      <>
      <h3 style={{ marginTop: "1rem" }}>Palette preset</h3>

      <div className="colors-editor">
        <div className="color-preset-wrap color-palette-bar">
          <label htmlFor="theme-preset-select">Palette Preset</label>
          <select
            id="theme-preset-select"
            className="theme-select"
            value={theme}
            onChange={handlePresetChange}
            aria-label="Color palette preset"
          >
            {paletteOptions.map((opt) => (
              <option key={opt.id} value={opt.id}>
                {opt.label}
              </option>
            ))}
            <option value="custom">Custom</option>
          </select>
        </div>

        <div className="color-fields color-fields-grid">
          {(["bg", "primary", "text"] as const).map((key) => {
            const label = key === "bg" ? "Background" : key === "primary" ? "Highlight" : "Text";
            const value = key === "bg" ? customBg : key === "primary" ? customPrimary : customText;
            return (
              <div key={key} className="color-field color-field-card">
                <label htmlFor={`color-${key}`}>{label}</label>
                <div className="color-card-preview" style={{ background: value }} aria-hidden />
                <div className="color-input-row color-input-row-stacked">
                  <input
                    id={`color-${key}`}
                    type="color"
                    className="color-swatch color-swatch-large"
                    value={value}
                    onChange={(e) => handleCustomColorChange(key, e.target.value)}
                    aria-label={`${label} color`}
                  />
                  <input
                    type="text"
                    className="color-hex settings-input"
                    value={value}
                    onChange={(e) => handleCustomColorChange(key, e.target.value)}
                    aria-label={`${label} color (hex)`}
                  />
                </div>
              </div>
            );
          })}
        </div>
      </div>
      </>
      )}
    </section>
  );
}

