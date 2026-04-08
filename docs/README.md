# Documentation index

All paths below are relative to the **repository root** (parent of `athena/`).

## Product & UX

| Document | Contents |
|----------|----------|
| [`product/PLATFORM_COMPATIBILITY.md`](product/PLATFORM_COMPATIBILITY.md) | OS-specific behavior (device metrics, fullscreen, etc.) |
| [`plans/omnibus-usb-export-plan.md`](plans/omnibus-usb-export-plan.md) | Omnibus export notes (portable/offline packaging) |

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
