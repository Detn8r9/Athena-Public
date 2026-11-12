import { useCallback, useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../../shared/ipc/athenaIpc";
import { athenaEmit as emit } from "../../../shared/ipc/athenaIpc";
import { AlertTriangle } from "lucide-react";
import { estimatedDownloadMb, type MapType } from "../../../shared/lib/MapCatalogIndex";
import { startSatelliteRegionDownload, listDownloadedOfflineMapIds } from "./offlineMapManagerCatalog";
import { SoftToast } from "../../../ui/components/SoftToast";

const EXTRACT_TERMINAL_MAX = 200;

type AvailableStoragePayload = { availableBytes: number; totalBytes: number; path: string };

function readStoragePayload(raw: unknown): AvailableStoragePayload | null {
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const o = raw as Record<string, unknown>;
  const ab = o["availableBytes"] ?? o["available_bytes"];
  const tb = o["totalBytes"] ?? o["total_bytes"];
  const path = o["path"];
  if (typeof ab !== "number" || typeof tb !== "number" || typeof path !== "string") {
    return null;
  }
  return { availableBytes: ab, totalBytes: tb, path };
}

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

function notifyOfflineMapsUpdated() {
  void emit("athena:offline-maps-updated", {});
}

export type MapArchiveMapTypeCardProps = {
  mapType: MapType;
  radiusMiles: number;
  canRun: boolean;
  minLon: number;
  minLat: number;
  maxLon: number;
  maxLat: number;
  locationLabel: string;
  open: boolean;
};

export function MapArchiveMapTypeCard({
  mapType,
  radiusMiles,
  canRun,
  minLon,
  minLat,
  maxLon,
  maxLat,
  locationLabel,
  open,
}: MapArchiveMapTypeCardProps) {
  const [storage, setStorage] = useState<AvailableStoragePayload | null>(null);
  const [storageErr, setStorageErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [termLines, setTermLines] = useState<string[]>([]);
  const [statusLine, setStatusLine] = useState<string>("");
  const [jobId, setJobId] = useState<string | null>(null);
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);
  const activeJobRef = useRef<string | null>(null);
  const debounceRef = useRef<number | undefined>(undefined);

  const estMb = estimatedDownloadMb(mapType, radiusMiles);
  const estBytes = estMb * 1024 * 1024;
  const avail = storage?.availableBytes ?? 0;
  const overThreshold = storage !== null && estBytes > 0.8 * avail;
  const disabled = !canRun || busy || overThreshold || !mapType.acquirable || !mapType.tauriCommand;

  const refreshStorage = useCallback(() => {
    if (!open) {
      return;
    }
    void invoke<unknown>("check_available_storage", { path: null })
      .then((raw) => {
        const p = readStoragePayload(raw);
        if (p) {
          setStorage(p);
        }
        setStorageErr(null);
      })
      .catch((e) => {
        setStorageErr(String(e));
        setStorage(null);
      });
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(() => {
      void refreshStorage();
    }, 280);
    return () => window.clearTimeout(debounceRef.current);
  }, [open, radiusMiles, refreshStorage]);

  useEffect(() => {
    if (!open) {
      return;
    }
    if (mapType.tauriCommand !== "extract_pmtiles_region" && mapType.tauriCommand !== "start_satellite_region_download") {
      return;
    }
    const cmd = mapType.tauriCommand;
    const unsubs: Array<() => void> = [];
    let cancelled = false;

    const push = (u: () => void) => {
      if (cancelled) {
        u();
      } else {
        unsubs.push(u);
      }
    };

    void (async () => {
      if (cmd === "extract_pmtiles_region") {
        push(
          await listen<{ job_id: string; line: string; stream: "stdout" | "stderr" }>("pmtiles_region_extract_progress", (e) => {
            if (e.payload.job_id !== activeJobRef.current) {
              return;
            }
            const prefix = e.payload.stream === "stderr" ? "[stderr] " : "";
            setTermLines((prev) => {
              const next = [...prev, `${prefix}${e.payload.line}`];
              return next.length > EXTRACT_TERMINAL_MAX ? next.slice(-EXTRACT_TERMINAL_MAX) : next;
            });
          }),
        );
        if (cancelled) {
          return;
        }
        push(
          await listen<{ job_id: string; output_path: string }>("pmtiles_region_extract_complete", (e) => {
            if (e.payload.job_id !== activeJobRef.current) {
              return;
            }
            activeJobRef.current = null;
            setJobId(null);
            setBusy(false);
            setTermLines([]);
            setToast({ tone: "success", message: `${mapType.name} downloaded successfully.` });
            void listDownloadedOfflineMapIds().then(() => notifyOfflineMapsUpdated());
          }),
        );
        if (cancelled) {
          return;
        }
        push(
          await listen<{ job_id: string; message: string }>("pmtiles_region_extract_error", (e) => {
            if (e.payload.job_id !== activeJobRef.current) {
              return;
            }
            activeJobRef.current = null;
            setJobId(null);
            setBusy(false);
            setTermLines((p) => [...p, `Error: ${e.payload.message}`].slice(-EXTRACT_TERMINAL_MAX));
          }),
        );
      } else {
        push(
          await listen<{ job_id: string; downloaded_tiles: number; total_tiles: number }>("satellite_region_download_progress", (e) => {
            if (e.payload.job_id !== activeJobRef.current) {
              return;
            }
            const t = Math.max(1, e.payload.total_tiles);
            const pct = Math.min(100, (e.payload.downloaded_tiles / t) * 100);
            setStatusLine(
              `${e.payload.downloaded_tiles.toLocaleString()} / ${e.payload.total_tiles.toLocaleString()} tiles (${pct.toFixed(0)}%)`,
            );
          }),
        );
        if (cancelled) {
          return;
        }
        push(
          await listen<{ job_id: string; path: string }>("satellite_region_download_complete", (e) => {
            if (e.payload.job_id !== activeJobRef.current) {
              return;
            }
            activeJobRef.current = null;
            setJobId(null);
            setBusy(false);
            setTermLines((p) => [...p, `Saved: ${e.payload.path}`].slice(-EXTRACT_TERMINAL_MAX));
            setStatusLine("Complete");
            setToast({ tone: "success", message: `${mapType.name} downloaded successfully.` });
            void listDownloadedOfflineMapIds().then(() => notifyOfflineMapsUpdated());
          }),
        );
        if (cancelled) {
          return;
        }
        push(
          await listen<{ job_id: string; message: string }>("satellite_region_download_error", (e) => {
            if (e.payload.job_id !== activeJobRef.current) {
              return;
            }
            activeJobRef.current = null;
            setJobId(null);
            setBusy(false);
            setTermLines((p) => [...p, `Error: ${e.payload.message}`].slice(-EXTRACT_TERMINAL_MAX));
          }),
        );
      }
    })();

    return () => {
      cancelled = true;
      for (const u of unsubs) u();
    };
  }, [open, mapType.tauriCommand]);

  const onDownload = () => {
    if (disabled) {
      return;
    }
    if (mapType.tauriCommand === "start_satellite_region_download") {
      setBusy(true);
      setTermLines([]);
      setStatusLine("");
      activeJobRef.current = null;
      void (async () => {
        try {
          const j = await startSatelliteRegionDownload({
            minLon,
            minLat,
            maxLon,
            maxLat,
            minZoom: 10,
            maxZoom: 15,
            locationLabel: locationLabel || "region",
          });
          activeJobRef.current = j;
          setJobId(j);
        } catch (e) {
          setBusy(false);
          setTermLines([String(e)]);
        }
      })();
    } else if (mapType.tauriCommand === "extract_pmtiles_region") {
      setBusy(true);
      setTermLines([]);
      setStatusLine("");
      activeJobRef.current = null;
      void (async () => {
        try {
          const j = await invoke<string>("extract_pmtiles_region", { minLon, minLat, maxLon, maxLat });
          activeJobRef.current = j;
          setJobId(j);
        } catch (e) {
          setBusy(false);
          setTermLines([e != null ? String(e) : "Extract failed."]);
        }
      })();
    }
  };

  if (!mapType.acquirable) {
    return null;
  }

  return (
    <>
      <motion.article
        className="map-archive-card"
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -4 }}
        transition={{ type: "spring", stiffness: 320, damping: 32 }}
      >
        <h3 className="map-archive-card__title">{mapType.name}</h3>
        <p className="map-archive-card__body text-muted">{mapType.description}</p>
        <p className="map-archive-card__meta text-muted small">
          Est. download ~{estMb < 0.1 ? estMb.toFixed(2) : estMb < 10 ? estMb.toFixed(1) : estMb.toFixed(0)} MB
          {storage && !storageErr ? <span> · {formatBytes(avail)} free on this disk</span> : null}
        </p>
        {storageErr ? (
          <p className="small" role="alert" style={{ color: "var(--tertiary)" }}>
            Storage check: {storageErr}
          </p>
        ) : null}
        <AnimatePresence>
          {overThreshold ? (
            <motion.div
              className="map-archive-card__warning"
              key="w"
              initial={{ opacity: 0, y: -6, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: -4, scale: 0.99 }}
              transition={{ type: "spring", stiffness: 360, damping: 28 }}
              role="alert"
            >
              <AlertTriangle size={14} aria-hidden />
              <span>
                Estimated {formatBytes(estBytes)} is over 80% of {formatBytes(avail)} free. Free space on disk or choose a
                smaller radius.
              </span>
            </motion.div>
          ) : null}
        </AnimatePresence>
        <div className="map-archive-card__action">
          <button type="button" className="map-archive-card__dl" onClick={onDownload} disabled={Boolean(disabled)}>
            Download area
          </button>
        </div>
        {jobId ? (
          <p className="text-muted small" style={{ margin: "0.4rem 0 0" }}>
            Job: <code>{jobId}</code>
          </p>
        ) : null}
        {mapType.tauriCommand === "start_satellite_region_download" && statusLine ? (
          <p className="text-muted small" style={{ margin: "0.4rem 0 0" }}>{statusLine}</p>
        ) : null}
        {termLines.length > 0 ? (
          <pre className="map-archive-card__log" aria-live="polite">
            {termLines.join("\n")}
          </pre>
        ) : null}
      </motion.article>
      <SoftToast
        open={toast != null}
        tone={toast?.tone ?? "success"}
        message={toast?.message ?? ""}
        onClose={() => setToast(null)}
      />
    </>
  );
}
