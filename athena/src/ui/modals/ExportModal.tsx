import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AegisExportIoPanel, type AegisExportProgress, type AegisIoHistories } from "../../pages/export/AegisExportIoPanel";

export type UsbDriveInfo = {
  name: string;
  mount_point: string;
  total_space: number;
  available_space: number;
  device_path: string;
};

/** Rust `export_type` values for `export_athena_to_usb`. */
export type ExportTier = "outpost" | "bastion" | "citadel" | "expedition";

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
  expeditionPlan: AegisExpeditionPlanStep[] | null;
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
  exportIoHistories: AegisIoHistories;
  status: string | null;
  onStartExport: (args: {
    destinationPath: string;
    devicePath: string | null;
    formatFirst: boolean;
    exportType: ExportTier;
    expeditionBudgetBytes: number | null;
  }) => void;
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

const MANUAL_TIER_CARDS: Array<{
  id: Exclude<ExportTier, "expedition">;
  title: string;
  systems: string;
}> = [
  {
    id: "outpost",
    title: "Outpost",
    systems:
      "Lean field kit: application, Windows/Linux launchers, and a slim copy of the station database (personnel, chats, logbook, radio, mesh, calendar, map pins — library vectors and ingested file index omitted). No AI models, no offline map extract.",
  },
  {
    id: "bastion",
    title: "Bastion",
    systems:
      "Full station intelligence: AI models, complete SQLite index and indexed documents, Fairlee, VT proximity terrain extract (25 mi), priority payload packs, offline maps excluding satellite rasters, and library folders with the same satellite skip (terrain / pins / markers stay in the database).",
  },
  {
    id: "citadel",
    title: "Citadel",
    systems:
      "Everything ATHENA uses locally: full bundled payload, all offline maps, all library folders, models, and the full database. Use when the USB is large enough; Scorched Earth is only offered here.",
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
  exportIoHistories,
  status,
  onStartExport,
}: Props) {
  const [usbs, setUsbs] = useState<UsbDriveInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [formatFirst, setFormatFirst] = useState(false);
  const [pf, setPf] = useState<AegisExportPreflight | null>(null);
  const [pfLoading, setPfLoading] = useState(false);
  const [pfErr, setPfErr] = useState<string | null>(null);

  const isMac =
    typeof navigator !== "undefined" && /mac/i.test(navigator.platform || navigator.userAgent || "");

  useEffect(() => {
    if (tier !== "citadel" && scorchedEarth) {
      onScorchedEarthChange(false);
    }
  }, [tier, scorchedEarth, onScorchedEarthChange]);

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

  useEffect(() => {
    if (!open) {
      setPf(null);
      setPfErr(null);
      return;
    }
    let cancelled = false;
    setPfLoading(true);
    setPfErr(null);
    const dest = selected?.mount_point?.trim() ? selected.mount_point : null;
    void invoke<AegisExportPreflight>("aegis_export_preflight", { destinationPath: dest })
      .then((r) => {
        if (!cancelled) setPf(r);
      })
      .catch((e) => {
        if (!cancelled) {
          setPf(null);
          setPfErr(e instanceof Error ? e.message : String(e));
        }
      })
      .finally(() => {
        if (!cancelled) setPfLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, selected?.mount_point]);

  const tierFits = useMemo(() => {
    if (!selected || !pf) return true;
    if (tier === "outpost") return pf.destinationFitsOutpost !== false;
    if (tier === "bastion") return pf.destinationFitsBastion !== false;
    if (tier === "citadel") return pf.destinationFitsCitadel !== false;
    const u = pf.destinationUsableExpeditionBudgetBytes ?? 0;
    return u >= 16 * 1024 * 1024;
  }, [selected, pf, tier]);

  const expeditionBudget =
    pf?.destinationUsableExpeditionBudgetBytes != null
      ? Math.floor(pf.destinationUsableExpeditionBudgetBytes)
      : null;

  const exportReady =
    !busy &&
    !noUsb &&
    !loading &&
    !formatBlocked &&
    !pfLoading &&
    pf != null &&
    tierFits &&
    (tier !== "expedition" || (expeditionBudget != null && expeditionBudget >= 16 * 1024 * 1024));

  function tierSizeLine(id: Exclude<ExportTier, "expedition">): string {
    if (!pf) return "Measuring…";
    const mib = pf.assumedWriteMebibytesPerSec;
    if (id === "outpost") {
      return `${formatBytes(pf.outpostTotalBytes)} · ${formatDurationSeconds(pf.outpostEstimatedSeconds)} @ ${mib} MiB/s`;
    }
    if (id === "bastion") {
      return `${formatBytes(pf.bastionTotalBytes)} · ${formatDurationSeconds(pf.bastionEstimatedSeconds)} @ ${mib} MiB/s`;
    }
    return `${formatBytes(pf.citadelTotalBytes)} · ${formatDurationSeconds(pf.citadelEstimatedSeconds)} @ ${mib} MiB/s`;
  }

  function tierGreyed(id: Exclude<ExportTier, "expedition">): boolean {
    if (!pf || !selected) return false;
    if (id === "outpost") return pf.destinationFitsOutpost === false;
    if (id === "bastion") return pf.destinationFitsBastion === false;
    return pf.destinationFitsCitadel === false;
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
      expeditionBudgetBytes: tier === "expedition" ? expeditionBudget : null,
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
            Pick a removable drive, review measured sizes, then choose how much of ATHENA to copy. Expedition fills the drive in priority order after a small safety buffer.
          </p>
          <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
            <strong>Why tiers differ:</strong> Bastion and Citadel measure the <em>full</em> application bundle plus USB <code>data/</code> models, database, maps, and libraries. Outpost uses a <em>slimmer</em> app copy: it skips non-launcher folders under bundled{" "}
            <code>payload/</code> <em>and</em> the bundled <code>Resources/embedding</code> and <code>Resources/reranker</code> trees (default ONNX weights from <code>tauri.conf.json</code>), while still writing Windows/Linux launchers at the drive root. That
            is usually where most of the “~50+ GB” similarity was coming from before.
          </p>
          <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
            <strong>Compression:</strong> Zipping the whole stick would not match “run from USB” unless you extract first. Outpost shrinks the real bytes written by omitting non-essential pack folders from the Mac/Windows app copy; already-compressed assets
            (models, some archives) rarely shrink much further.
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
              Free on volume: {formatBytes(pf.destinationAvailableBytes ?? selected.available_space)} · Usable for Expedition
              (after {formatBytes(pf.expeditionSecurityBufferBytes)} buffer):{" "}
              {formatBytes(pf.destinationUsableExpeditionBudgetBytes ?? 0)}
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
            Manual export
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

          <h3 className="text-muted small" style={{ margin: "0.5rem 0 0.35rem", letterSpacing: "0.04em" }}>
            Expedition (auto)
          </h3>
          <button
            type="button"
            className={tier === "expedition" ? "glass-action-btn" : "secondary"}
            style={{ textAlign: "left", width: "100%", justifyContent: "flex-start", marginBottom: "0.5rem" }}
            onClick={() => onTierChange("expedition")}
            disabled={
              busy ||
              (pf != null &&
                selected != null &&
                (pf.destinationUsableExpeditionBudgetBytes == null || pf.destinationUsableExpeditionBudgetBytes < 16 * 1024 * 1024))
            }
            title={
              pf != null && (pf.destinationUsableExpeditionBudgetBytes ?? 0) < 16 * 1024 * 1024
                ? "USB too small for Expedition (needs at least 16 MiB after the safety buffer)."
                : undefined
            }
          >
            <div style={{ display: "grid", gap: "0.15rem" }}>
              <div style={{ fontWeight: 700 }}>Expedition</div>
              <div className="text-muted small">
                Copies essentials first (app, launchers, models, database, terrain extract), then maps without satellite rasters, library
                folders, payload packs, and finally satellite tiles until the budget is exhausted.
              </div>
            </div>
          </button>

          {tier === "expedition" && pf?.expeditionPlan && pf.expeditionPlan.length > 0 ? (
            <div
              style={{
                marginBottom: "0.75rem",
                maxHeight: "220px",
                overflow: "auto",
                border: "1px solid rgba(255,255,255,0.08)",
                borderRadius: "8px",
                padding: "0.5rem 0.65rem",
              }}
            >
              <div className="text-muted small" style={{ marginBottom: "0.35rem" }}>
                Staging preview (same order as export)
              </div>
              <table className="text-muted small" style={{ width: "100%", borderCollapse: "collapse" }}>
                <thead>
                  <tr>
                    <th style={{ textAlign: "left", padding: "0.2rem 0.35rem" }}>Stage</th>
                    <th style={{ textAlign: "right", padding: "0.2rem 0.35rem" }}>Bytes</th>
                    <th style={{ textAlign: "center", padding: "0.2rem 0.35rem" }}>Fits</th>
                  </tr>
                </thead>
                <tbody>
                  {pf.expeditionPlan.map((row) => (
                    <tr key={row.key}>
                      <td style={{ padding: "0.2rem 0.35rem" }}>{row.label}</td>
                      <td style={{ textAlign: "right", padding: "0.2rem 0.35rem" }}>{formatBytes(row.bytes)}</td>
                      <td style={{ textAlign: "center", padding: "0.2rem 0.35rem" }}>{row.willCopy ? "Yes" : "No"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          {!tierFits && selected && pf && !pfLoading ? (
            <p className="text-muted small" role="alert" style={{ marginTop: 0 }}>
              The selected volume does not have enough free space for this export mode. Choose a larger drive or Expedition.
            </p>
          ) : null}

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

          <label className="dev-toggle-row" style={{ alignItems: "flex-start", gap: "0.65rem", marginBottom: "0.75rem" }}>
            <input
              type="checkbox"
              checked={scorchedEarth}
              onChange={(e) => onScorchedEarthChange(e.target.checked)}
              disabled={busy || tier !== "citadel"}
            />
            <span style={{ color: "var(--danger, #ef4444)" }}>
              <span className="dev-toggle-label">Scorched Earth Protocol</span>
              <p className="small" style={{ margin: "0.35rem 0 0", opacity: 0.92 }}>
                When enabled, all local data will be erased from this computer after a successful Citadel export.
              </p>
            </span>
          </label>

          {formatFirst && isMac && selected && !String(selected.device_path || "").trim() ? (
            <p className="text-muted small" role="alert">
              Could not resolve a raw device path for this volume. Unplug the drive, reinsert it, or pick another drive before formatting.
            </p>
          ) : null}

          <div className="settings-inline-actions" style={{ flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
            <button
              type="button"
              className="secondary"
              disabled={!exportReady}
              title={noUsb ? "No USB drive detected." : !exportReady ? "Wait for sizing or pick a valid tier." : undefined}
              onClick={() => void handleStart()}
            >
              {busy ? "Exporting…" : "Export"}
            </button>
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
