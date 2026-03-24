# Athena — Master theme reference

This document describes **every visual theme system** in Athena so designers and developers can keep UI consistent. It maps **React state**, **DOM hooks**, **CSS**, and **Rust config** to what users actually see.

**Primary source files**

| Area | File |
|------|------|
| Palette list, hex presets, labels, experience presets, personas, custom colors | `src/App.tsx` (`THEMES`, `PRESET_COLORS`, `THEME_LABELS`, `EXPERIENCE_PRESETS`, `BackendCustomTheme`, effects on `document.documentElement` / `document.body`) |
| All styling | `src/App.css` (especially `[data-theme]`, `[data-persona]`, `body.theme-fallout`, `body.theme-survival-manual`, `[data-nightvision]`, `[data-fx-*]`) |
| Backend theme metadata | `src-tauri/src/theme.rs` |

---

## 1. Mental model: layers that combine

Athena’s look is **not** a single enum. Several axes stack:

1. **Color palette** — `data-theme` on `.app` (`blue`, `cyan`, … `navy`, or `custom`).
2. **Experience / layout personality** — `activeThemeId` in React (`default`, `fallout`, `fieldguide`, user-defined ids like `survival-manual`, or saved custom theme ids). This drives **persona** and sometimes **forced palette** (see below).
3. **Persona (AI + typography)** — `data-persona` on `.app`: `default` | `fallout` | `fieldguide`. Affects fonts, chat chrome, and many scoped styles.
4. **Body-level chrome flags** (not on `.app`):
   - **`body.theme-fallout`** — CRT phosphor treatment, green readout defaults, scanline-friendly base.
   - **`body.theme-survival-manual`** — “Old survival manual” vellum/paper book UI (warm paper, wood desk, ink colors).
5. **Night vision** — `data-nightvision="true"` on `.app` when enabled (and auto + night). Overrides palette to deep reds.
6. **Theme Studio effects** — `data-fx-scanlines`, `data-fx-vignette`, `data-fx-grain`, `data-fx-sharp` on `.app`, plus inline `--fx-scanline-opacity` and `--fx-grain`.
7. **Document CSS variables** — `--theme-bg-rgb` and `--theme-accent-rgb` on `:root` for glass surfaces; Fallout forces fixed green CRT RGBs when active.

When debugging “why does this screen look wrong?”, check **palette** + **persona** + **body classes** + **night vision** + **survival manual** in that order.

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

Glass tokens on `.app` derive from `--theme-bg-rgb` / accents (see §7).

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

When the user picks **custom colors** (or a **saved Theme Studio theme** is active), `data-theme="custom"` and inline styles set:

- `--custom-bg`, `--custom-primary`, `--custom-text`

`App.css` maps these to the same semantic tokens as presets (`.app[data-theme="custom"]`).

**Persistence:** `localStorage` keys `athena-custom-bg`, `athena-custom-primary`, `athena-custom-text`, and `athena-theme` = `custom`.

---

## 3. Experience presets (`activeThemeId`) vs color palette (`theme`)

**Experience** rows in Settings (see `EXPERIENCE_PRESETS` in `App.tsx`) are **not** the same as the 21 color names:

| `activeThemeId` | Title in UI | Effect |
|-----------------|-------------|--------|
| `default` | Survival Guide (Default) | Sets palette to **`blue`**, persona **`default`**, updates custom color snapshot to blue preset. |
| `fallout` | Fallout terminal (Default) | Keeps current **color theme** (or defaults to blue if needed), sets persona to **`fallout`**, applies CRT body treatment (§5). |
| `fieldguide` | Old survival manual (Default) | Forces palette to **`amber`**, persona **`fieldguide`**, snapshots amber colors. |

**Additional built-in id (not in `EXPERIENCE_PRESETS` dropdown but used in code/CSS):**

| Id | Meaning |
|----|---------|
| `survival-manual` | Activates **`body.theme-survival-manual`** — full vellum book chrome. Intended for a dedicated saved/bundled theme; Theme Studio can reference base `survival-manual` in saved JSON while the in-app “Starting point” select only lists `default` / `fallout` / `fieldguide`. |

**Saved custom themes** use arbitrary string ids (e.g. `custom-abc123`); selecting one sets `theme` to `custom` and loads colors + personality + effects from `BackendCustomTheme`.

---

## 4. Personas (`data-persona`)

`PERSONAS`: `default` | `fallout` | `fieldguide`.

| Persona | Label | Typography default (`resolveFontStack`) | Role |
|---------|-------|-------------------------------------------|------|
| `default` | Calm survival guide | System UI stack | Baseline glass/tactical UI. |
| `fallout` | Fallout terminal (dry, retro) | `SF Mono` / monospace stack | Pip-Boy–adjacent layout; pairs with **`body.theme-fallout`**. |
| `fieldguide` | Old survival manual | `Georgia` serif stack | Paper margins, warm “manual” panels in chat. |

**CSS:** Large blocks under `.app[data-persona="fallout"]` and `.app[data-persona="fieldguide"]` in `App.css` (sidebar width, borders, message typography, send button treatments, etc.).

**Theme Studio** stores `personality_id` on each saved theme; only the three values above are fully supported in UI selectors.

---

## 5. `body.theme-fallout` (CRT / Pip-Boy)

**When:** `activeThemeId === "fallout"` **or** `activePersonalityId === "fallout"` **or** Theme Studio preview with Fallout persona.

**What it does (summary):**

- Sets **document** `--theme-bg-rgb` / `--theme-accent-rgb` to fixed **dark green CRT** values (`FALLOUT_DEFAULT_BG_RGB`, `FALLOUT_DEFAULT_ACCENT_RGB` in `App.tsx`), overriding the selected palette for those RGB channels used by glass.
- Adds **`body.theme-fallout`**: green-tinted global text, image desaturation, heavy `.glass-panel` restyle, optional `body::after` CRT glow.
- **Comment in CSS:** Fallout accent colors still **follow** `[data-theme]` for many controls — palette choice remains meaningful.

**Fonts:** Prefer monospace; Theme Studio “Monospace / CRT” sets `font_family` to `mono-crt`.

---

## 6. `body.theme-survival-manual` (old survival manual book)

**When:** `activeThemeId === "survival-manual"` **or** Theme Studio preview with `base === "fieldguide"` **or** `base === "survival-manual"`.

**Important:** Fieldguide **persona alone** does **not** add this class — only the **`survival-manual`** theme id or studio preview bases above. This avoids leaving vellum chrome stuck when switching back to default (see comment in `App.tsx`).

**What it does:**

- Wood desk background on `body`, vellum tokens (`--manual-vellum`, `--manual-ink`, etc.).
- Overrides `.app` semantic colors to paper/ink.
- Restyles `.glass-panel`, modals, triage, chat regions with **fibrous masks**, **pulp noise**, **non-rectangular radii**, EB Garamond / typewriter accents.
- Extra overrides when **also** `data-persona="fieldguide"` (stacked selectors like `body.theme-survival-manual .app[data-persona="fieldguide"] …`).

**For new components:** If a surface must match the manual, test with **`theme-survival-manual` + `fieldguide`** together.

---

## 7. Global CSS variables (`:root` / `.app`)

### 7.1 Set on `document.documentElement` (in `App.tsx` effect)

- **`--theme-bg-rgb`** — Comma-separated RGB from resolved background hex, **unless Fallout CRT** (then forced dark green).
- **`--theme-accent-rgb`** — From resolved primary hex, **unless Fallout** (forced green accent).

Used by `.app` for:

- `--glass-bg`, `--glass-bg-soft`, `--glass-bg-strong`, borders, shadows.

### 7.2 Set on `.app` via inline style (custom / Theme Studio preview)

- `--custom-bg`, `--custom-primary`, `--custom-text` when `data-theme="custom"`.
- `--font` when font override or persona implies a stack.
- `--fx-scanline-opacity`, `--fx-grain` for effects.

### 7.3 Night vision (`.app[data-nightvision="true"]`)

Controlled by **night vision mode** + optional **auto** (sunrise/sunset via geolocation + `get_sun_times`).

**Effect:** Replaces semantic palette with **deep red** scheme, `filter: saturate(0.6)`, `color-scheme: dark`. Stacks on top of whatever theme was active.

---

## 8. Theme Studio saved themes (`BackendCustomTheme`)

Fields (see `App.tsx`):

| Field | Purpose |
|-------|---------|
| `id`, `name` | Stable id and display name. |
| `base` | Starting layout: `default` \| `fallout` \| `fieldguide` (and `survival-manual` allowed in saved data for manual chrome). Used by `resolveBaseColors()`. |
| `bg_color`, `primary_color`, `text_color` | Optional overrides; fall back to base palette. |
| `accent_color` | Reserved in type; wiring depends on feature usage. |
| `font_family` | `null` = match persona; `system` \| `mono-crt`. |
| `personality_id` | Persona stored with theme. |
| `scanlines_enabled`, `scanlines_opacity` | CRT lines overlay. |
| `vignette_enabled` | Edge darkening. |
| `film_grain` | Grain strength (0+). |
| `sharp_corners` | Squared UI (see §9). |

**Preview:** While Settings → Themes → Theme Studio is open with a draft, the main `.app` uses **preview** colors/persona/effects so changes are visible behind the modal.

---

## 9. FX attributes on `.app` (`data-fx-*`)

| Attribute | When | CSS behavior (`App.css` ~6750+) |
|-----------|------|-----------------------------------|
| `data-fx-vignette="1"` | Vignette enabled | Fixed inset box-shadow vignette, high z-index. |
| `data-fx-scanlines="1"` | Scanlines on | Horizontal lines, opacity from `--fx-scanline-opacity`. |
| `data-fx-grain="1"` | `film_grain > 0` | Dot grain; combined rules if scanlines + grain. |
| `data-fx-sharp="1"` | Sharp corners on | `border-radius: 0` on key chat controls. |

---

## 10. Night vision mode (user setting)

- **Storage:** `localStorage` `athena-night-vision-mode`: `auto` | `on` | `off`.
- **Auto:** Requires geolocation + successful `get_sun_times`; “night” = before sunrise or after sunset.
- **DOM:** `data-nightvision="true"` on the `.app` element (see §7.3).

---

## 11. Backend / Rust (`theme.rs`)

`ThemePreset` lists three ids aligned with experience presets: `default`, `fallout`, `fieldguide`. Used for config resolution (`get_active_theme`, `get_active_personality_id`). Custom themes are stored in app config (`CustomTheme`); see Rust `config` module.

---

## 12. Voice presets (related, not visual)

`VOICE_PRESETS`: `default`, `fallout`, `ultron` — TTS selection heuristics only; **not** `data-theme`. Listed here because Settings bundles “personality” with voice.

---

## 13. Checklist for new UI

1. Use **semantic tokens** (`var(--bg)`, `var(--primary)`, `var(--glass-bg)`, etc.), not hardcoded hex.
2. Test **`[data-theme]`** with at least **blue** and **custom**.
3. Test **`data-persona="fallout"`** and **`fieldguide`** for chat-adjacent UI.
4. Test **`body.theme-fallout`** and **`body.theme-survival-manual`** if touching global chrome or modals.
5. Test **`data-nightvision="true"`** for anything color-critical.
6. If adding overlays, confirm z-index relative to **`data-fx-*`** layers (vignette ~8999, scanlines/grain ~9000).
7. Prefer **`button.secondary`** or **`.send-btn`** patterns documented in `App.css` (global button rules under `.app`).

---

## 14. Quick reference: DOM hooks

| Hook | Where | Role |
|------|-------|------|
| `.app` | Root app div | `data-theme`, `data-persona`, `data-nightvision`, `data-fx-*`, inline vars |
| `body.theme-fallout` | `body` | CRT global look + RGB override source for glass |
| `body.theme-survival-manual` | `body` | Vellum book UI |
| `:root` style | JS | `--theme-bg-rgb`, `--theme-accent-rgb` |

---

*Last reviewed against `App.tsx` / `App.css` / `theme.rs` structure. When adding themes or tokens, update this file in the same PR.*
