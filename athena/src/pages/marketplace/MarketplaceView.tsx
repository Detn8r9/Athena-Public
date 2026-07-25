import { useCallback, useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  ChevronRight,
  Download,
  ExternalLink,
  Github,
  Globe,
  Play,
  Search,
  Square,
  Store,
} from "lucide-react";

type CatalogEntry = {
  id: string;
  name: string;
  tagline?: string;
  description: string;
  developer?: string;
  kind: string;
  category: string;
  image: string;
  sizeBytesEstimate: number;
  primary: boolean;
  featured?: boolean;
  featuredRank?: number;
  iconGlyph?: string;
  iconColor?: string;
  websiteUrl?: string | null;
  githubUrl?: string | null;
  tags: string[];
};

type InstalledItem = {
  id: string;
  name: string;
  kind: string;
  hostPort: number;
  status: string;
};

type DockerAvailability = {
  available: boolean;
  version: string | null;
  installHint: string;
  installUrl: string | null;
};

type MarketplaceOverview = {
  docker: DockerAvailability;
  internetAllowed: boolean;
  catalog: CatalogEntry[];
  installed: InstalledItem[];
};

type InstallJob = {
  phase: string;
  message: string;
  percent: number | null;
  error?: string | null;
};

type StoreTab = "discover" | "apps" | "modules" | "library";

const CATEGORY_ORDER = [
  "All",
  "Media",
  "Voice",
  "Ops",
  "Automation",
  "Productivity",
  "Security",
  "Network",
  "Developer",
] as const;

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(0)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

function AppIcon({ entry, size = 64 }: { entry: CatalogEntry; size?: number }) {
  const bg = entry.iconColor?.trim() || "var(--primary)";
  const glyph = entry.iconGlyph?.trim() || (entry.kind === "module" ? "🧩" : "📦");
  return (
    <div
      className="store-app-icon"
      style={{
        width: size,
        height: size,
        fontSize: size * 0.42,
        background: `linear-gradient(145deg, ${bg} 0%, color-mix(in srgb, ${bg} 55%, #0a1210) 100%)`,
      }}
      aria-hidden
    >
      {glyph}
    </div>
  );
}

function GetButton({
  job,
  installed,
  busy,
  onGet,
  onOpen,
  onStart,
  onStop,
}: {
  job?: InstallJob;
  installed?: InstalledItem;
  busy?: boolean;
  onGet: () => void;
  onOpen: () => void;
  onStart: () => void;
  onStop: () => void;
}) {
  if (job && job.phase !== "done" && job.phase !== "error") {
    const pct = job.percent != null ? Math.max(0, Math.min(100, job.percent)) : null;
    return (
      <div className="store-get-progress" title={job.message} role="status" aria-live="polite">
        <svg className="store-get-ring" viewBox="0 0 36 36" aria-hidden>
          <circle className="store-get-ring-track" cx="18" cy="18" r="15.5" />
          <circle
            className={`store-get-ring-fill${pct == null ? " store-get-ring-fill--indeterminate" : ""}`}
            cx="18"
            cy="18"
            r="15.5"
            style={
              pct != null
                ? { strokeDasharray: `${(pct / 100) * 97.4} 97.4` }
                : undefined
            }
          />
        </svg>
        <span className="store-get-progress-label">
          {pct != null ? `${Math.round(pct)}%` : "…"}
        </span>
      </div>
    );
  }

  if (!installed || installed.status === "missing") {
    return (
      <button type="button" className="store-get-btn" disabled={busy} onClick={onGet}>
        <Download size={14} strokeWidth={2.5} aria-hidden />
        Get
      </button>
    );
  }

  return (
    <div className="store-get-actions">
      {installed.status === "running" ? (
        <button type="button" className="store-secondary-btn" disabled={busy} onClick={onStop}>
          <Square size={12} strokeWidth={2} aria-hidden /> Stop
        </button>
      ) : (
        <button type="button" className="store-get-btn" disabled={busy} onClick={onStart}>
          <Play size={14} strokeWidth={2.5} aria-hidden /> Start
        </button>
      )}
      <button type="button" className="store-open-btn" disabled={busy} onClick={onOpen}>
        Open
      </button>
    </div>
  );
}

type Props = {
  focusAppId?: string | null;
};

export function MarketplaceView({ focusAppId = null }: Props) {
  const [overview, setOverview] = useState<MarketplaceOverview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [jobs, setJobs] = useState<Record<string, InstallJob>>({});
  const [tab, setTab] = useState<StoreTab>("discover");
  const [category, setCategory] = useState<string>("All");
  const [query, setQuery] = useState("");
  const [detailId, setDetailId] = useState<string | null>(focusAppId);

  const reload = useCallback(async () => {
    setErr(null);
    try {
      const o = await invoke<MarketplaceOverview>("marketplace_overview");
      setOverview(o);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (focusAppId) {
      setDetailId(focusAppId);
      setTab("discover");
    }
  }, [focusAppId]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<InstallJob & { id: string }>("athena:marketplace-install-progress", (ev) => {
      const p = ev.payload;
      if (!p?.id) return;
      setJobs((prev) => ({
        ...prev,
        [p.id]: {
          phase: p.phase,
          message: p.message,
          percent: p.percent ?? null,
          error: p.error,
        },
      }));
      if (p.phase === "done") {
        void reload();
        window.setTimeout(() => {
          setJobs((prev) => {
            const next = { ...prev };
            delete next[p.id];
            return next;
          });
        }, 2500);
      }
      if (p.phase === "error") {
        setErr(p.error || p.message || "Install failed");
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
  }, [reload]);

  const installedById = useMemo(() => {
    const m = new Map<string, InstalledItem>();
    for (const i of overview?.installed ?? []) m.set(i.id, i);
    return m;
  }, [overview]);

  const catalog = overview?.catalog ?? [];

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return catalog.filter((c) => {
      if (tab === "apps" && c.kind !== "app") return false;
      if (tab === "modules" && c.kind !== "module") return false;
      if (tab === "library") {
        const inst = installedById.get(c.id);
        if (!inst || inst.status === "missing") return false;
      }
      if (category !== "All" && c.category !== category) return false;
      if (!q) return true;
      const hay = [
        c.name,
        c.tagline,
        c.description,
        c.developer,
        c.category,
        c.kind,
        ...(c.tags ?? []),
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return hay.includes(q);
    });
  }, [catalog, tab, category, query, installedById]);

  const featured = useMemo(() => {
    return [...catalog]
      .filter((c) => c.featured)
      .sort((a, b) => (a.featuredRank ?? 99) - (b.featuredRank ?? 99));
  }, [catalog]);

  const rails = useMemo(() => {
    const byCat = new Map<string, CatalogEntry[]>();
    for (const c of catalog) {
      if (!byCat.has(c.category)) byCat.set(c.category, []);
      byCat.get(c.category)!.push(c);
    }
    const voice = catalog.filter((c) => c.kind === "module");
    const essentials = catalog.filter((c) => c.primary);
    const list: Array<{ title: string; items: CatalogEntry[] }> = [
      { title: "Essentials", items: essentials },
      { title: "Voice & AI", items: voice },
    ];
    for (const cat of CATEGORY_ORDER) {
      if (cat === "All" || cat === "Voice") continue;
      const items = byCat.get(cat) ?? [];
      if (items.length) list.push({ title: cat, items });
    }
    return list.filter((r) => r.items.length > 0);
  }, [catalog]);

  const detail = detailId ? catalog.find((c) => c.id === detailId) ?? null : null;

  async function startInstall(id: string) {
    setErr(null);
    setJobs((prev) => ({
      ...prev,
      [id]: { phase: "validating", message: "Preparing download…", percent: 0 },
    }));
    try {
      await invoke("marketplace_install", { id });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setErr(msg);
      setJobs((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
    }
  }

  async function runAction(id: string, action: "start" | "stop" | "open") {
    setBusyId(id);
    setErr(null);
    try {
      if (action === "start") await invoke("marketplace_start", { id });
      else if (action === "stop") await invoke("marketplace_stop", { id });
      else {
        const url = await invoke<string>("marketplace_open_url", { id });
        await openUrl(url);
      }
      await reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  }

  function actionsFor(entry: CatalogEntry) {
    return (
      <GetButton
        job={jobs[entry.id]}
        installed={installedById.get(entry.id)}
        busy={busyId === entry.id}
        onGet={() => void startInstall(entry.id)}
        onOpen={() => void runAction(entry.id, "open")}
        onStart={() => void runAction(entry.id, "start")}
        onStop={() => void runAction(entry.id, "stop")}
      />
    );
  }

  if (!overview) {
    return (
      <div className="store-shell home-view-scroll">
        <div className="store-loading">
          <Store size={28} strokeWidth={2} aria-hidden />
          <p>{err ?? "Loading Marketplace…"}</p>
        </div>
      </div>
    );
  }

  if (!overview.docker.available) {
    return (
      <div className="store-shell home-view-scroll">
        <header className="store-topbar">
          <h1 className="store-brand">Marketplace</h1>
        </header>
        <div className="store-docker-needed glass-panel">
          <div className="store-hero-eyebrow">DOCKER REQUIRED</div>
          <h2>Install Docker to unlock the catalog</h2>
          <p>
            Core Athena still runs without Docker. Marketplace apps and voice modules need Docker on this
            machine.
          </p>
          <p className="text-muted">{overview.docker.installHint}</p>
          <div className="store-detail-links">
            {overview.docker.installUrl ? (
              <button type="button" className="store-get-btn" onClick={() => void openUrl(overview.docker.installUrl!)}>
                Open Docker install page
              </button>
            ) : null}
            <button type="button" className="store-secondary-btn" onClick={() => void reload()}>
              Retry detect
            </button>
          </div>
        </div>
      </div>
    );
  }

  /* ——— Product detail (App Store style) ——— */
  if (detail) {
    const inst = installedById.get(detail.id);
    const job = jobs[detail.id];
    return (
      <div className="store-shell home-view-scroll">
        <header className="store-topbar store-topbar--detail">
          <button type="button" className="store-back-btn" onClick={() => setDetailId(null)}>
            <ArrowLeft size={18} strokeWidth={2} aria-hidden />
            Store
          </button>
          <div className="store-search-wrap store-search-wrap--compact">
            <Search size={15} strokeWidth={2} aria-hidden />
            <input
              type="search"
              className="store-search-input"
              placeholder="Search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Search Marketplace"
            />
          </div>
        </header>

        {err ? (
          <p className="store-error" role="alert">
            {err}
          </p>
        ) : null}

        <article className="store-detail">
          <div className="store-detail-hero">
            <AppIcon entry={detail} size={112} />
            <div className="store-detail-hero-text">
              <h1 className="store-detail-title">{detail.name}</h1>
              <p className="store-detail-tagline">{detail.tagline || detail.developer || detail.category}</p>
              <p className="store-detail-dev">{detail.developer || "Athena catalog"}</p>
              <div className="store-detail-cta">{actionsFor(detail)}</div>
              {job && job.phase !== "done" ? (
                <p className="store-download-status" aria-live="polite">
                  <span className="store-download-status-phase">
                    {job.phase === "pull"
                      ? "Downloading"
                      : job.phase === "create"
                        ? "Installing"
                        : job.phase === "start"
                          ? "Starting"
                          : "Working"}
                  </span>
                  <span className="store-download-status-msg">{job.message}</span>
                </p>
              ) : null}
              {job?.phase === "error" ? (
                <p className="store-error" role="alert">
                  {job.error || job.message}
                </p>
              ) : null}
            </div>
          </div>

          <div className="store-detail-meta-strip">
            <div>
              <div className="store-meta-label">Category</div>
              <div className="store-meta-value">{detail.category}</div>
            </div>
            <div>
              <div className="store-meta-label">Size</div>
              <div className="store-meta-value">~{formatBytes(detail.sizeBytesEstimate || 0)}</div>
            </div>
            <div>
              <div className="store-meta-label">Type</div>
              <div className="store-meta-value">{detail.kind === "module" ? "Module" : "App"}</div>
            </div>
            <div>
              <div className="store-meta-label">Status</div>
              <div className="store-meta-value">{inst ? inst.status : "Not installed"}</div>
            </div>
          </div>

          <section className="store-detail-section">
            <h2>Description</h2>
            <p>{detail.description}</p>
          </section>

          <section className="store-detail-section">
            <h2>Information</h2>
            <dl className="store-info-list">
              <div>
                <dt>Developer</dt>
                <dd>{detail.developer || "—"}</dd>
              </div>
              <div>
                <dt>Category</dt>
                <dd>{detail.category}</dd>
              </div>
              <div>
                <dt>Compatibility</dt>
                <dd>Docker on this station</dd>
              </div>
              <div>
                <dt>Image</dt>
                <dd>
                  <code>{detail.image}</code>
                </dd>
              </div>
              {inst ? (
                <div>
                  <dt>Local URL</dt>
                  <dd>
                    <code>
                      http://127.0.0.1:{inst.hostPort}
                    </code>
                  </dd>
                </div>
              ) : null}
            </dl>
          </section>

          <section className="store-detail-section">
            <h2>Links</h2>
            <div className="store-detail-links">
              {detail.websiteUrl ? (
                <button type="button" className="store-link-btn" onClick={() => void openUrl(detail.websiteUrl!)}>
                  <Globe size={16} strokeWidth={2} aria-hidden />
                  Website
                  <ExternalLink size={14} strokeWidth={2} aria-hidden />
                </button>
              ) : null}
              {detail.githubUrl ? (
                <button type="button" className="store-link-btn" onClick={() => void openUrl(detail.githubUrl!)}>
                  <Github size={16} strokeWidth={2} aria-hidden />
                  GitHub
                  <ExternalLink size={14} strokeWidth={2} aria-hidden />
                </button>
              ) : null}
              {!detail.websiteUrl && !detail.githubUrl ? (
                <p className="text-muted small">No public links listed for this item.</p>
              ) : null}
            </div>
          </section>

          {(detail.tags?.length ?? 0) > 0 ? (
            <section className="store-detail-section">
              <h2>Tags</h2>
              <div className="store-tag-row">
                {detail.tags.map((t) => (
                  <span key={t} className="store-tag">
                    {t}
                  </span>
                ))}
              </div>
            </section>
          ) : null}
        </article>
      </div>
    );
  }

  /* ——— Discover / browse ——— */
  return (
    <div className="store-shell home-view-scroll">
      <header className="store-topbar">
        <div className="store-topbar-left">
          <h1 className="store-brand">
            <Store size={22} strokeWidth={2} aria-hidden />
            Marketplace
          </h1>
          <nav className="store-tabs" aria-label="Marketplace sections">
            {(
              [
                ["discover", "Discover"],
                ["apps", "Apps"],
                ["modules", "Modules"],
                ["library", "Library"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                className={`store-tab${tab === id ? " store-tab--active" : ""}`}
                onClick={() => {
                  setTab(id);
                  setCategory("All");
                }}
              >
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="store-topbar-right">
          <div className="store-search-wrap">
            <Search size={15} strokeWidth={2} aria-hidden />
            <input
              type="search"
              className="store-search-input"
              placeholder="Games, apps, and more"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Search Marketplace"
            />
          </div>
          <button type="button" className="store-secondary-btn" onClick={() => void reload()}>
            Refresh
          </button>
        </div>
      </header>

      <p className="store-status-line text-muted small">
        Docker {overview.docker.version ? `v${overview.docker.version}` : "ready"}
        {!overview.internetAllowed
          ? " · Offline — new downloads blocked"
          : " · Online — image pulls allowed"}
        {" · "}
        {catalog.length} items
      </p>

      {err ? (
        <p className="store-error" role="alert">
          {err}
        </p>
      ) : null}

      {Object.keys(jobs).length > 0 ? (
        <div className="store-active-downloads glass-panel" aria-live="polite">
          <strong>Downloads</strong>
          <ul>
            {Object.entries(jobs).map(([id, job]) => {
              const name = catalog.find((c) => c.id === id)?.name ?? id;
              return (
                <li key={id}>
                  <span>{name}</span>
                  <span className="text-muted small">
                    {job.percent != null ? `${Math.round(job.percent)}% · ` : ""}
                    {job.message}
                  </span>
                </li>
              );
            })}
          </ul>
        </div>
      ) : null}

      <div className="store-category-chips" role="tablist" aria-label="Categories">
        {CATEGORY_ORDER.map((cat) => (
          <button
            key={cat}
            type="button"
            role="tab"
            aria-selected={category === cat}
            className={`store-chip${category === cat ? " store-chip--active" : ""}`}
            onClick={() => setCategory(cat)}
          >
            {cat}
          </button>
        ))}
      </div>

      {tab === "discover" && !query.trim() && category === "All" ? (
        <>
          {featured[0] ? (
            <button
              type="button"
              className="store-featured glass-panel"
              onClick={() => setDetailId(featured[0].id)}
            >
              <div className="store-featured-copy">
                <div className="store-hero-eyebrow">FEATURED</div>
                <h2>{featured[0].name}</h2>
                <p>{featured[0].tagline || featured[0].description}</p>
                <span className="store-featured-cta">
                  View <ChevronRight size={16} strokeWidth={2} aria-hidden />
                </span>
              </div>
              <AppIcon entry={featured[0]} size={128} />
            </button>
          ) : null}

          {rails.map((rail) => (
            <section key={rail.title} className="store-rail" aria-label={rail.title}>
              <div className="store-rail-head">
                <h3>{rail.title}</h3>
              </div>
              <div className="store-rail-scroll">
                {rail.items.map((entry) => (
                  <div key={entry.id} className="store-card">
                    <button
                      type="button"
                      className="store-card-main"
                      onClick={() => setDetailId(entry.id)}
                    >
                      <AppIcon entry={entry} size={72} />
                      <div className="store-card-body">
                        <div className="store-card-name">{entry.name}</div>
                        <div className="store-card-sub text-muted">
                          {entry.tagline || entry.category}
                        </div>
                      </div>
                    </button>
                    <div className="store-card-get">{actionsFor(entry)}</div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </>
      ) : (
        <section className="store-grid-section" aria-label="Results">
          <h3 className="store-rail-head">
            {query.trim()
              ? `Results for “${query.trim()}”`
              : tab === "library"
                ? "Installed"
                : category === "All"
                  ? tab === "modules"
                    ? "All modules"
                    : "All apps"
                  : category}
          </h3>
          {filtered.length === 0 ? (
            <p className="text-muted">No matches. Try another search or category.</p>
          ) : (
            <div className="store-list">
              {filtered.map((entry) => (
                <div key={entry.id} className="store-list-row">
                  <button
                    type="button"
                    className="store-list-main"
                    onClick={() => setDetailId(entry.id)}
                  >
                    <AppIcon entry={entry} size={56} />
                    <div className="store-list-text">
                      <div className="store-card-name">{entry.name}</div>
                      <div className="store-card-sub text-muted">
                        {entry.developer ? `${entry.developer} · ` : ""}
                        {entry.category}
                        {" · "}~{formatBytes(entry.sizeBytesEstimate || 0)}
                      </div>
                    </div>
                    <ChevronRight size={18} strokeWidth={2} className="store-list-chevron" aria-hidden />
                  </button>
                  <div className="store-list-action">{actionsFor(entry)}</div>
                </div>
              ))}
            </div>
          )}
        </section>
      )}
    </div>
  );
}
