# Plan: Fast wins — small features, high impact

**Scope:** Cross-cutting UX and supportability; each item should fit in **roughly 1–3 days** of focused work (excluding product review).  
**Related plans:** [quality-testing-and-bugs.md](quality-testing-and-bugs.md) (tests for new commands), [resilience-ux-trust.md](resilience-ux-trust.md) (backup, health banner).

---

## Goals

- Reduce **support burden** (diagnostics, discoverability).
- Improve **operator efficiency** (keyboard, data folder).
- Surface **background work** without redesigning the whole chrome.

---

## Priority 1 — Diagnostics export (recommended first)

**Problem:** Field issues are hard to reproduce without logs and environment facts.

**Backend**

- New Tauri command, e.g. `export_diagnostics_pack` or `build_diagnostics_json`:
  - App version / build identifier (from Tauri package info).
  - OS and arch (Rust `std::env::consts` or `sysinfo`).
  - Contents of **error log** via existing `get_error_log` path (truncate to last **N** KiB if huge).
  - **No secrets:** API keys only as booleans (`openai_key_set: true/false`); never echo key material.
  - Optional: sidecar presence flags (reuse resilience checks if available).

**Frontend**

- Troubleshooting / Developer settings: button **“Export diagnostics…”**
- Use `@tauri-apps/plugin-dialog` `save` to write a **`.json`** or **`.txt`** file.

**Key files**

- [`athena/src-tauri/src/lib.rs`](../../athena/src-tauri/src/lib.rs) — register command; or new [`athena/src-tauri/src/diagnostics.rs`](../../athena/src-tauri/src/diagnostics.rs).
- [`athena/src/App.tsx`](../../athena/src/App.tsx) — troubleshooting panel wiring.

**Acceptance**

- Exported file opens in a text editor; support can request it by name.
- Redacted fields verified by code review.

---

## Priority 2 — Keyboard shortcuts overlay

**Behavior**

- Global chord: **`?`** (shift+/) or **`Cmd+/`** (mac) / **`Ctrl+/`** (win/linux) opens a modal.
- Content: table of **navigation** from [`APP_VIEW_LABELS`](../../athena/src/appView.ts) + documented actions (new chat, lock, settings).

**Key files**

- New small component `ShortcutsModal.tsx` or extend [`HelpModal.tsx`](../../athena/src/HelpModal.tsx).
- [`athena/src/App.tsx`](../../athena/src/App.tsx) — keydown listener (respect inputs: ignore when `textarea`/`input` focused unless chord includes modifier).

**Acceptance**

- Overlay closes on Escape; focus returns to prior element.

---

## Priority 3 — Structured errors (incremental)

**Problem:** Opaque `invoke` strings complicate triage.

**Approach**

- For **selected** commands, return or map errors to `{ code: string, message: string }` (serde) where Tauri allows.
- UI: show **Copy** button for JSON or `code: message` line on error toasts.

**Scope control:** Start with **chat** + **export** only; expand in later PRs.

**Acceptance**

- At least two flows show copyable error codes on failure.

---

## Priority 4 — Background activity strip

**Problem:** Rebuild vector index, package downloads, and USB export each have progress signals; users lack a **single** place to look.

**Approach**

- Lightweight **top or bottom strip** aggregating:
  - Existing events: `athena:rebuild-progress`, `athena:aegis-export-progress` (and similar if present).
- Show: label + indeterminate or determinate progress; tap opens detail modal.

**Key files**

- [`athena/src/App.tsx`](../../athena/src/App.tsx) — consolidate `listen` handlers or small `useBackgroundTasks` hook.

**Acceptance**

- Two different long-running jobs both visible without conflicting state.

---

## Priority 5 — Open data folder

**Problem:** Users and support cannot quickly find `athena_index.db` and app data.

**Approach**

- Tauri command: resolve `app_data_dir` (or portable override) and call **reveal in folder** via `tauri-plugin-opener` or shell `open`/`explorer`/`xdg-open`.

**Key files**

- lib.rs command + Settings / Troubleshooting button.

**Acceptance**

- Finder / Explorer opens correct directory on each target OS once.

---

## Plan-level acceptance criteria

- [ ] **At least two** items from this list **shipped**; **diagnostics export** strongly preferred as one of them.
- [ ] New commands documented in [quality-testing-and-bugs.md](quality-testing-and-bugs.md) command contract table.

---

## Suggested order

1. Diagnostics export  
2. Open data folder  
3. Shortcuts overlay  
4. Background strip  
5. Structured errors (widest API surface—last)
