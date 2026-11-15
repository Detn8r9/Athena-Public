/** One file in the download queue, as the station reports it (`deployment_get_queue_snapshot`). */
export type QueueJobRecord = {
  jobId: string;
  sourceUrl: string;
  displayName: string;
  /** queued, running, retrying, completed, failed, paused, interrupted, not_started */
  state: string;
  bytesDownloaded: number;
  totalBytes: number | null;
  message?: string | null;
};

export type DeploymentQueueSnapshot = {
  formatVersion: number;
  queueActive: boolean;
  cancelRequested?: boolean;
  jobs: QueueJobRecord[];
  lastSession?: QueueJobRecord[];
  lastSessionEndedAt?: number | null;
};

export type DiskReport = {
  availableBytes: number;
  totalBytes: number;
  mountPoint: string;
};

export type InventoryItem = {
  name: string;
  title: string;
  kind: "archive" | "map" | "model" | string;
  bytes: number;
  location: string;
  status: "ready" | "partial" | "unreadable" | "duplicate" | string;
  note?: string;
};

export type StationInventory = {
  destinations: Array<{ label: string; folder: string; freeBytes: number | null }>;
  items: InventoryItem[];
  indexedDocuments: number;
  libraryReaderRunning: boolean;
  /** Running but still opening its archives (minutes on a large library). */
  libraryReaderStarting?: boolean;
};

/** "library/medical/wikipedia_en_medicine_maxi_2026-04.zim" → "wikipedia_en_medicine_maxi_2026-04.zim" */
export function fileBaseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** Plain words for a queue state. */
export function jobStateLabel(state: string): string {
  switch (state) {
    case "running":
      return "Downloading";
    case "retrying":
      return "Retrying";
    case "completed":
      return "Done";
    case "failed":
      return "Failed";
    case "paused":
      return "Paused";
    case "interrupted":
      return "Interrupted";
    case "not_started":
      return "Not started";
    default:
      return "Waiting";
  }
}

export type JobTone = "ok" | "busy" | "warn" | "bad" | "idle";

export function jobStateTone(state: string): JobTone {
  switch (state) {
    case "completed":
      return "ok";
    case "running":
      return "busy";
    case "retrying":
    case "paused":
    case "interrupted":
    case "not_started":
      return "warn";
    case "failed":
      return "bad";
    default:
      return "idle";
  }
}

export type LastRunSummary = { done: number; failed: number; unfinished: number; total: number };

export function summarizeRun(rows: QueueJobRecord[]): LastRunSummary {
  let done = 0;
  let failed = 0;
  let unfinished = 0;
  for (const r of rows) {
    if (r.state === "completed") done++;
    else if (r.state === "failed") failed++;
    else unfinished++;
  }
  return { done, failed, unfinished, total: rows.length };
}
