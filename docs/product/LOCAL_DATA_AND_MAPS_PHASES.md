# Local data & offline maps — phased rollout

## Phase 1 (done)

- Separate **Location** and **Offline maps** settings, install-local-data sheet, smallest matching Kiwix ZIM, network reachability probes, header download progress.

## Phase 1.5 — Map header icon (done)

- The **map (pin) icon** in the main header is shown **only when `get_downloaded_maps` is non-empty** (at least one Kiwix map ZIM registered in app storage).
- On launch, the app refreshes downloaded maps; removing the last map closes the map viewer and hides the icon.

## Phase 2 — Offline raster basemaps (done)

- **Rust** (`tile_pack.rs`): geocode via **Nominatim** (`geocode_place_query`), download **OSM** or **OpenTopoMap** PNG tiles into `app_data/tile_packs/<id>/z/x/y.png`, throttled requests + configurable max tiles (~8k). Progress/done/error reuse **`athena:kiwix-*`** events with ids `tiles:<pack_id>`.
- **Commands**: `geocode_place_query`, `list_tile_packs_cmd`, `offline_basemap_paths_cmd`, `remove_tile_pack_cmd`, `start_tile_pack_download`.
- **Settings → Offline maps**: radius slider, zoom min/max, optional geocode override, download street vs terrain, list/remove packs.
- **Map viewer**: basemap dropdown — online OSM, **offline OSM** (if pack exists), **offline OpenTopoMap** (if pack exists). Local tiles use Tauri **`convertFileSrc`** in a custom Leaflet `getTileUrl`.
- **Satellite**: not bundled (licensing); terrain uses OpenTopoMap (SRTM + OSM) as noted in the UI.

## Phase 3 — MBTiles interchange & UX (done)

- **Rust** (`mbtiles_io.rs`): pack folder (XYZ PNG) ↔ SQLite **MBTiles** (`metadata` + `tiles`, TMS `tile_row` from XYZ `y`).
- **`tile_pack.rs`**: `OfflineBasemapPaths` adds `imported` (latest `source == "mbtiles"` pack); `estimate_tile_download`; background **export** (`mbtiles_export:<pack_id>`) and **import** (`import:<job_id>`) on the same Kiwix progress events.
- **Commands**: `estimate_tile_pack_tiles`, `export_tile_pack_to_mbtiles_file`, `start_mbtiles_import`; `offline_basemap_paths_cmd` returns `{ osm, opentopo, imported }`.
- **Settings → Offline maps**: radius/zoom **presets**, **Preview tile count** before download, **Active tile jobs** list, **Import .mbtiles**, per-pack **Export .mbtiles** (native save dialog).
- **Map viewer**: basemap **Imported raster (MBTiles)** when an imported pack exists.

## Phase 4 — PMTiles & queue polish (done)

- **Rust** (`pmtiles_io.rs`, `pmtiles2`): import **raster** `.pmtiles` (PNG / JPEG / WebP / AVIF) → XYZ PNG on disk; **rejects MVT** vector archives with a clear error. Same **~8k tile** cap as downloads/imports.
- **`tile_pack.rs`**: `import_pmtiles_background`; packs with `source: "pmtiles"`; `offline_basemap_paths` treats **`mbtiles` and `pmtiles`** as user imports for the shared `imported` basemap slot.
- **Command**: `start_pmtiles_import`.
- **Settings**: **Import .pmtiles…**, copy for **user-supplied satellite/aerial** (legal/licensing), **active tile jobs** show **tiles vs bytes** and counts; **header download bar** shows **tile counts** or **byte sizes** depending on job type.
- **Map viewer**: imported basemap label **MBTiles / PMTiles**.

## Phase 5+ (optional)

- Cancel in-flight jobs, PMTiles **without** full XYZ extraction (direct read), richer global downloads panel.
