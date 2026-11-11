import { useCallback, useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  Check,
  ChevronRight,
  Download,
  ExternalLink,
  Github,
  Globe,
  Play,
  RefreshCw,
  Search,
  Square,
  Store,
} from "lucide-react";
import {
  downloadOfflineMap,
  getOfflineMapManagerCatalog,
  listDownloadedOfflineMapIds,
  type MapDownloadCompleteEventPayload,
  type MapDownloadErrorEventPayload,
  type MapDownloadProgressEventPayload,
} from "../../domains/features/map/offlineMapManagerCatalog";
import {
  dockerEntryToStoreCatalog,
  mapEntriesToStoreCatalog,
  ollamaEntriesToStoreCatalog,
  storeSizeLabel,
  type StoreCatalogEntry,
} from "./marketplaceHubCatalog";

type DockerInstalledItem = {
  id: string;
  name: string;
  kind: string;
  hostPort: number;
  status: string;
  updateAvailable?: boolean;
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
  catalog: Array<{
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
  }>;
  installed: DockerInstalledItem[];
  autoUpdate: boolean;
};

type InstallJob = {
  phase: string;
  message: string;
  percent: number | null;
  error?: string | null;
};

type StoreTab = "discover" | "apps" | "modules" | "models" | "maps" | "library";

export type MarketplaceInitialTab = StoreTab;

const CATEGORY_ORDER = [
  "All",
  "Models",
  "Maps",
  "Media",
  "Voice",
  "Ops",
  "Automation",
  "Productivity",
  "Security",
  "Network",
  "Developer",
] as const;

function SizeBesideGet({ bytes }: { bytes: number }) {
  return <span className="store-item-size">{storeSizeLabel(bytes)}</span>;
}

function kindLabel(kind: StoreCatalogEntry["kind"]): string {
  if (kind === "module") return "Module";
  if (kind === "model") return "Model";
  if (kind === "map") return "Map";
  return "App";
}

function AppIcon({ entry, size = 64 }: { entry: StoreCatalogEntry; size?: number }) {
  const bg = entry.iconColor?.trim() || "var(--primary)";
  const glyph =
    entry.iconGlyph?.trim() ||
    (entry.kind === "module" ? "🧩" : entry.kind === "model" ? "🧠" : entry.kind === "map" ? "🗺️" : "📦");
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

function ProgressRing({ job }: { job: InstallJob }) {
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
          style={pct != null ? { strokeDasharray: `${(pct / 100) * 97.4} 97.4` } : undefined}
        />
      </svg>
      <span className="store-get-progress-label">{pct != null ? `${Math.round(pct)}%` : "…"}</span>
    </div>
  );
}

function DockerGetButton({
  job,
  installed,
  busy,
  onGet,
  onOpen,
  onStart,
  onStop,
  onUpdate,
}: {
  job?: InstallJob;
  installed?: DockerInstalledItem;
  busy?: boolean;
  onGet: () => void;
  onOpen: () => void;
  onStart: () => void;
  onStop: () => void;
  onUpdate: () => void;
}) {
  if (job && job.phase !== "done" && job.phase !== "error") {
    return <ProgressRing job={job} />;
  }

  if (!installed || installed.status === "missing") {
    return (
      <button type="button" className="store-get-btn" disabled={busy} onClick={onGet}>
        <Download size={14} strokeWidth={2.5} aria-hidden />
        Get
      </button>
    );
  }

  const showUpdate = Boolean(installed.updateAvailable);

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
      {showUpdate ? (
        <button
          type="button"
          className="store-secondary-btn"
          disabled={busy}
          onClick={onUpdate}
          title="Apply current catalog settings (keeps downloaded model cache)"
        >
          <RefreshCw size={12} strokeWidth={2} aria-hidden /> Update
        </button>
      ) : null}
      <button type="button" className="store-open-btn" disabled={busy} onClick={onOpen}>
        Open
      </button>
    </div>
  );
}

function HubAssetGetButton({
  job,
  installed,
  active,
  busy,
  onGet,
  onActivate,
}: {
  job?: InstallJob;
  installed: boolean;
  active?: boolean;
  busy?: boolean;
  onGet: () => void;
  onActivate?: () => void;
}) {
  if (job && job.phase !== "done" && job.phase !== "error") {
    return <ProgressRing job={job} />;
  }

  if (!installed) {
    return (
      <button type="button" className="store-get-btn" disabled={busy} onClick={onGet}>
        <Download size={14} strokeWidth={2.5} aria-hidden />
        Get
      </button>
    );
  }

  if (active) {
    return (
      <span className="store-installed-pill" title="Active for chat">
        <Check size={14} strokeWidth={2.5} aria-hidden />
        Active
      </span>
    );
  }

  if (onActivate) {
    return (
      <button type="button" className="store-open-btn" disabled={busy} onClick={onActivate}>
        Use
      </button>
    );
  }

  return (
    <span className="store-installed-pill">
      <Check size={14} strokeWidth={2.5} aria-hidden />
      Installed
    </span>
  );
}

type Props = {
  focusAppId?: string | null;
  initialTab?: MarketplaceInitialTab | null;
  initialActiveModel?: string;
  onActiveModelChange?: (model: string) => void;
  onModelsChanged?: (models: string[]) => void;
  showUnrestrictedCatalog?: boolean;
  isAdult?: boolean;
  onOpenDataAcquisitionMaps?: () => void;
  onOpenDataAcquisitionModels?: () => void;
};

export function MarketplaceView({
  focusAppId = null,
  initialTab = null,
  initialActiveModel = "",
  onActiveModelChange,
  onModelsChanged,
  showUnrestrictedCatalog = false,
  isAdult = false,
  onOpenDataAcquisitionMaps,
  onOpenDataAcquisitionModels,
}: Props) {
  const [overview, setOverview] = useState<MarketplaceOverview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [jobs, setJobs] = useState<Record<string, InstallJob>>({});
  const [tab, setTab] = useState<StoreTab>(() => initialTab ?? "discover");
  const [category, setCategory] = useState<string>("All");
  const [query, setQuery] = useState("");
  const [detailId, setDetailId] = useState<string | null>(focusAppId);
  const [modelEntries, setModelEntries] = useState<StoreCatalogEntry[]>([]);
  const [mapEntries, setMapEntries] = useState<StoreCatalogEntry[]>([]);
  const [localOllamaModels, setLocalOllamaModels] = useState<string[]>([]);
  const [downloadedMapIds, setDownloadedMapIds] = useState<Set<string>>(() => new Set());
  const [activeModel, setActiveModel] = useState(initialActiveModel);
  const [hubLoading, setHubLoading] = useState(true);

  const reloadDocker = useCallback(async () => {
    setErr(null);
    try {
      const o = await invoke<MarketplaceOverview>("marketplace_overview");
      setOverview(o);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const reloadHubAssets = useCallback(async () => {
    setHubLoading(true);
    try {
      const allowNsfw = Boolean(showUnrestrictedCatalog && isAdult);
      const [ollamaJson, localModels, mapCat, mapIds] = await Promise.all([
        fetch("/ollama-model-catalog.json")
          .then((r) => (r.ok ? r.json() : { entries: [] }))
          .catch(() => ({ entries: [] })),
        invoke<string[]>("get_local_models").catch(() => [] as string[]),
        getOfflineMapManagerCatalog().catch(() => []),
        listDownloadedOfflineMapIds().catch(() => [] as string[]),
      ]);
      const rawEntries = Array.isArray((ollamaJson as { entries?: unknown }).entries)
        ? (ollamaJson as { entries: Array<{ name: string; label: string; category: string; nsfw?: boolean }> }).entries
        : [];
      setModelEntries(ollamaEntriesToStoreCatalog(rawEntries, { allowNsfw }));
      const normalizedLocal = (Array.isArray(localModels) ? localModels : [])
        .map((m) => (typeof m === "string" ? m.trim() : ""))
        .filter(Boolean);
      setLocalOllamaModels(normalizedLocal);
      setMapEntries(mapEntriesToStoreCatalog(Array.isArray(mapCat) ? mapCat : []));
      setDownloadedMapIds(new Set((Array.isArray(mapIds) ? mapIds : []).map((x) => String(x).trim()).filter(Boolean)));
      return normalizedLocal;
    } finally {
      setHubLoading(false);
    }
  }, [showUnrestrictedCatalog, isAdult]);

  const reload = useCallback(async () => {
    await Promise.all([reloadDocker(), reloadHubAssets()]);
  }, [reloadDocker, reloadHubAssets]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    setActiveModel(initialActiveModel);
  }, [initialActiveModel]);

  useEffect(() => {
    if (!overview?.autoUpdate || !overview.docker.available) return;
    const outdated = overview.installed.some(
      (i) =>
        i.updateAvailable &&
        i.status !== "missing" &&
        !(jobs[i.id] && jobs[i.id].phase !== "done" && jobs[i.id].phase !== "error"),
    );
    if (!outdated) return;
    void invoke<number>("marketplace_tick_auto_updates").catch(() => {});
  }, [overview?.autoUpdate, overview?.docker.available, overview?.installed, jobs]);

  async function setAutoUpdate(enabled: boolean) {
    setErr(null);
    try {
      await invoke<boolean>("marketplace_set_auto_update", { enabled });
      await reloadDocker();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }

  useEffect(() => {
    if (focusAppId) {
      setDetailId(focusAppId);
      setTab("discover");
    }
  }, [focusAppId]);

  useEffect(() => {
    if (initialTab) {
      setTab(initialTab);
      setDetailId(null);
    }
  }, [initialTab]);

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
        void reloadDocker();
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
  }, [reloadDocker]);

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    void (async () => {
      const u1 = await listen<MapDownloadProgressEventPayload>("map_download_progress", (ev) => {
        const p = ev.payload;
        if (!p?.map_id) return;
        const id = `map:${p.map_id}`;
        const pct =
          p.total_bytes != null && p.total_bytes > 0
            ? Math.min(100, Math.round((p.bytes_downloaded / p.total_bytes) * 100))
            : null;
        setJobs((prev) => ({
          ...prev,
          [id]: {
            phase: "pull",
            message: "Downloading map archive…",
            percent: pct,
          },
        }));
      });
      const u2 = await listen<MapDownloadCompleteEventPayload>("map_download_complete", (ev) => {
        const p = ev.payload;
        if (!p?.map_id) return;
        const id = `map:${p.map_id}`;
        setBusyId((cur) => (cur === id ? null : cur));
        setDownloadedMapIds((prev) => new Set([...prev, p.map_id]));
        setJobs((prev) => ({
          ...prev,
          [id]: { phase: "done", message: "Installed", percent: 100 },
        }));
        window.setTimeout(() => {
          setJobs((prev) => {
            const next = { ...prev };
            delete next[id];
            return next;
          });
        }, 2000);
      });
      const u3 = await listen<MapDownloadErrorEventPayload>("map_download_error", (ev) => {
        const p = ev.payload;
        if (!p?.map_id) return;
        const id = `map:${p.map_id}`;
        setBusyId((cur) => (cur === id ? null : cur));
        setJobs((prev) => ({
          ...prev,
          [id]: { phase: "error", message: p.message || "Download failed", percent: null, error: p.message },
        }));
        setErr(p.message || "Map download failed");
      });
      const u4 = await listen("athena:offline-maps-updated", () => {
        void reloadHubAssets();
      });
      unsubs = [u1, u2, u3, u4];
    })();
    return () => {
      for (const u of unsubs) u();
    };
  }, [reloadHubAssets]);

  const dockerInstalledById = useMemo(() => {
    const m = new Map<string, DockerInstalledItem>();
    for (const i of overview?.installed ?? []) m.set(i.id, i);
    return m;
  }, [overview]);

  const catalog = useMemo(() => {
    const docker = (overview?.catalog ?? []).map(dockerEntryToStoreCatalog);
    return [...modelEntries, ...mapEntries, ...docker];
  }, [overview?.catalog, modelEntries, mapEntries]);

  const localModelSet = useMemo(() => new Set(localOllamaModels.map((m) => m.toLowerCase())), [localOllamaModels]);

  const isHubInstalled = useCallback(
    (entry: StoreCatalogEntry) => {
      if (entry.kind === "model" && entry.ollamaTag) {
        return localModelSet.has(entry.ollamaTag.toLowerCase());
      }
      if (entry.kind === "map" && entry.mapId) {
        return downloadedMapIds.has(entry.mapId);
      }
      return false;
    },
    [localModelSet, downloadedMapIds],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return catalog.filter((c) => {
      if (tab === "apps" && c.kind !== "app") return false;
      if (tab === "modules" && c.kind !== "module") return false;
      if (tab === "models" && c.kind !== "model") return false;
      if (tab === "maps" && c.kind !== "map") return false;
      if (tab === "library") {
        if (c.kind === "model" || c.kind === "map") {
          if (!isHubInstalled(c)) return false;
        } else {
          const inst = dockerInstalledById.get(c.id);
          if (!inst || inst.status === "missing") return false;
        }
      }
      if (category !== "All" && c.category !== category) return false;
      if (!q) return true;
      const hay = [c.name, c.tagline, c.description, c.developer, c.category, c.kind, ...(c.tags ?? [])]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return hay.includes(q);
    });
  }, [catalog, tab, category, query, dockerInstalledById, isHubInstalled]);

  const featured = useMemo(() => {
    return [...catalog]
      .filter((c) => c.featured)
      .sort((a, b) => (a.featuredRank ?? 99) - (b.featuredRank ?? 99));
  }, [catalog]);

  const rails = useMemo(() => {
    const models = catalog.filter((c) => c.kind === "model");
    const maps = catalog.filter((c) => c.kind === "map");
    const voice = catalog.filter((c) => c.kind === "module");
    const essentials = catalog.filter((c) => c.primary);
    const byCat = new Map<string, StoreCatalogEntry[]>();
    for (const c of catalog) {
      if (c.kind === "model" || c.kind === "map") continue;
      if (!byCat.has(c.category)) byCat.set(c.category, []);
      byCat.get(c.category)!.push(c);
    }
    const list: Array<{ title: string; items: StoreCatalogEntry[] }> = [
      { title: "Essentials", items: essentials },
      { title: "AI Models", items: models },
      { title: "Offline Maps", items: maps },
      { title: "Voice & AI", items: voice },
    ];
    for (const cat of CATEGORY_ORDER) {
      if (cat === "All" || cat === "Voice" || cat === "Models" || cat === "Maps") continue;
      const items = byCat.get(cat) ?? [];
      if (items.length) list.push({ title: cat, items });
    }
    return list.filter((r) => r.items.length > 0);
  }, [catalog]);

  const detail = detailId ? (catalog.find((c) => c.id === detailId) ?? null) : null;

  async function startDockerInstall(id: string) {
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

  async function startDockerUpdate(id: string) {
    setErr(null);
    setJobs((prev) => ({
      ...prev,
      [id]: { phase: "validating", message: "Preparing update…", percent: 0 },
    }));
    try {
      await invoke("marketplace_recreate", { id });
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

  async function runDockerAction(id: string, action: "start" | "stop" | "open") {
    setBusyId(id);
    setErr(null);
    try {
      if (action === "start") await invoke("marketplace_start", { id });
      else if (action === "stop") await invoke("marketplace_stop", { id });
      else {
        const url = await invoke<string>("marketplace_open_url", { id });
        await openUrl(url);
      }
      await reloadDocker();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  }

  async function pullModel(entry: StoreCatalogEntry) {
    const tag = entry.ollamaTag?.trim();
    if (!tag) return;
    setBusyId(entry.id);
    setErr(null);
    setJobs((prev) => ({
      ...prev,
      [entry.id]: { phase: "pull", message: `Pulling ${tag}…`, percent: null },
    }));
    try {
      const ok = await invoke<boolean>("pull_new_model", { modelName: tag });
      if (!ok) throw new Error(`Pull failed for ${tag}.`);
      setJobs((prev) => ({
        ...prev,
        [entry.id]: { phase: "done", message: "Installed", percent: 100 },
      }));
      const locals = await reloadHubAssets();
      onModelsChanged?.(locals ?? []);
      window.setTimeout(() => {
        setJobs((prev) => {
          const next = { ...prev };
          delete next[entry.id];
          return next;
        });
      }, 2000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setErr(msg);
      setJobs((prev) => ({
        ...prev,
        [entry.id]: { phase: "error", message: msg, percent: null, error: msg },
      }));
    } finally {
      setBusyId(null);
    }
  }

  async function activateModel(entry: StoreCatalogEntry) {
    const tag = entry.ollamaTag?.trim();
    if (!tag) return;
    setBusyId(entry.id);
    setErr(null);
    try {
      await invoke("set_explicit_ollama_model", { value: tag });
      setActiveModel(tag);
      onActiveModelChange?.(tag);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  }

  async function getMap(entry: StoreCatalogEntry) {
    if (!entry.mapId || !entry.mapDownloadUrl) return;
    setBusyId(entry.id);
    setErr(null);
    setJobs((prev) => ({
      ...prev,
      [entry.id]: { phase: "pull", message: "Downloading map archive…", percent: 0 },
    }));
    try {
      await downloadOfflineMap(entry.mapId, entry.mapDownloadUrl);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setErr(msg);
      setBusyId(null);
      setJobs((prev) => {
        const next = { ...prev };
        delete next[entry.id];
        return next;
      });
    }
  }

  function actionsFor(entry: StoreCatalogEntry) {
    if (entry.kind === "model") {
      const installed = isHubInstalled(entry);
      const active = Boolean(entry.ollamaTag && activeModel.trim() === entry.ollamaTag.trim());
      return (
        <HubAssetGetButton
          job={jobs[entry.id]}
          installed={installed}
          active={active}
          busy={busyId === entry.id}
          onGet={() => void pullModel(entry)}
          onActivate={installed ? () => void activateModel(entry) : undefined}
        />
      );
    }
    if (entry.kind === "map") {
      return (
        <HubAssetGetButton
          job={jobs[entry.id]}
          installed={isHubInstalled(entry)}
          busy={busyId === entry.id}
          onGet={() => void getMap(entry)}
        />
      );
    }
    return (
      <DockerGetButton
        job={jobs[entry.id]}
        installed={dockerInstalledById.get(entry.id)}
        busy={busyId === entry.id}
        onGet={() => void startDockerInstall(entry.id)}
        onOpen={() => void runDockerAction(entry.id, "open")}
        onStart={() => void runDockerAction(entry.id, "start")}
        onStop={() => void runDockerAction(entry.id, "stop")}
        onUpdate={() => void startDockerUpdate(entry.id)}
      />
    );
  }

  const dockerOk = Boolean(overview?.docker.available);
  const showDockerCatalog = dockerOk;
  const stillBooting = !overview && hubLoading && modelEntries.length === 0 && mapEntries.length === 0;

  if (stillBooting) {
    return (
      <div className="store-shell home-view-scroll">
        <div className="store-loading">
          <Store size={28} strokeWidth={2} aria-hidden />
          <p>{err ?? "Loading Marketplace…"}</p>
        </div>
      </div>
    );
  }

  /* ——— Product detail ——— */
  if (detail) {
    const isDocker = detail.kind === "app" || detail.kind === "module";
    const inst = isDocker ? dockerInstalledById.get(detail.id) : undefined;
    const job = jobs[detail.id];
    const hubInstalled = !isDocker && isHubInstalled(detail);
    const statusLabel = isDocker
      ? inst
        ? inst.status
        : "Not installed"
      : hubInstalled
        ? detail.kind === "model" && detail.ollamaTag === activeModel.trim()
          ? "Active"
          : "Installed"
        : "Not installed";

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
              {job && job.phase !== "done" && job.phase !== "error" ? (
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
              <div className="store-meta-value">{storeSizeLabel(detail.sizeBytesEstimate)}</div>
            </div>
            <div>
              <div className="store-meta-label">Type</div>
              <div className="store-meta-value">{kindLabel(detail.kind)}</div>
            </div>
            <div>
              <div className="store-meta-label">Status</div>
              <div className="store-meta-value">{statusLabel}</div>
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
                <dd>
                  {detail.kind === "model"
                    ? "Ollama on this station"
                    : detail.kind === "map"
                      ? "Athena offline maps folder"
                      : "Docker on this station"}
                </dd>
              </div>
              {detail.kind === "model" && detail.ollamaTag ? (
                <div>
                  <dt>Ollama tag</dt>
                  <dd>
                    <code>{detail.ollamaTag}</code>
                  </dd>
                </div>
              ) : null}
              {detail.kind === "map" && detail.mapType ? (
                <div>
                  <dt>Format</dt>
                  <dd>{detail.mapType.toUpperCase()}</dd>
                </div>
              ) : null}
              {isDocker ? (
                <div>
                  <dt>Image</dt>
                  <dd>
                    <code>{detail.image}</code>
                  </dd>
                </div>
              ) : null}
              {inst ? (
                <div>
                  <dt>Local URL</dt>
                  <dd>
                    <code>http://127.0.0.1:{inst.hostPort}</code>
                  </dd>
                </div>
              ) : null}
            </dl>
          </section>

          {detail.kind === "model" && onOpenDataAcquisitionModels ? (
            <section className="store-detail-section">
              <h2>GGUF weights</h2>
              <p className="text-muted small">
                Prefer raw Hugging Face GGUF files instead of Ollama? Queue them from Data Acquisition.
              </p>
              <button type="button" className="store-secondary-btn" onClick={onOpenDataAcquisitionModels}>
                Open Data Acquisition (Models)
              </button>
            </section>
          ) : null}

          {detail.kind === "map" && onOpenDataAcquisitionMaps ? (
            <section className="store-detail-section">
              <h2>Area extracts</h2>
              <p className="text-muted small">
                Need a custom radius extract or Kiwix regional pack? Continue in Data Acquisition.
              </p>
              <button type="button" className="store-secondary-btn" onClick={onOpenDataAcquisitionMaps}>
                Open Data Acquisition (Maps)
              </button>
            </section>
          ) : null}

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

  const showDiscoverRails = tab === "discover" && !query.trim() && category === "All";
  const dockerBlockedBrowse = (tab === "apps" || tab === "modules") && !showDockerCatalog;

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
                ["models", "Models"],
                ["maps", "Maps"],
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
                  setDetailId(null);
                }}
              >
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="store-topbar-right">
          {overview && dockerOk ? (
            <label className="store-auto-update" title="Recreate installed modules when catalog settings change">
              <input
                type="checkbox"
                checked={Boolean(overview.autoUpdate)}
                onChange={(e) => void setAutoUpdate(e.target.checked)}
              />
              <span>Auto-update</span>
            </label>
          ) : null}
          <div className="store-search-wrap">
            <Search size={15} strokeWidth={2} aria-hidden />
            <input
              type="search"
              className="store-search-input"
              placeholder="Models, maps, apps…"
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
        {overview
          ? `Docker ${overview.docker.version ? `v${overview.docker.version}` : dockerOk ? "ready" : "needed"}`
          : "Docker…"}
        {overview && !overview.internetAllowed
          ? " · Offline — new downloads blocked"
          : " · Online — downloads allowed"}
        {overview?.autoUpdate ? " · Auto-update on" : ""}
        {" · "}
        {catalog.length} items
        {modelEntries.length > 0 ? ` · ${modelEntries.length} models` : ""}
        {mapEntries.length > 0 ? ` · ${mapEntries.length} maps` : ""}
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

      {!showDockerCatalog && (tab === "discover" || tab === "apps" || tab === "modules") && overview ? (
        <div className="store-docker-needed glass-panel" style={{ marginBottom: "1rem" }}>
          <div className="store-hero-eyebrow">DOCKER OPTIONAL</div>
          <h2 style={{ marginTop: "0.35rem", fontSize: "1.1rem" }}>Apps &amp; modules need Docker</h2>
          <p className="text-muted small" style={{ marginBottom: "0.75rem" }}>
            Models and Maps still work. Install Docker to unlock the rest of the catalog.
          </p>
          <p className="text-muted small">{overview.docker.installHint}</p>
          <div className="store-detail-links">
            {overview.docker.installUrl ? (
              <button type="button" className="store-get-btn" onClick={() => void openUrl(overview.docker.installUrl!)}>
                Open Docker install page
              </button>
            ) : null}
            <button type="button" className="store-secondary-btn" onClick={() => void reloadDocker()}>
              Retry detect
            </button>
          </div>
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

      {dockerBlockedBrowse ? (
        <p className="text-muted">Install Docker to browse this section, or open Models / Maps.</p>
      ) : showDiscoverRails ? (
        <>
          {featured[0] ? (
            <button type="button" className="store-featured glass-panel" onClick={() => setDetailId(featured[0].id)}>
              <div className="store-featured-copy">
                <div className="store-hero-eyebrow">FEATURED</div>
                <h2>{featured[0].name}</h2>
                <p>{featured[0].tagline || featured[0].description}</p>
                <p className="store-featured-size">{storeSizeLabel(featured[0].sizeBytesEstimate)}</p>
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
                {rail.title === "AI Models" ? (
                  <button
                    type="button"
                    className="store-rail-see-all"
                    onClick={() => {
                      setTab("models");
                      setCategory("All");
                    }}
                  >
                    See All
                  </button>
                ) : null}
                {rail.title === "Offline Maps" ? (
                  <button
                    type="button"
                    className="store-rail-see-all"
                    onClick={() => {
                      setTab("maps");
                      setCategory("All");
                    }}
                  >
                    See All
                  </button>
                ) : null}
              </div>
              <div className="store-rail-scroll">
                {rail.items.map((entry) => (
                  <div key={entry.id} className="store-card">
                    <button type="button" className="store-card-main" onClick={() => setDetailId(entry.id)}>
                      <AppIcon entry={entry} size={72} />
                      <div className="store-card-body">
                        <div className="store-card-name">{entry.name}</div>
                        <div className="store-card-sub text-muted">{entry.tagline || entry.category}</div>
                      </div>
                    </button>
                    <div className="store-card-get">
                      <SizeBesideGet bytes={entry.sizeBytesEstimate} />
                      {actionsFor(entry)}
                    </div>
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
                    : tab === "models"
                      ? "All models"
                      : tab === "maps"
                        ? "All maps"
                        : tab === "apps"
                          ? "All apps"
                          : "All items"
                  : category}
          </h3>
          {tab === "models" && onOpenDataAcquisitionModels ? (
            <p className="text-muted small" style={{ margin: "0 0 0.85rem" }}>
              Ollama pulls install here.{" "}
              <button type="button" className="store-inline-link" onClick={onOpenDataAcquisitionModels}>
                Queue GGUF files in Data Acquisition
              </button>
            </p>
          ) : null}
          {tab === "maps" && onOpenDataAcquisitionMaps ? (
            <p className="text-muted small" style={{ margin: "0 0 0.85rem" }}>
              Regional archives install here.{" "}
              <button type="button" className="store-inline-link" onClick={onOpenDataAcquisitionMaps}>
                Custom extracts in Data Acquisition
              </button>
            </p>
          ) : null}
          {filtered.length === 0 ? (
            <p className="text-muted">No matches. Try another search or category.</p>
          ) : (
            <div className="store-list">
              {filtered.map((entry) => (
                <div key={entry.id} className="store-list-row">
                  <button type="button" className="store-list-main" onClick={() => setDetailId(entry.id)}>
                    <AppIcon entry={entry} size={56} />
                    <div className="store-list-text">
                      <div className="store-card-name">{entry.name}</div>
                      <div className="store-card-sub text-muted">
                        {entry.developer ? `${entry.developer} · ` : ""}
                        {entry.category}
                        {" · "}
                        {storeSizeLabel(entry.sizeBytesEstimate)}
                      </div>
                    </div>
                    <ChevronRight size={18} strokeWidth={2} className="store-list-chevron" aria-hidden />
                  </button>
                  <div className="store-list-action">
                    <SizeBesideGet bytes={entry.sizeBytesEstimate} />
                    {actionsFor(entry)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      )}
    </div>
  );
}
