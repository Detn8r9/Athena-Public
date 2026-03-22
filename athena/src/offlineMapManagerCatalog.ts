import { invoke } from "@tauri-apps/api/core";

/** Mirrors `OfflineMapCatalogEntry` from the Rust catalog (JSON key `type` for archive format). */
export interface OfflineMapCatalogEntry {
  id: string;
  name: string;
  region: string;
  type: string;
  size_mb: number;
  download_url: string;
}

/** Mirrors `MapDownloadProgressPayload` from the Rust download manager (snake_case JSON keys). */
export interface MapDownloadProgressEventPayload {
  map_id: string;
  bytes_downloaded: number;
  total_bytes: number | null;
}

/** Mirrors `MapDownloadCompletePayload` from the Rust download manager. */
export interface MapDownloadCompleteEventPayload {
  map_id: string;
  path: string;
}

/** Mirrors `MapDownloadErrorPayload` from the Rust download manager. */
export interface MapDownloadErrorEventPayload {
  map_id: string;
  message: string;
}

/** Loads the offline map manager catalog (embedded mock JSON until remote URL is wired). */
export async function getOfflineMapManagerCatalog(): Promise<OfflineMapCatalogEntry[]> {
  return invoke<OfflineMapCatalogEntry[]>("get_offline_map_manager_catalog");
}

/** Returns catalog map ids that already have a `.pmtiles` or `.mbtiles` file under the app data `maps/` directory. */
export async function listDownloadedOfflineMapIds(): Promise<string[]> {
  return invoke<string[]>("list_downloaded_offline_map_ids");
}

/**
 * Starts a background download of a PMTiles or MBTiles archive into the application data `maps/` folder.
 * Progress is reported through the `map_download_progress` event; completion through `map_download_complete` or `map_download_error`.
 */
export async function downloadOfflineMap(mapId: string, downloadUrl: string): Promise<void> {
  return invoke<void>("download_offline_map", { mapId, downloadUrl });
}
