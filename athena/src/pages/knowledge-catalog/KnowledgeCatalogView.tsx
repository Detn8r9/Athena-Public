import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";

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
}

interface RagSearchFilters {
  file_types?: string[];
  source_contains?: string;
  content_type?: string;
  tags?: string[];
}

const CATEGORIES: CatalogCategory[] = ["Medical", "Tactical", "Maps", "Tech", "History", "Local"];
const RAG_FILTERS_STORAGE_KEY = "athena-rag-search-filters-v1";

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
  const [sourceFilter, setSourceFilter] = useState("");
  const [tagFilter, setTagFilter] = useState("");
  const [contentTypeFilter, setContentTypeFilter] = useState("any");

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
    try {
      const raw = window.localStorage.getItem(RAG_FILTERS_STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as RagSearchFilters;
        setSourceFilter(parsed.source_contains ?? "");
        setContentTypeFilter(parsed.content_type ?? "any");
        setTagFilter(Array.isArray(parsed.tags) ? parsed.tags.join(", ") : "");
      }
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    const tags = tagFilter
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
    const payload: RagSearchFilters = {};
    if (sourceFilter.trim().length > 0) payload.source_contains = sourceFilter.trim();
    if (contentTypeFilter !== "any") {
      payload.content_type = contentTypeFilter;
      payload.file_types = [contentTypeFilter];
    }
    if (tags.length > 0) payload.tags = tags;
    try {
      window.localStorage.setItem(RAG_FILTERS_STORAGE_KEY, JSON.stringify(payload));
    } catch {
      // ignore
    }
  }, [sourceFilter, tagFilter, contentTypeFilter]);

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
          <div className="glass-panel" style={{ padding: "0.6rem", display: "grid", gap: "0.45rem" }}>
            <div style={{ color: "var(--primary)" }}>RAG Filters</div>
            <input
              type="text"
              value={sourceFilter}
              onChange={(e) => setSourceFilter(e.target.value)}
              placeholder="Source contains"
              className="map-chat-input"
            />
            <select
              className="theme-select"
              value={contentTypeFilter}
              onChange={(e) => setContentTypeFilter(e.target.value)}
            >
              <option value="any">Any file type</option>
              <option value="txt">TXT</option>
              <option value="md">MD</option>
              <option value="pdf">PDF</option>
              <option value="csv">CSV</option>
            </select>
            <input
              type="text"
              value={tagFilter}
              onChange={(e) => setTagFilter(e.target.value)}
              placeholder="Tags csv: tactical,field"
              className="map-chat-input"
            />
          </div>
        </aside>

        <main className="glass-panel" style={{ padding: "0.7rem", overflow: "auto" }}>
          <h3 style={{ margin: "0 0 0.65rem", color: "var(--primary)" }}>
            {activeCategory === "Local" ? "Local Documents" : "Knowledge Catalog"}
          </h3>
          {activeCategory === "Local" ? (
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))", gap: "0.7rem" }}>
              {localDocs.length === 0 ? (
                <div className="glass-panel" style={{ padding: "0.7rem" }}>
                  No local documents indexed yet.
                </div>
              ) : (
                localDocs.map((doc) => (
                  <article key={doc.id} className="glass-panel" style={{ padding: "0.7rem", display: "grid", gap: "0.4rem" }}>
                    <div className="glass-panel" style={{ padding: "0.5rem", color: "var(--primary)" }}>
                      {doc.file_type.toUpperCase()}
                    </div>
                    <div>{doc.filename}</div>
                    <div className="text-muted small">Chunks: {doc.total_chunks}</div>
                    <div className="text-muted small">{new Date(doc.ingested_at).toLocaleString()}</div>
                  </article>
                ))
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
