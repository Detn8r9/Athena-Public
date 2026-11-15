import { isAthenaDesktopRuntime } from "./athenaRuntime";
import { lanApiBase } from "./lanClient";

/** Path kiwix-serve answers under (matches KIWIX_URL_ROOT in kiwix_serve.rs). */
export const KIWIX_ROOT = "/kiwix";

/**
 * Origin that serves the offline library. The station app talks to its own kiwix-serve; a phone
 * or tablet goes through the station's LAN server, which passes `/kiwix/...` through (kiwix-serve
 * itself only listens on the station).
 */
export function kiwixOrigin(): string {
  return isAthenaDesktopRuntime() ? "http://127.0.0.1:8080" : lanApiBase();
}

/** Home page of the offline library. */
export function kiwixHomeUrl(): string {
  return `${kiwixOrigin()}${KIWIX_ROOT}/`;
}

/** Chat cites archive articles as `kiwix:/kiwix/content/<book>/<article>`. */
export const KIWIX_SOURCE_PREFIX = "kiwix:";

export function isKiwixSource(filePath: string | null | undefined): boolean {
  return typeof filePath === "string" && filePath.startsWith(KIWIX_SOURCE_PREFIX);
}

/** Reader URL for a cited archive article. */
export function kiwixSourceUrl(filePath: string): string {
  const path = filePath.slice(KIWIX_SOURCE_PREFIX.length);
  return `${kiwixOrigin()}${path.startsWith("/") ? path : `/${path}`}`;
}
