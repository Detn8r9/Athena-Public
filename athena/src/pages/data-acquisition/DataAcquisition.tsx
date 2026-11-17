import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { KnowledgeCatalogView } from "../knowledge-catalog/KnowledgeCatalogView";
import { DynamicLibraryInstaller } from "../../domains/features/library-installer/DynamicLibraryInstaller";
import { ArchiveMapsTab, type ArchiveMapsTabProps } from "../../domains/features/library-installer/ArchiveMapsTab";
import { MapProvisioner } from "../../domains/maps/MapProvisioner";
import { AcademyProvisioner } from "../../domains/academy/AcademyProvisioner";
import { ModelProvisioner } from "../../domains/models/ModelProvisioner";
import { useNetworkStatus } from "../../shared/hooks/useNetworkStatus";
import { mergeStagedDownloadJobs } from "../../shared/dataAcquisition/stagedDownloadJobs";
import {
  partitionJobsForHubUi,
  useDownloadQueueStore,
} from "../../stores/downloadQueueStore";
import { DownloadQueueView } from "./DownloadQueueView";
import { StationInventoryView } from "./StationInventoryView";
import type { DeploymentQueueSnapshot, DiskReport } from "./downloadsTypes";
import { DownloadFolderSetting } from "./DownloadFolderSetting";
import "./DataAcquisition.css";

export type DataAcquisitionTabId = "station" | "queue" | "packages" | "academy" | "maps" | "models" | "documents";

function queueDownloadedBytes(snapshot: DeploymentQueueSnapshot | null): number {
  if (!snapshot?.jobs?.length) return 0;
  return snapshot.jobs.reduce((s, j) => s + Math.max(0, j.bytesDownloaded ?? 0), 0);
}

function fmtBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GiB`;
  const mb = bytes / 1024 ** 2;
  if (mb >= 1) return `${mb.toFixed(0)} MiB`;
  const kb = bytes / 1024;
  return `${kb.toFixed(0)} KiB`;
}

function queueTotalBytes(snapshot: DeploymentQueueSnapshot | null): number {
  if (!snapshot?.jobs?.length) return 0;
  let sum = 0;
  for (const j of snapshot.jobs) {
    const t = j.totalBytes;
    if (t != null && t > 0) sum += Math.max(0, t - (j.bytesDownloaded ?? 0));
    else sum += j.bytesDownloaded ?? 0;
  }
  return sum;
}

export interface DataAcquisitionProps {
  mapsTabProps: ArchiveMapsTabProps;
  libraryError: string | null;
  activeTab: DataAcquisitionTabId;
  onActiveTabChange: (tab: DataAcquisitionTabId) => void;
  onOpenMesh: () => void;
}

export const CATEGORY_TABS: Array<{ id: DataAcquisitionTabId; label: string }> = [
  { id: "station", label: "Installed" },
  { id: "packages", label: "Library" },
  { id: "academy", label: "Academy" },
  { id: "maps", label: "Maps" },
  { id: "models", label: "AI models" },
  { id: "documents", label: "Documents" },
];

export function DataAcquisition({
  mapsTabProps,
  libraryError,
  activeTab,
  onActiveTabChange,
  onOpenMesh,
}: DataAcquisitionProps) {
  const { isOnline } = useNetworkStatus();
  const [queueSnap, setQueueSnap] = useState<DeploymentQueueSnapshot | null>(null);
  const [disk, setDisk] = useState<DiskReport | null>(null);
  const [mapProvisionEstimateBytes, setMapProvisionEstimateBytes] = useState(0);
  const [academyProvisionEstimateBytes, setAcademyProvisionEstimateBytes] = useState(0);

  const jobs = useDownloadQueueStore((s) => s.jobs);
  const packageCatalogPlannedBytes = useDownloadQueueStore((s) => s.packageCatalogPlannedBytes);
  const mapProvisionPinnedBytes = useDownloadQueueStore((s) => s.mapProvisionPinnedBytes);
  const cartClearNonce = useDownloadQueueStore((s) => s.cartClearNonce);
  const [unifiedBusy, setUnifiedBusy] = useState(false);
  const [queueToast, setQueueToast] = useState<string | null>(null);
  const queueToastTimerRef = useRef<number | null>(null);
  const dlSampleRef = useRef<{ at: number; downloaded: number } | null>(null);
  const [measuredSpeedMibPerSec, setMeasuredSpeedMibPerSec] = useState<number | null>(null);
  // Remote-size probe is intentionally removed: header metrics use planned bytes (fast + stable).
  const showQueueToast = useCallback((message: string) => {
    if (queueToastTimerRef.current) window.clearTimeout(queueToastTimerRef.current);
    setQueueToast(message);
    queueToastTimerRef.current = window.setTimeout(() => {
      setQueueToast(null);
      queueToastTimerRef.current = null;
    }, 3200);
  }, []);

  const clearPlannedQueue = useCallback(() => {
    useDownloadQueueStore.getState().clearQueue();
    setMapProvisionEstimateBytes(0);
    setAcademyProvisionEstimateBytes(0);
    showQueueToast("Cleared the planned download queue.");
  }, [showQueueToast]);

  const mergedDeployJobs = useMemo(() => mergeStagedDownloadJobs([jobs]), [jobs]);

  const { packageJobs: queuedPackageJobs } = useMemo(() => partitionJobsForHubUi(jobs), [jobs]);
  const mergedDeployBytes = useMemo(
    () => mergedDeployJobs.reduce((s, j) => s + Math.max(0, j.expectedBytes), 0),
    [mergedDeployJobs],
  );

  const packageJobBytes = useMemo(
    () => queuedPackageJobs.reduce((s, j) => s + Math.max(0, j.expectedBytes), 0),
    [queuedPackageJobs],
  );

  const mergedDeployBytesDisplay = useMemo(
    () => mergedDeployBytes - packageJobBytes + Math.max(packageJobBytes, packageCatalogPlannedBytes),
    [mergedDeployBytes, packageJobBytes, packageCatalogPlannedBytes],
  );

  const mapBytesForPlan =
    mapProvisionPinnedBytes != null && mapProvisionPinnedBytes > 0
      ? mapProvisionPinnedBytes
      : Math.max(0, mapProvisionEstimateBytes);

  const refreshFooter = useCallback(async () => {
    const now = typeof performance !== "undefined" ? performance.now() : Date.now();
    try {
      const [q, d] = await Promise.all([
        invoke<DeploymentQueueSnapshot>("deployment_get_queue_snapshot"),
        invoke<DiskReport>("deployment_check_disk_space"),
      ]);
      const downloaded = queueDownloadedBytes(q);
      const prev = dlSampleRef.current;
      if (q?.queueActive && prev != null) {
        const dtSec = (now - prev.at) / 1000;
        const delta = downloaded - prev.downloaded;
        if (dtSec > 0.35 && delta > 0) {
          const mibps = delta / (1024 * 1024) / dtSec;
          setMeasuredSpeedMibPerSec((old) =>
            old == null ? mibps : old * 0.65 + mibps * 0.35,
          );
        }
      }
      if (!q?.queueActive) {
        setMeasuredSpeedMibPerSec(null);
      }
      dlSampleRef.current = { at: now, downloaded };
      setQueueSnap(q ?? null);
      setDisk(d ?? null);
    } catch {
      setQueueSnap(null);
      setDisk(null);
    }
  }, []);

  useEffect(() => {
    void refreshFooter();
    const id = window.setInterval(() => void refreshFooter(), 1500);
    return () => window.clearInterval(id);
  }, [refreshFooter]);


  /** Only count running deployment bytes when a session is active *and* the snapshot still lists jobs. */
  const backendActiveJobCount = queueSnap?.jobs?.length ?? 0;
  const backendQueueBytes =
    queueSnap?.queueActive && backendActiveJobCount > 0 ? queueTotalBytes(queueSnap) : 0;

  // (reserved for future dashboard enhancements)

  const plannedHttpBytes =
    mergedDeployJobs.length === 0 && packageCatalogPlannedBytes === 0 ? 0 : mergedDeployBytesDisplay;
  const queueBytes = backendQueueBytes;
  const footerQueueBytes =
    queueBytes +
    plannedHttpBytes +
    mapBytesForPlan +
    Math.max(0, academyProvisionEstimateBytes);

  const planExceedsDisk =
    disk != null &&
    Number.isFinite(disk.availableBytes) &&
    disk.availableBytes > 0 &&
    footerQueueBytes > disk.availableBytes;

  const activeDownloadPct = useMemo(() => {
    if (!queueSnap?.queueActive || !queueSnap?.jobs?.length) return 0;
    const withTotals = queueSnap.jobs.filter((j) => j.totalBytes != null && (j.totalBytes ?? 0) > 0);
    if (withTotals.length === 0) return 0;
    const sumD = withTotals.reduce((s, j) => s + Math.max(0, j.bytesDownloaded ?? 0), 0);
    const sumT = withTotals.reduce((s, j) => s + Math.max(0, j.totalBytes ?? 0), 0);
    return sumT > 0 ? Math.max(0, Math.min(100, Math.round((sumD / sumT) * 100))) : 0;
  }, [queueSnap]);

  const startUnifiedDownloads = async () => {
    if (mergedDeployJobs.length === 0) {
      showQueueToast("Select at least one package, Academy archive, or GGUF model with a pending download.");
      return;
    }
    if (planExceedsDisk) {
      showQueueToast(
        `This plan needs about ${fmtBytes(footerQueueBytes)} but only ${fmtBytes(disk?.availableBytes ?? 0)} is free on ${disk?.mountPoint ?? "disk"}. Reduce selections or free space before starting.`,
      );
      return;
    }
    if (queueSnap?.queueActive) {
      showQueueToast("A download session is already running. Pause or wait for it to finish first.");
      return;
    }
    setUnifiedBusy(true);
    try {
      await invoke("deployment_start_downloads", { jobs: mergedDeployJobs, destSubdir: null });
      useDownloadQueueStore.getState().clearQueue();
      void refreshFooter();
    } catch (e) {
      showQueueToast(e != null ? String(e) : "Could not start downloads.");
    } finally {
      setUnifiedBusy(false);
    }
  };

  const pauseDownloads = async () => {
    setUnifiedBusy(true);
    try {
      await invoke("deployment_pause_downloads");
      showQueueToast("Pausing. Finished files stay, and partial files resume next time.");
      void refreshFooter();
    } catch (e) {
      showQueueToast(String(e));
    } finally {
      setUnifiedBusy(false);
    }
  };

  const retryUnfinished = async () => {
    setUnifiedBusy(true);
    try {
      const n = await invoke<number>("deployment_retry_unfinished");
      showQueueToast(`Downloading ${n} file${n === 1 ? "" : "s"} again. Partial files pick up where they stopped.`);
      void refreshFooter();
    } catch (e) {
      showQueueToast(String(e));
    } finally {
      setUnifiedBusy(false);
    }
  };

  const clearLastRun = async () => {
    try {
      await invoke("deployment_clear_last_session");
      void refreshFooter();
    } catch (e) {
      showQueueToast(String(e));
    }
  };

  const queueBadgeCount =
    mergedDeployJobs.length + (queueSnap?.queueActive && backendActiveJobCount > 0 ? backendActiveJobCount : 0);

  return (
    <div className="relative w-full h-full flex flex-col">
      <div
        className="data-acquisition"
        aria-label="Downloads"
      >
        <header className="data-acquisition__head">
          <div className="data-acquisition__head-row">
            <div className="data-acquisition__head-right">
              <div className="data-acquisition__head-metrics" aria-label="Planning and disk metrics">
                <div className="data-acquisition__head-metric">
                  <span className="data-acquisition__head-metric-k">To download</span>
                  <span className="data-acquisition__head-metric-v">{fmtBytes(footerQueueBytes)}</span>
                </div>
                <div className="data-acquisition__head-metric">
                  <span className="data-acquisition__head-metric-k">Free space</span>
                  <span className="data-acquisition__head-metric-v">
                    {disk ? fmtBytes(disk.availableBytes) : "—"}
                  </span>
                </div>
              </div>
              <button
                type="button"
                className={"data-acquisition__queue-btn" + (activeTab === "queue" ? " data-acquisition__queue-btn--active" : "")}
                onClick={() => onActiveTabChange("queue")}
                aria-label="Open queue"
                title="Open queue"
              >
                Queue
                {queueBadgeCount > 0 ? <span className="data-acquisition__queue-badge">{queueBadgeCount}</span> : null}
              </button>
            </div>
          </div>
          <div className="data-acquisition__tabs" role="tablist" aria-label="Acquisition categories">
            <div className="data-acquisition__tabs-main">
              {CATEGORY_TABS.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  role="tab"
                  aria-selected={activeTab === t.id}
                  className={"data-acquisition__tab" + (activeTab === t.id ? " data-acquisition__tab--active" : "")}
                  onClick={() => onActiveTabChange(t.id)}
                >
                  {t.label}
                </button>
              ))}
            </div>
          </div>
        </header>

      <main className={"data-acquisition__body" + (activeTab === "queue" ? " data-acquisition__body--queue" : "")}>
        <div className={"data-acquisition__body-inner" + (activeTab === "queue" ? " data-acquisition__body-inner--queue" : "")}>
          {libraryError != null ? <p className="error" style={{ marginBottom: 12 }}>{libraryError}</p> : null}
          <div hidden={activeTab !== "station"} className="data-acquisition__tab-panel" role="tabpanel">
            <StationInventoryView onOpenTab={(t) => onActiveTabChange(t)} />
          </div>
          {isOnline ? (
            <div
              hidden={activeTab !== "queue"}
              className="data-acquisition__tab-panel data-acquisition__tab-panel--queue"
              role="tabpanel"
            >
              <div className="data-acquisition__queue-tab-wrap">
                <DownloadQueueView
                  queueActive={Boolean(queueSnap?.queueActive)}
                  runningJobs={queueSnap?.queueActive ? queueSnap.jobs : []}
                  planned={mergedDeployJobs.map((j) => ({ id: j.fileName, label: j.fileName, bytes: j.expectedBytes }))}
                  plannedBytes={footerQueueBytes}
                  activePct={activeDownloadPct}
                  speedMibPerSec={queueSnap?.queueActive ? measuredSpeedMibPerSec : null}
                  disk={disk}
                  planExceedsDisk={planExceedsDisk}
                  lastRun={queueSnap?.lastSession ?? []}
                  lastRunEndedAt={queueSnap?.lastSessionEndedAt ?? null}
                  busy={unifiedBusy}
                  onStart={() => void startUnifiedDownloads()}
                  onPause={() => void pauseDownloads()}
                  onClearPlanned={() => clearPlannedQueue()}
                  onRetryUnfinished={() => void retryUnfinished()}
                  onClearLastRun={() => void clearLastRun()}
                  footer={
                    <DownloadFolderSetting
                      disabled={Boolean(queueSnap?.queueActive)}
                      onChanged={() => void refreshFooter()}
                    />
                  }
                />
              </div>
            </div>
          ) : null}
          {isOnline ? (
            <div hidden={activeTab !== "packages"} className="data-acquisition__tab-panel" role="tabpanel">
              <DynamicLibraryInstaller key={`pkg-${cartClearNonce}`} deferPrimaryDownloadButton />
            </div>
          ) : null}
          {isOnline ? (
            <div hidden={activeTab !== "academy"} className="data-acquisition__tab-panel" role="tabpanel">
              <AcademyProvisioner
                key={`academy-${cartClearNonce}`}
                hubActiveTab={activeTab}
                onAcademyEstimateBytesChange={setAcademyProvisionEstimateBytes}
              />
            </div>
          ) : null}
          {isOnline ? (
            <div hidden={activeTab !== "maps"} className="data-acquisition__tab-panel" role="tabpanel">
              <ArchiveMapsTab {...mapsTabProps} />
              <MapProvisioner
                downloadedMaps={mapsTabProps.downloadedMaps}
                onProvisionEstimateBytesChange={setMapProvisionEstimateBytes}
              />
            </div>
          ) : null}
          {isOnline ? (
            <div hidden={activeTab !== "models"} className="data-acquisition__tab-panel" role="tabpanel">
              <ModelProvisioner hubActiveTab={activeTab} />
            </div>
          ) : null}
          {isOnline ? (
            <div hidden={activeTab !== "documents"} className="data-acquisition__tab-panel" role="tabpanel">
              <KnowledgeCatalogView />
            </div>
          ) : null}
        </div>

        {!isOnline && activeTab !== "station" ? (
          <div className="data-acquisition__offline" role="alertdialog" aria-modal="true" aria-label="Offline mode">
            <div className="data-acquisition__offline-card">
              <h2 className="data-acquisition__offline-title">Offline mode</h2>
              <p className="data-acquisition__offline-body">
                External Uplink Offline. Connect to WAN or use Local Mesh Sync
              </p>
              <div className="data-acquisition__offline-actions">
                <button type="button" className="data-acquisition__mesh-btn" onClick={onOpenMesh}>
                  Open P2P Mesh Sync dashboard
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </main>

      {queueToast ? (
        <div className="data-acquisition__toast" role="status">
          {queueToast}
        </div>
      ) : null}
      </div>
    </div>
  );
}
