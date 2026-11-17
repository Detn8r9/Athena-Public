import type { AppView } from "../../app/appView";
import type { SettingsPanelId } from "../../pages/Settings/settingsSearchIndex";

/** Panels inside the map screen that can be opened from elsewhere. */
export type MapPanel = "layers" | "map_files" | "map_settings" | "directions" | "routing_setup";

/**
 * A place in ATHENA: a screen, optionally a tab inside it, optionally a section to scroll to.
 * Or a Settings panel. Any part of the app can `goTo(...)` one; App.tsx does the navigating.
 */
export type GoToTarget =
  | { view: AppView; tab?: string; section?: string; anchor?: string; mapPanel?: MapPanel }
  | { settings: SettingsPanelId; anchor?: string };

export const GO_TO_EVENT = "athena:go-to";

export function goTo(target: GoToTarget): void {
  window.dispatchEvent(new CustomEvent<GoToTarget>(GO_TO_EVENT, { detail: target }));
}

/**
 * Scroll a section into view once it exists (screens load lazily, so it may take a moment),
 * and mark it briefly with `data-goto-flash` so the eye lands on it.
 */
export function scrollToAnchorWhenReady(id: string, timeoutMs = 6000): void {
  const started = Date.now();
  const tick = () => {
    const el = document.getElementById(id);
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "start" });
      el.setAttribute("data-goto-flash", "1");
      window.setTimeout(() => el.removeAttribute("data-goto-flash"), 1800);
      return;
    }
    if (Date.now() - started < timeoutMs) window.setTimeout(tick, 120);
  };
  window.setTimeout(tick, 60);
}

// ── Map panel requests ──────────────────────────────────────────────────────
// The map screen may not be mounted yet when a request is made, so keep the last one until
// the map takes it, and also announce it for a map that is already open.

export const MAP_PANEL_EVENT = "athena:map-panel";
let pendingMapPanel: MapPanel | null = null;

export function requestMapPanel(panel: MapPanel): void {
  pendingMapPanel = panel;
  window.dispatchEvent(new CustomEvent<MapPanel>(MAP_PANEL_EVENT, { detail: panel }));
}

/** Called by the map when it mounts or hears MAP_PANEL_EVENT; returns and clears the request. */
export function takeMapPanelRequest(): MapPanel | null {
  const p = pendingMapPanel;
  pendingMapPanel = null;
  return p;
}
