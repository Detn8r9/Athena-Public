/**
 * A browser on the LAN bridge can hold an app page from before the station was rebuilt or
 * updated. Its screens are loaded on demand from content-hashed files, and after an update those
 * old files are gone, so opening a screen fails. The fix is a reload, which fetches the new build.
 */

const RELOAD_KEY = "athena:stale-build-reload-at";

/** True for the errors browsers give when an on-demand screen file cannot be loaded. */
export function isStaleBuildError(error: unknown): boolean {
  const msg = error instanceof Error ? error.message : String(error ?? "");
  return (
    /Failed to fetch dynamically imported module/i.test(msg) ||
    /error loading dynamically imported module/i.test(msg) ||
    /Importing a module script failed/i.test(msg) ||
    /is not a valid JavaScript MIME type/i.test(msg) ||
    /Unable to preload CSS/i.test(msg)
  );
}

/** Reload to pick up the station's current build, at most once a minute so a real outage cannot loop. */
export function reloadForNewBuild(): boolean {
  try {
    const last = Number(sessionStorage.getItem(RELOAD_KEY) ?? 0);
    if (Date.now() - last < 60_000) return false;
    sessionStorage.setItem(RELOAD_KEY, String(Date.now()));
  } catch {
    // storage blocked: still reload once
  }
  window.location.reload();
  return true;
}

/** Vite reports failed screen loads as `vite:preloadError`; answer it with one reload. */
export function installStaleBuildRecovery() {
  window.addEventListener("vite:preloadError", (event) => {
    if (reloadForNewBuild()) event.preventDefault();
  });
}
