/**
 * Builds URLs for offline map archives served by Tauri custom protocols.
 * Windows uses `http://{scheme}.localhost/...`; macOS and Linux use `{scheme}://localhost/...`.
 */

import { isAthenaWebRuntime } from "../../../shared/ipc/athenaRuntime";
import { lanApiBase } from "../../../shared/ipc/lanClient";

/** In a browser on the network, map files come from the station's web server at `/maps/`. */
function webMapsBase(): string | null {
  if (!isAthenaWebRuntime()) return null;
  const base = lanApiBase() || (typeof window !== "undefined" ? window.location.origin : "");
  return `${base}/maps`;
}

export function isWindowsWebviewHost(): boolean {
  if (typeof navigator === "undefined") {
    return false;
  }
  return navigator.userAgent.includes("Windows");
}

/** Authority URL for whole-archive range requests (`athena-map` protocol). */
export function buildAthenaMapArchiveAuthorityUrl(fileName: string): string {
  const encoded = encodeURIComponent(fileName);
  const web = webMapsBase();
  if (web !== null) {
    return `${web}/${encoded}`;
  }
  if (isWindowsWebviewHost()) {
    return `http://athena-map.localhost/${encoded}`;
  }
  return `athena-map://localhost/${encoded}`;
}

/**
 * MapLibre PMTiles source URL: the `pmtiles://` prefix activates the global `pmtiles` protocol handler,
 * which loads bytes via the nested `athena-map` archive URL.
 */
export function buildPmtilesMapLibreUrl(fileName: string): string {
  return `pmtiles://${buildAthenaMapArchiveAuthorityUrl(fileName)}`;
}

/** XYZ raster tile URL template for MBTiles (`athena-mbtiles` protocol). */
export function buildMbTilesRasterTilesTemplate(fileName: string): string {
  const encoded = encodeURIComponent(fileName);
  const web = webMapsBase();
  if (web !== null) {
    return `${web}/${encoded}/{z}/{x}/{y}`;
  }
  if (isWindowsWebviewHost()) {
    return `http://athena-mbtiles.localhost/${encoded}/{z}/{x}/{y}`;
  }
  return `athena-mbtiles://localhost/${encoded}/{z}/{x}/{y}`;
}

/** XYZ raster tiles from `.mbtiles` or `.pmtiles` via unified `athena-map` (per-tile SQLite/PMTiles reads on the host). */
export function buildAthenaMapRasterTilesTemplate(fileName: string): string {
  const encoded = encodeURIComponent(fileName);
  const web = webMapsBase();
  if (web !== null) {
    return `${web}/${encoded}/{z}/{x}/{y}`;
  }
  if (isWindowsWebviewHost()) {
    return `http://athena-map.localhost/${encoded}/{z}/{x}/{y}`;
  }
  return `athena-map://localhost/${encoded}/{z}/{x}/{y}`;
}
