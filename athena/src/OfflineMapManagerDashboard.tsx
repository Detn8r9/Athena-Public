import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { boundingBoxFromCenterAndRadiusMiles } from "./geographicBoundingBox";
import {
  downloadOfflineMap,
  getOfflineMapManagerCatalog,
  listDownloadedOfflineMapIds,
  type MapDownloadCompleteEventPayload,
  type MapDownloadErrorEventPayload,
  type MapDownloadProgressEventPayload,
  type OfflineMapCatalogEntry,
} from "./offlineMapManagerCatalog";
import { ErrorBoundary } from "./ErrorBoundary";
import { OfflineMapViewport } from "./OfflineMapViewport";

export interface OfflineMapManagerDashboardProps {
  open: boolean;
  onClose: () => void;
  /** Saved address from Settings; used when the geocode override is empty. */
  savedUserLocationAddress: string;
  /** Optional override for the place to geocode (same pattern as Settings tile download). */
  tileGeocodeOverride: string;
  /** Radius in miles around the geocoded center (same slider as Settings tile download). */
  tileRadiusMiles: number;
}

type DownloadProgressState = {
  bytesDownloaded: number;
  totalBytes: number | null;
};

type PmtilesExtractStream = "stdout" | "stderr";

interface PmtilesRegionExtractProgressPayload {
  job_id: string;
  line: string;
  stream: PmtilesExtractStream;
}

interface PmtilesRegionExtractCompletePayload {
  job_id: string;
  output_path: string;
}

interface PmtilesRegionExtractErrorPayload {
  job_id: string;
  message: string;
}

const EXTRACT_TERMINAL_MAX_LINES = 400;

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

function buildSyntheticDownloadedEntry(id: string): OfflineMapCatalogEntry {
  return {
    id,
    name: `Local archive · ${id}`,
    region: "Application data",
    type: "pmtiles",
    size_mb: 0,
    download_url: "",
  };
}

function resolveActiveCatalogEntry(
  activePreviewId: string,
  entries: OfflineMapCatalogEntry[],
  downloadedMapIds: Set<string>,
): OfflineMapCatalogEntry | undefined {
  const fromCatalog = entries.find((e) => e.id === activePreviewId);
  if (fromCatalog !== undefined) {
    return fromCatalog;
  }
  if (activePreviewId !== "" && downloadedMapIds.has(activePreviewId)) {
    return buildSyntheticDownloadedEntry(activePreviewId);
  }
  return undefined;
}

function extractKeywordsFromSavedLocation(savedLocation: string): string[] {
  const normalized = savedLocation.trim().toLowerCase();
  if (normalized.length === 0) {
    return [];
  }
  const tokens = normalized
    .split(/[\s,;]+/)
    .map((token) => token.replace(/[^a-z0-9]/g, ""))
    .filter((token) => token.length >= 2);
  return [...new Set(tokens)];
}

function catalogEntryMatchesSavedLocationKeywords(
  entry: OfflineMapCatalogEntry,
  keywords: string[],
): boolean {
  if (keywords.length === 0) {
    return false;
  }
  const bundle = `${entry.region} ${entry.name}`.toLowerCase();
  return keywords.some((keyword) => bundle.includes(keyword));
}

export function partitionCatalogBySavedLocation(
  catalog: OfflineMapCatalogEntry[],
  savedLocationAddress: string,
): { localRegionEntries: OfflineMapCatalogEntry[]; otherRegionEntries: OfflineMapCatalogEntry[] } {
  const keywords = extractKeywordsFromSavedLocation(savedLocationAddress);
  const localRegionEntries: OfflineMapCatalogEntry[] = [];
  const otherRegionEntries: OfflineMapCatalogEntry[] = [];
  for (const item of catalog) {
    if (catalogEntryMatchesSavedLocationKeywords(item, keywords)) {
      localRegionEntries.push(item);
    } else {
      otherRegionEntries.push(item);
    }
  }
  return { localRegionEntries, otherRegionEntries };
}

type OfflineMapCatalogListItemProps = {
  entry: OfflineMapCatalogEntry;
  downloadedMapIds: Set<string>;
  downloadProgressByMapId: Record<string, DownloadProgressState>;
  downloadInvokeErrorByMapId: Record<string, string>;
  onClearDownloadError: (mapId: string) => void;
  onDownloadFailed: (mapId: string, message: string) => void;
};

function OfflineMapCatalogListItem({
  entry,
  downloadedMapIds,
  downloadProgressByMapId,
  downloadInvokeErrorByMapId,
  onClearDownloadError,
  onDownloadFailed,
}: OfflineMapCatalogListItemProps) {
  const isDownloaded = downloadedMapIds.has(entry.id);
  const progress = downloadProgressByMapId[entry.id];
  const isDownloading = progress !== undefined;
  const invokeError = downloadInvokeErrorByMapId[entry.id];

  let percent: number | null = null;
  if (progress !== undefined && progress.totalBytes !== null && progress.totalBytes > 0) {
    percent = Math.min(100, (progress.bytesDownloaded / progress.totalBytes) * 100);
  }

  return (
    <li className="map-manager-catalog-card">
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
        <div className="map-manager-download-progress-wrap" aria-label={`Download progress for ${entry.name}`}>
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
              <div className="map-manager-download-progress-fill" style={{ width: `${percent}%` }} />
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
            onClearDownloadError(entry.id);
            void downloadOfflineMap(entry.id, entry.download_url).catch((err: unknown) => {
              onDownloadFailed(entry.id, err != null ? String(err) : "Download could not be started.");
            });
          }}
        >
          {isDownloaded ? "Downloaded" : "Download"}
        </button>
      )}
    </li>
  );
}

export function OfflineMapManagerDashboard({
  open,
  onClose,
  savedUserLocationAddress,
  tileGeocodeOverride,
  tileRadiusMiles,
}: OfflineMapManagerDashboardProps) {
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

  const [extractJobId, setExtractJobId] = useState<string | null>(null);
  const [isExtracting, setIsExtracting] = useState(false);
  const [extractTerminalLines, setExtractTerminalLines] = useState<string[]>([]);
  const activeExtractJobIdRef = useRef<string | null>(null);

  const { localRegionEntries, otherRegionEntries } = useMemo(
    () => partitionCatalogBySavedLocation(entries, savedUserLocationAddress),
    [entries, savedUserLocationAddress],
  );

  const catalogEntriesOrderedForControls = useMemo(
    () => [...localRegionEntries, ...otherRegionEntries],
    [localRegionEntries, otherRegionEntries],
  );

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

  useEffect(() => {
    if (!open) {
      return;
    }
    let disposed = false;
    const registeredUnlisten: Array<() => void> = [];

    void (async () => {
      const unlistenProgress = await listen<PmtilesRegionExtractProgressPayload>(
        "pmtiles_region_extract_progress",
        (event) => {
          const payload = event.payload;
          if (payload.job_id !== activeExtractJobIdRef.current) {
            return;
          }
          const prefix = payload.stream === "stderr" ? "[stderr] " : "";
          const nextLine = `${prefix}${payload.line}`;
          setExtractTerminalLines((previous) => {
            const appended = [...previous, nextLine];
            return appended.length > EXTRACT_TERMINAL_MAX_LINES
              ? appended.slice(-EXTRACT_TERMINAL_MAX_LINES)
              : appended;
          });
        },
      );
      if (disposed) {
        unlistenProgress();
        return;
      }
      registeredUnlisten.push(unlistenProgress);

      const unlistenComplete = await listen<PmtilesRegionExtractCompletePayload>(
        "pmtiles_region_extract_complete",
        (event) => {
          const payload = event.payload;
          if (payload.job_id !== activeExtractJobIdRef.current) {
            return;
          }
          activeExtractJobIdRef.current = null;
          setExtractJobId(null);
          setIsExtracting(false);
          setExtractTerminalLines([]);
          void listDownloadedOfflineMapIds().then((ids) => {
            setDownloadedMapIds(new Set(ids));
          });
        },
      );
      if (disposed) {
        unlistenComplete();
        return;
      }
      registeredUnlisten.push(unlistenComplete);

      const unlistenError = await listen<PmtilesRegionExtractErrorPayload>("pmtiles_region_extract_error", (event) => {
        const payload = event.payload;
        if (payload.job_id !== activeExtractJobIdRef.current) {
          return;
        }
        activeExtractJobIdRef.current = null;
        setExtractJobId(null);
        setIsExtracting(false);
        setExtractTerminalLines((previous) => {
          const next = [...previous, `Error: ${payload.message}`];
          return next.length > EXTRACT_TERMINAL_MAX_LINES ? next.slice(-EXTRACT_TERMINAL_MAX_LINES) : next;
        });
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

  const activeEntry = resolveActiveCatalogEntry(activePreviewId, entries, downloadedMapIds);
  const catalogTypeLower = activeEntry !== undefined ? activeEntry.type.toLowerCase() : "";
  const activeDownloaded = activeEntry !== undefined && downloadedMapIds.has(activeEntry.id);

  const geocodeQueryTrimmed = (tileGeocodeOverride.trim() || savedUserLocationAddress.trim()).trim();
  const canStartStreetExtract = geocodeQueryTrimmed.length > 0 && tileRadiusMiles > 0;

  const catalogEntryIds = new Set(entries.map((e) => e.id));
  const downloadedIdsNotInCatalog = [...downloadedMapIds].filter((id) => !catalogEntryIds.has(id)).sort();

  useEffect(() => {
    const entry = resolveActiveCatalogEntry(activePreviewId, entries, downloadedMapIds);
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
  }, [activePreviewId, entries, downloadedMapIds]);

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

  const clearDownloadError = (mapId: string) => {
    setDownloadInvokeErrorByMapId((previous) => {
      const next = { ...previous };
      delete next[mapId];
      return next;
    });
  };

  const onDownloadFailed = (mapId: string, message: string) => {
    setDownloadInvokeErrorByMapId((previous) => ({
      ...previous,
      [mapId]: message,
    }));
  };

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
          <h2 id="offline-map-manager-title">Map archive manager</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close map archive manager">
            ×
          </button>
        </div>
        <p className="library-desc map-manager-lead">
          Browse the offline map catalog (PMTiles and MBTiles). Downloads are saved under the application data maps folder.
          Choose a catalog row, download it, then enable layers to preview archives in the viewport. Catalog entries are grouped
          using keywords from your saved location in Settings.
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
              {!loading && loadError === null && entries.length > 0 && (
                <>
                  <section className="map-manager-region-section" aria-labelledby="offline-map-local-region-heading">
                    <h4 id="offline-map-local-region-heading" className="map-manager-sidebar-title map-manager-region-heading">
                      Local Region
                    </h4>
                    <div
                      className="map-manager-catalog-card map-manager-sidecar-extractor-card"
                      aria-label="PMTiles sidecar region extractor"
                    >
                      <div className="map-manager-catalog-card-head">
                        <span className="map-manager-catalog-name">PMTiles sidecar extractor</span>
                        <span className="map-manager-catalog-badge">Protomaps</span>
                      </div>
                      <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
                        Extract a high-resolution street and terrain vector map for your currently saved location (from Settings).
                        Uses the same geocode target and tile radius as the map tile controls in Settings. Requires network during
                        extraction; output is saved into the application data maps folder.
                      </p>
                      <button
                        type="button"
                        className="map-manager-download-btn"
                        disabled={isExtracting || !canStartStreetExtract}
                        onClick={() => {
                          void (async () => {
                            const query = (tileGeocodeOverride.trim() || savedUserLocationAddress.trim()).trim();
                            if (query.length === 0) {
                              return;
                            }
                            setIsExtracting(true);
                            setExtractTerminalLines([]);
                            setExtractJobId(null);
                            activeExtractJobIdRef.current = null;
                            try {
                              const geo = await invoke<[number, number]>("geocode_place_query", { query });
                              const centerLatitudeDegrees = geo[0];
                              const centerLongitudeDegrees = geo[1];
                              const boundingBox = boundingBoxFromCenterAndRadiusMiles(
                                centerLatitudeDegrees,
                                centerLongitudeDegrees,
                                tileRadiusMiles,
                              );
                              const jobId = await invoke<string>("extract_pmtiles_region", {
                                minLon: boundingBox.min_lon,
                                minLat: boundingBox.min_lat,
                                maxLon: boundingBox.max_lon,
                                maxLat: boundingBox.max_lat,
                              });
                              activeExtractJobIdRef.current = jobId;
                              setExtractJobId(jobId);
                            } catch (err: unknown) {
                              activeExtractJobIdRef.current = null;
                              setExtractJobId(null);
                              setIsExtracting(false);
                              setExtractTerminalLines([err != null ? String(err) : "Extraction could not be started."]);
                            }
                          })();
                        }}
                      >
                        {isExtracting ? "Extracting map" : "Run region extract"}
                      </button>
                      {extractJobId !== null && (
                        <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
                          Job: <code>{extractJobId}</code>
                        </p>
                      )}
                      {extractTerminalLines.length > 0 && (
                        <pre
                          className="map-manager-extract-terminal"
                          aria-live="polite"
                          aria-label="Protomaps extract output"
                          style={{ marginTop: "0.5rem" }}
                        >
                          {extractTerminalLines.join("\n")}
                        </pre>
                      )}
                    </div>
                    <ul className="map-manager-catalog-list" aria-label="Catalog entries matching your saved location">
                      {localRegionEntries.length === 0 && (
                        <li className="map-manager-catalog-empty text-muted small">
                          No catalog entries match keywords from your saved location. Add or refine your address in Settings, or
                          browse Global &amp; Other Regions.
                        </li>
                      )}
                      {localRegionEntries.map((entry) => (
                        <OfflineMapCatalogListItem
                          key={entry.id}
                          entry={entry}
                          downloadedMapIds={downloadedMapIds}
                          downloadProgressByMapId={downloadProgressByMapId}
                          downloadInvokeErrorByMapId={downloadInvokeErrorByMapId}
                          onClearDownloadError={clearDownloadError}
                          onDownloadFailed={onDownloadFailed}
                        />
                      ))}
                    </ul>
                  </section>

                  <section
                    className="map-manager-region-section map-manager-region-section--spaced"
                    aria-labelledby="offline-map-global-other-heading"
                  >
                    <h4 id="offline-map-global-other-heading" className="map-manager-sidebar-title map-manager-region-heading">
                      Global &amp; Other Regions
                    </h4>
                    <ul className="map-manager-catalog-list" aria-label="Other catalog entries">
                      {otherRegionEntries.length === 0 && (
                        <li className="map-manager-catalog-empty text-muted small">
                          Every catalog entry matched your saved location keywords, so there are no separate global listings.
                        </li>
                      )}
                      {otherRegionEntries.map((entry) => (
                        <OfflineMapCatalogListItem
                          key={entry.id}
                          entry={entry}
                          downloadedMapIds={downloadedMapIds}
                          downloadProgressByMapId={downloadProgressByMapId}
                          downloadInvokeErrorByMapId={downloadInvokeErrorByMapId}
                          onClearDownloadError={clearDownloadError}
                          onDownloadFailed={onDownloadFailed}
                        />
                      ))}
                    </ul>
                  </section>

                  <p className="text-muted small map-manager-catalog-footnote" style={{ marginTop: "0.75rem" }}>
                    True satellite imagery usually depends on a licensed provider and is not bundled here; import satellite MBTiles
                    manually if you need aerial imagery. The sidecar extractor supplies street and terrain vector data for your saved
                    area.
                  </p>
                </>
              )}
            </div>
          </aside>
          <div className="map-manager-main">
            <div className="map-manager-controls-scroll" aria-label="Map layer selection (preview)">
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
                  {catalogEntriesOrderedForControls.map((e) => (
                    <option key={e.id} value={e.id}>
                      {e.name}
                    </option>
                  ))}
                  {downloadedIdsNotInCatalog.map((id) => (
                    <option key={id} value={id}>
                      Local · {id}
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
                <ErrorBoundary sectionLabel="map preview (MapLibre)">
                  <OfflineMapViewport
                    activeSelectionDescription={activeSelectionDescription}
                    baseRasterArchiveFileName={baseRasterArchiveFileName}
                    overlayArchiveFileName={overlayArchiveFileName}
                  />
                </ErrorBoundary>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
