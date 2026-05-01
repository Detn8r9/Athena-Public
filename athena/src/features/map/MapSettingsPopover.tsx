import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { Layers } from "lucide-react";
import type { MapBasemap } from "./mapViewTypes";

const GLASS_PANEL =
  "map-settings-glass-panel";

type LayerFilters = { pois: boolean; roads: boolean; rivers: boolean };

export type MapSettingsPopoverProps = {
  basemap: MapBasemap;
  onBasemapChange: (next: MapBasemap) => void;
  canOfflineOsm: boolean;
  canOfflineTopo: boolean;
  canOfflineImported: boolean;
  canOfflineArchiveRaster: boolean;
  /** When false, hide satellite basemap chip, select option, and satellite download tools (e.g. no satellite MBTiles downloaded). */
  showSatelliteUi?: boolean;
  layerFilters: LayerFilters;
  setLayerFilters: Dispatch<SetStateAction<LayerFilters>>;
  satelliteCoverageOnMap: boolean;
  onSatelliteCoverageOnMapChange: (value: boolean) => void;
  satelliteDrawModeEnabled: boolean;
  onSatelliteDrawModeToggle: () => void;
  satelliteSelectionBounds: [[number, number], [number, number]] | null;
  satelliteDownloadBusy: boolean;
  satelliteDownloadStatus: string;
  satelliteDownloadJobId: string | null;
  onStartSatelliteDownloadFromSelection: () => void;
  onCancelSatelliteBoxSelection: () => void;
};

export function MapSettingsPopover({
  basemap,
  onBasemapChange,
  canOfflineOsm,
  canOfflineTopo,
  canOfflineImported,
  canOfflineArchiveRaster,
  layerFilters,
  setLayerFilters,
  satelliteCoverageOnMap,
  onSatelliteCoverageOnMapChange,
  satelliteDrawModeEnabled,
  onSatelliteDrawModeToggle,
  satelliteSelectionBounds,
  satelliteDownloadBusy,
  satelliteDownloadStatus,
  satelliteDownloadJobId,
  onStartSatelliteDownloadFromSelection,
  onCancelSatelliteBoxSelection,
  showSatelliteUi = true,
}: MapSettingsPopoverProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handlePointerDown(event: PointerEvent) {
      const el = rootRef.current;
      if (el && !el.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div className="map-settings-popover-root" ref={rootRef}>
      <button
        type="button"
        className="map-settings-trigger"
        onClick={() => setOpen((previous) => !previous)}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-controls="map-settings-popover-panel"
        aria-label="Map settings"
        title="Map settings"
      >
        <Layers size={22} strokeWidth={2} aria-hidden />
      </button>

      {open && (
        <div
          id="map-settings-popover-panel"
          className={GLASS_PANEL}
          role="dialog"
          aria-label="Map settings"
        >
          <div className="map-settings-popover-inner">
            <div className="map-settings-section">
              <div className="map-settings-section-title">Map type</div>
              <div className="map-settings-chip-row">
                <button
                  type="button"
                  className={`map-settings-layer-chip${basemap === "online_osm" ? " is-active" : ""}`}
                  onClick={() => onBasemapChange("online_osm")}
                >
                  Default
                </button>
                {canOfflineArchiveRaster && showSatelliteUi && (
                  <button
                    type="button"
                    className={`map-settings-layer-chip${basemap === "offline_archive_raster" ? " is-active" : ""}`}
                    onClick={() => onBasemapChange("offline_archive_raster")}
                  >
                    Satellite
                  </button>
                )}
                {canOfflineTopo && (
                  <button
                    type="button"
                    className={`map-settings-layer-chip${basemap === "offline_topo" ? " is-active" : ""}`}
                    onClick={() => onBasemapChange("offline_topo")}
                  >
                    Terrain
                  </button>
                )}
              </div>
            </div>

            <div className="map-settings-section">
              <div className="map-settings-section-title">Basemap</div>
              <select
                className="map-settings-basemap-select"
                value={basemap}
                onChange={(event) => onBasemapChange(event.target.value as MapBasemap)}
                aria-label="Map basemap"
              >
                <option value="online_osm">OpenStreetMap (online)</option>
                <option value="offline_osm" disabled={!canOfflineOsm}>
                  OpenStreetMap (offline cache){!canOfflineOsm ? " — download in Settings" : ""}
                </option>
                <option value="offline_topo" disabled={!canOfflineTopo}>
                  Terrain / hillshade (OpenTopoMap offline){!canOfflineTopo ? " — download in Settings" : ""}
                </option>
                <option value="offline_imported" disabled={!canOfflineImported}>
                  Imported raster (MBTiles / PMTiles){!canOfflineImported ? " — import in Settings" : ""}
                </option>
                {showSatelliteUi ? (
                  <option value="offline_archive_raster" disabled={!canOfflineArchiveRaster}>
                    Satellite from archives{!canOfflineArchiveRaster ? " — enable in Layers (gear)" : ""}
                  </option>
                ) : null}
              </select>
            </div>

            <div className="map-settings-section">
              <div className="map-settings-section-title">Layers</div>
              <label className="map-settings-check">
                <input
                  type="checkbox"
                  checked={layerFilters.pois}
                  onChange={(event) => setLayerFilters((previous) => ({ ...previous, pois: event.target.checked }))}
                />
                Points of interest
              </label>
              <label className="map-settings-check">
                <input
                  type="checkbox"
                  checked={layerFilters.roads}
                  onChange={(event) => setLayerFilters((previous) => ({ ...previous, roads: event.target.checked }))}
                />
                Road names (online OSM only)
              </label>
              <label className="map-settings-check">
                <input
                  type="checkbox"
                  checked={layerFilters.rivers}
                  onChange={(event) => setLayerFilters((previous) => ({ ...previous, rivers: event.target.checked }))}
                />
                Rivers (online OSM only)
              </label>
            </div>

            {showSatelliteUi ? (
            <div className="map-settings-section map-settings-section-satellite">
              <div className="map-settings-section-title map-settings-section-title-satellite">Satellite tools</div>
              <label className="map-settings-check">
                <input
                  type="checkbox"
                  checked={satelliteCoverageOnMap}
                  onChange={(event) => onSatelliteCoverageOnMapChange(event.target.checked)}
                />
                Show satellite coverage overlay
              </label>
              <p className="map-settings-help">
                Solid red box: downloaded area. Dashed red box: your next download area.
              </p>
              <div className="map-settings-satellite-actions">
                <button
                  type="button"
                  className={`map-settings-satellite-btn${satelliteDrawModeEnabled ? " is-active" : ""}`}
                  onClick={onSatelliteDrawModeToggle}
                >
                  {satelliteDrawModeEnabled ? "Stop drawing box" : "Draw download box"}
                </button>
                <button
                  type="button"
                  className="map-settings-satellite-btn"
                  disabled={satelliteSelectionBounds === null || satelliteDownloadBusy}
                  onClick={onStartSatelliteDownloadFromSelection}
                >
                  {satelliteDownloadBusy ? "Downloading…" : "Download selected area"}
                </button>
                <button
                  type="button"
                  className="map-settings-satellite-btn"
                  disabled={satelliteDownloadBusy === true && satelliteDownloadJobId !== null}
                  onClick={onCancelSatelliteBoxSelection}
                >
                  Cancel
                </button>
              </div>
              {satelliteDownloadJobId !== null && (
                <p className="map-settings-help">
                  Job: <code>{satelliteDownloadJobId}</code>
                </p>
              )}
              {satelliteDownloadStatus.length > 0 && (
                <p className="map-settings-satellite-status" aria-live="polite">
                  {satelliteDownloadStatus}
                </p>
              )}
              <p className="map-settings-help">Tip: click once for one corner, click again for the opposite corner.</p>
            </div>
            ) : null}
          </div>
        </div>
      )}
    </div>
  );
}
