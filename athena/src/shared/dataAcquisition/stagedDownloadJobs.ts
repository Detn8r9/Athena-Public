export type StagedDownloadJob = {
  url: string;
  fileName: string;
  expectedBytes: number;
  /** Same-file mirrors / fallbacks for resolvers in the download worker. */
  alternateUrls?: string[];
  /** Source package label for Library + ingestion categorization (e.g. iFixit, Wikipedia, Custom Uploads). */
  sourcePackage?: string;
};

function mergeAlternateUrls(a?: string[], b?: string[]): string[] | undefined {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const list of [a, b]) {
    if (!list) continue;
    for (const raw of list) {
      const u = raw.trim();
      if (!u || seen.has(u)) continue;
      seen.add(u);
      out.push(u);
    }
  }
  return out.length ? out : undefined;
}

/** Merge jobs from multiple tabs; later entries win on duplicate file names. */
export function mergeStagedDownloadJobs(lists: StagedDownloadJob[][]): StagedDownloadJob[] {
  const byFile = new Map<string, StagedDownloadJob>();
  for (const list of lists) {
    for (const j of list) {
      const key = j.fileName.trim();
      if (!key || !j.url.trim()) continue;
      const prev = byFile.get(key);
      if (!prev) {
        byFile.set(key, { ...j });
        continue;
      }
      if (j.expectedBytes < prev.expectedBytes) continue;
      byFile.set(key, {
        ...j,
        alternateUrls: mergeAlternateUrls(prev.alternateUrls, j.alternateUrls),
        sourcePackage: j.sourcePackage ?? prev.sourcePackage,
      });
    }
  }
  return [...byFile.values()];
}
