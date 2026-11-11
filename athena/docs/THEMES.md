# Athena — Master theme reference

This document describes **every visual theme system** in Athena so designers and developers can keep UI consistent. It maps **theme packages**, **React state**, **DOM hooks**, **CSS**, and **Rust config** to what users actually see.

**Primary source files**

| Area | File |
|------|------|
| Theme packages (contract, registry, apply) | `src/themes/` (`types.ts`, `registry.ts`, `applyTheme.ts`, `tactical/`, `fallout/`, `fieldguide/`, `fruit/`) |
| Palette list, hex presets, experience wiring, Theme Studio | `src/app/App.tsx` |
| Shared layout + legacy chrome CSS | `src/app/App.css` (`[data-theme]`, `[data-persona]`, `body.theme-fallout`, `body.theme-survival-manual`, `[data-nightvision]`, `[data-fx-*]`) |
| Fruit full-app CSS | `src/themes/fruit/fruit.css` (scoped under `[data-athena-theme="fruit"]`) |
| Lexicon / persona copy | `src/shared/lexicon.tsx` |
| Backend theme metadata + appearance | `src-tauri/src/shared/theme.rs`, `config.rs` (`theme_id`, `personality_id`, `theme_appearance`) |

---

## 0. Theme package system (canonical)

Adding a theme = **new package folder + register** in `src/themes/registry.ts`. Packages declare:

- `id` / `label` / `description`
- `personaId` + `lexiconId`
- chrome flags (`bodyClass`, palette lock, CRT RGB)
- FX defaults
- token maps (optional; Fruit uses light/dark maps)
- `supportsAppearanceModes` (Fruit: Light / Dark / Auto)

**Apply path:** `applyTheme()` sets `data-athena-theme`, optional `data-appearance`, body chrome classes (`theme-fallout`, `theme-survival-manual`, `theme-fruit`), and Fruit-owned CSS variables. `App.tsx` still resolves palette hex → `--theme-bg-rgb` for non-Fruit experiences.

| Package folder | Experience id | Notes |
|----------------|---------------|--------|
| `tactical/` | `default` | Survival Guide; palette-driven; no intentional visual change from legacy default |
| `fallout/` | `fallout` | CRT via `body.theme-fallout`; CSS still in `App.css` |
| `fieldguide/` | `fieldguide` | Amber + fieldguide persona; vellum book chrome is separate `survival-manual` id |
| `fruit/` | `fruit` | Apple-clean full-app; light/dark/auto; own CSS + lexicon/persona |

---

## 1. Mental model: layers that combine

Athena’s look is **not** a single enum. Several axes stack:

1. **Theme package / experience** — `activeThemeId` (`default`, `fallout`, `fieldguide`, `fruit`, custom ids). Drives `data-athena-theme`, persona, lexicon, and sometimes forced palette.
2. **Color palette** — `data-theme` on `.app` (`blue`, `cyan`, … `navy`, or `custom`). Locked for **fieldguide** and **fruit**.
3. **Persona (AI + typography)** — `data-persona` on `.app`: `default` \| `fallout` \| `fieldguide` \| `fruit`.
4. **Fruit appearance** — `data-appearance="light|dark"` (preference: Light / Dark / **Auto** from local day/night). Independent of tactical night vision.
5. **Body-level chrome flags**:
   - **`body.theme-fallout`** — CRT phosphor treatment.
   - **`body.theme-survival-manual`** — vellum book UI (`survival-manual` id / studio bases).
   - **`body.theme-fruit`** — Fruit package hook (styles via `[data-athena-theme="fruit"]`).
6. **Night vision** — `data-nightvision="true"` on `.app` (tactical reds; not the same as Fruit Auto).
7. **Theme Studio effects** — `data-fx-scanlines`, `data-fx-vignette`, `data-fx-grain`, `data-fx-sharp` (Fruit turns FX off by default and suppresses leftover CRT overlays in CSS).
8. **Document CSS variables** — `--theme-bg-rgb` / `--theme-accent-rgb` on `:root`.

When debugging “why does this screen look wrong?”, check **package** + **palette** + **persona** + **appearance** + **body classes** + **night vision**.

---

## 2. Color palettes (`data-theme`)

Defined in `App.tsx` as `THEMES` + `PRESET_COLORS` + `THEME_LABELS`.

Each built-in palette maps in **`App.css`** to semantic tokens on **`.app[data-theme="…"]`**:

- `--bg`, `--surface`, `--surface-solid`
- `--primary`, `--secondary`
- `--border`, `--glow`
- `--user-bubble`, `--hover-overlay`
- `--text` / `--text-muted` (where applicable)
- `scrollbar-color`

Fruit overrides these under `[data-athena-theme="fruit"][data-appearance="…"]` in `fruit.css`.

### 2.1 Full preset table (canonical hex from `PRESET_COLORS`)

| `data-theme` | Background `#` | Primary `#` | Text `#` | UI label (`THEME_LABELS`) |
|--------------|----------------|-------------|----------|---------------------------|
| `blue` | `#0c1222` | `#38bdf8` | `#e8ecf4` | Blue |
| `cyan` | `#0a1628` | `#22d3ee` | `#e8ecf4` | Cyan |
| `violet` | `#140c1c` | `#a78bfa` | `#e8ecf4` | Violet |
| `silver` | `#0f0f12` | `#cbd5e1` | `#e8ecf4` | Silver |
| `green` | `#0a1612` | `#34d399` | `#e8ecf4` | Green |
| `amber` | `#1a1508` | `#fbbf24` | `#e8ecf4` | Amber |
| `rose` | `#1a0c12` | `#fb7185` | `#e8ecf4` | Rose |
| `teal` | `#0a1a1c` | `#2dd4bf` | `#e8ecf4` | Teal |
| `indigo` | `#0e1020` | `#818cf8` | `#e8e9ff` | Indigo |
| `slate` | `#0b1220` | `#94a3b8` | `#f1f5f9` | Slate |
| `crimson` | `#1a0a10` | `#fb7185` | `#ffe4e8` | Crimson |
| `orange` | `#1a1208` | `#fb923c` | `#fff7ed` | Orange |
| `lime` | `#0f1608` | `#a3e635` | `#ecfccb` | Lime |
| `emerald` | `#0a1512` | `#34d399` | `#d1fae5` | Emerald |
| `red` | `#1a0c0c` | `#f87171` | `#fef2f2` | Red |
| `yellow` | `#1a1608` | `#facc15` | `#fefce8` | Yellow |
| `magenta` | `#1a0a16` | `#f472b6` | `#fce7f3` | Magenta |
| `purple` | `#150c1a` | `#c084fc` | `#f3e8ff` | Purple |
| `pink` | `#1a0c14` | `#f9a8d4` | `#fdf2f8` | Pink |
| `brown` | `#1a120c` | `#d4a574` | `#faf5f0` | Brown |
| `navy` | `#060d1a` | `#60a5fa` | `#e8f0ff` | Navy |

### 2.2 Custom palette (`data-theme="custom"`)

When the user picks **custom colors** (or a **saved Theme Studio theme** is active), `data-theme="custom"` and inline styles set `--custom-bg`, `--custom-primary`, `--custom-text`.

**Persistence:** `localStorage` keys `athena-custom-bg`, `athena-custom-primary`, `athena-custom-text`, and `athena-theme` = `custom`.

---

## 3. Experience presets (`activeThemeId`)

Built-ins come from `experiencePresetOptions()` in `src/themes/registry.ts`:

| `activeThemeId` | Title in UI | Effect |
|-----------------|-------------|--------|
| `default` | Survival Guide (Default) | Palette **blue**, persona **default**, lexicon survival |
| `fallout` | Fallout terminal (Default) | CRT body treatment, persona **fallout**, Fallout lexicon |
| `fieldguide` | Old survival manual (Default) | Forces palette **amber**, persona **fieldguide** |
| `fruit` | Fruit | Clean system look; persona **fruit**; lexicon **fruit**; Appearance Light/Dark/Auto |

**Additional built-in id (not always in the dropdown):**

| Id | Meaning |
|----|---------|
| `survival-manual` | Activates **`body.theme-survival-manual`** — full vellum book chrome |

**Saved custom themes** use arbitrary string ids; selecting one sets `theme` to `custom` and loads colors + personality + effects from `BackendCustomTheme`. Theme Studio customs remain compatible via the existing shim.

---

## 4. Personas (`data-persona`)

`PERSONAS`: `default` | `fallout` | `fieldguide` | `fruit`.

| Persona | Label | Typography | Role |
|---------|-------|------------|------|
| `default` | Calm survival guide | System UI | Baseline glass/tactical UI |
| `fallout` | Fallout terminal | Monospace | Pip-Boy–adjacent; pairs with CRT body class |
| `fieldguide` | Old survival manual | Georgia serif | Paper margins / manual panels |
| `fruit` | Fruit | SF / system stack | Calm modern assistant; pairs with Fruit package |

**Lexicon:** `LexiconThemeId` = `survival` | `fallout` | `fruit` (`src/shared/lexicon.tsx`).

**Rust AI tone:** `build_personality_section` in `lib.rs` includes a Fruit branch.

---

## 5. Fruit appearance (Light / Dark / Auto)

- **UI:** Settings → Themes & Colors → when Fruit is selected, **Appearance** select.
- **Preference storage:** `localStorage` `athena-fruit-appearance` and config `theme_appearance` via `get_theme_config` / `set_theme_config`.
- **Auto:** uses the same local day/night signal as night-vision auto (`isNight` / sun times) but **does not** enable tactical night-vision reds.
- **DOM:** `data-appearance="light|dark"` on `.app` and `body`.

---

## 6. `body.theme-fallout` (CRT / Pip-Boy)

**When:** `activeThemeId === "fallout"` **or** Fallout persona / Theme Studio Fallout preview (via `applyTheme` `forceFalloutChrome`).

**What it does (summary):**

- Forces `--theme-bg-rgb` / `--theme-accent-rgb` to CRT greens.
- Adds **`body.theme-fallout`**: green-tinted global text, image desaturation, heavy `.glass-panel` restyle.
- Accent colors still largely follow `[data-theme]` for many controls.

Package ownership: `src/themes/fallout/` (CSS rules currently still in `App.css` for zero visual regression).

---

## 7. `body.theme-survival-manual` (old survival manual book)

**When:** `activeThemeId === "survival-manual"` **or** Theme Studio preview with `base === "fieldguide"` **or** `base === "survival-manual"`.

**Important:** Fieldguide **persona alone** does **not** add this class.

Package ownership: `src/themes/fieldguide/` (CSS still in `App.css`).

---

## 8. Global CSS variables (`:root` / `.app`)

### 8.1 Set on `document.documentElement`

- **`--theme-bg-rgb`** / **`--theme-accent-rgb`** — from palette, Fallout CRT, or Fruit appearance.

### 8.2 Fruit package tokens

Under Fruit, semantic vars (`--bg`, `--primary`, `--text`, …) are owned by `fruit.css` + `applyTheme` token application.

### 8.3 Night vision (`.app[data-nightvision="true"]`)

Controlled by night vision mode + optional auto. Replaces semantic palette with deep reds. Independent of Fruit Auto.

---

## 9. Theme Studio saved themes (`BackendCustomTheme`)

Fields unchanged; `base` may be `default` | `fallout` | `fieldguide` | `fruit` (and `survival-manual` in saved data). `personality_id` may be `fruit`.

---

## 10. FX attributes on `.app` (`data-fx-*`)

| Attribute | When | CSS behavior |
|-----------|------|--------------|
| `data-fx-vignette="1"` | Vignette enabled | Fixed inset box-shadow vignette |
| `data-fx-scanlines="1"` | Scanlines on | Horizontal lines |
| `data-fx-grain="1"` | `film_grain > 0` | Dot grain |
| `data-fx-sharp="1"` | Sharp corners on | Squared UI |

Fruit defaults FX off and suppresses leftover CRT overlays under `[data-athena-theme="fruit"]`.

---

## 11. Backend / Rust

`ThemePreset` ids: `default`, `fallout`, `fieldguide`, `fruit`.

Config fields: `theme_id`, `personality_id`, `theme_appearance` (`light` | `dark` | `auto`), `custom_themes`.

LAN bridge mirrors the same tuple shape for `get_theme_config` / `set_theme_config`.

---

## 12. Voice presets (related, not visual)

`VOICE_PRESETS`: `default`, `fallout`, `ultron` — TTS only; not `data-theme`.

---

## 13. Checklist for new UI

1. Prefer semantic tokens (`var(--bg)`, `var(--primary)`, …).
2. Test **`[data-theme]`** blue + custom.
3. Test personas **fallout**, **fieldguide**, **fruit**.
4. Test **`body.theme-fallout`**, **`body.theme-survival-manual`**, **`[data-athena-theme="fruit"]`** light + dark.
5. Test **`data-nightvision="true"`** separately from Fruit Auto.
6. Scope new theme chrome under package CSS / `[data-athena-theme="…"]` — do not patch other packages.

---

## 14. Switch-theme regression matrix

Manual smoke after theme work:

| From → To | Expect |
|-----------|--------|
| Fruit Light ↔ Fruit Dark | Surfaces/text/accent flip; no CRT/scanlines |
| Fruit Auto at day / night | Appearance follows `isNight`; night vision stays off unless user enabled it |
| Fruit → Survival Guide (`default`) | Tactical blue glass returns; Fruit lexicon gone; no `theme-fruit` |
| Fruit → Fallout | CRT body + Fallout lexicon; Fruit attrs cleared |
| Fruit → Fieldguide | Amber + fieldguide persona; no Fruit light UI |
| Fallout → Fieldguide → default | No stuck `theme-fallout` / `theme-survival-manual` |
| Fieldguide → survival-manual (if available) | Vellum book chrome appears |
| Custom Theme Studio theme | Still loads colors/persona/FX; does not break Fruit allowlist |

---

## 15. Quick reference: DOM hooks

| Hook | Where | Role |
|------|-------|------|
| `.app` | Root | `data-athena-theme`, `data-appearance`, `data-theme`, `data-persona`, `data-nightvision`, `data-fx-*` |
| `body[data-athena-theme]` | `body` | Package id for global chrome |
| `body.theme-fallout` | `body` | CRT |
| `body.theme-survival-manual` | `body` | Vellum book |
| `body.theme-fruit` | `body` | Fruit hook |
| `:root` style | JS | `--theme-bg-rgb`, `--theme-accent-rgb` |

---

*Last updated for theme package system + Fruit. When adding themes or tokens, update this file in the same PR.*
