// Reference: Kiwix-related code that was in lib.rs (commands, events, handler registration).
// To restore: merge these back into lib.rs and re-register the commands.

// use kiwix::{download_zim, get_catalog_page, KiwixCatalogPage, KiwixEntry, KiwixFacets, KiwixSort};

// const KIWIX_PROGRESS_EVENT: &str = "athena:kiwix-progress";
// const KIWIX_DONE_EVENT: &str = "athena:kiwix-done";
// const KIWIX_ERROR_EVENT: &str = "athena:kiwix-error";

// #[tauri::command]
// fn get_kiwix_catalog(app: tauri::AppHandle, force_refresh: bool) -> Vec<KiwixEntry> { ... }

// #[tauri::command]
// fn get_kiwix_catalog_page(app, search, category, language, sort, offset, limit, force_refresh) -> KiwixCatalogPage { ... }

// #[tauri::command]
// fn download_kiwix_zim(app, url) -> Result<(), String> { ... }  // spawns thread, emits KIWIX_*_EVENT

// invoke_handler: get_kiwix_catalog, get_kiwix_catalog_page, download_kiwix_zim
