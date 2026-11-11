import { useCallback, useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { type Event } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FileText, Link2, Youtube } from "lucide-react";
import { useTauriListen } from "../../shared/hooks/useTauriListen";
import { LibrarySearchTest } from "./LibrarySearchTest";

type CatalogCategory = "Medical" | "Tactical" | "Maps" | "Tech" | "History" | "Local";

interface CatalogPackage {
  id: string;
  category: CatalogCategory;
  title: string;
  version: string;
  description: string;
  sizeLabel: string;
  url: string;
}

interface DownloadProgressPayload {
  package_id: string;
  progress: number;
  completed: boolean;
  error?: string | null;
}

interface RagDocumentSummary {
  id: string;
  filename: string;
  file_type: string;
  total_chunks: number;
  ingested_at: string;
  source_package: string;
}

function normalizeRagFileType(raw: string): string {
  const t = (raw ?? "").trim().toLowerCase();
  if (!t) return "unknown";
  if (t === "pdf") return "pdf";
  if (t === "doc" || t === "docx") return "word";
  if (t === "txt" || t === "md" || t === "csv" || t === "text") return "text";
  if (t === "vtt" || t === "srt") return "transcript";
  if (t === "html" || t === "htm") return "html";
  if (t === "zim") return "zim";
  return t;
}

function formatTypeHeading(t: string): string {
  if (t === "pdf") return "PDF";
  if (t === "word") return "Word / Docx";
  if (t === "text") return "Text (txt / md / csv)";
  if (t === "transcript") return "Transcripts (vtt / srt)";
  if (t === "html") return "HTML";
  if (t === "zim") return "ZIM archives";
  if (t === "unknown") return "Unknown";
  return t.toUpperCase();
}

type HarvestUiStatus = "Pending" | "Downloading" | "Ingesting" | "Complete" | "Error";

type HarvestSourceKind = "clip" | "youtube" | "local";

interface HarvestProgressPayload {
  harvestId: string;
  phase: string;
  message?: string | null;
  targetPath?: string | null;
  companionTranscriptPath?: string | null;
}

interface RagIngestionProgressPayload {
  file_path: string;
  filename: string;
  completed: boolean;
  chunks: number;
  document_id?: string | null;
  error?: string | null;
}

interface HarvestRow {
  id: string;
  sourceName: string;
  sourceKind: HarvestSourceKind;
  /** Display bucket for the type column */
  fileKind: "web" | "video" | "pdf" | "word" | "subtitle" | "other";
  status: HarvestUiStatus;
  targetPath?: string;
  companionTranscriptPath?: string;
  error?: string;
}

function inferFileKindFromPath(p: string): HarvestRow["fileKind"] {
  const lower = p.toLowerCase();
  if (lower.endsWith(".pdf")) return "pdf";
  if (lower.endsWith(".doc") || lower.endsWith(".docx")) return "word";
  if (lower.endsWith(".vtt") || lower.endsWith(".srt")) return "subtitle";
  if (lower.endsWith(".mp4") || lower.endsWith(".webm") || lower.endsWith(".mkv")) return "video";
  if (lower.startsWith("http://") || lower.startsWith("https://")) return "web";
  return "other";
}

function shouldAutoIngestPath(p: string): boolean {
  return /\.(pdf|txt|md|csv|docx?|vtt|srt)$/i.test(p);
}

const CATEGORIES: CatalogCategory[] = ["Medical", "Tactical", "Maps", "Tech", "History", "Local"];

const MOCK_CATALOG: CatalogPackage[] = [
  {
    id: "adv-field-surgery-v1",
    category: "Medical",
    title: "Advanced Field Surgery",
    version: "v1.2",
    description: "Emergency trauma procedures, austere care triage, and post-op protocols.",
    sizeLabel: "45MB",
    url: "https://example.com/athena/advanced-field-surgery.txt",
  },
  {
    id: "shtf-radio-protocols-v3",
    category: "Tactical",
    title: "SHTF Radio Protocols",
    version: "v3.1",
    description: "Off-grid comms discipline, brevity codes, and relay procedures.",
    sizeLabel: "18MB",
    url: "https://example.com/athena/shtf-radio-protocols.txt",
  },
  {
    id: "regional-terrain-primer-v2",
    category: "Maps",
    title: "Regional Terrain Primer",
    version: "v2.0",
    description: "Land navigation references, route planning, and terrain hazard notes.",
    sizeLabel: "61MB",
    url: "https://example.com/athena/regional-terrain-primer.txt",
  },
  {
    id: "microgrid-handbook-v4",
    category: "Tech",
    title: "Microgrid Operations Handbook",
    version: "v4.0",
    description: "Power resilience planning, battery systems, and field repair checklists.",
    sizeLabel: "52MB",
    url: "https://example.com/athena/microgrid-operations-handbook.txt",
  },
  {
    id: "civilization-rebuild-cases-v1",
    category: "History",
    title: "Civilization Rebuild Case Files",
    version: "v1.5",
    description: "Historical recovery patterns, logistics failures, and governance lessons.",
    sizeLabel: "39MB",
    url: "https://example.com/athena/civilization-rebuild-cases.txt",
  },
  {
    id: "water-sanitation-guide-v2",
    category: "Medical",
    title: "Water & Sanitation Guide",
    version: "v2.3",
    description: "Water purification methods, contamination risks, and sanitation SOPs.",
    sizeLabel: "27MB",
    url: "https://example.com/athena/water-sanitation-guide.txt",
  },
];

export function KnowledgeCatalogView() {
  const [activeCategory, setActiveCategory] = useState<CatalogCategory>("Medical");
  const [inFlight, setInFlight] = useState<Record<string, number>>({});
  const [installed, setInstalled] = useState<Record<string, boolean>>({});
  const [localDocs, setLocalDocs] = useState<RagDocumentSummary[]>([]);
  const [watchFolder, setWatchFolder] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [ingestingLocalPaths, setIngestingLocalPaths] = useState<string[]>([]);

  const [harvestStagingDir, setHarvestStagingDir] = useState<string | null>(null);
  const [webClipUrl, setWebClipUrl] = useState("");
  const [youtubeUrl, setYoutubeUrl] = useState("");
  const [harvestRows, setHarvestRows] = useState<HarvestRow[]>([]);
  const [docDropHover, setDocDropHover] = useState(false);

  useEffect(() => {
    void invoke<string>("resolve_media_harvest_dir")
      .then((d) => setHarvestStagingDir(d))
      .catch(() => setHarvestStagingDir(null));
  }, []);

  const runPostHarvestIngest = useCallback(async (rowId: string, primaryPath: string, companion?: string | null) => {
    const tryIngest = async (path: string) => {
      if (!shouldAutoIngestPath(path)) return;
      setHarvestRows((prev) =>
        prev.map((r) => (r.id === rowId ? { ...r, status: "Ingesting" as const } : r)),
      );
      try {
        await invoke("ingest_local_document", { filePath: path });
        setHarvestRows((prev) =>
          prev.map((r) => (r.id === rowId ? { ...r, status: "Complete" as const, error: undefined } : r)),
        );
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setHarvestRows((prev) =>
          prev.map((r) => (r.id === rowId ? { ...r, status: "Error" as const, error: msg } : r)),
        );
      }
    };

    if (primaryPath.toLowerCase().match(/\.(mp4|webm|mkv)$/)) {
      const sub = companion?.trim();
      if (sub && shouldAutoIngestPath(sub)) {
        await tryIngest(sub);
        return;
      }
      setHarvestRows((prev) =>
        prev.map((r) => (r.id === rowId ? { ...r, status: "Complete" as const } : r)),
      );
      return;
    }
    await tryIngest(primaryPath);
  }, []);

  const onHarvestProgress = useCallback(
    (ev: Event<HarvestProgressPayload>) => {
      const p = ev.payload;
      if (!p || typeof p.harvestId !== "string") return;
      const phase = (p.phase ?? "").toLowerCase();
      setHarvestRows((prev) => {
        const i = prev.findIndex((r) => r.id === p.harvestId);
        const base = (): HarvestRow => ({
          id: p.harvestId,
          sourceName: p.message ?? p.targetPath ?? p.harvestId,
          sourceKind: "clip",
          fileKind: p.targetPath ? inferFileKindFromPath(p.targetPath) : "web",
          status: phase === "downloading" ? "Downloading" : "Pending",
        });
        if (i < 0) {
          return [...prev, base()];
        }
        const cur = prev[i]!;
        let status: HarvestUiStatus = cur.status;
        if (phase === "error") status = "Error";
        else if (phase === "pending") status = "Pending";
        else if (phase === "downloading") status = "Downloading";
        else if (phase === "complete") status = "Downloading";
        const next: HarvestRow = {
          ...cur,
          status,
          targetPath: p.targetPath ?? cur.targetPath,
          companionTranscriptPath: p.companionTranscriptPath ?? cur.companionTranscriptPath,
          error: phase === "error" ? (p.message ?? "Harvest failed") : undefined,
          sourceName:
            p.targetPath && typeof p.targetPath === "string"
              ? (p.targetPath.split(/[/\\]/).pop() ?? cur.sourceName)
              : cur.sourceName,
          fileKind: p.targetPath ? inferFileKindFromPath(p.targetPath) : cur.fileKind,
        };
        const copy = [...prev];
        copy[i] = next;
        return copy;
      });
      if (phase === "complete" && p.targetPath) {
        void runPostHarvestIngest(p.harvestId, p.targetPath, p.companionTranscriptPath ?? undefined);
      }
    },
    [runPostHarvestIngest],
  );

  useTauriListen<HarvestProgressPayload>("athena:harvest-progress", { handler: onHarvestProgress });

  const onRagIngestProgress = useCallback((ev: Event<RagIngestionProgressPayload>) => {
    const p = ev.payload;
    if (!p || typeof p.file_path !== "string") return;
    setHarvestRows((prev) =>
      prev.map((r) => {
        if (r.targetPath !== p.file_path && r.companionTranscriptPath !== p.file_path) return r;
        if (p.completed) {
          if (p.error) return { ...r, status: "Error" as const, error: p.error };
          return { ...r, status: "Complete" as const, error: undefined };
        }
        return { ...r, status: "Ingesting" as const };
      }),
    );
  }, []);

  useTauriListen<RagIngestionProgressPayload>("rag-ingestion-progress", { handler: onRagIngestProgress });

  useEffect(() => {
    const unlistenPromise = listen<DownloadProgressPayload>("download-progress", (event) => {
      const payload = event.payload;
      if (!payload || typeof payload.package_id !== "string") return;
      const progress = Math.max(0, Math.min(100, Number(payload.progress) || 0));
      setInFlight((prev) => ({ ...prev, [payload.package_id]: progress }));
      if (payload.completed) {
        if (!payload.error) {
          setInstalled((prev) => ({ ...prev, [payload.package_id]: true }));
        }
        setTimeout(() => {
          setInFlight((prev) => {
            const next = { ...prev };
            delete next[payload.package_id];
            return next;
          });
        }, 1200);
      }
    });
    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, []);

  const loadLocalDocs = async () => {
    try {
      const docs = await invoke<RagDocumentSummary[]>("list_rag_documents");
      setLocalDocs(Array.isArray(docs) ? docs : []);
    } catch {
      setLocalDocs([]);
    }
  };

  useEffect(() => {
    void loadLocalDocs();
    void invoke<string | null>("get_watch_folder")
      .then((folder) => setWatchFolder(folder))
      .catch(() => setWatchFolder(null));
  }, []);

  const ingestDroppedDocumentPaths = async (paths: string[]) => {
    const uniquePaths = Array.from(new Set(paths.filter((p) => p.trim().length > 0)));
    if (uniquePaths.length === 0) return;
    setIngestingLocalPaths(uniquePaths);
    for (const filePath of uniquePaths) {
      try {
        await invoke("ingest_local_document", { filePath });
      } catch {
        // Keep trying remaining files.
      }
    }
    setIngestingLocalPaths([]);
    await loadLocalDocs();
  };

  const handleChooseFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) return;
    try {
      await invoke("set_watch_folder", { path: selected });
      setWatchFolder(selected);
    } catch {
      // ignore
    }
  };

  const filtered = useMemo(
    () => MOCK_CATALOG.filter((item) => item.category === activeCategory),
    [activeCategory],
  );

  const localDocsBySourceThenType = useMemo(() => {
    const bySource = new Map<string, RagDocumentSummary[]>();
    for (const d of localDocs) {
      const src = (d.source_package ?? "").trim() || "Custom Uploads";
      const arr = bySource.get(src);
      if (arr) arr.push(d);
      else bySource.set(src, [d]);
    }

    const order = ["pdf", "word", "text", "transcript", "html", "zim", "unknown"];

    const out: Array<[string, Array<[string, RagDocumentSummary[]]>]> = [];
    for (const [src, docs] of bySource.entries()) {
      const byType = new Map<string, RagDocumentSummary[]>();
      for (const d of docs) {
        const k = normalizeRagFileType(d.file_type);
        const arr = byType.get(k);
        if (arr) arr.push(d);
        else byType.set(k, [d]);
      }
      for (const arr of byType.values()) {
        arr.sort((a, b) => {
          const at = Date.parse(a.ingested_at || "");
          const bt = Date.parse(b.ingested_at || "");
          return (Number.isFinite(bt) ? bt : 0) - (Number.isFinite(at) ? at : 0);
        });
      }
      const typesSorted = [...byType.entries()].sort((a, b) => {
        const ai = order.indexOf(a[0]);
        const bi = order.indexOf(b[0]);
        const ak = ai >= 0 ? ai : 999;
        const bk = bi >= 0 ? bi : 999;
        if (ak !== bk) return ak - bk;
        return a[0].localeCompare(b[0]);
      });
      out.push([src, typesSorted]);
    }
    out.sort((a, b) => a[0].localeCompare(b[0]));
    return out;
  }, [localDocs]);
  const queueItems = useMemo(
    () =>
      Object.entries(inFlight)
        .map(([id, progress]) => {
          const pkg = MOCK_CATALOG.find((item) => item.id === id);
          if (!pkg) return null;
          return { id, title: pkg.title, progress };
        })
        .filter((row): row is { id: string; title: string; progress: number } => row !== null),
    [inFlight],
  );

  const handleDownload = async (pkg: CatalogPackage) => {
    if (inFlight[pkg.id] !== undefined) return;
    setInFlight((prev) => ({ ...prev, [pkg.id]: 0 }));
    try {
      await invoke<boolean>("download_knowledge_package", { packageId: pkg.id, url: pkg.url });
    } catch {
      setInFlight((prev) => {
        const next = { ...prev };
        delete next[pkg.id];
        return next;
      });
    }
  };

  const markHarvestRow = (id: string, patch: Partial<HarvestRow>) => {
    setHarvestRows((prev) => prev.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  };

  const handleWebClipHarvest = async () => {
    const url = webClipUrl.trim();
    if (!url || !harvestStagingDir) return;
    const id = crypto.randomUUID();
    setHarvestRows((prev) => [
      { id, sourceName: url, sourceKind: "clip", fileKind: "web", status: "Pending" },
      ...prev,
    ]);
    setWebClipUrl("");
    try {
      await invoke("harvest_media_url", { url, targetDir: harvestStagingDir, correlationId: id });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      markHarvestRow(id, { status: "Error", error: msg });
    }
  };

  const handleYoutubeHarvest = async () => {
    const url = youtubeUrl.trim();
    if (!url || !harvestStagingDir) return;
    const id = crypto.randomUUID();
    setHarvestRows((prev) => [
      { id, sourceName: url, sourceKind: "youtube", fileKind: "video", status: "Pending" },
      ...prev,
    ]);
    setYoutubeUrl("");
    try {
      await invoke("harvest_media_url", { url, targetDir: harvestStagingDir, correlationId: id });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      markHarvestRow(id, { status: "Error", error: msg });
    }
  };

  const harvestPdfWordPaths = async (paths: string[]) => {
    const filtered = paths.filter((p) => /\.(pdf|doc|docx)$/i.test(p));
    if (filtered.length === 0 || !harvestStagingDir) return;
    const id = crypto.randomUUID();
    const label = filtered.map((p) => p.replace(/^.*[/\\]/, "")).join(", ");
    const kind: HarvestRow["fileKind"] = filtered.some((p) => /\.docx?$/i.test(p)) ? "word" : "pdf";
    setHarvestRows((prev) => [
      { id, sourceName: label, sourceKind: "local", fileKind: kind, status: "Pending" },
      ...prev,
    ]);
    try {
      markHarvestRow(id, { status: "Downloading" });
      const outs = await invoke<string[]>("harvest_local_documents", {
        paths: filtered,
        targetDir: harvestStagingDir,
        correlationId: id,
      });
      if (!outs.length) {
        markHarvestRow(id, { status: "Error", error: "No files were copied." });
        return;
      }
      markHarvestRow(id, { status: "Ingesting", targetPath: outs[0] });
      for (const p of outs) {
        markHarvestRow(id, { targetPath: p });
        await invoke("ingest_local_document", { filePath: p });
      }
      markHarvestRow(id, { status: "Complete" });
      await loadLocalDocs();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      markHarvestRow(id, { status: "Error", error: msg });
    }
  };

  return (
    <section
      className="glass-panel"
      style={{ padding: "0.8rem", display: "grid", gap: "0.8rem", gridTemplateRows: "1fr auto", minHeight: "62vh", position: "relative" }}
      onDragEnter={(e) => {
        e.preventDefault();
        setDragging(true);
      }}
      onDragOver={(e) => {
        e.preventDefault();
        setDragging(true);
      }}
      onDragLeave={(e) => {
        e.preventDefault();
        const related = e.relatedTarget as Node | null;
        if (!related || !e.currentTarget.contains(related)) setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        const paths: string[] = [];
        for (const f of Array.from(e.dataTransfer.files ?? [])) {
          const path = (f as File & { path?: string }).path;
          if (typeof path === "string" && path.trim() !== "") paths.push(path);
        }
        void ingestDroppedDocumentPaths(paths);
      }}
    >
      {dragging ? (
        <div
          className="glass-panel"
          style={{
            position: "absolute",
            inset: "0.6rem",
            zIndex: 10,
            display: "grid",
            placeItems: "center",
            pointerEvents: "none",
            background: "color-mix(in srgb, var(--surface-solid) 76%, transparent)",
          }}
        >
          <div style={{ fontSize: "1rem", color: "var(--primary)" }}>Drop to Ingest into Library</div>
        </div>
      ) : null}
      <div style={{ display: "grid", gridTemplateColumns: "220px 1fr", gap: "0.8rem", minHeight: 0 }}>
        <aside className="glass-panel" style={{ padding: "0.7rem", overflow: "auto", display: "grid", gap: "0.8rem", alignContent: "start" }}>
          <div>
            <h3 style={{ margin: "0 0 0.55rem", color: "var(--primary)" }}>Categories</h3>
            <div style={{ display: "grid", gap: "0.45rem" }}>
              {CATEGORIES.map((category) => (
                <button
                  key={category}
                  type="button"
                  className={
                    activeCategory === category ? "glass-action-btn library-category-btn-active" : "secondary"
                  }
                  onClick={() => setActiveCategory(category)}
                  aria-pressed={activeCategory === category}
                >
                  {category}
                </button>
              ))}
            </div>
          </div>
          <div className="glass-panel" style={{ padding: "0.6rem", display: "grid", gap: "0.5rem" }}>
            <div style={{ color: "var(--primary)" }}>Local Actions</div>
            <button type="button" className="secondary" onClick={() => void handleChooseFolder()}>
              Choose Folder
            </button>
            <div className="text-muted small" style={{ wordBreak: "break-word" }}>
              {watchFolder ?? "No folder selected"}
            </div>
            <div className="glass-panel" style={{ padding: "0.5rem", fontSize: "0.82rem" }}>
              Drag files here or anywhere in this page to ingest.
            </div>
            {ingestingLocalPaths.length > 0 ? (
              <div className="text-muted small">Ingesting {ingestingLocalPaths.length} file(s)</div>
            ) : null}
          </div>
          <LibrarySearchTest />
        </aside>

        <main className="glass-panel" style={{ padding: "0.7rem", overflow: "auto", display: "grid", gap: "0.85rem" }}>
          <div
            className="glass-panel"
            style={{
              padding: "0.85rem",
              display: "grid",
              gap: "0.75rem",
              border: "1px solid color-mix(in srgb, var(--primary) 22%, transparent)",
            }}
          >
            <div style={{ color: "var(--primary)", fontWeight: 600, fontSize: "1.02rem" }}>Documents / Media harvest</div>
            {!harvestStagingDir ? (
              <div className="text-muted small">Could not resolve harvest folder (app data). Restart the app or check permissions.</div>
            ) : (
              <div className="text-muted small" style={{ wordBreak: "break-all" }}>
                Staging: {harvestStagingDir}
              </div>
            )}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.65rem" }}>
              <label style={{ display: "grid", gap: "0.35rem" }}>
                <span className="text-muted small" style={{ display: "flex", alignItems: "center", gap: "0.35rem" }}>
                  <Link2 size={14} aria-hidden /> Web URL clipper
                </span>
                <input
                  type="url"
                  className="map-chat-input"
                  placeholder="https://example.com/report.pdf"
                  value={webClipUrl}
                  onChange={(e) => setWebClipUrl(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleWebClipHarvest();
                  }}
                />
                <button type="button" className="glass-action-btn" disabled={!harvestStagingDir} onClick={() => void handleWebClipHarvest()}>
                  Harvest URL
                </button>
              </label>
              <label style={{ display: "grid", gap: "0.35rem" }}>
                <span className="text-muted small" style={{ display: "flex", alignItems: "center", gap: "0.35rem" }}>
                  <Youtube size={14} aria-hidden /> YouTube / media link
                </span>
                <input
                  type="url"
                  className="map-chat-input"
                  placeholder="https://www.youtube.com/watch?v=…"
                  value={youtubeUrl}
                  onChange={(e) => setYoutubeUrl(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleYoutubeHarvest();
                  }}
                />
                <button type="button" className="glass-action-btn" disabled={!harvestStagingDir} onClick={() => void handleYoutubeHarvest()}>
                  Harvest media
                </button>
              </label>
            </div>
            <div
              className="glass-panel"
              onDragEnter={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setDocDropHover(true);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setDocDropHover(true);
              }}
              onDragLeave={(e) => {
                e.preventDefault();
                e.stopPropagation();
                const related = e.relatedTarget as Node | null;
                if (!related || !e.currentTarget.contains(related)) setDocDropHover(false);
              }}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setDocDropHover(false);
                const paths: string[] = [];
                for (const f of Array.from(e.dataTransfer.files ?? [])) {
                  const path = (f as File & { path?: string }).path;
                  if (typeof path === "string" && path.trim() !== "") paths.push(path);
                }
                void harvestPdfWordPaths(paths);
              }}
              style={{
                minHeight: "140px",
                display: "grid",
                placeItems: "center",
                textAlign: "center",
                padding: "1.25rem",
                border: docDropHover
                  ? "2px dashed color-mix(in srgb, var(--primary) 55%, transparent)"
                  : "2px dashed color-mix(in srgb, var(--surface-solid) 55%, transparent)",
                background: docDropHover ? "color-mix(in srgb, var(--primary) 8%, transparent)" : undefined,
                transition: "border-color 0.15s ease, background 0.15s ease",
              }}
            >
              <div style={{ display: "grid", gap: "0.35rem", justifyItems: "center", maxWidth: "420px" }}>
                <FileText size={36} color="var(--primary)" aria-hidden />
                <div style={{ fontSize: "1.05rem", color: "var(--primary)" }}>Drop PDF or Word documents here</div>
                <div className="text-muted small">Copies into the harvest staging folder, writes sidecars, then ingests into the vector library.</div>
              </div>
            </div>
            <div>
              <div style={{ color: "var(--primary)", marginBottom: "0.45rem", fontSize: "0.95rem" }}>Recent harvests</div>
              {harvestRows.length === 0 ? (
                <div className="text-muted small">No harvests yet.</div>
              ) : (
                <div style={{ overflow: "auto", maxHeight: "220px" }}>
                  <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.88rem" }}>
                    <thead>
                      <tr className="text-muted small">
                        <th style={{ textAlign: "left", padding: "0.35rem 0.5rem" }}>Type</th>
                        <th style={{ textAlign: "left", padding: "0.35rem 0.5rem" }}>Source</th>
                        <th style={{ textAlign: "left", padding: "0.35rem 0.5rem" }}>Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {harvestRows.map((row) => (
                        <tr key={row.id} style={{ borderTop: "1px solid color-mix(in srgb, var(--surface-solid) 80%, transparent)" }}>
                          <td style={{ padding: "0.45rem 0.5rem", verticalAlign: "middle" }}>
                            {row.fileKind === "video" ? (
                              <Youtube size={18} aria-label="Video" />
                            ) : row.fileKind === "pdf" || row.fileKind === "word" ? (
                              <FileText size={18} aria-label="Document" />
                            ) : row.fileKind === "subtitle" ? (
                              <span title="Subtitle" aria-label="Subtitle">
                                CC
                              </span>
                            ) : (
                              <Link2 size={18} aria-label="Web or other" />
                            )}
                          </td>
                          <td style={{ padding: "0.45rem 0.5rem", wordBreak: "break-word" }} title={row.targetPath}>
                            {row.sourceName}
                            {row.error ? <div className="text-muted small">{row.error}</div> : null}
                          </td>
                          <td style={{ padding: "0.45rem 0.5rem", whiteSpace: "nowrap" }}>{row.status}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </div>
          <h3 style={{ margin: "0 0 0.65rem", color: "var(--primary)" }}>
            {activeCategory === "Local" ? "Local Documents" : "Knowledge Catalog"}
          </h3>
          {activeCategory === "Local" ? (
            <div style={{ display: "grid", gap: "0.75rem" }}>
              <div className="glass-panel" style={{ padding: "0.75rem", display: "flex", justifyContent: "space-between", gap: "0.75rem", flexWrap: "wrap", alignItems: "center" }}>
                <div style={{ display: "grid", gap: 4 }}>
                  <div style={{ color: "var(--primary)", fontWeight: 800, letterSpacing: "0.06em", textTransform: "uppercase", fontSize: "0.72rem" }}>
                    RAG Sources (Ingested)
                  </div>
                  <div className="text-muted small">
                    {localDocs.length} document{localDocs.length === 1 ? "" : "s"} ·{" "}
                    {localDocs.reduce((s, d) => s + Math.max(0, d.total_chunks || 0), 0)} chunks
                  </div>
                </div>
                <button type="button" className="secondary" onClick={() => void loadLocalDocs()}>
                  Refresh
                </button>
              </div>

              {localDocs.length === 0 ? (
                <div className="glass-panel" style={{ padding: "0.75rem" }}>
                  No ingested RAG documents yet.
                  <div className="text-muted small" style={{ marginTop: 6 }}>
                    Add files to your watch folder (or download content), then Athena will ingest and index them for RAG.
                  </div>
                </div>
              ) : (
                <div style={{ display: "grid", gap: "0.75rem" }}>
                  {localDocsBySourceThenType.map(([src, groups]) => (
                    <details key={src} className="glass-panel" style={{ padding: "0.75rem" }} open>
                      <summary style={{ cursor: "pointer", display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline" }}>
                        <span style={{ color: "var(--primary)", fontWeight: 800, letterSpacing: "0.06em", textTransform: "uppercase", fontSize: "0.75rem" }}>
                          {src}
                        </span>
                        <span className="text-muted small">
                          {groups.reduce((n, [, docs]) => n + docs.length, 0)} file{groups.reduce((n, [, docs]) => n + docs.length, 0) === 1 ? "" : "s"}
                        </span>
                      </summary>
                      <div style={{ marginTop: "0.65rem", display: "grid", gap: "0.75rem" }}>
                        {groups.map(([typ, docs]) => (
                          <details key={`${src}:${typ}`} className="glass-panel" style={{ padding: "0.75rem" }} open>
                            <summary style={{ cursor: "pointer", display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline" }}>
                              <span style={{ color: "var(--primary)", fontWeight: 800, letterSpacing: "0.06em", textTransform: "uppercase", fontSize: "0.75rem" }}>
                                {formatTypeHeading(typ)}
                              </span>
                              <span className="text-muted small">{docs.length} file{docs.length === 1 ? "" : "s"}</span>
                            </summary>
                            <div style={{ marginTop: "0.65rem", display: "grid", gap: "0.5rem" }}>
                              {docs.map((doc) => {
                                const ok = (doc.total_chunks ?? 0) > 0;
                                return (
                                  <div key={doc.id} className="glass-panel" style={{ padding: "0.65rem 0.75rem", borderRadius: 12, display: "grid", gap: 6 }}>
                                    <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", alignItems: "baseline", flexWrap: "wrap" }}>
                                      <div style={{ fontWeight: 650 }}>{doc.filename}</div>
                                      <span
                                        className="text-muted small"
                                        style={{
                                          padding: "2px 8px",
                                          borderRadius: 999,
                                          border: "1px solid color-mix(in srgb, var(--border) 80%, transparent)",
                                          background: ok ? "color-mix(in srgb, var(--primary) 10%, transparent)" : "color-mix(in srgb, var(--tertiary) 10%, transparent)",
                                          color: ok ? "var(--text)" : "var(--text-muted)",
                                          fontWeight: 800,
                                          letterSpacing: "0.06em",
                                          textTransform: "uppercase",
                                          fontSize: "0.62rem",
                                          whiteSpace: "nowrap",
                                        }}
                                        title={ok ? "Ingested and ready for semantic RAG" : "Ingest failed or produced no chunks"}
                                      >
                                        {ok ? "Ingested" : "Not indexed"}
                                      </span>
                                    </div>
                                    <div className="text-muted small">
                                      Chunks: {doc.total_chunks} · {new Date(doc.ingested_at).toLocaleString()}
                                    </div>
                                    <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
                                      <button
                                        type="button"
                                        className="glass-action-btn"
                                        onClick={() => void invoke("open_library_source", { path: doc.id, page: null }).catch(() => {})}
                                      >
                                        Open
                                      </button>
                                    </div>
                                  </div>
                                );
                              })}
                            </div>
                          </details>
                        ))}
                      </div>
                    </details>
                  ))}
                </div>
              )}
            </div>
          ) : (
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))", gap: "0.7rem" }}>
              {filtered.map((pkg) => {
                const progress = inFlight[pkg.id];
                const isDownloading = progress !== undefined;
                const isInstalled = installed[pkg.id] === true;
                return (
                  <article key={pkg.id} className="glass-panel" style={{ padding: "0.7rem", display: "grid", gap: "0.45rem" }}>
                    <div
                      className="glass-panel"
                      style={{
                        height: "92px",
                        display: "grid",
                        placeItems: "center",
                        color: "var(--primary)",
                        background: "color-mix(in srgb, var(--surface-solid) 82%, transparent)",
                      }}
                    >
                      {pkg.category}
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", gap: "0.6rem" }}>
                      <span>{pkg.title}</span>
                      <span className="text-muted small">{pkg.version}</span>
                    </div>
                    <p className="text-muted small" style={{ margin: 0 }}>
                      {pkg.description}
                    </p>
                    <div className="text-muted small">Size: {pkg.sizeLabel}</div>
                    <button
                      type="button"
                      disabled={isDownloading}
                      onClick={() => void handleDownload(pkg)}
                      className={isInstalled ? "secondary" : "glass-action-btn"}
                    >
                      {isDownloading ? `Downloading ${Math.round(progress)}%` : isInstalled ? "Installed" : "Download"}
                    </button>
                  </article>
                );
              })}
            </div>
          )}
        </main>
      </div>

      <div
        className="glass-panel"
        style={{ position: "sticky", bottom: 0, padding: "0.6rem 0.7rem", zIndex: 2 }}
        role="status"
        aria-live="polite"
      >
        <span style={{ color: "var(--primary)" }}>Download Queue</span>
        {queueItems.length === 0 ? (
          <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
            No active downloads.
          </p>
        ) : (
          <div style={{ display: "grid", gap: "0.45rem", marginTop: "0.45rem" }}>
            {queueItems.map((item) => (
              <div key={item.id} className="glass-panel" style={{ padding: "0.4rem 0.55rem" }}>
                <div style={{ display: "flex", justifyContent: "space-between", gap: "0.6rem" }}>
                  <span>{item.title}</span>
                  <span className="text-muted small">{Math.round(item.progress)}%</span>
                </div>
                <div className="glass-panel" style={{ height: "6px", padding: 0, overflow: "hidden", marginTop: "0.3rem" }}>
                  <div
                    style={{
                      width: `${Math.max(0, Math.min(100, item.progress))}%`,
                      height: "100%",
                      background: "color-mix(in srgb, var(--primary) 72%, var(--tertiary))",
                    }}
                  />
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
