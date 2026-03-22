# Documentation index

All paths below are relative to the **repository root** (parent of `athena/`).

## Product & UX

| Document | Contents |
|----------|----------|
| [`product/LOCAL_DATA_AND_MAPS_PHASES.md`](product/LOCAL_DATA_AND_MAPS_PHASES.md) | Local data, maps, phased rollout notes |
| [`product/PLATFORM_COMPATIBILITY.md`](product/PLATFORM_COMPATIBILITY.md) | OS-specific behavior (device metrics, fullscreen, etc.) |

## Engineering

| Document | Contents |
|----------|----------|
| [`development/CODE_REVIEW.md`](development/CODE_REVIEW.md) | Code review notes |
| [`development/OPTIMIZATION_AUDIT.md`](development/OPTIMIZATION_AUDIT.md) | Performance / optimization audit |

## Archived reference (not part of the build)

| Path | Contents |
|------|----------|
| [`archive/kiwix/`](archive/kiwix/) | Old Kiwix catalog-search backend + command reference (restore instructions in that folder’s README) |

## Canonical app reference (in tree for `include_str!`)

The full app description used in-app for **`/athena`** is kept next to the Rust crate (not here):

- `athena/ATHENA_PROJECT_REFERENCE.md`

Update that file when changing product behavior the model should know about; then rebuild the app.
