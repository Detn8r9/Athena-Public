import { create } from "zustand";
import manifestJson from "../domains/features/library-installer/manifest.json";
import { normalizePathSlashes } from "../shared/lib/path";
import { isAthenaCurriculumStagedJob } from "../domains/academy/academyCurriculumQueueJobs";
import { mergeStagedDownloadJobs, type StagedDownloadJob } from "../shared/dataAcquisition/stagedDownloadJobs";

const PACKAGE_FILE_NAMES = new Set(
  (Array.isArray(manifestJson) ? manifestJson : []).map((row: { fileName: string }) =>
    normalizePathSlashes(String(row.fileName ?? "").trim()),
  ),
);

export type DownloadQueueStore = {
  /** Planned HTTP downloads (packages, academy archives, GGUF, etc.). */
  jobs: StagedDownloadJob[];
  /** Catalog GB footprint from Packages selection (includes tiers without manifest URLs). */
  packageCatalogPlannedBytes: number;
  /** When set, hub totals use this map footprint instead of the live slider estimate. */
  mapProvisionPinnedBytes: number | null;
  /** Bumped on clear so the packages installer can remount and reset toggles. */
  cartClearNonce: number;

  addToQueue: (incoming: StagedDownloadJob | StagedDownloadJob[]) => void;
  removeFromQueue: (fileNameOrUrl: string) => void;
  clearQueue: () => void;
  /** Replaces manifest-backed package jobs and updates catalog footprint; keeps other jobs. */
  syncPackagePlan: (jobs: StagedDownloadJob[], catalogPlanBytesApprox: number) => void;
  /** Replaces matrix-generated curriculum jobs; keeps packages, models, and other archives. */
  syncAcademyCurriculumPlan: (jobs: StagedDownloadJob[]) => void;
  pinMapProvisionBytes: (bytes: number) => void;
};

function normalizeKey(raw: string): string {
  const t = raw.trim();
  if (!t) return "";
  try {
    return normalizePathSlashes(t);
  } catch {
    return t;
  }
}

export const useDownloadQueueStore = create<DownloadQueueStore>((set) => ({
  jobs: [],
  packageCatalogPlannedBytes: 0,
  mapProvisionPinnedBytes: null,
  cartClearNonce: 0,

  addToQueue: (incoming) => {
    const list = (Array.isArray(incoming) ? incoming : [incoming]).filter((j) => j.url.trim() && j.fileName.trim());
    if (list.length === 0) return;
    set((s) => {
      const seenUrls = new Set(s.jobs.map((j) => j.url.trim()).filter(Boolean));
      const dedupedIncoming = list.filter((j) => {
        const u = j.url.trim();
        if (seenUrls.has(u)) return false;
        seenUrls.add(u);
        return true;
      });
      if (dedupedIncoming.length === 0) return s;
      return { jobs: mergeStagedDownloadJobs([s.jobs, dedupedIncoming]) };
    });
  },

  removeFromQueue: (fileNameOrUrl) => {
    const key = fileNameOrUrl.trim();
    if (!key) return;
    const keyNorm = normalizeKey(key);
    set((s) => ({
      jobs: s.jobs.filter((j) => {
        const fn = normalizePathSlashes(j.fileName.trim());
        const url = j.url.trim();
        return fn !== keyNorm && fn !== key && url !== key;
      }),
    }));
  },

  clearQueue: () =>
    set((s) => ({
      jobs: [],
      packageCatalogPlannedBytes: 0,
      mapProvisionPinnedBytes: null,
      cartClearNonce: s.cartClearNonce + 1,
    })),

  syncPackagePlan: (jobs, catalogPlanBytesApprox) => {
    const catalog =
      Number.isFinite(catalogPlanBytesApprox) && catalogPlanBytesApprox >= 0 ? catalogPlanBytesApprox : 0;
    set((s) => {
      const rest = s.jobs.filter((j) => !PACKAGE_FILE_NAMES.has(normalizePathSlashes(j.fileName.trim())));
      return {
        jobs: mergeStagedDownloadJobs([rest, jobs]),
        packageCatalogPlannedBytes: catalog,
      };
    });
  },

  syncAcademyCurriculumPlan: (jobs) => {
    set((s) => {
      const rest = s.jobs.filter((j) => !isAthenaCurriculumStagedJob(j));
      return { jobs: mergeStagedDownloadJobs([rest, jobs]) };
    });
  },

  pinMapProvisionBytes: (bytes) => {
    if (!Number.isFinite(bytes) || bytes <= 0) return;
    set({ mapProvisionPinnedBytes: bytes });
  },
}));

/** Manifest-backed deployment paths (tactical packages tab). */
export function isPackageManifestJob(job: StagedDownloadJob): boolean {
  return PACKAGE_FILE_NAMES.has(normalizePathSlashes(job.fileName.trim()));
}

export function isLikelyModelJob(job: StagedDownloadJob): boolean {
  const f = job.fileName.toLowerCase();
  return f.includes("models/") || f.endsWith(".gguf");
}

/** Academy / Kiwix archives: mirror URLs or staged under `kiwix_downloads/`. */
export function isLikelyAcademyKiwixJob(job: StagedDownloadJob): boolean {
  const u = job.url.toLowerCase();
  const fn = normalizePathSlashes(job.fileName.trim()).toLowerCase();
  if (fn.includes("kiwix_downloads")) return true;
  if (u.includes("kiwix.org") || u.includes("kiwix")) return true;
  return false;
}

/**
 * Split staged jobs for hub UI. Academy archives = Kiwix mirror URLs or `kiwix_downloads/` paths, plus any other
 * non-tactical / non-model payloads grouped after them for the same “Academy archives” list.
 */
export function partitionJobsForHubUi(jobs: readonly StagedDownloadJob[]) {
  const packageJobs: StagedDownloadJob[] = [];
  const modelJobs: StagedDownloadJob[] = [];
  const academyKiwix: StagedDownloadJob[] = [];
  const academyOther: StagedDownloadJob[] = [];
  for (const j of jobs) {
    if (isPackageManifestJob(j)) packageJobs.push(j);
    else if (isLikelyModelJob(j)) modelJobs.push(j);
    else if (isLikelyAcademyKiwixJob(j)) academyKiwix.push(j);
    else academyOther.push(j);
  }
  return { packageJobs, modelJobs, academyJobs: [...academyKiwix, ...academyOther] };
}
