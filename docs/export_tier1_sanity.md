# Tier 1 (Essentials) export — sanity checklist

Use this after implementing **Essentials** behavior in `export_athena_to_usb` (`portable.rs`).

## Expected layout (destination root, e.g. `export_test/`)

- **`ATHENA/`** — install folder copied from beside the running executable (non-macOS) or platform-specific layout.
- **`data/maps/fairlee_vt_25mi.pmtiles`** — proximity extract from the `pmtiles` sidecar (`extract` with bbox around Fairlee, VT: **43.908°N, 72.143°W**, **25 mi** radius).
- **`data/athena_index.db`** — present **when** the local app data directory contains `athena_index.db` at export time (personnel / roster / library index).

## Should *not* appear on Essentials

- **`data/models/embeddings/...`** and **`data/models/rerankers/...`** — only **Recommended** and **Absolute** copy models (per current tier logic).
- **Priority payload packs** (Tier 2 fill) — only **Recommended** runs `tier2_copy_payload_priority_fill`.
- **Mirrored library / RAG document trees** under `documents/` — **Absolute** tier behavior.
- **Full `payload/` tree** — **Essentials** copies only **`Windows`** and **`Linux`** branches from bundled resources, not every payload sibling.

## PMTILES note

Extraction needs a **source** `.pmtiles` (local offline map or remote URL as configured). If no source is available, the export may fail at the `pmtiles extract` step—resolve a planet or regional PMTiles in app data first.

## Automated check (Rust)

Add a unit test on `approx_bbox_for_radius_miles(-72.143, 43.908, 25.0)` to lock the Fairlee bbox math (see `domains/deployment/portable.rs`).

## Academy Hub (manual)

After a dev build (`npm run tauri dev` in `athena/`), open **Academy**, switch to **PDF archive**, confirm units appear from `payload/Academy/**/metadata.json`, open a unit, and confirm student/teacher panes (add `reader.pdf` / `lecture_guide.pdf` to test PDF rendering).
