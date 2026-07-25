import { useCallback, useEffect, useRef, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import type { StagedDownloadJob } from "../../shared/dataAcquisition/stagedDownloadJobs";
import { normalizePathSlashes } from "../../shared/lib/path";
import { useDownloadQueueStore } from "../../stores/downloadQueueStore";
import { formatBytesBinary } from "../../shared/format/formatBytesBinary";
import { MODEL_FAMILY_CATALOG, ramBadgeForMinGb, type ModelTierOption, type RamBadgeClass } from "./modelCatalog";
import "./ModelProvisioner.css";

function tierBandClass(band: RamBadgeClass): string {
  if (band === "edge") return "model-provisioner__tier-box--edge";
  if (band === "workstation") return "model-provisioner__tier-box--workstation";
  return "model-provisioner__tier-box--command";
}

function tierToJob(tier: ModelTierOption): StagedDownloadJob {
  return {
    url: tier.url,
    fileName: tier.fileName,
    expectedBytes: tier.fileSizeBytes,
    alternateUrls: tier.alternateUrls,
  };
}

function ggufBasenameLower(fileName: string): string {
  const n = normalizePathSlashes(fileName.trim());
  const i = n.lastIndexOf("/");
  const base = i >= 0 ? n.slice(i + 1) : n;
  return base.toLowerCase();
}

export interface ModelProvisionerProps {
  hubActiveTab: "queue" | "packages" | "academy" | "maps" | "models" | "documents";
}

export function ModelProvisioner({ hubActiveTab }: ModelProvisionerProps) {
  const addToQueue = useDownloadQueueStore((s) => s.addToQueue);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimerRef = useRef<number | null>(null);
  const [downloadedBasenamesLower, setDownloadedBasenamesLower] = useState<ReadonlySet<string>>(() => new Set());

  useEffect(() => {
    if (hubActiveTab !== "models") return;
    let cancelled = false;
    void invoke<string[]>("get_downloaded_models")
      .then((rows) => {
        if (cancelled) return;
        const s = new Set<string>();
        for (const r of Array.isArray(rows) ? rows : []) {
          const t = String(r).trim().toLowerCase();
          if (t) s.add(t);
        }
        setDownloadedBasenamesLower(s);
      })
      .catch(() => {
        if (!cancelled) setDownloadedBasenamesLower(new Set());
      });
    return () => {
      cancelled = true;
    };
  }, [hubActiveTab]);

  const showToast = useCallback((message: string) => {
    if (toastTimerRef.current) window.clearTimeout(toastTimerRef.current);
    setToast(message);
    toastTimerRef.current = window.setTimeout(() => {
      setToast(null);
      toastTimerRef.current = null;
    }, 3200);
  }, []);

  const tierOnDisk = useCallback(
    (tier: ModelTierOption) => downloadedBasenamesLower.has(ggufBasenameLower(tier.fileName)),
    [downloadedBasenamesLower],
  );

  const queueTier = useCallback(
    (familyId: string, index: number) => {
      const family = MODEL_FAMILY_CATALOG.find((f) => f.id === familyId);
      const tier = family?.tiers[index];
      if (!tier) return;
      if (tierOnDisk(tier)) return;
      addToQueue([tierToJob(tier)]);
      showToast(`Queued ${tier.label} for models/chat.`);
    },
    [addToQueue, showToast, tierOnDisk],
  );

  const queueAllTiersForFamily = useCallback(
    (familyId: string) => {
      const family = MODEL_FAMILY_CATALOG.find((f) => f.id === familyId);
      if (!family?.tiers.length) return;
      const fresh = family.tiers.filter((t) => !tierOnDisk(t));
      if (fresh.length === 0) {
        showToast("Every tier for this family is already in models/chat.");
        return;
      }
      addToQueue(fresh.map(tierToJob));
      showToast(`Queued ${fresh.length} GGUF file${fresh.length === 1 ? "" : "s"} for models/chat.`);
    },
    [addToQueue, showToast, tierOnDisk],
  );

  return (
    <section className="model-provisioner" aria-label="AI model GGUF catalog">
      {toast ? (
        <p className="model-provisioner__toast" role="status">
          {toast}
        </p>
      ) : null}
      <p className="model-provisioner__intro">
        <strong>Click a tier</strong> to add it to the download queue. Files use raw HTTP into{" "}
        <code>models/chat/</code> (same machinery as tactical packages). Tiers that already exist on disk are grayed
        out and cannot be queued again.
      </p>

      <div className="model-provisioner__grid">
        {MODEL_FAMILY_CATALOG.map((family) => {
          const tiersAllOnDisk = family.tiers.length > 0 && family.tiers.every((t) => tierOnDisk(t));
          return (
            <article key={family.id} className="model-provisioner__card">
              <div className="model-provisioner__card-head">
                <h3 className="model-provisioner__title">
                  {family.name} — {family.categoryLabel}
                </h3>
                <p className="model-provisioner__category">{family.tiers.length} hardware tiers</p>
              </div>
              <p className="model-provisioner__strengths">{family.strengths}</p>

              <div className="model-provisioner__tier-toolbar">
                <span className="model-provisioner__tier-toolbar-label text-muted small">Tiers</span>
                <button
                  type="button"
                  className="model-provisioner__select-all-btn"
                  onClick={() => queueAllTiersForFamily(family.id)}
                  disabled={family.tiers.length === 0 || tiersAllOnDisk}
                >
                  Queue all tiers
                </button>
              </div>

              <div className="model-provisioner__tier-grid" role="group" aria-label={`${family.name} tier options`}>
                {family.tiers.map((tier, index) => {
                  const badge = ramBadgeForMinGb(tier.minRamGb);
                  const installed = tierOnDisk(tier);
                  return (
                    <button
                      key={tier.id}
                      type="button"
                      className={
                        "model-provisioner__tier-box " +
                        tierBandClass(badge.band) +
                        (installed ? " model-provisioner__tier-box--installed" : "")
                      }
                      title={installed ? "Already in models/chat" : "Add to download queue"}
                      onClick={() => queueTier(family.id, index)}
                      disabled={installed}
                    >
                      <span className="model-provisioner__tier-label">{tier.label}</span>
                      <span className="model-provisioner__tier-size">{formatBytesBinary(tier.fileSizeBytes)}</span>
                      <span className="model-provisioner__tier-ram">
                        <span className="model-provisioner__tier-ram-k">{badge.shortLabel}</span>
                        <span className="model-provisioner__tier-ram-v">≥{tier.minRamGb} GB RAM</span>
                      </span>
                      {installed ? <span className="model-provisioner__tier-installed-badge">Downloaded</span> : null}
                    </button>
                  );
                })}
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
