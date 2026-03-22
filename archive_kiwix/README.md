# Archived Kiwix search feature

This folder contains the **previous** Kiwix catalog search UI and backend (OPDS fetch, XML parse, caching, infinite scroll, download-from-URL) so you can restore them later if needed.

## Contents

- **kiwix.rs** — Full Rust backend: OPDS/library XML fetch, parse, cache, `get_catalog_page`, `get_catalog_cached`, `download_zim`, fallback entries.
- **lib_kiwix_commands.reference.rs** — Reference copy of the Tauri commands and helpers that used this module: `get_kiwix_catalog`, `get_kiwix_catalog_page`, `download_kiwix_zim`, and the download thread that emitted `athena:kiwix-progress` / `athena:kiwix-done` / `athena:kiwix-error`.
- **kiwix_search_ui.reference.tsx** — Reference for the React state, handlers, and modal JSX that were in `App.tsx`: Kiwix state (entries, total, facets, search, filters, sort, loading, progress, message, banner dismissed), `loadKiwixPage`, infinite-scroll sentinel/observer, event listeners for progress/done/error, and the full “Search Kiwix” modal (toolbar, filters, list, load more, download buttons).

## Current behavior (after archive)

The app now uses a **manual download + local scan** flow:

- User opens https://library.kiwix.org in the browser to download ZIM files.
- User places ZIM files in their **library folder** (the same folder used for the app’s library).
- User clicks **Scan for Local Files**; the backend recursively scans that folder for `.zim` files (using `walkdir`) and returns the list. No in-app catalog fetch or download-from-URL.

## How to restore the old search feature

1. Replace `athena/src-tauri/src/kiwix.rs` with the contents of `archive_kiwix/kiwix.rs`.
2. In `athena/src-tauri/src/lib.rs`: re-add the `use` for the old types (`KiwixCatalogPage`, `KiwixEntry`, `KiwixFacets`, `KiwixSort`), re-add `get_kiwix_catalog`, `get_kiwix_catalog_page`, `download_kiwix_zim`, the download thread, and register those commands again (see `lib_kiwix_commands.reference.rs`).
3. In `athena/src/App.tsx`: re-add the Kiwix state, `loadKiwixPage`, the Kiwix event listeners, and the “Search Kiwix” modal (see `kiwix_search_ui.reference.tsx`). Re-add the “Search Kiwix” button and any related copy.
4. Adjust any renames or API changes (e.g. if the new flow introduced `DiscoveredZim` or `scan_kiwix_zim_files`) so the UI and backend match again.
