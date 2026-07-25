import { useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { FolderOpen } from "lucide-react";
import { formatBytesBinary } from "../../shared/format/formatBytesBinary";

type DiskReport = {
  availableBytes: number;
  totalBytes: number;
  mountPoint: string;
};

export type DashboardQueueRow = {
  id: string;
  label: string;
  bytes: number;
};

export interface DownloadManagerDashboardProps {
  queueActive?: boolean;
  activePct?: number;
  currentSpeedMibPerSec?: number | null;
  peakSpeedMibPerSec?: number | null;
  totalPlannedBytes?: number;
  disk?: DiskReport | null;
  speedSamples?: number[];
  upNext?: DashboardQueueRow[];
  completed?: DashboardQueueRow[];
  planExceedsDisk?: boolean;
  canStartDownloads?: boolean;
  startDownloads?: () => void;
  clearPlannedQueue?: () => void;
  clearCompleted?: () => void;
  /** Optional: when used as an overlay, provides a dismiss action. */
  onClose?: () => void;
}

function fmtMib(v: number | null): string {
  if (v == null || !Number.isFinite(v) || v <= 0) return "—";
  return `${v.toFixed(2)} MiB/s`;
}

export function DownloadManagerDashboard({
  queueActive = false,
  activePct = 0,
  currentSpeedMibPerSec = null,
  peakSpeedMibPerSec = null,
  totalPlannedBytes = 0,
  disk = null,
  speedSamples = [],
  upNext = [],
  completed = [],
  planExceedsDisk = false,
  canStartDownloads: _canStartDownloads = false,
  startDownloads = () => undefined,
  clearPlannedQueue = () => undefined,
  clearCompleted = () => undefined,
  onClose = undefined,
}: DownloadManagerDashboardProps) {
  const [watchFolder, setWatchFolder] = useState<string | null>(null);

  useEffect(() => {
    void invoke<string | null>("get_watch_folder")
      .then((p) => setWatchFolder(p ?? null))
      .catch(() => setWatchFolder(null));
  }, []);

  const diskUsedPct =
    disk && Number.isFinite(disk.totalBytes) && disk.totalBytes > 0
      ? Math.max(0, Math.min(100, ((disk.totalBytes - disk.availableBytes) / disk.totalBytes) * 100))
      : null;

  const maxSample = speedSamples.length ? Math.max(...speedSamples, 0.1) : 0.1;
  const graphBars = speedSamples.length ? speedSamples : [];

  return (
    <div className="w-full max-w-5xl h-full min-h-0 bg-slate-950/90 backdrop-blur-2xl rounded-[1.25rem] shadow-[0_18px_60px_rgba(0,0,0,0.55)] flex flex-col overflow-hidden text-slate-200">
      <div className="flex-1 overflow-y-auto p-6 flex flex-col gap-6">
        {/* Steam-style: top "Downloading" band with graph + compact metrics */}
        <section className="glass-panel" style={{ padding: "0.95rem", borderRadius: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline", flexWrap: "wrap" }}>
            <div style={{ display: "grid", gap: 2 }}>
              <div style={{ fontWeight: 900, letterSpacing: "0.08em", textTransform: "uppercase", fontSize: "0.72rem", color: "var(--text-muted)" }}>
                {queueActive ? "Downloading" : "Downloads"}
              </div>
              <div style={{ fontWeight: 750, fontSize: "1.05rem" }}>
                {queueActive ? "Queue running" : upNext.length > 0 ? "Ready to download" : "Queue empty"}
              </div>
            </div>
            <div style={{ display: "flex", gap: "0.5rem", alignItems: "center", flexWrap: "wrap" }}>
              {onClose ? (
                <button
                  type="button"
                  className="data-acquisition__queue-secondary"
                  onClick={onClose}
                  title="Back"
                >
                  Back
                </button>
              ) : null}
              <button
                type="button"
                className="data-acquisition__queue-secondary"
                onClick={() => void invoke("reveal_downloads_folder")}
                title="Open Athena downloads folder"
              >
                <span className="inline-flex items-center gap-2">
                  <FolderOpen size={16} strokeWidth={2} aria-hidden />
                  Open downloads
                </span>
              </button>
              {watchFolder ? (
                <button
                  type="button"
                  className="data-acquisition__queue-secondary"
                  onClick={() => void invoke("open_path_in_file_manager", { path: watchFolder })}
                  title="Open watch folder"
                >
                  <span className="inline-flex items-center gap-2">
                    <FolderOpen size={16} strokeWidth={2} aria-hidden />
                    Open watch folder
                  </span>
                </button>
              ) : null}
              <button
                type="button"
                className="data-acquisition__queue-secondary"
                onClick={clearPlannedQueue}
                disabled={upNext.length === 0}
                title="Clear the planned queue"
              >
                Clear planned
              </button>
            </div>
          </div>

          <div style={{ marginTop: "0.75rem", display: "grid", gap: "0.75rem" }}>
            <div className="data-acquisition__queue-progress" aria-hidden>
              <div
                className="data-acquisition__queue-progress-fill"
                style={{ width: `${Math.max(0, Math.min(100, activePct))}%` }}
              />
            </div>

            <div style={{ display: "grid", gridTemplateColumns: "1fr", gap: "0.85rem" }}>
              <div className="data-acquisition__queue-graph">
                <div className="data-acquisition__queue-graph-label">Network graph</div>
                <div className="data-acquisition__queue-graph-frame">
                  {graphBars.length === 0 ? (
                    <div className="data-acquisition__queue-graph-empty">No samples yet.</div>
                  ) : (
                    graphBars.map((v, i) => {
                      const h = Math.max(4, Math.min(100, (v / maxSample) * 100));
                      return (
                        <div
                          // eslint-disable-next-line react/no-array-index-key
                          key={i}
                          className="data-acquisition__queue-graph-bar"
                          style={{ height: `${h}%` }}
                          aria-hidden
                        />
                      );
                    })
                  )}
                </div>
                {diskUsedPct != null ? (
                  <div className="data-acquisition__queue-graph-foot">Disk used: {diskUsedPct.toFixed(0)}%</div>
                ) : null}
              </div>

              <div className="data-acquisition__queue-metrics">
                <div className="data-acquisition__queue-metric-grid">
                  <div className="data-acquisition__queue-metric-cell">
                    <div className="data-acquisition__queue-metric-label">Current</div>
                    <div className="data-acquisition__queue-metric-value">{fmtMib(currentSpeedMibPerSec)}</div>
                  </div>
                  <div className="data-acquisition__queue-metric-cell">
                    <div className="data-acquisition__queue-metric-label">Peak</div>
                    <div className="data-acquisition__queue-metric-value">{fmtMib(peakSpeedMibPerSec)}</div>
                  </div>
                  <div className="data-acquisition__queue-metric-cell">
                    <div className="data-acquisition__queue-metric-label">Total</div>
                    <div className="data-acquisition__queue-metric-value">
                      {formatBytesBinary(Math.max(0, totalPlannedBytes))}
                    </div>
                  </div>
                  <div className="data-acquisition__queue-metric-cell">
                    <div className="data-acquisition__queue-metric-label">Disk usage</div>
                    <div className="data-acquisition__queue-metric-value">
                      {disk
                        ? `${formatBytesBinary(Math.max(0, disk.totalBytes - disk.availableBytes))} / ${formatBytesBinary(
                            Math.max(0, disk.totalBytes),
                          )}`
                        : "—"}
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {planExceedsDisk ? (
              <div className="data-acquisition__queue-hint data-acquisition__queue-hint--danger">Not enough disk space.</div>
            ) : null}
            {queueActive ? <div className="data-acquisition__queue-hint">Queue is already running.</div> : null}
          </div>
        </section>

        {/* Steam-style lists */}
        <section className="glass-panel" style={{ padding: "0.9rem", borderRadius: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline", flexWrap: "wrap" }}>
            <div style={{ fontWeight: 900, letterSpacing: "0.08em", textTransform: "uppercase", fontSize: "0.72rem", color: "var(--text-muted)" }}>
              Up next
            </div>
            <div className="text-muted small">{upNext.length} item{upNext.length === 1 ? "" : "s"}</div>
          </div>
          <div style={{ marginTop: "0.65rem", display: "grid", gap: "0.5rem" }}>
            {upNext.length === 0 ? (
              <div className="data-acquisition__queue-graph-empty">Queue is empty.</div>
            ) : (
              upNext.map((row) => (
                <div key={row.id} className="data-acquisition__queue-row">
                  <div className="data-acquisition__queue-row-main">
                    <div className="data-acquisition__queue-row-label">{row.label}</div>
                    <div className="data-acquisition__queue-row-meta">{formatBytesBinary(Math.max(0, row.bytes))}</div>
                  </div>
                  <div className="data-acquisition__queue-row-badge">Queued</div>
                </div>
              ))
            )}
          </div>
        </section>

        <section className="glass-panel" style={{ padding: "0.9rem", borderRadius: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline", flexWrap: "wrap" }}>
            <div style={{ fontWeight: 900, letterSpacing: "0.08em", textTransform: "uppercase", fontSize: "0.72rem", color: "var(--text-muted)" }}>
              Scheduled
            </div>
            <div className="text-muted small">0 items</div>
          </div>
          <div style={{ marginTop: "0.65rem" }} className="data-acquisition__queue-graph-empty">
            No scheduled downloads.
          </div>
        </section>

        <section className="glass-panel" style={{ padding: "0.9rem", borderRadius: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline", flexWrap: "wrap" }}>
            <div style={{ fontWeight: 900, letterSpacing: "0.08em", textTransform: "uppercase", fontSize: "0.72rem", color: "var(--text-muted)" }}>
              Completed
            </div>
            <button
              type="button"
              className="data-acquisition__queue-secondary"
              onClick={clearCompleted}
              disabled={completed.length === 0}
            >
              Clear all
            </button>
          </div>
          <div style={{ marginTop: "0.65rem", display: "grid", gap: "0.5rem" }}>
            {completed.length === 0 ? (
              <div className="data-acquisition__queue-graph-empty">No completed downloads yet.</div>
            ) : (
              completed.map((row) => (
                <div key={row.id} className="data-acquisition__queue-row">
                  <div className="data-acquisition__queue-row-main">
                    <div className="data-acquisition__queue-row-label">{row.label}</div>
                    <div className="data-acquisition__queue-row-meta">{formatBytesBinary(Math.max(0, row.bytes))}</div>
                  </div>
                  <div className="data-acquisition__queue-row-badge data-acquisition__queue-row-badge--done">Done</div>
                </div>
              ))
            )}
          </div>
        </section>
      </div>

      <div className="shrink-0 w-full p-6 border-t border-white/10 bg-slate-950/80 backdrop-blur-md flex justify-center items-center">
        <button
          type="button"
          onClick={startDownloads}
          className="btn-primary-cta"
        >
          Start downloads
        </button>
      </div>
    </div>
  );
}
