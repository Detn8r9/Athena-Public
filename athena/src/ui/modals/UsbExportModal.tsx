import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type UsbDriveInfo = {
  name: string;
  mount_point: string;
  total_space: number;
  available_space?: number;
  device_path: string;
};

type Props = {
  open: boolean;
  onClose: () => void;
  exportType: "essentials" | "complete";
  scorchedEarth: boolean;
  onScorchedEarthChange: (v: boolean) => void;
  busy: boolean;
  status: string | null;
  onStartExport: (args: {
    destinationPath: string;
    devicePath: string | null;
    formatFirst: boolean;
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

export function UsbExportModal({
  open,
  onClose,
  exportType,
  scorchedEarth,
  onScorchedEarthChange,
  busy,
  status,
  onStartExport,
}: Props) {
  const [usbs, setUsbs] = useState<UsbDriveInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [formatFirst, setFormatFirst] = useState(false);

  const isMac =
    typeof navigator !== "undefined" && /mac/i.test(navigator.platform || navigator.userAgent || "");

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

  function handleStart() {
    if (!selected || busy || noUsb || formatBlocked) return;

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
    });
  }

  if (!open) return null;

  const title = exportType === "essentials" ? "Export essentials" : "Export full archive";

  return (
    <div className="modal-backdrop mesh-radio-backdrop" onClick={onClose}>
      <div className="library-modal mesh-radio-modal glass-panel usb-export-modal" onClick={(e) => e.stopPropagation()}>
        <div className="library-header mesh-radio-header">
          <h2>{title}</h2>
          <button type="button" className="secondary" onClick={onClose} disabled={busy}>
            Close
          </button>
        </div>
        <div className="library-body" style={{ padding: "0.75rem 1rem 1rem", maxWidth: "min(520px, 92vw)" }}>
          <p className="text-muted small" style={{ marginTop: 0 }}>
            Select a removable drive. Models and data copy into a portable layout under <code>data</code> on the drive.
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
                    {u.name} — {u.mount_point} ({formatBytes(u.total_space)})
                  </option>
                ))}
              </select>
            </label>
          )}

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
              disabled={busy}
            />
            <span>
              <span className="dev-toggle-label">Scorched Earth Protocol</span>
              <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
                When enabled, all local data will be erased from this computer after a successful export.
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
              disabled={busy || noUsb || loading || formatBlocked}
              title={noUsb ? "No USB drive detected." : undefined}
              onClick={() => void handleStart()}
            >
              {busy ? "Working…" : "Export"}
            </button>
            {busy ? (
              <span className="text-muted small dev-rebuild-progress-text" aria-live="polite">
                <span className="dev-rebuild-spinner" aria-hidden />
                Copying…
              </span>
            ) : null}
          </div>
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
