import { create } from "zustand";

export type IngestionQueueItem = {
  id: string;
  filePath: string;
  displayName: string;
  sourcePackage: string;
  phase: string;
  pct: number;
  queuedAtMs: number;
};

export type IngestionQueueSnapshot = {
  queued: IngestionQueueItem[];
  inProgress: IngestionQueueItem[];
};

type IngestionQueueStore = IngestionQueueSnapshot & {
  setSnapshot: (snap: IngestionQueueSnapshot) => void;
  clear: () => void;
};

export const useIngestionQueueStore = create<IngestionQueueStore>((set) => ({
  queued: [],
  inProgress: [],
  setSnapshot: (snap) => set({ queued: snap.queued ?? [], inProgress: snap.inProgress ?? [] }),
  clear: () => set({ queued: [], inProgress: [] }),
}));

