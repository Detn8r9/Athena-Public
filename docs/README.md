# Documentation index

All paths below are relative to the **repository root** (parent of `athena/`).

## Product & UX

| Document | Contents |
|----------|----------|
| [`product/PLATFORM_COMPATIBILITY.md`](product/PLATFORM_COMPATIBILITY.md) | OS-specific behavior (device metrics, fullscreen, etc.) |
| [`plans/omnibus-usb-export-plan.md`](plans/omnibus-usb-export-plan.md) | Planned encrypted USB backup notes (separate from shipped **Aegis Protocol** tiered export) |

## Improvement roadmaps (whole app)

Five scoped plans for UI, quality, features, resilience, and bundle size. Paths are relative to the repo root.

| Plan | Focus |
|------|--------|
| [`plans/ui-app-wide.md`](plans/ui-app-wide.md) | Navigation, decomposing `App.tsx`, lazy loading, map UX, accessibility |
| [`plans/quality-testing-and-bugs.md`](plans/quality-testing-and-bugs.md) | Smoke tests, Vitest/Rust tests, command contracts, concurrency |
| [`plans/features-fast-wins.md`](plans/features-fast-wins.md) | Diagnostics export, shortcuts, structured errors, activity strip, open data folder |
| [`plans/resilience-ux-trust.md`](plans/resilience-ux-trust.md) | Drafts, DB backup, health banner, ErrorBoundary coverage, maintenance docs |
| [`plans/bundle-size-and-slim-builds.md`](plans/bundle-size-and-slim-builds.md) | `manualChunks`, Tauri resources, release profile, sidecars, USB size reality |

**Suggested order:** bundle/UI lazy loading → quality checklist + tests → resilience → fast features → slim CI artifacts (see each plan).

## QA

| Document | Contents |
|----------|----------|
| [`qa/SMOKE_TEST.md`](qa/SMOKE_TEST.md) | Short manual checklist before release |
| [`qa/COMMAND_CONTRACTS.md`](qa/COMMAND_CONTRACTS.md) | Tauri `invoke` contracts (extend when adding commands) |

## Resilience & offline readiness

ATHENA includes a resilience layer that verifies required sidecars at launch, runs WAL + `PRAGMA integrity_check` against the station database, and exposes a Maintenance Mode screen that lists the exact failures to correct.

## Archived reference (not part of the build)

| Path | Contents |
|------|----------|
| [`archive/kiwix/`](archive/kiwix/) | Old Kiwix catalog-search backend + command reference (restore instructions in that folder’s README) |

## Canonical app reference (in tree for `include_str!`)

The full app description used in-app for **`/athena`** is kept next to the Rust crate (not here):

- `athena/ATHENA_PROJECT_REFERENCE.md`

Update that file when changing product behavior the model should know about; then rebuild the app.
