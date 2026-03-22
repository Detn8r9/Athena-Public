import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  downloadOfflineMap,
  getOfflineMapManagerCatalog,
  listDownloadedOfflineMapIds,
  type MapDownloadCompleteEventPayload,
  type MapDownloadErrorEventPayload,
  type MapDownloadProgressEventPayload,
  type OfflineMapCatalogEntry,
} from "./offlineMapManagerCatalog";
import { OfflineMapViewport } from "./OfflineMapViewport";

export interface OfflineMapManagerDashboardProps {
  open: boolean;
  onClose: () => void;
}

type DownloadProgressState = {
  bytesDownloaded: number;
  totalBytes: number | null;
};

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"] as const;
  let n = value;
  let u = 0;
  while (n >= 1024 && u < units.length - 1) {
    n /= 1024;
    u += 1;
  }
  const digits = u === 0 ? 0 : u === 1 ? 1 : 2;
  return `${n.toFixed(digits)} ${units[u]}`;
}

export function OfflineMapManagerDashboard({ open, onClose }: OfflineMapManagerDashboardProps) {
  const [entries, setEntries] = useState<OfflineMapCatalogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [activePreviewId, setActivePreviewId] = useState<string | "">("");
  const [downloadedMapIds, setDownloadedMapIds] = useState<Set<string>>(() => new Set());
  const [downloadProgressByMapId, setDownloadProgressByMapId] = useState<Record<string, DownloadProgressState>>(
    () => ({}),
  );
  const [downloadInvokeErrorByMapId, setDownloadInvokeErrorByMapId] = useState<Record<string, string>>(() => ({}));
  const [rasterBaseLayerEnabled, setRasterBaseLayerEnabled] = useState(false);
  const [vectorOverlayLayerEnabled, setVectorOverlayLayerEnabled] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    let cancelled = false;
    setLoading(true);
    setLoadError(null);
    void Promise.all([getOfflineMapManagerCatalog(), listDownloadedOfflineMapIds()])
      .then(([catalogData, downloadedIds]) => {
        if (!cancelled) {
          setEntries(catalogData);
          setDownloadedMapIds(new Set(downloadedIds));
          setLoading(false);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setLoadError(err != null ? String(err) : "Could not load map catalog");
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    let disposed = false;
    const registeredUnlisten: Array<() => void> = [];

    void (async () => {
      const unlistenProgress = await listen<MapDownloadProgressEventPayload>("map_download_progress", (event) => {
        const payload = event.payload;
        setDownloadProgressByMapId((previous) => ({
          ...previous,
          [payload.map_id]: {
            bytesDownloaded: payload.bytes_downloaded,
            totalBytes: payload.total_bytes,
          },
        }));
      });
      if (disposed) {
        unlistenProgress();
        return;
      }
      registeredUnlisten.push(unlistenProgress);

      const unlistenComplete = await listen<MapDownloadCompleteEventPayload>("map_download_complete", (event) => {
        const payload = event.payload;
        setDownloadProgressByMapId((previous) => {
          const next = { ...previous };
          delete next[payload.map_id];
          return next;
        });
        setDownloadInvokeErrorByMapId((previous) => {
          const next = { ...previous };
          delete next[payload.map_id];
          return next;
        });
        setDownloadedMapIds((previous) => {
          const next = new Set(previous);
          next.add(payload.map_id);
          return next;
        });
      });
      if (disposed) {
        unlistenComplete();
        return;
      }
      registeredUnlisten.push(unlistenComplete);

      const unlistenError = await listen<MapDownloadErrorEventPayload>("map_download_error", (event) => {
        const payload = event.payload;
        setDownloadProgressByMapId((previous) => {
          const next = { ...previous };
          delete next[payload.map_id];
          return next;
        });
        setDownloadInvokeErrorByMapId((previous) => ({
          ...previous,
          [payload.map_id]: payload.message,
        }));
      });
      if (disposed) {
        unlistenError();
        return;
      }
      registeredUnlisten.push(unlistenError);
    })();

    return () => {
      disposed = true;
      for (const unlisten of registeredUnlisten) {
        unlisten();
      }
    };
  }, [open]);

  const activeEntry = entries.find((e) => e.id === activePreviewId);
  const catalogTypeLower = activeEntry !== undefined ? activeEntry.type.toLowerCase() : "";
  const activeDownloaded = activeEntry !== undefined && downloadedMapIds.has(activeEntry.id);

  useEffect(() => {
    const entry = entries.find((e) => e.id === activePreviewId);
    if (entry === undefined) {
      setRasterBaseLayerEnabled(false);
      setVectorOverlayLayerEnabled(false);
      return;
    }
    const entryTypeLower = entry.type.toLowerCase();
    if (entryTypeLower !== "mbtiles") {
      setRasterBaseLayerEnabled(false);
    }
    if (entryTypeLower !== "pmtiles") {
      setVectorOverlayLayerEnabled(false);
    }
  }, [activePreviewId, entries]);

  if (!open) {
    return null;
  }

  const baseRasterArchiveFileName =
    rasterBaseLayerEnabled && activeDownloaded && catalogTypeLower === "mbtiles" && activeEntry !== undefined
      ? `${activeEntry.id}.mbtiles`
      : null;

  const overlayArchiveFileName =
    vectorOverlayLayerEnabled && activeDownloaded && catalogTypeLower === "pmtiles" && activeEntry !== undefined
      ? `${activeEntry.id}.pmtiles`
      : null;

  const canEnableRasterBase = activeDownloaded && catalogTypeLower === "mbtiles";
  const canEnableVectorOverlay = activeDownloaded && catalogTypeLower === "pmtiles";

  const activeSelectionDescriptionParts: string[] = [];
  if (activeEntry !== undefined) {
    activeSelectionDescriptionParts.push(`Selected catalog entry: ${activeEntry.name} (${activeEntry.type}).`);
  }
  if (baseRasterArchiveFileName !== null) {
    activeSelectionDescriptionParts.push(`Raster base archive file: ${baseRasterArchiveFileName}.`);
  }
  if (overlayArchiveFileName !== null) {
    activeSelectionDescriptionParts.push(`PMTiles overlay archive file: ${overlayArchiveFileName}.`);
  }
  const activeSelectionDescription =
    activeSelectionDescriptionParts.length > 0 ? activeSelectionDescriptionParts.join(" ") : undefined;

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="library-modal offline-map-manager-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="offline-map-manager-title"
      >
        <div className="library-header">
          <h2 id="offline-map-manager-title">Map manager</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close map manager">
            ×
          </button>
        </div>
        <p className="library-desc map-manager-lead">
          Browse the offline map catalog (PMTiles and MBTiles). Downloads are saved under the application data maps folder.
          Choose a catalog row, download it, then enable layers to preview archives in the viewport.
        </p>
        <div className="map-manager-body">
          <aside className="map-manager-sidebar" aria-label="Map catalog">
            <h3 className="map-manager-sidebar-title">Catalog</h3>
            <div className="map-manager-sidebar-scroll">
              {loading && <p className="text-muted map-manager-sidebar-status">Loading catalog…</p>}
              {loadError !== null && (
                <p className="error map-manager-sidebar-status" role="alert">
                  {loadError}
                </p>
              )}
              {!loading && loadError === null && entries.length === 0 && (
                <p className="text-muted map-manager-sidebar-status">No catalog entries.</p>
              )}
              <ul className="map-manager-catalog-list">
                {entries.map((entry) => {
                  const isDownloaded = downloadedMapIds.has(entry.id);
                  const progress = downloadProgressByMapId[entry.id];
                  const isDownloading = progress !== undefined;
                  const invokeError = downloadInvokeErrorByMapId[entry.id];

                  let percent: number | null = null;
                  if (progress !== undefined && progress.totalBytes !== null && progress.totalBytes > 0) {
                    percent = Math.min(100, (progress.bytesDownloaded / progress.totalBytes) * 100);
                  }

                  return (
                    <li key={entry.id} className="map-manager-catalog-card">
                      <div className="map-manager-catalog-card-head">
                        <span className="map-manager-catalog-name">{entry.name}</span>
                        <span className="map-manager-catalog-badge">{entry.type}</span>
                      </div>
                      <div className="map-manager-catalog-meta text-muted small">
                        {entry.region} · {entry.size_mb.toLocaleString()} MB
                      </div>
                      {invokeError !== undefined && invokeError.length > 0 && (
                        <p className="map-manager-download-inline-error small" role="alert">
                          {invokeError}
                        </p>
                      )}
                      {isDownloading ? (
                        <div
                          className="map-manager-download-progress-wrap"
                          aria-label={`Download progress for ${entry.name}`}
                        >
                          <div className="map-manager-download-progress-label-row">
                            <span className="map-manager-download-progress-label">Downloading</span>
                            <span className="map-manager-download-progress-bytes text-muted small">
                              {formatBytes(progress.bytesDownloaded)}
                              {progress.totalBytes !== null && progress.totalBytes > 0
                                ? ` / ${formatBytes(progress.totalBytes)}`
                                : ""}
                            </span>
                          </div>
                          <div
                            className={`map-manager-download-progress-bar${percent === null ? " map-manager-download-progress-bar--indeterminate" : ""}`}
                            role="progressbar"
                            aria-valuemin={0}
                            aria-valuemax={100}
                            aria-valuenow={percent === null ? undefined : Math.round(percent)}
                          >
                            {percent !== null && (
                              <div
                                className="map-manager-download-progress-fill"
                                style={{ width: `${percent}%` }}
                              />
                            )}
                          </div>
                          {percent !== null && (
                            <span className="map-manager-download-progress-pct small">{percent.toFixed(1)}%</span>
                          )}
                        </div>
                      ) : (
                        <button
                          type="button"
                          className={`map-manager-download-btn${isDownloaded ? " map-manager-download-btn--downloaded" : ""}`}
                          disabled={isDownloaded}
                          onClick={() => {
                            setDownloadInvokeErrorByMapId((previous) => {
                              const next = { ...previous };
                              delete next[entry.id];
                              return next;
                            });
                            void downloadOfflineMap(entry.id, entry.download_url).catch((err: unknown) => {
                              setDownloadInvokeErrorByMapId((previous) => ({
                                ...previous,
                                [entry.id]: err != null ? String(err) : "Download could not be started.",
                              }));
                            });
                          }}
                        >
                          {isDownloaded ? "Downloaded" : "Download"}
                        </button>
                      )}
                    </li>
                  );
                })}
              </ul>
            </div>
          </aside>
          <div className="map-manager-main">
            <div className="map-manager-controls" aria-label="Map layer selection (preview)">
              <div className="map-manager-control-row">
                <label htmlFor="map-manager-active-slot" className="map-manager-control-label">
                  Active map slot (preview)
                </label>
                <select
                  id="map-manager-active-slot"
                  className="theme-select map-manager-select"
                  value={activePreviewId}
                  onChange={(e) => setActivePreviewId(e.target.value)}
                  aria-label="Select which catalog map drives the preview and layer toggles"
                >
                  <option value="">None</option>
                  {entries.map((e) => (
                    <option key={e.id} value={e.id}>
                      {e.name}
                    </option>
                  ))}
                </select>
              </div>
              <fieldset className="map-manager-toggles-fieldset">
                <legend className="map-manager-toggles-legend">Layer stack (offline)</legend>
                <p className="text-muted small map-manager-toggles-hint">
                  Select a downloaded catalog row, then enable layers. MBTiles render as a raster base; PMTiles render as an overlay above the base. Vector overlays are drawn above raster layers.
                </p>
                <label
                  className={`map-manager-toggle-row${canEnableVectorOverlay ? "" : " map-manager-toggle-row--disabled"}`}
                >
                  <input
                    type="checkbox"
                    className="map-manager-toggle-input"
                    checked={vectorOverlayLayerEnabled}
                    disabled={!canEnableVectorOverlay}
                    onChange={(e) => setVectorOverlayLayerEnabled(e.target.checked)}
                  />
                  <span className="map-manager-toggle-label">Street vector (PMTiles) overlay</span>
                </label>
                <label
                  className={`map-manager-toggle-row${canEnableRasterBase ? "" : " map-manager-toggle-row--disabled"}`}
                >
                  <input
                    type="checkbox"
                    className="map-manager-toggle-input"
                    checked={rasterBaseLayerEnabled}
                    disabled={!canEnableRasterBase}
                    onChange={(e) => setRasterBaseLayerEnabled(e.target.checked)}
                  />
                  <span className="map-manager-toggle-label">Satellite / raster (MBTiles) base</span>
                </label>
              </fieldset>
            </div>
            <div className="map-manager-viewport" role="region" aria-label="Map viewport">
              <div className="map-manager-viewport-map-shell">
                {activeEntry !== undefined && (
                  <div className="map-manager-viewport-caption text-muted small" aria-hidden="true">
                    <span className="map-manager-viewport-caption-title">Preview</span>
                    <span className="map-manager-viewport-caption-body">
                      {activeEntry.name} ({activeEntry.type})
                      {!activeDownloaded ? " — download to enable layers" : ""}
                    </span>
                  </div>
                )}
                <OfflineMapViewport
                  activeSelectionDescription={activeSelectionDescription}
                  baseRasterArchiveFileName={baseRasterArchiveFileName}
                  overlayArchiveFileName={overlayArchiveFileName}
                />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
