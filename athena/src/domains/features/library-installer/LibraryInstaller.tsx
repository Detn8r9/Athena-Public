import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../../shared/ipc/athenaIpc";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useLexicon } from "../../../shared/lexicon";
import { useTauriListen } from "../../../shared/hooks/useTauriListen";
import { joinPath, normalizePathSlashes } from "../../../shared/lib/path";

type LibraryManifestFile = {
  name: string;
  category: string;
  url: string;
  destination: string;
  size_bytes?: number | null;
  academy_age_group_id?: string | null;
  academy_subject_id?: string | null;
};

type LibraryManifest = {
  tiers: Array<{
    name: string;
    description: string;
    display_total_label?: string | null;
    files: LibraryManifestFile[];
  }>;
};

type AcademyCurriculumSubject = { id: string; label: string; lessons: unknown[] };
type AcademyCurriculumAge = { id: string; label: string; subjects: AcademyCurriculumSubject[] };
type AcademyCurriculumRoot = { ageGroups: AcademyCurriculumAge[] };

type LocalArchiveTabId = "all" | "packages" | "documents" | "maps" | "academy";
type LocalSortKey = "name" | "size" | "path";

type DownloadProgressPayload = {
  manifestType: string;
  downloadId: string;
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond?: number | null;
};

type DownloadCompletePayload = { manifestType: string; downloadId: string; path: string };
type DownloadErrorPayload = { manifestType: string; downloadId: string; error: string };

type LibraryInstallerLocation = {
  query: string;
  display_name: string;
  lat: number;
  lon: number;
  state_code: string;
};

type ArchiveWatchRootRow = { id: number; path: string; label: string | null; sort_index: number };
type LocalLibraryFile = { path: string; name: string; size_bytes: number; exists_on_disk: boolean };

function clampPct(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(100, n));
}

export type LibraryManifestRow = LibraryManifestFile & { tierName: string };

function formatDownloadSizeGb(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = bytes / 1024 ** 2;
  if (mb >= 1) return `${mb.toFixed(0)} MB`;
  // Notes and short documents were all shown as "0 MB".
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function formatThroughput(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec < 0) return "—";
  if (bytesPerSec >= 1024 ** 2) return `${(bytesPerSec / 1024 ** 2).toFixed(1)} MB/s`;
  return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
}

function sumTierBytes(files: LibraryManifest["tiers"][0]["files"]): { total: number; knownCount: number } {
  let total = 0;
  let knownCount = 0;
  for (const f of files) {
    const n = f.size_bytes;
    if (n != null && typeof n === "number" && Number.isFinite(n) && n > 0) {
      total += n;
      knownCount += 1;
    }
  }
  return { total, knownCount };
}

function categorizeLocalFile(path: string, name: string): Exclude<LocalArchiveTabId, "all"> {
  const p = normalizePathSlashes(path).toLowerCase();
  const n = name.toLowerCase();
  if (p.includes("/academy/")) return "academy";
  if (p.includes("/maps/") || n.endsWith(".pmtiles") || n.endsWith(".mbtiles")) return "maps";
  if (n.endsWith(".zim")) {
    if (n.startsWith("maps_")) return "maps";
    return "documents";
  }
  if (p.includes("/literature/")) return "documents";
  return "packages";
}

function compareLocalFiles(a: LocalLibraryFile, b: LocalLibraryFile, key: LocalSortKey, dir: 1 | -1): number {
  if (key === "size") {
    const d = a.size_bytes - b.size_bytes;
    return d === 0 ? a.path.localeCompare(b.path) * dir : (d > 0 ? 1 : -1) * dir;
  }
  if (key === "name") {
    const c = a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
    return c === 0 ? a.path.localeCompare(b.path) * dir : c * dir;
  }
  return a.path.localeCompare(b.path) * dir;
}

function fileNameFromUrl(url: string): string {
  try {
    const u = new URL(url);
    const seg = u.pathname.split("/").filter(Boolean).pop();
    return seg && seg.trim() ? seg.trim() : "download.bin";
  } catch {
    const seg = url.split("/").filter(Boolean).pop();
    return seg && seg.trim() ? seg.trim() : "download.bin";
  }
}

function manifestFileOnDisk(f: { url: string }, localBasenames: Set<string>): boolean {
  const base = fileNameFromUrl(f.url).toLowerCase();
  return localBasenames.has(base);
}

function ThinBar({ percent }: { percent: number }) {
  const clamped = clampPct(percent);
  return (
    <div className="glass-panel" style={{ height: "6px", padding: 0, overflow: "hidden", borderRadius: "999px" }} aria-hidden>
      <div
        style={{
          width: `${clamped}%`,
          height: "100%",
          background: "color-mix(in srgb, var(--primary) 70%, var(--tertiary))",
          transition: "width 180ms ease-out",
        }}
      />
    </div>
  );
}

function FileInstallStatusPill({ onDisk, lx }: { onDisk: boolean; lx: ReturnType<typeof useLexicon> }) {
  return (
    <span
      style={{
        fontSize: "0.72rem",
        fontWeight: 650,
        padding: "0.12rem 0.45rem",
        borderRadius: "999px",
        border: "1px solid color-mix(in srgb, var(--border) 55%, transparent)",
        background: onDisk ? "color-mix(in srgb, var(--accent, #4f8cff) 14%, transparent)" : "transparent",
        color: onDisk ? "var(--text)" : "var(--text-muted)",
        flexShrink: 0,
      }}
    >
      {onDisk ? lx.zimOnDeviceBadge : lx.zimNotDownloadedBadge}
    </span>
  );
}

export function ArchiveLocalPanel() {
  const lx = useLexicon();
  const [watchRoots, setWatchRoots] = useState<ArchiveWatchRootRow[]>([]);
  const [localFiles, setLocalFiles] = useState<LocalLibraryFile[]>([]);
  const [localErr, setLocalErr] = useState<string | null>(null);
  const [localPanelErr, setLocalPanelErr] = useState<string | null>(null);
  const [dataSourcesOpen, setDataSourcesOpen] = useState(false);
  const [pathDraft, setPathDraft] = useState("");
  const [addBusy, setAddBusy] = useState(false);
  const [localTab, setLocalTab] = useState<LocalArchiveTabId>("all");
  const [localSortKey, setLocalSortKey] = useState<LocalSortKey>("name");
  const [localSortDesc, setLocalSortDesc] = useState(false);
  const [localSearch, setLocalSearch] = useState("");

  const refreshRoots = useCallback(async () => {
    try {
      const rows = await invoke<ArchiveWatchRootRow[]>("list_archive_watch_roots");
      setWatchRoots(Array.isArray(rows) ? rows : []);
    } catch {
      setWatchRoots([]);
    }
  }, []);

  const refreshLocal = useCallback(async () => {
    try {
      const list = await invoke<LocalLibraryFile[]>("list_library_contents");
      setLocalFiles(Array.isArray(list) ? list : []);
      setLocalErr(null);
    } catch (e) {
      setLocalErr(e != null ? String(e) : "Could not list files.");
    }
  }, []);

  useEffect(() => {
    void refreshRoots();
    void refreshLocal();
  }, [refreshRoots, refreshLocal]);

  useTauriListen("athena:archive-fs-changed", {
    handler: () => {
      void refreshLocal();
    },
  });

  async function handlePickFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected == null || Array.isArray(selected)) return;
    setAddBusy(true);
    setLocalPanelErr(null);
    try {
      await invoke("add_archive_watch_root", { path: selected, label: null });
      await refreshRoots();
      void refreshLocal();
      setPathDraft("");
    } catch (e) {
      setLocalPanelErr(e != null ? String(e) : "Could not add folder.");
    } finally {
      setAddBusy(false);
    }
  }

  async function handleAddPathText() {
    const t = pathDraft.trim();
    if (!t) return;
    setAddBusy(true);
    setLocalPanelErr(null);
    try {
      await invoke("add_archive_watch_root", { path: t, label: null });
      await refreshRoots();
      void refreshLocal();
      setPathDraft("");
    } catch (e) {
      setLocalPanelErr(e != null ? String(e) : "Could not add path.");
    } finally {
      setAddBusy(false);
    }
  }

  async function handleRemoveRoot(id: number) {
    setLocalPanelErr(null);
    try {
      await invoke("remove_archive_watch_root", { id });
      await refreshRoots();
      void refreshLocal();
    } catch (e) {
      setLocalPanelErr(e != null ? String(e) : "Could not remove data source.");
    }
  }

  const filteredSortedFiles = useMemo(() => {
    const q = localSearch.trim().toLowerCase();
    const dir: 1 | -1 = localSortDesc ? -1 : 1;
    const list = localFiles.filter((f) => {
      if (localTab !== "all" && categorizeLocalFile(f.path, f.name) !== localTab) return false;
      if (!q) return true;
      return f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q);
    });
    return [...list].sort((a, b) => compareLocalFiles(a, b, localSortKey, dir));
  }, [localFiles, localTab, localSearch, localSortKey, localSortDesc]);

  return (
    <div className="glass-panel app-view-placeholder library-installer-view" style={{ display: "grid", gap: "0.85rem", padding: "1rem", position: "relative" }}>
      {dataSourcesOpen ? (
        <div style={{ position: "fixed", inset: 0, zIndex: 11950, background: "color-mix(in srgb, var(--bg) 50%, black)", display: "flex", alignItems: "center", justifyContent: "center", padding: "1rem" }} role="presentation">
          <div className="glass-panel" role="dialog" aria-modal="true" aria-labelledby="archive-ds-title" style={{ maxWidth: "32rem", width: "100%", padding: "1.1rem", display: "grid", gap: "0.75rem" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: "0.5rem", flexWrap: "wrap" }}>
              <h3 id="archive-ds-title" style={{ margin: 0, fontSize: "1.05rem", fontWeight: 750 }}>
                {lx.dataSources}
              </h3>
              <button type="button" className="secondary" onClick={() => setDataSourcesOpen(false)}>
                {lx.closeModal}
              </button>
            </div>
            <p className="text-muted small" style={{ margin: 0, lineHeight: 1.45 }}>
              {lx.noDataSourcesHint}
            </p>
            <button type="button" className="glass-action-btn" disabled={addBusy} onClick={() => void handlePickFolder()}>
              {lx.addDataSource}
            </button>
            <label className="text-muted small" style={{ display: "grid", gap: "0.25rem" }}>
              {lx.pathPlaceholder}
              <input
                className="sidebar-search"
                value={pathDraft}
                onChange={(e) => setPathDraft(e.target.value)}
                disabled={addBusy}
                placeholder={lx.pathPlaceholder}
                aria-label={lx.pathPlaceholder}
              />
            </label>
            <button type="button" className="secondary" disabled={addBusy || !pathDraft.trim()} onClick={() => void handleAddPathText()}>
              {lx.addDataSource}
            </button>
            <ul style={{ margin: 0, paddingLeft: "1rem", display: "grid", gap: "0.35rem", listStyle: "disc" }}>
              {watchRoots.map((r) => (
                <li key={r.id} className="text-muted small" style={{ wordBreak: "break-word" }}>
                  <span style={{ color: "var(--text)" }}>{r.path}</span>
                  <button
                    type="button"
                    className="secondary"
                    style={{ marginLeft: "0.5rem" }}
                    onClick={() => void handleRemoveRoot(r.id)}
                    aria-label={`${lx.removeDataSource} ${r.path}`}
                  >
                    {lx.removeDataSource}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}

      <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
        <div>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>
            {lx.archiveLocalTitle}
          </h2>
          <p className="text-muted" style={{ margin: "0.25rem 0 0", maxWidth: "42rem" }}>
            {lx.archiveLocalSubtitle}
          </p>
        </div>
        <button type="button" className="glass-action-btn" onClick={() => setDataSourcesOpen(true)} aria-label={lx.dataSources}>
          {lx.dataSources}
        </button>
      </div>

      {localPanelErr ? (
        <p className="error" role="alert" style={{ margin: 0 }}>
          {localPanelErr}
        </p>
      ) : null}

      <div style={{ display: "grid", gap: "0.65rem" }}>
        <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.35rem" }}>
          <div style={{ fontWeight: 650 }}>{lx.watchFoldersLabel}</div>
          {watchRoots.length === 0 ? (
            <div className="text-muted small">{lx.noDataSourcesHint}</div>
          ) : (
            <ul style={{ margin: 0, paddingLeft: "1.1rem" }}>
              {watchRoots.map((r) => (
                <li key={r.id} className="text-muted small" style={{ wordBreak: "break-all" }}>
                  {r.path}
                </li>
              ))}
            </ul>
          )}
        </div>
        {localErr ? (
          <p className="error" role="alert" style={{ margin: 0 }}>
            {localErr}
          </p>
        ) : null}

        <div className="library-tablist" role="tablist" aria-label={lx.archiveLocalTablistAria} style={{ flexShrink: 0 }}>
          {(
            [
              ["all", lx.tabArchiveAll],
              ["packages", lx.tabArchivePackages],
              ["documents", lx.tabArchiveDocuments],
              ["maps", lx.tabArchiveMaps],
              ["academy", lx.tabArchiveAcademy],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="tab"
              id={`archive-local-sub-${id}`}
              aria-selected={localTab === id}
              aria-controls={`archive-local-panel-${id}`}
              className={"library-tab" + (localTab === id ? " library-tab-active" : "")}
              onClick={() => setLocalTab(id)}
            >
              {label}
            </button>
          ))}
        </div>

        <div
          id={`archive-local-panel-${localTab}`}
          role="tabpanel"
          aria-labelledby={`archive-local-sub-${localTab}`}
          style={{ display: "grid", gap: "0.55rem" }}
        >
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
            <label className="text-muted small" style={{ display: "flex", alignItems: "center", gap: "0.35rem", margin: 0 }}>
              <span>{lx.archiveSortLabel}</span>
              <select
                className="settings-startup-view-select"
                value={localSortKey}
                onChange={(e) => setLocalSortKey(e.target.value as LocalSortKey)}
                aria-label={lx.archiveSortLabel}
              >
                <option value="name">{lx.archiveSortName}</option>
                <option value="size">{lx.archiveSortSize}</option>
                <option value="path">{lx.archiveSortPath}</option>
              </select>
            </label>
            <button type="button" className="secondary" onClick={() => setLocalSortDesc((d) => !d)} aria-label={localSortDesc ? lx.archiveSortDirDesc : lx.archiveSortDirAsc}>
              {localSortDesc ? lx.archiveSortDirDesc : lx.archiveSortDirAsc}
            </button>
            <input
              type="search"
              className="sidebar-search"
              value={localSearch}
              onChange={(e) => setLocalSearch(e.target.value)}
              placeholder={lx.archiveSearchFilesPlaceholder}
              aria-label={lx.archiveSearchFilesPlaceholder}
              style={{ flex: "1 1 12rem", minWidth: "10rem" }}
            />
          </div>

          {localFiles.length === 0 ? (
            <div className="glass-panel" style={{ padding: "1rem" }}>
              <p className="text-muted" style={{ margin: 0 }}>
                {lx.emptyLocalArchive}
              </p>
            </div>
          ) : filteredSortedFiles.length === 0 ? (
            <div className="glass-panel" style={{ padding: "1rem" }}>
              <p className="text-muted" style={{ margin: 0 }}>
                {lx.archiveLocalNoMatches}
              </p>
            </div>
          ) : (
            <ul style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: "0.45rem" }}>
              {filteredSortedFiles.map((f) => (
                <li
                  key={f.path}
                  className="glass-panel"
                  style={{
                    padding: "0.65rem 0.75rem",
                    display: "flex",
                    flexWrap: "wrap",
                    gap: "0.5rem",
                    alignItems: "center",
                    justifyContent: "space-between",
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontWeight: 650, wordBreak: "break-word" }}>{f.name}</div>
                    <div className="text-muted small" style={{ wordBreak: "break-all" }}>
                      {f.path}
                    </div>
                    {!f.exists_on_disk ? (
                      <div className="text-muted small" role="status">
                        {lx.fileMissingOnDisk}
                      </div>
                    ) : null}
                  </div>
                  <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", alignItems: "center" }}>
                    <span className="text-muted small">{formatDownloadSizeGb(f.size_bytes)}</span>
                    {f.exists_on_disk ? (
                      <button type="button" className="secondary" onClick={() => void revealItemInDir(f.path).catch(() => {})}>
                        {lx.openInFinder}
                      </button>
                    ) : null}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

function useLibraryNetworkInstaller() {
  const lx = useLexicon();
  const [manifest, setManifest] = useState<LibraryManifest | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [watchRoots, setWatchRoots] = useState<ArchiveWatchRootRow[]>([]);

  const [installerLocation, setInstallerLocation] = useState<LibraryInstallerLocation | null>(null);
  const [showLocationModal, setShowLocationModal] = useState(false);
  const [locationQuery, setLocationQuery] = useState("New York, NY");
  const [locationBusy, setLocationBusy] = useState(false);
  const [locationModalErr, setLocationModalErr] = useState<string | null>(null);

  const [installingTier, setInstallingTier] = useState<string | null>(null);
  const [activeDownloadId, setActiveDownloadId] = useState<string | null>(null);
  const [progressById, setProgressById] = useState<Record<string, DownloadProgressPayload>>({});
  const [installErr, setInstallErr] = useState<string | null>(null);
  const [installNote, setInstallNote] = useState<string | null>(null);
  const [singleBusyId, setSingleBusyId] = useState<string | null>(null);
  const [academyCurriculum, setAcademyCurriculum] = useState<AcademyCurriculumRoot | null>(null);
  const [localLibraryFiles, setLocalLibraryFiles] = useState<LocalLibraryFile[]>([]);

  const completionRef = useRef<Map<string, { ok: true } | { ok: false; message?: string } | { cancelled: true }>>(new Map());

  const primaryRootPath = watchRoots[0]?.path?.trim() ?? "";

  const refreshRoots = useCallback(async () => {
    try {
      const rows = await invoke<ArchiveWatchRootRow[]>("list_archive_watch_roots");
      setWatchRoots(Array.isArray(rows) ? rows : []);
    } catch {
      setWatchRoots([]);
    }
  }, []);

  const refreshLocal = useCallback(async () => {
    try {
      const list = await invoke<LocalLibraryFile[]>("list_library_contents");
      setLocalLibraryFiles(Array.isArray(list) ? list : []);
    } catch {
      setLocalLibraryFiles([]);
    }
  }, []);

  const refreshInstallerData = useCallback(async () => {
    try {
      const [m, loc] = await Promise.all([
        invoke<LibraryManifest>("get_library_manifest"),
        invoke<LibraryInstallerLocation | null>("get_library_installer_location"),
      ]);
      setManifest(m);
      setInstallerLocation(loc);
      setLoadErr(null);
      if (!loc) setShowLocationModal(true);
    } catch (e) {
      setLoadErr(e != null ? String(e) : "Could not load manifest.");
    }
  }, []);

  useEffect(() => {
    void refreshInstallerData();
    void refreshRoots();
    void refreshLocal();
  }, [refreshInstallerData, refreshRoots, refreshLocal]);

  useEffect(() => {
    void invoke<AcademyCurriculumRoot>("academy_get_curriculum")
      .then((c) => setAcademyCurriculum(c))
      .catch(() => setAcademyCurriculum(null));
  }, []);

  useTauriListen("athena:archive-fs-changed", {
    handler: () => {
      void refreshLocal();
    },
  });

  useEffect(() => {
    let a: (() => void) | null = null;
    let b: (() => void) | null = null;
    let c: (() => void) | null = null;
    let d: (() => void) | null = null;
    void (async () => {
      a = await listen<DownloadProgressPayload>("package_download_progress", (ev) => {
        const p = ev.payload;
        if (!p || p.manifestType !== "library" || typeof p.downloadId !== "string") return;
        setProgressById((prev) => ({ ...prev, [p.downloadId]: p }));
      });
      b = await listen<DownloadCompletePayload>("package_download_complete", (ev) => {
        const p = ev.payload;
        if (!p || p.manifestType !== "library" || typeof p.downloadId !== "string") return;
        completionRef.current.set(p.downloadId, { ok: true });
        setInstallNote(`Installed: ${p.path}`);
        void refreshLocal();
      });
      c = await listen<DownloadErrorPayload>("package_download_error", (ev) => {
        const p = ev.payload;
        if (!p || p.manifestType !== "library" || typeof p.downloadId !== "string") return;
        completionRef.current.set(p.downloadId, { ok: false, message: String(p.error ?? "Download failed.") });
      });
      d = await listen<{ manifestType: string; downloadId: string }>("package_download_cancelled", (ev) => {
        const p = ev.payload;
        if (!p || p.manifestType !== "library" || typeof p.downloadId !== "string") return;
        completionRef.current.set(p.downloadId, { cancelled: true });
      });
    })().catch(() => {});
    return () => {
      if (a) a();
      if (b) b();
      if (c) c();
      if (d) d();
    };
  }, [refreshLocal]);

  const tiers = useMemo(() => manifest?.tiers ?? [], [manifest]);

  const alacarteFiles = useMemo(() => {
    const map = new Map<string, LibraryManifestRow>();
    for (const t of tiers) {
      for (const f of t.files) {
        if (f.category === "Academy") continue;
        const key = `${f.url}::${f.destination}`;
        if (!map.has(key)) map.set(key, { ...f, tierName: t.name });
      }
    }
    return Array.from(map.values()).sort((a, b) => {
      const c = a.category.localeCompare(b.category);
      return c !== 0 ? c : a.name.localeCompare(b.name);
    });
  }, [tiers]);

  const academyDownloadRows = useMemo(() => {
    const map = new Map<string, LibraryManifestRow>();
    for (const t of tiers) {
      for (const f of t.files) {
        if (f.category !== "Academy") continue;
        const key = `${f.url}::${f.destination}`;
        if (!map.has(key)) map.set(key, { ...f, tierName: t.name });
      }
    }
    return Array.from(map.values());
  }, [tiers]);

  const orphanAcademyRows = useMemo(() => {
    if (!academyCurriculum) return academyDownloadRows;
    return academyDownloadRows.filter((f) => {
      const ag = (f.academy_age_group_id ?? "").trim();
      const sj = (f.academy_subject_id ?? "").trim();
      if (!ag || !sj) return true;
      for (const age of academyCurriculum.ageGroups) {
        if (age.id !== ag) continue;
        for (const sub of age.subjects) {
          if (sub.id === sj) return false;
        }
      }
      return true;
    });
  }, [academyDownloadRows, academyCurriculum]);

  const localFileBasenames = useMemo(() => {
    const s = new Set<string>();
    for (const f of localLibraryFiles) {
      s.add(f.name.trim().toLowerCase());
    }
    return s;
  }, [localLibraryFiles]);

  async function waitForDownloadResult(
    downloadId: string,
    timeoutMs: number,
  ): Promise<{ ok: true } | { ok: false; message?: string } | { cancelled: true }> {
    const start = Date.now();
    for (;;) {
      const status = completionRef.current.get(downloadId);
      if (status) return status;
      if (Date.now() - start > timeoutMs) return { ok: false, message: "Timed out waiting for download to finish." };
      await new Promise((r) => setTimeout(r, 120));
    }
  }

  async function installTier(tierName: string) {
    if (!manifest) return;
    if (!primaryRootPath) {
      setInstallErr(lx.noDataSourcesHint);
      return;
    }
    const tier = manifest.tiers.find((t) => t.name === tierName);
    if (!tier) return;
    setInstallErr(null);
    setInstallNote(null);
    setInstallingTier(tier.name);
    completionRef.current.clear();
    try {
      let cancelled = false;
      for (const f of tier.files) {
        const fileName = fileNameFromUrl(f.url);
        const destFolder = joinPath(primaryRootPath, f.destination);
        const downloadId = `${tier.name}::${fileName}`;
        setActiveDownloadId(downloadId);
        await invoke("download_package", {
          args: {
            manifestType: "library",
            downloadUrl: f.url,
            destinationFolder: destFolder,
            downloadId,
            fileName,
          },
        });
        const result = await waitForDownloadResult(downloadId, 60 * 60 * 1000);
        completionRef.current.delete(downloadId);
        if ("cancelled" in result && result.cancelled) {
          cancelled = true;
          break;
        }
        if ("ok" in result && result.ok === false) throw new Error(result.message ?? "Download failed.");
      }
      setInstallNote(cancelled ? "Install cancelled." : `Tier installed: ${tier.name}`);
    } catch (e) {
      setInstallErr(e != null ? String(e) : "Install failed.");
    } finally {
      setActiveDownloadId(null);
      setInstallingTier(null);
    }
  }

  async function installSingleFile(entry: LibraryManifestRow) {
    if (!primaryRootPath) {
      setInstallErr(lx.noDataSourcesHint);
      return;
    }
    const fileName = fileNameFromUrl(entry.url);
    const downloadId = `single::${encodeURIComponent(entry.url)}::${entry.destination}`;
    setSingleBusyId(downloadId);
    setInstallErr(null);
    completionRef.current.delete(downloadId);
    try {
      await invoke("download_package", {
        args: {
          manifestType: "library",
          downloadUrl: entry.url,
          destinationFolder: joinPath(primaryRootPath, entry.destination),
          downloadId,
          fileName,
        },
      });
      const result = await waitForDownloadResult(downloadId, 60 * 60 * 1000);
      completionRef.current.delete(downloadId);
      if ("cancelled" in result && result.cancelled) {
        setInstallNote("Download cancelled.");
        return;
      }
      if ("ok" in result && result.ok === false) throw new Error(result.message ?? "Download failed.");
      setInstallNote(`Downloaded: ${entry.name}`);
    } catch (e) {
      setInstallErr(e != null ? String(e) : "Download failed.");
    } finally {
      setSingleBusyId(null);
    }
  }

  async function saveLocationFromGeocode() {
    setLocationBusy(true);
    setLocationModalErr(null);
    try {
      await invoke("save_library_installer_location_geocode", { query: locationQuery.trim() });
      setShowLocationModal(false);
      await refreshInstallerData();
    } catch (e) {
      setLocationModalErr(e != null ? String(e) : "Could not look up that place.");
    } finally {
      setLocationBusy(false);
    }
  }

  async function saveLocationNycDefault() {
    setLocationBusy(true);
    setLocationModalErr(null);
    try {
      await invoke("save_library_installer_location_nyc_default");
      setShowLocationModal(false);
      await refreshInstallerData();
    } catch (e) {
      setLocationModalErr(e != null ? String(e) : "Could not save default.");
    } finally {
      setLocationBusy(false);
    }
  }

  function openLocationModal() {
    setLocationModalErr(null);
    if (installerLocation?.query) setLocationQuery(installerLocation.query);
    setShowLocationModal(true);
  }

  return {
    lx,
    manifest,
    loadErr,
    watchRoots,
    primaryRootPath,
    installerLocation,
    showLocationModal,
    setShowLocationModal,
    locationQuery,
    setLocationQuery,
    locationBusy,
    locationModalErr,
    installingTier,
    activeDownloadId,
    progressById,
    installErr,
    installNote,
    singleBusyId,
    academyCurriculum,
    tiers,
    alacarteFiles,
    academyDownloadRows,
    orphanAcademyRows,
    localFileBasenames,
    installTier,
    installSingleFile,
    saveLocationFromGeocode,
    saveLocationNycDefault,
    openLocationModal,
  };
}

type ArchiveNetworkInstallerShellMode = "packages" | "academy";

function ArchiveNetworkInstallerShell({
  mode,
  v,
  children,
}: {
  mode: ArchiveNetworkInstallerShellMode;
  v: ReturnType<typeof useLibraryNetworkInstaller>;
  children: ReactNode;
}) {
  const {
    lx,
    loadErr,
    primaryRootPath,
    installerLocation,
    showLocationModal,
    setShowLocationModal,
    locationQuery,
    setLocationQuery,
    locationBusy,
    locationModalErr,
    installingTier,
    saveLocationFromGeocode,
    saveLocationNycDefault,
    openLocationModal,
    installErr,
    installNote,
  } = v;

  return (
    <div className="glass-panel app-view-placeholder library-installer-view" style={{ display: "grid", gap: "0.85rem", padding: "1rem", position: "relative" }}>
      {showLocationModal ? (
        <div style={{ position: "fixed", inset: 0, zIndex: 12000, background: "color-mix(in srgb, var(--bg) 55%, black)", display: "flex", alignItems: "center", justifyContent: "center", padding: "1rem" }} role="presentation">
          <div className="glass-panel" role="dialog" aria-modal="true" aria-labelledby="archive-loc-title" style={{ maxWidth: "26rem", width: "100%", padding: "1.1rem", display: "grid", gap: "0.65rem" }}>
            <h3 id="archive-loc-title" style={{ margin: 0, fontSize: "1.05rem", fontWeight: 750 }}>
              {lx.setDownloadArea}
            </h3>
            <p className="text-muted small" style={{ margin: 0, lineHeight: 1.45 }}>
              {lx.setDownloadAreaBody}
            </p>
            <label className="text-muted small" style={{ display: "grid", gap: "0.25rem" }}>
              {lx.locationFieldLabel}
              <input
                className="sidebar-search"
                value={locationQuery}
                onChange={(e) => setLocationQuery(e.target.value)}
                disabled={locationBusy}
                placeholder="e.g. Austin, TX"
                aria-label={lx.locationFieldLabel}
              />
            </label>
            {locationModalErr ? (
              <p className="error" role="alert" style={{ margin: 0, fontSize: "0.9rem" }}>
                {locationModalErr}
              </p>
            ) : null}
            <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem" }}>
              <button type="button" className="glass-action-btn" disabled={locationBusy} onClick={() => void saveLocationFromGeocode()}>
                {lx.lookUpAndSave}
              </button>
              <button type="button" className="secondary" disabled={locationBusy} onClick={() => void saveLocationNycDefault()}>
                {lx.useNycDefault}
              </button>
              <button type="button" className="secondary" disabled={locationBusy} onClick={() => setShowLocationModal(false)}>
                {lx.notNow}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
        <div>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>
            {mode === "packages" ? lx.archiveScreenTitle : lx.archiveAcademyNetworkTitle}
          </h2>
          <p className="text-muted" style={{ margin: "0.25rem 0 0", maxWidth: "42rem" }}>
            {mode === "packages" ? lx.archiveScreenSubtitle : lx.archiveAcademyNetworkSubtitle}
          </p>
        </div>
      </div>

      {!installerLocation && !showLocationModal ? (
        <div className="glass-panel" style={{ padding: "0.65rem 0.75rem", borderLeft: "3px solid var(--primary)" }}>
          <div className="text-muted small" style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
            <span>{lx.noAreaWarning}</span>
            <button type="button" className="glass-action-btn" onClick={() => openLocationModal()}>
              {lx.setLocation}
            </button>
          </div>
        </div>
      ) : null}

      {loadErr ? (
        <p className="error" role="alert" style={{ margin: 0 }}>
          {loadErr}
        </p>
      ) : null}

      <div style={{ display: "grid", gap: "0.85rem" }}>
        <div className="glass-panel" style={{ padding: "0.75rem" }}>
          <div style={{ fontWeight: 650, marginBottom: "0.35rem" }}>
            {mode === "packages" ? lx.sectionTierPackages : lx.sectionAcademyCurriculum}
          </div>
          <p className="text-muted small" style={{ margin: "0 0 0.5rem" }}>
            {mode === "packages" ? lx.catalogDuplicateNote : lx.sectionAcademyCurriculumBody}
          </p>
          <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.35rem" }}>
            <div style={{ fontWeight: 650 }}>{lx.destinationSummary}</div>
            <div className="text-muted small">{primaryRootPath ? primaryRootPath : lx.noDataSourcesHint}</div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "baseline", justifyContent: "space-between" }}>
              <div className="text-muted small">
                <span style={{ fontWeight: 650, color: "var(--text)" }}>{lx.regionalMaps}: </span>
                {installerLocation ? (
                  <>
                    {installerLocation.display_name} ({installerLocation.state_code}) — lat {installerLocation.lat.toFixed(3)}, lon{" "}
                    {installerLocation.lon.toFixed(3)}
                  </>
                ) : (
                  lx.regionalMapsNotSet
                )}
              </div>
              <button type="button" className="secondary" onClick={() => openLocationModal()} disabled={!!installingTier}>
                {installerLocation ? lx.changeArea : lx.setLocation}
              </button>
            </div>
          </div>
        </div>

        {children}
      </div>

      {installErr ? (
        <p className="error" role="alert" style={{ margin: 0 }}>
          {installErr}
        </p>
      ) : null}
      {installNote ? (
        <p className="text-muted small" role="status" style={{ margin: 0 }}>
          {installNote}
        </p>
      ) : null}
    </div>
  );
}

function AcademyNetworkDownloadsContent({
  lx,
  academyCurriculum,
  academyDownloadRows,
  orphanAcademyRows,
  installingTier,
  singleBusyId,
  progressById,
  primaryRootPath,
  installSingleFile,
  localFileBasenames,
}: {
  lx: ReturnType<typeof useLexicon>;
  academyCurriculum: AcademyCurriculumRoot | null;
  academyDownloadRows: LibraryManifestRow[];
  orphanAcademyRows: LibraryManifestRow[];
  installingTier: string | null;
  singleBusyId: string | null;
  progressById: Record<string, DownloadProgressPayload>;
  primaryRootPath: string;
  installSingleFile: (entry: LibraryManifestRow) => void | Promise<void>;
  localFileBasenames: Set<string>;
}) {
  if (academyDownloadRows.length === 0) {
    return (
      <div className="glass-panel" style={{ padding: "0.9rem" }}>
        <p className="text-muted small" style={{ margin: 0, lineHeight: 1.45 }}>
          {lx.academyNetworkEmpty}
        </p>
      </div>
    );
  }

  return (
    <div style={{ display: "grid", gap: "0.75rem" }}>
      <div className="glass-panel" style={{ padding: "0.65rem 0.75rem", borderLeft: "3px solid color-mix(in srgb, var(--primary) 65%, transparent)" }}>
        <p className="text-muted small" style={{ margin: 0, lineHeight: 1.5 }}>
          {lx.academyArchiveTwoLocationsNote}
        </p>
      </div>
      {academyCurriculum ? (
        <>
          {academyCurriculum.ageGroups.map((age) => (
            <div key={age.id} className="glass-panel" style={{ padding: "0.65rem 0.75rem", display: "grid", gap: "0.55rem" }}>
              <div style={{ fontWeight: 750 }}>{age.label}</div>
              {age.subjects.map((subject) => {
                const rows = academyDownloadRows.filter(
                  (f) =>
                    (f.academy_age_group_id ?? "").trim() === age.id && (f.academy_subject_id ?? "").trim() === subject.id,
                );
                if (rows.length === 0) return null;
                return (
                  <div key={subject.id} style={{ display: "grid", gap: "0.4rem" }}>
                    <div className="text-muted small" style={{ fontWeight: 650, color: "var(--text)" }}>
                      {subject.label}
                    </div>
                    <ul style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: "0.35rem" }}>
                      {rows.map((f) => {
                        const downloadId = `single::${encodeURIComponent(f.url)}::${f.destination}`;
                        const busy = singleBusyId === downloadId;
                        const p = progressById[downloadId] ?? null;
                        const pct = p && p.totalBytes && p.totalBytes > 0 ? (p.downloadedBytes / p.totalBytes) * 100 : p ? 5 : 0;
                        const onDisk = manifestFileOnDisk(f, localFileBasenames);
                        return (
                          <li
                            key={downloadId}
                            className="glass-panel"
                            style={{
                              padding: "0.55rem 0.65rem",
                              display: "flex",
                              flexWrap: "wrap",
                              gap: "0.5rem",
                              alignItems: "center",
                              justifyContent: "space-between",
                            }}
                          >
                            <div style={{ minWidth: 0 }}>
                              <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem", alignItems: "center" }}>
                                <span style={{ fontWeight: 650, wordBreak: "break-word" }}>{f.name}</span>
                                <FileInstallStatusPill onDisk={onDisk} lx={lx} />
                              </div>
                              <div className="text-muted small">
                                {f.tierName} ·{" "}
                                {f.size_bytes != null && typeof f.size_bytes === "number" && f.size_bytes > 0
                                  ? formatDownloadSizeGb(f.size_bytes)
                                  : lx.sizeUnknown}
                              </div>
                              {busy && pct > 0 ? (
                                <div style={{ marginTop: "0.35rem", maxWidth: "18rem" }}>
                                  <ThinBar percent={pct} />
                                </div>
                              ) : null}
                            </div>
                            <button
                              type="button"
                              className="glass-action-btn"
                              disabled={!!installingTier || busy || !primaryRootPath}
                              onClick={() => void installSingleFile(f)}
                            >
                              {busy ? "Downloading…" : lx.actionRequisitionFile}
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                );
              })}
            </div>
          ))}
          {orphanAcademyRows.length > 0 ? (
            <div style={{ display: "grid", gap: "0.4rem" }}>
              <div className="text-muted small" style={{ fontWeight: 650, color: "var(--text)" }}>
                {lx.academyOtherDownloads}
              </div>
              <ul style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: "0.35rem" }}>
                {orphanAcademyRows.map((f) => {
                  const downloadId = `single::${encodeURIComponent(f.url)}::${f.destination}`;
                  const busy = singleBusyId === downloadId;
                  const onDisk = manifestFileOnDisk(f, localFileBasenames);
                  return (
                    <li
                      key={downloadId}
                      className="glass-panel"
                      style={{
                        padding: "0.55rem 0.65rem",
                        display: "flex",
                        flexWrap: "wrap",
                        gap: "0.5rem",
                        alignItems: "center",
                        justifyContent: "space-between",
                      }}
                    >
                      <div style={{ minWidth: 0 }}>
                        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem", alignItems: "center" }}>
                          <span style={{ fontWeight: 650, wordBreak: "break-word" }}>{f.name}</span>
                          <FileInstallStatusPill onDisk={onDisk} lx={lx} />
                        </div>
                        <div className="text-muted small">{f.tierName}</div>
                      </div>
                      <button
                        type="button"
                        className="glass-action-btn"
                        disabled={!!installingTier || busy || !primaryRootPath}
                        onClick={() => void installSingleFile(f)}
                      >
                        {busy ? "Downloading…" : lx.actionRequisitionFile}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          ) : null}
        </>
      ) : (
        <ul style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: "0.4rem" }}>
          {academyDownloadRows.map((f) => {
            const downloadId = `single::${encodeURIComponent(f.url)}::${f.destination}`;
            const busy = singleBusyId === downloadId;
            const onDisk = manifestFileOnDisk(f, localFileBasenames);
            return (
              <li
                key={downloadId}
                className="glass-panel"
                style={{
                  padding: "0.55rem 0.65rem",
                  display: "flex",
                  flexWrap: "wrap",
                  gap: "0.5rem",
                  alignItems: "center",
                  justifyContent: "space-between",
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem", alignItems: "center" }}>
                    <span style={{ fontWeight: 650, wordBreak: "break-word" }}>{f.name}</span>
                    <FileInstallStatusPill onDisk={onDisk} lx={lx} />
                  </div>
                  <div className="text-muted small">{f.tierName}</div>
                </div>
                <button
                  type="button"
                  className="glass-action-btn"
                  disabled={!!installingTier || busy || !primaryRootPath}
                  onClick={() => void installSingleFile(f)}
                >
                  {busy ? "Downloading…" : lx.actionRequisitionFile}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

export function ArchiveAcademyDownloadsPanel() {
  const v = useLibraryNetworkInstaller();
  const {
    lx,
    academyCurriculum,
    academyDownloadRows,
    orphanAcademyRows,
    installingTier,
    singleBusyId,
    progressById,
    primaryRootPath,
    installSingleFile,
    localFileBasenames,
  } = v;

  return (
    <ArchiveNetworkInstallerShell mode="academy" v={v}>
      <AcademyNetworkDownloadsContent
        lx={lx}
        academyCurriculum={academyCurriculum}
        academyDownloadRows={academyDownloadRows}
        orphanAcademyRows={orphanAcademyRows}
        installingTier={installingTier}
        singleBusyId={singleBusyId}
        progressById={progressById}
        primaryRootPath={primaryRootPath}
        installSingleFile={installSingleFile}
        localFileBasenames={localFileBasenames}
      />
    </ArchiveNetworkInstallerShell>
  );
}

export function ArchivePackagesPanel() {
  const v = useLibraryNetworkInstaller();
  const {
    lx,
    primaryRootPath,
    installingTier,
    activeDownloadId,
    progressById,
    singleBusyId,
    tiers,
    alacarteFiles,
    localFileBasenames,
    installTier,
    installSingleFile,
  } = v;

  return (
    <ArchiveNetworkInstallerShell mode="packages" v={v}>
        <div style={{ display: "grid", gap: "0.75rem" }}>
          {tiers.map((t) => {
            const isActive = installingTier === t.name;
            const fileCount = t.files.length;
            const { total: tierBytes, knownCount } = sumTierBytes(t.files);
            const prog = activeDownloadId ? progressById[activeDownloadId] : undefined;
            const pct = prog && prog.totalBytes && prog.totalBytes > 0 ? (prog.downloadedBytes / prog.totalBytes) * 100 : prog ? 5 : 0;
            const bps = prog && typeof prog.bytesPerSecond === "number" && Number.isFinite(prog.bytesPerSecond) && prog.bytesPerSecond > 0 ? prog.bytesPerSecond : null;
            return (
              <div key={t.name} className="glass-panel" style={{ padding: "0.9rem", borderRadius: "14px", display: "grid", gap: "0.5rem" }}>
                <div style={{ display: "flex", gap: "0.75rem", alignItems: "baseline", flexWrap: "wrap" }}>
                  <div style={{ fontWeight: 750, fontSize: "1.05rem" }}>{t.name}</div>
                  <div className="text-muted small">
                    {fileCount} {lx.tierFilesCount}
                  </div>
                  {typeof t.display_total_label === "string" && t.display_total_label.trim() ? (
                    <div className="text-muted small">Tier total: {t.display_total_label.trim()}</div>
                  ) : knownCount > 0 && tierBytes > 0 ? (
                    <div className="text-muted small">
                      ~{formatDownloadSizeGb(tierBytes)} total{knownCount < fileCount ? ` (${knownCount}/${fileCount} in manifest)` : ""}
                    </div>
                  ) : (
                    <div className="text-muted small">{lx.sizeUnknown}</div>
                  )}
                </div>
                <div className="text-muted" style={{ margin: 0 }}>
                  {t.description}
                </div>
                <ul className="text-muted small" style={{ margin: "0.15rem 0 0", paddingLeft: "1.1rem", display: "grid", gap: "0.35rem", textAlign: "left" }}>
                  {t.files.map((f) => {
                    const onDisk = manifestFileOnDisk(f, localFileBasenames);
                    return (
                      <li key={`${t.name}-${f.name}-${f.url}`} style={{ wordBreak: "break-word", listStyle: "disc" }}>
                        <span style={{ display: "inline-flex", flexWrap: "wrap", gap: "0.35rem", alignItems: "center", verticalAlign: "middle" }}>
                          <span style={{ color: "var(--text)" }}>{f.name}</span>
                          <FileInstallStatusPill onDisk={onDisk} lx={lx} />
                          <span>·</span>
                          <span>
                            {f.size_bytes != null && f.size_bytes > 0 ? formatDownloadSizeGb(f.size_bytes) : lx.sizeUnknown}
                          </span>
                        </span>
                      </li>
                    );
                  })}
                </ul>
                {isActive ? (
                  <div style={{ display: "grid", gap: "0.35rem" }}>
                    <ThinBar percent={pct} />
                    <div className="text-muted small" style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
                      <span>
                        {activeDownloadId ? `${lx.installingLabel}: ${activeDownloadId}` : `${lx.installingLabel}…`}
                        {prog && prog.totalBytes != null && prog.totalBytes > 0 ? (
                          <span style={{ marginLeft: "0.35rem" }}>
                            {formatDownloadSizeGb(prog.downloadedBytes)} / {formatDownloadSizeGb(prog.totalBytes)}
                          </span>
                        ) : null}
                        {bps != null ? <span style={{ marginLeft: "0.35rem", color: "var(--text)" }}>· {formatThroughput(bps)}</span> : null}
                      </span>
                      <button
                        type="button"
                        className="secondary"
                        disabled={!activeDownloadId}
                        onClick={() => {
                          if (!activeDownloadId) return;
                          void invoke("cancel_package_download", { args: { manifestType: "library", downloadId: activeDownloadId } }).catch(() => {});
                        }}
                      >
                        {lx.cancelDownload}
                      </button>
                    </div>
                  </div>
                ) : null}
                <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
                  <button type="button" className="glass-action-btn" onClick={() => void installTier(t.name)} disabled={!!installingTier || !primaryRootPath}>
                    {lx.actionDeployTier}
                  </button>
                </div>
              </div>
            );
          })}
        </div>

        <div className="glass-panel" style={{ padding: "0.9rem", display: "grid", gap: "0.5rem" }}>
          <div style={{ fontWeight: 750 }}>{lx.sectionAlacarte}</div>
          <ul style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: "0.4rem" }}>
            {alacarteFiles.map((f) => {
              const downloadId = `single::${encodeURIComponent(f.url)}::${f.destination}`;
              const busy = singleBusyId === downloadId;
              const onDisk = manifestFileOnDisk(f, localFileBasenames);
              return (
                <li
                  key={downloadId}
                  className="glass-panel"
                  style={{ padding: "0.55rem 0.65rem", display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem", alignItems: "center" }}>
                      <span style={{ fontWeight: 650, wordBreak: "break-word" }}>{f.name}</span>
                      <FileInstallStatusPill onDisk={onDisk} lx={lx} />
                    </div>
                    <div className="text-muted small">
                      {f.category} · {f.tierName}
                    </div>
                    <div className="text-muted small">
                      {f.size_bytes != null && typeof f.size_bytes === "number" && f.size_bytes > 0 ? formatDownloadSizeGb(f.size_bytes) : lx.sizeUnknown}
                    </div>
                  </div>
                  <button type="button" className="glass-action-btn" disabled={!!installingTier || busy || !primaryRootPath} onClick={() => void installSingleFile(f)}>
                    {lx.actionRequisitionFile}
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
    </ArchiveNetworkInstallerShell>
  );
}
