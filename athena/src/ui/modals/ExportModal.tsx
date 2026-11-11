import { useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke, athenaListen as listen, canUseHostCapability } from "../../shared/ipc/athenaIpc";
import { type UnlistenFn } from "@tauri-apps/api/event";
import { AegisExportIoPanel, type AegisExportProgress, type AegisIoHistories } from "../../pages/export/AegisExportIoPanel";

export type UsbDriveInfo = {
  name: string;
  mount_point: string;
  total_space: number;
  available_space: number;
  device_path: string;
};

/** UI tiers for Aegis Hive Mind export. */
export type ExportTier = "fastNode" | "sisterServer";

type DiscoveredLocalGgufModel = {
  fileName: string;
  absolutePath: string;
  sizeBytes: number;
};

export type AegisExpeditionPlanStep = {
  key: string;
  label: string;
  bytes: number;
  willCopy: boolean;
};

export type AegisExportPreflight = {
  expeditionSecurityBufferBytes: number;
  assumedWriteMebibytesPerSec: number;
  /** Full app/install tree including every bundled `payload/*` pack (Bastion / Citadel / Expedition). */
  applicationBytes: number;
  /** Outpost USB app copy: same tree minus non-launcher folders under any `payload/` directory. */
  applicationBundleBytesOutpost: number;
  bundledLaunchersBytes: number;
  slimSqliteBytes: number;
  fullSqliteBytes: number;
  modelsBytes: number;
  chatModelsAllBytes: number;
  chatModelsSmallestBytes: number;
  exampleRegionExtractEstimateBytes: number;
  exampleRegionReservedBytes: number;
  offlineMapsNonSatelliteBytes: number;
  offlineMapsSatelliteBytes: number;
  offlineMapsAllBytes: number;
  libraryNonSatelliteBytes: number;
  libraryAllBytes: number;
  bundledPayloadChildrenBytes: number;
  bundledPayloadRootTotalBytes: number;
  outpostTotalBytes: number;
  bastionTotalBytes: number;
  citadelTotalBytes: number;
  outpostEstimatedSeconds: number;
  bastionEstimatedSeconds: number;
  citadelEstimatedSeconds: number;
  destinationAvailableBytes: number | null;
  destinationUsableExpeditionBudgetBytes: number | null;
  destinationFitsOutpost: boolean | null;
  destinationFitsBastion: boolean | null;
  destinationFitsCitadel: boolean | null;
  destinationFitsFastNode?: boolean | null;
  destinationFitsSisterServer?: boolean | null;
  fastNodeMeasureStatus?: string;
  sisterServerMeasureStatus?: string;
  expeditionPlan: AegisExpeditionPlanStep[] | null;
};

export type AegisPreflightProgressEvent = {
  done: boolean;
  error?: string | null;
  preflight?: AegisExportPreflight | null;
};

type Props = {
  open: boolean;
  onClose: () => void;
  tier: ExportTier;
  onTierChange: (tier: ExportTier) => void;
  scorchedEarth: boolean;
  onScorchedEarthChange: (v: boolean) => void;
  busy: boolean;
  exportProgress: AegisExportProgress | null;
  /** Rust/backend or IPC error detail while export is running or after failure. */
  exportErrorDetail: string | null;
  exportIoHistories: AegisIoHistories;
  status: string | null;
  onStartExport: (args: {
    destinationPath: string;
    devicePath: string | null;
    formatFirst: boolean;
    exportType: ExportTier;
    expeditionBudgetBytes: number | null;
    selectedExternalChatModelPath: string | null;
    includeDockerVolumes: boolean;
  }) => void;
  onCancelExport?: () => void;
};

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

function formatDurationSeconds(secs: number): string {
  if (secs <= 0) return "~0 s";
  if (secs < 90) return `~${secs} s`;
  const mins = Math.round(secs / 60);
  if (mins < 60) return `~${mins} min`;
  const hrs = (mins / 60).toFixed(1);
  return `~${hrs} hr`;
}

function normalizeMeasureStatus(raw: string | undefined | null): "ready" | "calculating" | "error" | "unknown" {
  const s = (raw ?? "").trim().toLowerCase();
  if (s === "calculating") return "calculating";
  if (s === "error") return "error";
  if (s === "ready") return "ready";
  return "unknown";
}

const MANUAL_TIER_CARDS: Array<{
  id: ExportTier;
  title: string;
  systems: string;
}> = [
  {
    id: "fastNode",
    title: "Outpost (Fast Node)",
    systems:
      "Slim field clone: app binaries + OS launchers, slim station DB, one small chat model (.gguf), and a minimal offline map when available. Excludes full library mirrors, embeddings/rerankers, and large map packs. Boots quickly to join the Hive Mind network. Mesh/radio is in the app — not a separate USB payload.",
  },
  {
    id: "sisterServer",
    title: "Sister Server (Citadel)",
    systems:
      "Full clone: complete application payload, local AI models (including embeddings/rerankers when present), databases, library mirrors, and fuller offline maps. Intended to broadcast/serve resources to the Hive Mind network.",
  },
];

export function ExportModal({
  open,
  onClose,
  tier,
  onTierChange,
  scorchedEarth,
  onScorchedEarthChange,
  busy,
  exportProgress,
  exportErrorDetail,
  exportIoHistories,
  status,
  onStartExport,
  onCancelExport,
}: Props) {
  const [usbs, setUsbs] = useState<UsbDriveInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [formatFirst, setFormatFirst] = useState(false);
  const [pf, setPf] = useState<AegisExportPreflight | null>(null);
  const [pfLoading, setPfLoading] = useState(false);
  const [pfErr, setPfErr] = useState<string | null>(null);
  const [discoveredModels, setDiscoveredModels] = useState<DiscoveredLocalGgufModel[]>([]);
  const [discoveringModels, setDiscoveringModels] = useState(false);
  const [discoverErr, setDiscoverErr] = useState<string | null>(null);
  const [selectedExternalChatModelPath, setSelectedExternalChatModelPath] = useState<string | null>(null);
  const [includeDockerVolumes, setIncludeDockerVolumes] = useState(false);
  const [dockerVolumeBytes, setDockerVolumeBytes] = useState<number | null>(null);

  const isMac =
    typeof navigator !== "undefined" && /mac/i.test(navigator.platform || navigator.userAgent || "");

  useEffect(() => {
    if (tier !== "sisterServer" && scorchedEarth) {
      onScorchedEarthChange(false);
    }
  }, [tier, scorchedEarth, onScorchedEarthChange]);

  useEffect(() => {
    if (!open || tier !== "sisterServer" || !includeDockerVolumes) {
      setDockerVolumeBytes(null);
      return;
    }
    let cancelled = false;
    void invoke<number>("marketplace_docker_volume_bytes")
      .then((n) => {
        if (!cancelled) setDockerVolumeBytes(typeof n === "number" ? n : 0);
      })
      .catch(() => {
        if (!cancelled) setDockerVolumeBytes(null);
      });
    return () => {
      cancelled = true;
    };
  }, [open, tier, includeDockerVolumes]);

  useEffect(() => {
    if (!open) {
      setFormatFirst(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setLoadErr(null);
    void invoke<UsbDriveInfo[]>("get_available_usbs")
      .then((list) => {
        if (!cancelled) {
          setUsbs(list);
          setSelectedIdx(0);
        }
      })
      .catch((e) => {
        if (!cancelled) setLoadErr(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const selected = usbs[selectedIdx];
  const noUsb = !loading && usbs.length === 0;
  const formatBlocked = formatFirst && isMac && selected && !String(selected.device_path || "").trim();
  const hostUsb = canUseHostCapability("usbExport");
  const hostFormat = canUseHostCapability("usbFormat");

  useEffect(() => {
    if (!open) {
      setPf(null);
      setPfErr(null);
      setDiscoveredModels([]);
      setDiscoverErr(null);
      setDiscoveringModels(false);
      setSelectedExternalChatModelPath(null);
      return;
    }
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    setPfLoading(true);
    setPfErr(null);
    setPf(null);
    const dest = selected?.mount_point?.trim() ? selected.mount_point : null;

    void (async () => {
      try {
        unlisten = await listen<AegisPreflightProgressEvent>("athena:aegis-preflight-progress", (ev) => {
          if (cancelled) return;
          const p = ev.payload;
          if (p.error) setPfErr(p.error);
          if (p.preflight) setPf(p.preflight);
          if (p.done) setPfLoading(false);
        });
        await invoke("start_aegis_export_preflight", { destinationPath: dest });
      } catch (e) {
        if (!cancelled) {
          setPfErr(e instanceof Error ? e.message : String(e));
          setPfLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [open, selected?.mount_point]);

  const needsExternalModelScan =
    open && tier === "fastNode" && pf != null && (pf.chatModelsSmallestBytes ?? 0) <= 0;

  useEffect(() => {
    if (!needsExternalModelScan) return;
    if (discoveringModels) return;
    if (discoveredModels.length > 0) return;
    setDiscoveringModels(true);
    setDiscoverErr(null);
    void invoke<DiscoveredLocalGgufModel[]>("scan_local_device_for_models")
      .then((rows) => setDiscoveredModels(rows ?? []))
      .catch((e) => setDiscoverErr(e instanceof Error ? e.message : String(e)))
      .finally(() => setDiscoveringModels(false));
  }, [needsExternalModelScan, discoveringModels, discoveredModels.length]);

  const tierFits = useMemo(() => {
    if (!selected) return true;
    if (!pf) return true;
    const fitFast = pf.destinationFitsFastNode ?? pf.destinationFitsOutpost;
    const fitSister = pf.destinationFitsSisterServer ?? pf.destinationFitsCitadel;
    if (tier === "fastNode") return fitFast !== false;
    return fitSister !== false;
  }, [selected, pf, tier]);

  const exportReady =
    !busy &&
    !noUsb &&
    !loading &&
    !formatBlocked &&
    selected != null &&
    tierFits &&
    (!needsExternalModelScan || (selectedExternalChatModelPath != null && selectedExternalChatModelPath.trim() !== ""));

  const exportButtonTitle = useMemo(() => {
    if (noUsb) return "No USB drive detected.";
    if (!exportReady) {
      return "Pick a drive and tier that fit this volume, fix format/device path, or satisfy Outpost model requirements.";
    }
    if (pfLoading) {
      return "Sizes may still be updating in the background; export is allowed unless a tier is marked too small.";
    }
    return undefined;
  }, [noUsb, exportReady, pfLoading]);

  function tierSizeLine(id: ExportTier): string {
    if (!pf) return pfLoading ? "Calibrating…" : "Measuring…";
    const st = normalizeMeasureStatus(id === "fastNode" ? pf.fastNodeMeasureStatus : pf.sisterServerMeasureStatus);
    const mib = pf.assumedWriteMebibytesPerSec;
    const core =
      id === "fastNode"
        ? `${formatBytes(pf.outpostTotalBytes)} · ${formatDurationSeconds(pf.outpostEstimatedSeconds)} @ ${mib} MiB/s`
        : `${formatBytes(pf.citadelTotalBytes)} · ${formatDurationSeconds(pf.citadelEstimatedSeconds)} @ ${mib} MiB/s`;
    if (st === "calculating") {
      return `Calculating… (some folders used a quick shallow size; totals may be low) · ${core}`;
    }
    if (st === "error") {
      return `Size check incomplete · ${core}`;
    }
    return core;
  }

  function tierGreyed(id: ExportTier): boolean {
    if (!pf || !selected) return false;
    const fitFast = pf.destinationFitsFastNode ?? pf.destinationFitsOutpost;
    const fitSister = pf.destinationFitsSisterServer ?? pf.destinationFitsCitadel;
    if (id === "fastNode") return fitFast === false;
    return fitSister === false;
  }

  function handleStart() {
    if (!selected || !exportReady) return;

    let ok = true;
    if (scorchedEarth && formatFirst) {
      ok = window.confirm(
        "INITIATE OMEGA PROTOCOL\n\nThis will erase the selected USB drive, then export. Scorched Earth will remove all local data after the transfer. This is irreversible. Confirm?",
      );
    } else if (scorchedEarth) {
      ok = window.confirm(
        "SCORCHED EARTH ENABLED. This will permanently delete all local intelligence and documents after the USB transfer. This is irreversible. Confirm?",
      );
    }
    if (!ok) return;

    onStartExport({
      destinationPath: selected.mount_point,
      devicePath: selected.device_path?.trim() ? selected.device_path : null,
      formatFirst,
      exportType: tier,
      expeditionBudgetBytes: null,
      selectedExternalChatModelPath: tier === "fastNode" ? selectedExternalChatModelPath : null,
      includeDockerVolumes: tier === "sisterServer" ? includeDockerVolumes : false,
    });
  }

  if (!open) return null;

  return (
    <div className="modal-backdrop mesh-radio-backdrop" onClick={onClose}>
      <div className="library-modal mesh-radio-modal glass-panel usb-export-modal" onClick={(e) => e.stopPropagation()}>
        <div className="library-header mesh-radio-header">
          <h2>Aegis Protocol</h2>
          <button type="button" className="secondary" onClick={onClose}>
            {busy ? "Hide" : "Close"}
          </button>
        </div>
        <div className="library-body" style={{ padding: "0.75rem 1rem 1rem", maxWidth: "min(720px, 92vw)" }}>
          <p className="text-muted small" style={{ marginTop: 0 }}>
            Pick a removable drive, review measured sizes, then choose the Hive Mind role you want this USB to deploy.
          </p>
          <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
            <strong>Outpost (Fast Node):</strong> slim clone — copies <code>AthenaData/app</code> (bundled defaults / Ollama seed), slim DB, one chat model if present, minimal map; does not copy full <code>user/</code> library or model trees.
          </p>
          <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
            <strong>Sister Server (Citadel):</strong> full clone (<code>app/</code> + selected <code>user/</code> data, models, library mirrors) for serving the Hive Mind network.
          </p>

          {loading ? (
            <p className="text-muted small">Detecting USB drives…</p>
          ) : loadErr ? (
            <p className="text-muted small" role="alert">
              {loadErr}
            </p>
          ) : noUsb ? (
            <p className="text-muted small">No removable volumes detected. Insert a USB drive and try again.</p>
          ) : (
            <label className="dev-toggle-row" style={{ display: "block", marginBottom: "0.65rem" }}>
              <span className="dev-toggle-label">Drive</span>
              <select
                className="settings-input"
                value={selectedIdx}
                onChange={(e) => setSelectedIdx(Number(e.target.value))}
                disabled={busy}
                aria-label="USB drive"
              >
                {usbs.map((u, i) => (
                  <option key={`${u.mount_point}-${i}`} value={i}>
                    {u.name} — {u.mount_point} (free {formatBytes(u.available_space)} / {formatBytes(u.total_space)})
                  </option>
                ))}
              </select>
            </label>
          )}

          {pfLoading ? (
            <p className="text-muted small">Measuring export sizes for this machine…</p>
          ) : null}
          {pfErr ? (
            <p className="text-muted small" role="alert">
              Preflight failed: {pfErr}
            </p>
          ) : null}
          {pf && selected ? (
            <p className="text-muted small" style={{ marginTop: 0 }}>
              Free on volume: {formatBytes(pf.destinationAvailableBytes ?? selected.available_space)}
              {pf.applicationBundleBytesOutpost < pf.applicationBytes ? (
                <>
                  {" "}
                  · Outpost app copy saves ~{formatBytes(pf.applicationBytes - pf.applicationBundleBytesOutpost)} vs full bundle (
                  {formatBytes(pf.applicationBundleBytesOutpost)} vs {formatBytes(pf.applicationBytes)})
                </>
              ) : null}
            </p>
          ) : null}

          <h3 className="text-muted small" style={{ margin: "0.75rem 0 0.35rem", letterSpacing: "0.04em" }}>
            Hive Mind export tiers
          </h3>
          <div style={{ display: "grid", gap: "0.5rem", marginBottom: "0.75rem" }}>
            {MANUAL_TIER_CARDS.map((c) => {
              const selectedTier = tier === c.id;
              const grey = tierGreyed(c.id);
              return (
                <button
                  key={c.id}
                  type="button"
                  className={selectedTier ? "glass-action-btn" : "secondary"}
                  style={{
                    textAlign: "left",
                    width: "100%",
                    justifyContent: "flex-start",
                    opacity: grey ? 0.45 : 1,
                  }}
                  onClick={() => onTierChange(c.id)}
                  disabled={busy || grey}
                  title={grey ? "This volume is too small for this export (see measured total)." : undefined}
                >
                  <div style={{ display: "grid", gap: "0.15rem" }}>
                    <div style={{ fontWeight: 700 }}>{c.title}</div>
                    <div className="text-muted small">{c.systems}</div>
                    <div className="text-muted small">Measured transfer: {tierSizeLine(c.id)}</div>
                  </div>
                </button>
              );
            })}
          </div>

          {tier === "fastNode" ? (
            <div style={{ marginBottom: "0.75rem" }}>
              <div className="text-muted small" style={{ marginBottom: "0.35rem" }}>
                {needsExternalModelScan
                  ? "No local chat .gguf under AthenaData/user/models/chat and no bundled Ollama seed under AthenaData/app/models/ollama. Select a `.gguf` from this device to include as Outpost’s single model."
                  : "Outpost copies AthenaData/app (bundled defaults) and includes one small local chat model when present (smallest `.gguf` under user/models/chat)."}
              </div>
              {discoveringModels ? <p className="text-muted small">Scanning this device for `.gguf` models…</p> : null}
              {discoverErr ? (
                <p className="text-muted small" role="alert">
                  Model scan failed: {discoverErr}
                </p>
              ) : null}
              {needsExternalModelScan && discoveredModels.length > 0 ? (
                <div
                  style={{
                    maxHeight: "180px",
                    overflow: "auto",
                    border: "1px solid rgba(255,255,255,0.08)",
                    borderRadius: "8px",
                    padding: "0.5rem 0.65rem",
                  }}
                >
                  {discoveredModels.slice(0, 250).map((m) => (
                    <button
                      key={m.absolutePath}
                      type="button"
                      className={selectedExternalChatModelPath === m.absolutePath ? "glass-action-btn" : "secondary"}
                      style={{
                        width: "100%",
                        justifyContent: "space-between",
                        textAlign: "left",
                        marginBottom: "0.35rem",
                      }}
                      onClick={() => setSelectedExternalChatModelPath(m.absolutePath)}
                      disabled={busy}
                      title={m.absolutePath}
                    >
                      <span style={{ fontWeight: 700 }}>{m.fileName}</span>
                      <span className="text-muted small">{formatBytes(m.sizeBytes)}</span>
                    </button>
                  ))}
                </div>
              ) : null}
              {needsExternalModelScan && discoveredModels.length === 0 && !discoveringModels && !discoverErr ? (
                <p className="text-muted small" role="alert">
                  No `.gguf` models were found in common locations (Ollama / LM Studio / GPT4All / Downloads).
                </p>
              ) : null}
            </div>
          ) : null}

          {!tierFits && selected && pf && !pfLoading ? (
            <p className="text-muted small" role="alert" style={{ marginTop: 0 }}>
              The selected volume does not have enough free space for this export mode. Choose a larger drive.
            </p>
          ) : null}

          {hostFormat ? (
          <label className="dev-toggle-row" style={{ alignItems: "flex-start", gap: "0.65rem", marginBottom: "0.65rem" }}>
            <input
              type="checkbox"
              checked={formatFirst}
              onChange={(e) => setFormatFirst(e.target.checked)}
              disabled={busy || !isMac}
              title={!isMac ? "Formatting is only available on macOS." : undefined}
            />
            <span>
              <span className="dev-toggle-label">Format drive</span>
              <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
                Erases the entire disk as ExFAT &quot;ATHENA&quot; (macOS only). Requires a device path; closes other apps using the drive first.
              </p>
            </span>
          </label>
          ) : null}

          {hostUsb ? (
          <label className="dev-toggle-row" style={{ alignItems: "flex-start", gap: "0.65rem", marginBottom: "0.75rem" }}>
            <input
              type="checkbox"
              checked={scorchedEarth}
              onChange={(e) => onScorchedEarthChange(e.target.checked)}
              disabled={busy || tier !== "sisterServer"}
            />
            <span style={{ color: "var(--danger, #ef4444)" }}>
              <span className="dev-toggle-label">Scorched Earth Protocol</span>
              <p className="small" style={{ margin: "0.35rem 0 0", opacity: 0.92 }}>
                When enabled, all local data will be erased from this computer after a successful Sister Server export.
              </p>
            </span>
          </label>
          ) : (
            <p className="text-muted small">USB format and scorched-earth wipe are desktop-host only.</p>
          )}

          {tier === "sisterServer" && hostUsb ? (
            <label className="dev-toggle-row" style={{ alignItems: "flex-start", gap: "0.65rem", marginBottom: "0.75rem" }}>
              <input
                type="checkbox"
                checked={includeDockerVolumes}
                onChange={(e) => setIncludeDockerVolumes(e.target.checked)}
                disabled={busy}
              />
              <span>
                <span className="dev-toggle-label">Include Docker Marketplace volumes</span>
                <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
                  Off by default (can be large). When on, archives installed Marketplace Docker volumes onto the USB.
                  Requires Docker and at least one installed Marketplace app with volumes.
                  {dockerVolumeBytes != null
                    ? ` Current volume data ≈ ${formatBytes(dockerVolumeBytes)}.`
                    : ""}
                </p>
              </span>
            </label>
          ) : null}

          {formatFirst && isMac && selected && !String(selected.device_path || "").trim() ? (
            <p className="text-muted small" role="alert">
              Could not resolve a raw device path for this volume. Unplug the drive, reinsert it, or pick another drive before formatting.
            </p>
          ) : null}

          {tier === "sisterServer" && pf && !pfLoading ? (
            <p className="text-muted small" style={{ marginTop: 0, marginBottom: "0.65rem" }}>
              Sister Server will copy about {formatBytes(pf.citadelTotalBytes)}
              {pf.citadelEstimatedSeconds > 0
                ? ` (~${formatDurationSeconds(pf.citadelEstimatedSeconds)} at ${pf.assumedWriteMebibytesPerSec} MiB/s)`
                : ""}
              .
            </p>
          ) : null}

          <div className="settings-inline-actions" style={{ flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
            <button
              type="button"
              className="secondary"
              disabled={!exportReady}
              title={exportButtonTitle}
              onClick={() => void handleStart()}
            >
              {busy ? "Exporting…" : tier === "fastNode" ? "Export Outpost" : "Export Sister Server"}
            </button>
            {busy && onCancelExport ? (
              <button type="button" className="secondary" onClick={onCancelExport}>
                Cancel export
              </button>
            ) : null}
            {busy ? (
              <span className="text-muted small dev-rebuild-progress-text" aria-live="polite">
                <span className="dev-rebuild-spinner" aria-hidden />
                Runs in the background if you hide this window.
              </span>
            ) : null}
          </div>
          {busy ? (
            <div style={{ marginTop: "0.75rem", display: "flex", flexDirection: "column", alignItems: "stretch" }} aria-live="polite">
              {(() => {
                const prog = exportProgress ?? { phase: "Preparing…", fraction: 0 };
                return (
                  <>
                    <div className="text-muted small" style={{ marginBottom: "0.35rem" }}>
                      {prog.phase}
                    </div>
                    <div className="aegis-export-progress-track">
                      <div
                        className="aegis-export-progress-fill"
                        style={{ width: `${Math.round(prog.fraction * 100)}%` }}
                      />
                    </div>
                    <div className="text-muted small" style={{ marginTop: "0.25rem", textAlign: "right" }}>
                      {Math.round(prog.fraction * 100)}%
                    </div>
                    <AegisExportIoPanel progress={exportProgress ?? prog} histories={exportIoHistories} />
                  </>
                );
              })()}
            </div>
          ) : null}
          {exportErrorDetail ? (
            <p
              className="small"
              role="alert"
              style={{
                marginTop: "0.5rem",
                color: "var(--danger, #ef4444)",
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
              }}
            >
              {exportErrorDetail}
            </p>
          ) : null}
          {status ? (
            <p className="text-muted small" style={{ marginTop: "0.65rem" }} aria-live="polite">
              {status}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
