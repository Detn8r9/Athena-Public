export interface ArchiveMapCatalogEntry {
  id: string;
  name: string;
  description: string;
  region: string;
  style: string;
  url: string;
  size_bytes: number;
  size_label: string;
  for_user_region: boolean;
}

export interface ArchiveDownloadedMap {
  path: string;
  name: string;
  region: string;
  style: string;
}

export interface ArchiveMapLibraryFile {
  path: string;
  name: string;
  size_bytes: number;
  exists_on_disk?: boolean;
}

export interface ArchiveMapDownloadJob {
  downloaded: number;
  total: number | null;
  label: string;
}

export interface ArchiveMapsIndexStats {
  total_files: number;
  text_count: number;
  image_count: number;
  video_count: number;
  map_count: number;
  zim_count: number;
}

export interface ArchiveMapsTabProps {
  mapCatalog: ArchiveMapCatalogEntry[];
  downloadedMaps: ArchiveDownloadedMap[];
  networkReachable: boolean | null;
  mapsSuggestion: ArchiveMapCatalogEntry | null;
  mapDownloadJobs: Array<[string, ArchiveMapDownloadJob]>;
  mapZimsOnDiskOnly: ArchiveMapLibraryFile[];
  mapLibraryFileCount: number;
  stats: ArchiveMapsIndexStats | null;
  formatBytes: (n: number) => string;
  formatLibraryFileSizeBytes: (n: number) => string;
  onDownloadMap: (entry: ArchiveMapCatalogEntry) => void | Promise<void>;
  onOpenMapFileManager: () => void;
  onOpenOfflineMapSettings: () => void;
  onOpenExternalUrl: (url: string) => void | Promise<void>;
  onRefreshNetworkReachable: () => void;
  onRemoveDownloadedMap: (path: string) => void | Promise<void>;
  onRemoveLibraryFile: (path: string) => void | Promise<void>;
}

export function ArchiveMapsTab({
  mapCatalog,
  downloadedMaps,
  networkReachable,
  mapsSuggestion,
  mapDownloadJobs,
  mapZimsOnDiskOnly,
  mapLibraryFileCount,
  stats,
  formatBytes,
  formatLibraryFileSizeBytes,
  onDownloadMap: _onDownloadMap,
  onOpenMapFileManager,
  onOpenOfflineMapSettings,
  onOpenExternalUrl,
  onRefreshNetworkReachable,
  onRemoveDownloadedMap,
  onRemoveLibraryFile,
}: ArchiveMapsTabProps) {
  const hasKiwixMaps =
    mapCatalog.length > 0 ||
    mapDownloadJobs.length > 0 ||
    downloadedMaps.length > 0 ||
    mapZimsOnDiskOnly.length > 0 ||
    mapsSuggestion !== null;
  return (
    <>
      <section className="library-section library-section-map-archives">
        <h3>Map files</h3>
        <p className="text-muted small library-section-lead">
          Download a map of your area so the Map screen works without internet.
        </p>
        <div className="library-maps-action-row">
          <button type="button" className="primary" onClick={onOpenMapFileManager}>
            Download maps for an area
          </button>
          <button type="button" className="secondary" onClick={onOpenOfflineMapSettings}>
            Offline map settings
          </button>
        </div>
      </section>
      {hasKiwixMaps ? (
      <section className="library-section library-section-maps">
        <h3>Kiwix map packs</h3>
        <p className="text-muted small library-section-lead">
          Regional map archives from Kiwix. They also show in Settings → Offline maps.
        </p>
        <div className="library-maps-action-row">
          <button type="button" onClick={() => void onOpenExternalUrl("https://library.kiwix.org")}>
            Browse the Kiwix library
          </button>
        </div>
        <div className="settings-network-row text-muted small library-maps-network" aria-live="polite">
          Network:{" "}
          {networkReachable === null
            ? "—"
            : networkReachable
              ? "Reachable"
              : "Not reachable — connect to download packs"}
          <button type="button" className="secondary settings-network-refresh" onClick={onRefreshNetworkReachable}>
            Check again
          </button>
        </div>
        {mapsSuggestion !== null && (
          <div className="settings-map-suggestion-banner">
            <strong>Suggested for your location:</strong> {mapsSuggestion.name} ({mapsSuggestion.size_label})
            {downloadedMaps.some((m) => m.region === mapsSuggestion.region && m.style === mapsSuggestion.style) ? (
              <span className="text-muted"> — already downloaded</span>
            ) : (
              <span className="text-muted small"> — acquire via Downloads → Maps</span>
            )}
          </div>
        )}
        {mapCatalog.length > 0 && (
          <>
            <h4 className="library-maps-subheading">Regional map packs</h4>
            <ul className="settings-map-catalog-list">
              {mapCatalog.map((entry) => {
                const acquired = downloadedMaps.some((m) => m.region === entry.region && m.style === entry.style);
                return (
                  <li key={entry.id} className="settings-map-catalog-item">
                    <div className="settings-map-catalog-main">
                      <span className="settings-map-catalog-name">
                        {entry.name}
                        {entry.for_user_region ? <span className="settings-map-catalog-badge">Matches location</span> : null}
                      </span>
                      <span className="text-muted settings-map-catalog-meta">
                        {entry.size_label} · {entry.region}
                      </span>
                    </div>
                    {acquired ? (
                      <span className="text-muted small">Downloaded</span>
                    ) : (
                      <span className="text-muted small">Use Downloads → Maps</span>
                    )}
                  </li>
                );
              })}
            </ul>
          </>
        )}
        {mapDownloadJobs.length > 0 && (
          <div className="library-maps-downloads" role="status" aria-live="polite">
            <h4 className="library-maps-subheading">Downloading</h4>
            <ul className="library-map-download-list">
              {mapDownloadJobs.map(([id, job]) => (
                <li key={id} className="library-map-download-item">
                  <div className="library-map-download-head">
                    <span className="library-map-download-label">{job.label}</span>
                    <span className="text-muted small">
                      {job.total != null && job.total > 0
                        ? `${formatBytes(job.downloaded)} / ${formatBytes(job.total)} · ${Math.min(
                            100,
                            Math.round((job.downloaded / job.total) * 100),
                          )}%`
                        : job.downloaded > 0
                          ? `${formatBytes(job.downloaded)}…`
                          : "Starting…"}
                    </span>
                  </div>
                  {job.total != null && job.total > 0 && (
                    <div className="library-map-download-bar" aria-hidden>
                      <div
                        className="library-map-download-bar-fill"
                        style={{
                          width: `${Math.min(100, Math.round((job.downloaded / job.total) * 100))}%`,
                        }}
                      />
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}
        {downloadedMaps.length > 0 && (
          <>
            <h4 className="library-maps-subheading">Downloaded (map viewer)</h4>
            <ul className="library-file-list library-map-file-list">
              {downloadedMaps.map((m) => (
                <li key={m.path} className="library-file-item">
                  <span className="library-file-name" title={m.path}>
                    <strong>{m.name}</strong>
                    <span className="text-muted small"> · {m.region}</span>
                  </span>
                  <button
                    type="button"
                    className="library-file-remove"
                    onClick={() => void onRemoveDownloadedMap(m.path)}
                    title="Remove map pack"
                    aria-label={`Remove map ${m.name}`}
                  >
                    Remove
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
        {mapZimsOnDiskOnly.length > 0 && (
          <>
            <h4 className="library-maps-subheading">Map ZIMs on disk (indexed, not in viewer)</h4>
            <p className="text-muted small">
              These look like Kiwix map archives but are not registered for the map tab yet. They are still indexed for search if they sit in your library folder.
            </p>
            <ul className="library-file-list library-map-file-list">
              {mapZimsOnDiskOnly.map((file) => (
                <li key={file.path} className="library-file-item">
                  <span className="library-file-name" title={file.path}>
                    {file.name}
                  </span>
                  <span className="library-file-size">{formatLibraryFileSizeBytes(file.size_bytes)}</span>
                  <button
                    type="button"
                    className="library-file-remove"
                    onClick={() => void onRemoveLibraryFile(file.path)}
                    title="Remove file"
                    aria-label={"Remove " + file.name}
                  >
                    Remove
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>
      ) : null}
      {stats !== null && hasKiwixMaps && (
        <section className="library-section library-stats library-stats-maps">
          <div className="stats">
            <span className="stat">Map ZIM files: {mapLibraryFileCount}</span>
            <span className="stat">Active downloads: {mapDownloadJobs.length}</span>
          </div>
        </section>
      )}
    </>
  );
}
