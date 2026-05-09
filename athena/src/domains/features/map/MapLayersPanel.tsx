import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { isProbablySatelliteArchiveFileName } from "./satelliteMapArchive";
import { getOfflineMapManagerCatalog, listDownloadedOfflineMapIds, type OfflineMapCatalogEntry } from "./offlineMapManagerCatalog";
import { Settings } from "lucide-react";
import { OfflineMapManagerDashboard } from "./OfflineMapManagerDashboard";
import { createPortal } from "react-dom";

type MapLayersPanelProps = {
  open: boolean;
  onClose: () => void;
  enabledMainMapRasterArchiveFileNames?: string[];
  onMainMapArchiveRasterLayersChange?: (archiveFileNames: string[]) => void;
  onOpenDataAcquisition?: () => void;
};

/**
 * Basemap info + enabled offline raster (MBTiles) from downloaded archives.
 */
export function MapLayersPanel({
  open,
  onClose,
  enabledMainMapRasterArchiveFileNames = [],
  onMainMapArchiveRasterLayersChange,
  onOpenDataAcquisition,
}: MapLayersPanelProps) {
  const [hasSat, setHasSat] = useState(false);
  const [downloadedIds, setDownloadedIds] = useState<string[]>([]);
  const [catalog, setCatalog] = useState<OfflineMapCatalogEntry[]>([]);
  const [archivesOpen, setArchivesOpen] = useState(false);
  const [tileRadiusMiles, setTileRadiusMiles] = useState(12);
  const [savedUserLocationAddress, setSavedUserLocationAddress] = useState("");

  const enabledRasterIds = useMemo(() => {
    return new Set(
      enabledMainMapRasterArchiveFileNames
        .map((n) => n.trim().toLowerCase())
        .filter((n) => n.endsWith(".mbtiles"))
        .map((n) => n.slice(0, -".mbtiles".length))
        .filter((id) => id.length > 0),
    );
  }, [enabledMainMapRasterArchiveFileNames]);

  const refreshFromDisk = useCallback(() => {
    void Promise.all([getOfflineMapManagerCatalog(), listDownloadedOfflineMapIds()]).then(([c, ids]) => {
      setCatalog(Array.isArray(c) ? c : []);
      setDownloadedIds((Array.isArray(ids) ? ids : []).slice().sort((a, b) => a.localeCompare(b)));
    });
  }, []);

  useEffect(() => {
    void (async () => {
      try {
        const files = await invoke<string[]>("get_enabled_main_map_raster_archive_file_names");
        setHasSat((files ?? []).some((f) => isProbablySatelliteArchiveFileName(f)));
      } catch {
        setHasSat(false);
      }
    })();
  }, []);

  useEffect(() => {
    if (!open) {
      return;
    }
    refreshFromDisk();
    const u = listen("athena:offline-maps-updated", () => {
      void (async () => {
        try {
          const files = await invoke<string[]>("get_enabled_main_map_raster_archive_file_names");
          setHasSat((files ?? []).some((f) => isProbablySatelliteArchiveFileName(f)));
        } catch {
          setHasSat(false);
        }
      })();
      refreshFromDisk();
    });
    return () => {
      void u.then((x) => x());
    };
  }, [open, refreshFromDisk]);

  const catalogById = useMemo(() => {
    const m = new Map<string, OfflineMapCatalogEntry>();
    for (const e of catalog) m.set(e.id, e);
    return m;
  }, [catalog]);

  useEffect(() => {
    if (!open) return;
    void invoke<string | null>("get_user_location")
      .then((v) => setSavedUserLocationAddress(String(v ?? "").trim()))
      .catch(() => setSavedUserLocationAddress(""));
  }, [open]);

  useEffect(() => {
    // Intentionally local-only for now; MapDisplaySettings doesn’t persist a radius.
    // Default (12mi) gives a safe starter that stays consistent with prior settings.
  }, [open]);

  if (!open) {
    return null;
  }

  const archivesModal =
    archivesOpen && typeof document !== "undefined"
      ? createPortal(
          <OfflineMapManagerDashboard
            open={archivesOpen}
            onClose={() => setArchivesOpen(false)}
            savedUserLocationAddress={savedUserLocationAddress}
            tileGeocodeOverride={""}
            tileRadiusMiles={tileRadiusMiles}
            onTileRadiusMilesChange={(m) => setTileRadiusMiles(m)}
            onOpenDataAcquisition={onOpenDataAcquisition}
          />,
          document.body,
        )
      : null;

  const setRasterEnabled = (id: string, on: boolean) => {
    if (onMainMapArchiveRasterLayersChange == null) {
      return;
    }
    const next = new Set(enabledRasterIds);
    if (on) {
      next.add(id);
    } else {
      next.delete(id);
    }
    onMainMapArchiveRasterLayersChange([...next].sort((a, b) => a.localeCompare(b)).map((x) => `${x}.mbtiles`));
  };

  return (
    <div
      className="map-layers-popout glass-panel"
      role="dialog"
      style={{
        position: "fixed",
        top: "var(--header-offset, 72px)",
        zIndex: 1205,
        minWidth: 280,
        maxWidth: 360,
        padding: 12,
        pointerEvents: "auto",
      }}
    >
      <div className="map-panel__head" style={{ display: "flex", justifyContent: "space-between", marginBottom: 8, gap: 8 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button
            type="button"
            className={`close-btn${archivesOpen ? " is-active" : ""}`}
            onClick={() => setArchivesOpen(true)}
            aria-label="Open map archives"
            title="Map archives"
          >
            <Settings size={16} />
          </button>
          <strong>Layers</strong>
        </div>
        <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      <div className="map-panel__body">
        <p className="map-layers-hint" style={{ fontSize: 12, opacity: 0.9 }}>
          Use Map settings (gear) to switch basemaps. Tactical / resource / hazard overlays are under{" "}
          <strong>Overlays</strong> on the left rail.
        </p>
        {hasSat ? (
          <p className="map-satellite-chip" style={{ marginTop: 8, fontSize: 12, color: "var(--accent, #38bdf8)" }}>
            Satellite: enabled archives detected. Choose “Satellite” in map settings to overlay.
          </p>
        ) : (
          <p style={{ marginTop: 8, fontSize: 12, opacity: 0.75 }}>
            No satellite MBTiles in your enabled archives. Use the gear above to download an area.
          </p>
        )}

        <div
          className="map-layers-downloads"
          style={{ marginTop: 14, paddingTop: 14, boxShadow: "0 -1px 0 0 color-mix(in srgb, var(--border) 20%, transparent)" }}
        >
          <strong style={{ fontSize: 13, letterSpacing: 0.02, fontWeight: 600 }}>Downloaded archives</strong>
          <p className="text-muted small" style={{ margin: "6px 0 10px" }}>
            Vector (PMTiles) and raster (MBTiles) on disk. Only MBTiles can be added as a basemap layer here; PMTiles stay available to the Protomaps-style overlay path.
          </p>
          {downloadedIds.length === 0 ? (
            <p className="text-muted small" style={{ margin: 0 }}>
              None yet. Use the <strong>gear</strong> above to download an area.
            </p>
          ) : (
            <ul className="map-layers-archives" style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: 8 }}>
              {downloadedIds.map((id) => {
                const ent = catalogById.get(id);
                const typeLower = ent != null ? ent.type.toLowerCase() : id.startsWith("satellite_") ? "mbtiles" : "pmtiles";
                const isMbtiles = typeLower === "mbtiles";
                const label = ent != null ? ent.name : `Local · ${id}`;
                const pmtilesBadge = !isMbtiles ? "Ready" : null;
                return (
                  <li key={id} className="map-layers-archive-row">
                    <label
                      className="map-layers-archive-label"
                      style={{ display: "flex", alignItems: "center", gap: 8, cursor: isMbtiles ? "pointer" : "default" }}
                    >
                      {isMbtiles ? (
                        <input
                          type="checkbox"
                          checked={enabledRasterIds.has(id)}
                          onChange={(e) => setRasterEnabled(id, e.target.checked)}
                        />
                      ) : (
                        <span className="text-muted small" style={{ minWidth: 28 }} title="Vector layer — not a basemap toggles here">
                          Vec
                        </span>
                      )}
                      <span className="small" style={{ flex: 1 }}>
                        {label} <span className="text-muted">({typeLower.toUpperCase()})</span>
                      </span>
                      {pmtilesBadge ? (
                        <span
                          className="text-muted small"
                          style={{
                            padding: "0.15rem 0.45rem",
                            borderRadius: 999,
                            border: "1px solid color-mix(in srgb, var(--primary) 35%, transparent)",
                            background: "color-mix(in srgb, var(--primary) 10%, transparent)",
                            color: "color-mix(in srgb, var(--primary) 80%, white)",
                          }}
                          title="Vector PMTiles is present on disk and ready for the vector overlay pipeline."
                        >
                          {pmtilesBadge}
                        </span>
                      ) : null}
                    </label>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {archivesModal}
    </div>
  );
}
