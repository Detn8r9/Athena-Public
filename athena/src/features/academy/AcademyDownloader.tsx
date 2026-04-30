import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "framer-motion";
import manifestJson from "./academy_manifest.json";

type PresetId = "small" | "medium" | "large" | "xl";

type DeploymentDiskSpaceReport = {
  availableBytes: number;
  totalBytes: number;
  mountPoint: string;
};

type ExpectedFile = { path: string; sizeBytes: number };

type PartialFileStatus = {
  path: string;
  downloadedBytes: number;
  expectedBytes: number;
};

type FileScanReport = {
  complete: string[];
  partial: PartialFileStatus[];
  missing: string[];
  oversized: PartialFileStatus[];
};

type DownloadJob = { url: string; fileName: string; expectedBytes: number };

type DownloadProgressPayload = {
  fileName: string;
  bytesDownloaded: number;
  totalBytes: number;
  bytesPerSec: number;
  etaSeconds: number | null;
};

type DownloadErrorPayload = {
  fileName: string;
  message: string;
  recoverable: boolean;
};

type DownloadFileCompletePayload = { fileName: string };
type QueueCompletePayload = { ok: boolean };

/** Canonical deployment queue from Rust (`deployment_get_queue_snapshot`). */
type DeploymentQueueSnapshot = {
  formatVersion: number;
  queueActive: boolean;
  cancelRequested: boolean;
  jobs: Array<{
    jobId: string;
    sourceUrl: string;
    displayName: string;
    state: string;
    bytesDownloaded: number;
    totalBytes: number | null;
  }>;
};

type ManifestTier = PresetId;

type ManifestItem = {
  id: string;
  name: string;
  description: string;
  url: string;
  fileName: string;
  sizeBytes: number;
  defaultTiers: ManifestTier[];
};

const MANIFEST = (Array.isArray(manifestJson) ? manifestJson : []) as ManifestItem[];

function normalizePath(p: string): string {
  return String(p || "").replace(/\\/g, "/");
}

function fmtBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = bytes / 1024 ** 2;
  if (mb >= 1) return `${mb.toFixed(0)} MB`;
  const kb = bytes / 1024;
  return `${kb.toFixed(0)} KB`;
}

function clampPct(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(100, n));
}

function SegmentedControl({
  value,
  onChange,
}: {
  value: PresetId;
  onChange: (v: PresetId) => void;
}) {
  const items: Array<{ id: PresetId; label: string }> = [
    { id: "small", label: "Small" },
    { id: "medium", label: "Medium" },
    { id: "large", label: "Large" },
    { id: "xl", label: "Extra Large" },
  ];
  return (
    <div className="mac-seg" role="tablist" aria-label="Installer preset">
      {items.map((it) => (
        <button
          key={it.id}
          type="button"
          role="tab"
          aria-selected={value === it.id}
          className={"mac-seg__btn" + (value === it.id ? " mac-seg__btn--active" : "")}
          onClick={() => onChange(it.id)}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}

function ToggleRow({
  title,
  subtitle,
  installed,
  enabled,
  disabled,
  onToggle,
  rightLabel,
}: {
  title: string;
  subtitle: string;
  installed: boolean;
  enabled: boolean;
  disabled: boolean;
  onToggle: () => void;
  rightLabel: string;
}) {
  return (
    <div className={"mac-list__row" + (disabled ? " mac-list__row--disabled" : "")}>
      <div className="mac-list__text">
        <div className="mac-list__title">{title}</div>
        <div className="mac-list__subtitle">{subtitle}</div>
      </div>
      <div className="mac-list__right">
        <div className={"mac-pill" + (installed ? " mac-pill--ok" : "")}>{installed ? "Installed" : rightLabel}</div>
        <button
          type="button"
          className={"mac-toggle" + (enabled ? " mac-toggle--on" : "")}
          aria-pressed={enabled}
          disabled={disabled}
          onClick={onToggle}
          aria-label={`${enabled ? "Disable" : "Enable"} ${title}`}
        >
          <span className="mac-toggle__knob" />
        </button>
      </div>
    </div>
  );
}

function PillProgress({ percent }: { percent: number }) {
  const p = clampPct(percent);
  return (
    <div className="mac-progress" aria-hidden>
      <div className="mac-progress__fill" style={{ width: `${p}%` }} />
    </div>
  );
}

type UiStage = "select" | "downloading" | "summary";

export function AcademyDownloader() {
  const [stage, setStage] = useState<UiStage>("select");
  const [preset, setPreset] = useState<PresetId>("medium");
  const [enabledById, setEnabledById] = useState<Record<string, boolean>>(() => {
    const out: Record<string, boolean> = {};
    for (const it of MANIFEST) out[it.id] = Array.isArray(it.defaultTiers) && it.defaultTiers.includes("medium");
    return out;
  });

  const [disk, setDisk] = useState<DeploymentDiskSpaceReport | null>(null);
  const [scan, setScan] = useState<FileScanReport | null>(null);
  const [preflightErr, setPreflightErr] = useState<string | null>(null);

  const [activeProgress, setActiveProgress] = useState<DownloadProgressPayload | null>(null);
  const [activeError, setActiveError] = useState<DownloadErrorPayload | null>(null);
  const [completedFiles, setCompletedFiles] = useState<string[]>([]);
  const [failedFiles, setFailedFiles] = useState<Array<{ fileName: string; url: string; message: string }>>([]);
  const [queueOk, setQueueOk] = useState<boolean | null>(null);

  const installedSet = useMemo(() => {
    const s = new Set<string>();
    for (const p of scan?.complete ?? []) s.add(normalizePath(p));
    return s;
  }, [scan?.complete]);

  const partialByPath = useMemo(() => {
    const m = new Map<string, PartialFileStatus>();
    for (const r of scan?.partial ?? []) m.set(normalizePath(r.path), r);
    return m;
  }, [scan?.partial]);

  const oversizedByPath = useMemo(() => {
    const m = new Map<string, PartialFileStatus>();
    for (const r of scan?.oversized ?? []) m.set(normalizePath(r.path), r);
    return m;
  }, [scan?.oversized]);

  const allExpectedFiles: ExpectedFile[] = useMemo(() => {
    const out: ExpectedFile[] = [];
    for (const it of MANIFEST) out.push({ path: normalizePath(it.fileName), sizeBytes: it.sizeBytes });
    return out;
  }, []);

  const refreshPreflight = useCallback(async () => {
    setPreflightErr(null);
    try {
      const [d, r] = await Promise.all([
        invoke<DeploymentDiskSpaceReport>("deployment_check_disk_space"),
        invoke<FileScanReport>("deployment_scan_files", { expected: allExpectedFiles }),
      ]);
      setDisk(d ?? null);
      setScan(r ?? null);
    } catch (e) {
      setDisk(null);
      setScan(null);
      setPreflightErr(e != null ? String(e) : "Preflight failed.");
    }
  }, [allExpectedFiles]);

  useEffect(() => {
    void refreshPreflight();
  }, [refreshPreflight]);

  const applyPreset = useCallback((id: PresetId) => {
    setPreset(id);
    setEnabledById((prev) => {
      const next: Record<string, boolean> = { ...prev };
      for (const it of MANIFEST) next[it.id] = Array.isArray(it.defaultTiers) && it.defaultTiers.includes(id);
      return next;
    });
  }, []);

  const packagesWithStatus = useMemo(() => {
    return MANIFEST.map((it) => {
      const fileName = normalizePath(it.fileName);
      const installed = installedSet.has(fileName);
      const enabled = Boolean(enabledById[it.id]);
      const totalBytes = it.sizeBytes || 0;
      const partial = partialByPath.get(fileName);
      const overs = oversizedByPath.get(fileName);
      const remainingBytes = installed
        ? 0
        : partial
          ? Math.max(0, partial.expectedBytes - partial.downloadedBytes)
          : overs
            ? overs.expectedBytes
            : it.sizeBytes;
      return { it: { ...it, fileName }, installed, enabled, totalBytes, remainingBytes };
    });
  }, [enabledById, installedSet, partialByPath, oversizedByPath]);

  useEffect(() => {
    // Installed packages must be disabled.
    setEnabledById((prev) => {
      let changed = false;
      const next = { ...prev };
      for (const row of packagesWithStatus) {
        if (row.installed && next[row.it.id]) {
          next[row.it.id] = false;
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [packagesWithStatus]);

  const selectedRemainingBytes = useMemo(() => {
    let total = 0;
    for (const row of packagesWithStatus) {
      if (!row.enabled) continue;
      total += row.remainingBytes;
    }
    return total;
  }, [packagesWithStatus]);

  const availableBytes = disk?.availableBytes ?? 0;
  const overBudget = selectedRemainingBytes > 0 && availableBytes > 0 && selectedRemainingBytes > availableBytes;

  const selectedJobs: DownloadJob[] = useMemo(() => {
    const jobs: DownloadJob[] = [];
    for (const row of packagesWithStatus) {
      if (!row.enabled) continue;
      if (installedSet.has(row.it.fileName)) continue;
      const partial = partialByPath.get(row.it.fileName);
      jobs.push({
        url: row.it.url,
        fileName: row.it.fileName,
        expectedBytes: partial ? partial.expectedBytes : row.it.sizeBytes,
      });
    }
    return jobs;
  }, [installedSet, packagesWithStatus, partialByPath]);

  const startedRef = useRef(false);

  useEffect(() => {
    let unProgress: (() => void) | undefined;
    let unError: (() => void) | undefined;
    let unComplete: (() => void) | undefined;
    let unQueue: (() => void) | undefined;

    void (async () => {
      unProgress = await listen<DownloadProgressPayload>("deployment_download_progress", (ev) => {
        setActiveProgress(ev.payload ?? null);
      });
      unError = await listen<DownloadErrorPayload>("deployment_download_error", (ev) => {
        const p = ev.payload;
        if (!p) return;
        setActiveError(p);
        const url = MANIFEST.find((x) => x.fileName === p.fileName)?.url ?? "https://example.invalid/manual-download";
        setFailedFiles((prev) => {
          if (prev.some((x) => x.fileName === p.fileName)) return prev;
          return [...prev, { fileName: p.fileName, url, message: p.message }];
        });
      });
      unComplete = await listen<DownloadFileCompletePayload>("deployment_download_file_complete", (ev) => {
        const p = ev.payload;
        if (!p?.fileName) return;
        setCompletedFiles((prev) => (prev.includes(p.fileName) ? prev : [...prev, p.fileName]));
      });
      unQueue = await listen<QueueCompletePayload>("deployment_queue_complete", (ev) => {
        setQueueOk(Boolean(ev.payload?.ok));
        setStage("summary");
        startedRef.current = false;
        void refreshPreflight();
      });
    })();

    return () => {
      if (unProgress) unProgress();
      if (unError) unError();
      if (unComplete) unComplete();
      if (unQueue) unQueue();
    };
  }, [refreshPreflight]);

  const startDownload = async () => {
    setActiveError(null);
    setFailedFiles([]);
    setCompletedFiles([]);
    setQueueOk(null);
    if (startedRef.current) return;
    startedRef.current = true;
    try {
      await invoke("deployment_start_downloads", { jobs: selectedJobs, destSubdir: null });
      setStage("downloading");
    } catch (e) {
      startedRef.current = false;
      setActiveError({
        fileName: "",
        message: e != null ? String(e) : "Could not start download.",
        recoverable: true,
      });
      setStage("select");
    }
  };

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const snap = await invoke<DeploymentQueueSnapshot>("deployment_get_queue_snapshot");
        if (cancelled) return;
        if (snap.queueActive) {
          startedRef.current = true;
          setStage("downloading");
        }
      } catch {
        // ignore
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const pauseDownload = async () => {
    try {
      await invoke("deployment_pause_downloads");
    } catch {
      // ignore
    }
  };

  const cancelDownload = async () => {
    await pauseDownload();
  };

  const openUnsortedFolder = async () => {
    try {
      await invoke("deployment_open_athena_data_folder");
    } catch {
      // ignore
    }
  };

  const copyUrl = async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      // ignore
    }
  };

  const activePct =
    activeProgress && activeProgress.totalBytes > 0
      ? (activeProgress.bytesDownloaded / activeProgress.totalBytes) * 100
      : 0;

  return (
    <div className="mac-installer" aria-label="Academy downloader">
      <AnimatePresence mode="wait">
        {stage === "select" ? (
          <motion.div
            key="select"
            className="mac-installer__panel"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ type: "spring", stiffness: 320, damping: 34 }}
          >
            <div className="mac-installer__head">
              <div style={{ display: "grid", gap: 6 }}>
                <div className="mac-installer__title">Academy Downloads</div>
                <div className="mac-installer__subtitle">
                  Select an educational preset, then customize maps and reference archives. Installed items are locked.
                </div>
              </div>
              <button type="button" className="mac-link" onClick={() => void refreshPreflight()}>
                Refresh
              </button>
            </div>

            <SegmentedControl value={preset} onChange={applyPreset} />

            <div className="mac-list" role="list" aria-label="Academy packages">
              {packagesWithStatus.map((row) => {
                const disabled = row.installed || scan == null;
                const enabled = row.installed ? false : row.enabled;
                return (
                  <ToggleRow
                    key={row.it.id}
                    title={row.it.name}
                    subtitle={row.it.description}
                    installed={row.installed}
                    enabled={enabled}
                    disabled={disabled}
                    onToggle={() => {
                      if (disabled) return;
                      setEnabledById((prev) => ({ ...prev, [row.it.id]: !prev[row.it.id] }));
                    }}
                    rightLabel={fmtBytes(row.remainingBytes)}
                  />
                );
              })}
            </div>

            <div className="mac-installer__footer">
              <div className="mac-installer__footerLeft">
                <div className="mac-meta">
                  <div className="mac-meta__row">
                    <span className="mac-meta__k">Selected</span>
                    <span className="mac-meta__v">{fmtBytes(selectedRemainingBytes)}</span>
                  </div>
                  <div className="mac-meta__row">
                    <span className="mac-meta__k">Available</span>
                    <span className="mac-meta__v">
                      {disk ? fmtBytes(disk.availableBytes) : "—"}
                      {disk?.mountPoint ? <span className="mac-meta__mount"> · {disk.mountPoint}</span> : null}
                    </span>
                  </div>
                </div>

                {preflightErr ? <div className="mac-warn">{preflightErr}</div> : null}
                {overBudget ? <div className="mac-warn">Not enough free space for the selected downloads.</div> : null}
              </div>

              <button
                type="button"
                className="mac-primary"
                disabled={scan == null || selectedJobs.length === 0 || overBudget}
                onClick={() => void startDownload()}
              >
                Download
              </button>
            </div>
          </motion.div>
        ) : null}

        {stage === "downloading" ? (
          <motion.div
            key="downloading"
            className="mac-installer__panel"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.22 }}
          >
            <div className="mac-installer__head">
              <div style={{ display: "grid", gap: 6 }}>
                <div className="mac-installer__title">Downloading</div>
                <div className="mac-installer__subtitle">Queue is active. You can pause to resume later.</div>
              </div>
              <div style={{ display: "flex", gap: 10 }}>
                <button type="button" className="mac-secondary" onClick={() => void pauseDownload()}>
                  Pause
                </button>
                <button type="button" className="mac-secondary" onClick={() => void cancelDownload()}>
                  Cancel
                </button>
              </div>
            </div>

            <div className="mac-dlCard">
              <PillProgress percent={activePct} />
              <div className="mac-dlCard__row">
                <div className="mac-dlCard__file">{activeProgress?.fileName?.trim() || "Starting…"}</div>
                <div className="mac-dlCard__pct">{activeProgress ? `${clampPct(activePct).toFixed(2)}%` : "—"}</div>
              </div>
              <div className="mac-dlCard__row mac-dlCard__row--muted">
                <div>
                  {activeProgress
                    ? `${fmtBytes(activeProgress.bytesDownloaded)} / ${fmtBytes(activeProgress.totalBytes)}`
                    : "Waiting for progress…"}
                </div>
                <div>
                  {activeProgress && activeProgress.etaSeconds != null ? `ETA ${Math.max(0, activeProgress.etaSeconds)}s` : "ETA —"}
                </div>
              </div>
              {activeError ? (
                <div className="mac-warn" style={{ marginTop: 10 }}>
                  {activeError.message}
                </div>
              ) : null}
            </div>
          </motion.div>
        ) : null}

        {stage === "summary" ? (
          <motion.div
            key="summary"
            className="mac-installer__panel"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            transition={{ type: "spring", stiffness: 320, damping: 34 }}
          >
            <div className="mac-installer__head">
              <div style={{ display: "grid", gap: 6 }}>
                <div className="mac-installer__title">Summary</div>
                <div className="mac-installer__subtitle">
                  {queueOk === true ? "Downloads completed." : "Downloads finished with issues."}
                </div>
              </div>
              <div style={{ display: "flex", gap: 10 }}>
                <button
                  type="button"
                  className="mac-secondary"
                  onClick={() => {
                    setStage("select");
                    setActiveProgress(null);
                    setActiveError(null);
                    setCompletedFiles([]);
                    setFailedFiles([]);
                    setQueueOk(null);
                    void refreshPreflight();
                  }}
                >
                  Done
                </button>
              </div>
            </div>

            <div className="mac-summary">
              <div className="mac-summary__section">
                <div className="mac-summary__h">Downloaded</div>
                {completedFiles.length === 0 ? (
                  <div className="mac-summary__empty">No completed files recorded.</div>
                ) : (
                  <div className="mac-summary__list">
                    {completedFiles.map((f) => (
                      <div key={f} className="mac-summary__item">
                        {f}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="mac-summary__section">
                <div className="mac-summary__h">Ingestion Progress</div>
                <div className="mac-ingest">
                  <div className="mac-ingest__bar" />
                  <div className="mac-ingest__note">Indexing downloaded content…</div>
                </div>
              </div>

              <div className="mac-summary__section">
                <div className="mac-summary__h">Failed</div>
                {failedFiles.length === 0 ? (
                  <div className="mac-summary__empty">No failures reported.</div>
                ) : (
                  <div className="mac-summary__list">
                    {failedFiles.map((f) => (
                      <div key={f.fileName} className="mac-summary__item mac-summary__item--fail">
                        <div style={{ minWidth: 0 }}>
                          <div className="mac-summary__file">{f.fileName}</div>
                          <div className="mac-summary__err">{f.message}</div>
                        </div>
                        <button type="button" className="mac-link" onClick={() => void copyUrl(f.url)}>
                          Copy URL
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="mac-summary__actions">
                <button type="button" className="mac-secondary" onClick={() => void openUnsortedFolder()}>
                  Open Unsorted Folder
                </button>
              </div>
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

