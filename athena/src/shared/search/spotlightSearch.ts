import type { AppView } from "../../app/appView";
import type { SettingsPanelId } from "../../pages/Settings/settingsSearchIndex";
import { SETTINGS_SEARCH_INDEX } from "../../pages/Settings/settingsSearchIndex";
import { athenaInvoke as invoke } from "../ipc/athenaIpc";

/** Deep-link / navigation payload for Spotlight results. */
export type SpotlightAction =
  | { type: "view"; view: AppView }
  | { type: "settings"; panel: SettingsPanelId }
  | { type: "personnel"; personId: string }
  | { type: "map"; markerId?: string; poiId?: string; label?: string }
  | { type: "inventory"; itemId?: string; query?: string }
  | { type: "chat"; chatId: string }
  | { type: "marketplace"; appId?: string }
  | { type: "web"; url: string; query: string };

export type SpotlightCategory =
  | "feature"
  | "settings"
  | "personnel"
  | "map"
  | "inventory"
  | "chat"
  | "marketplace"
  | "web";

export type SpotlightResult = {
  id: string;
  category: SpotlightCategory;
  title: string;
  subtitle?: string;
  score: number;
  action: SpotlightAction;
  /** Shown in UI as a small badge. */
  badge?: string;
};

export type SpotlightSearchOptions = {
  query: string;
  internetAllowed: boolean;
  /** Cap per category before merging. */
  limitPerCategory?: number;
  /** Overall result cap. */
  limit?: number;
};

const FEATURE_CATALOG: Array<{
  id: string;
  title: string;
  subtitle: string;
  keywords: string[];
  action: SpotlightAction;
}> = [
  {
    id: "feat-home",
    title: "Home",
    subtitle: "Station dashboard",
    keywords: ["home", "dashboard", "station", "start"],
    action: { type: "view", view: "home" },
  },
  {
    id: "feat-chat",
    title: "Chat",
    subtitle: "Local offline guide",
    keywords: ["chat", "ai", "assistant", "conversation"],
    action: { type: "view", view: "chat" },
  },
  {
    id: "feat-map",
    title: "Tactical Map",
    subtitle: "Markers, POIs, offline tiles",
    keywords: ["map", "tactical", "gps", "poi", "markers"],
    action: { type: "view", view: "map" },
  },
  {
    id: "feat-comms",
    title: "Communications Hub",
    subtitle: "Mesh and voice comms",
    keywords: ["comms", "radio", "communications", "hub"],
    action: { type: "view", view: "comms_hub" },
  },
  {
    id: "feat-mesh",
    title: "Mesh network",
    subtitle: "Peer mesh status and tools",
    keywords: ["mesh", "network", "peer", "lorawan"],
    action: { type: "view", view: "mesh" },
  },
  {
    id: "feat-radio",
    title: "Voice comms",
    subtitle: "Radio voice channel",
    keywords: ["radio", "voice", "ptt"],
    action: { type: "view", view: "radio" },
  },
  {
    id: "feat-logbook",
    title: "Field log & logistics",
    subtitle: "Inventory and field log",
    keywords: ["logbook", "logistics", "inventory", "quartermaster", "supply"],
    action: { type: "view", view: "logbook" },
  },
  {
    id: "feat-calendar",
    title: "Calendar",
    subtitle: "Events and schedule",
    keywords: ["calendar", "events", "schedule"],
    action: { type: "view", view: "calendar" },
  },
  {
    id: "feat-field-ops",
    title: "Field Ops",
    subtitle: "Guided protocols, patient charts, check-ins, duty roster, skills, newcomers, group memory",
    keywords: ["field ops", "protocol", "first aid", "bleeding", "patient", "chart", "vitals", "check-in", "roll call", "roster", "watch", "duty", "skills", "intake", "quarantine", "memory", "decision", "vote", "ledger", "barter"],
    action: { type: "view", view: "field_ops" },
  },
  {
    id: "feat-academy",
    title: "Academy",
    subtitle: "Training and curriculum",
    keywords: ["academy", "training", "learn", "curriculum"],
    action: { type: "view", view: "academy" },
  },
  {
    id: "feat-archive",
    title: "The Archive",
    subtitle: "Offline library",
    keywords: ["archive", "library", "zim", "kiwix", "books"],
    action: { type: "view", view: "archive" },
  },
  {
    id: "feat-data",
    title: "Data acquisition",
    subtitle: "Packages and downloads",
    keywords: ["data", "acquisition", "download", "packages"],
    action: { type: "view", view: "data_acquisition" },
  },
  {
    id: "feat-personnel",
    title: "Personnel",
    subtitle: "Roster and profiles",
    keywords: ["personnel", "roster", "people", "operators"],
    action: { type: "view", view: "personnel" },
  },
  {
    id: "feat-org",
    title: "Org chart",
    subtitle: "Hierarchy view",
    keywords: ["org", "chart", "hierarchy", "structure"],
    action: { type: "view", view: "org_chart" },
  },
  {
    id: "feat-marketplace",
    title: "Marketplace",
    subtitle: "Docker apps and voice modules",
    keywords: ["marketplace", "store", "docker", "jellyfin", "portainer", "apps"],
    action: { type: "view", view: "marketplace" },
  },
];

/** Simple fuzzy score: substring + token coverage. Higher is better. */
export function scoreMatch(haystack: string, query: string): number {
  const h = haystack.toLowerCase().trim();
  const q = query.toLowerCase().trim();
  if (!q || !h) return 0;
  if (h === q) return 100;
  if (h.startsWith(q)) return 90;
  if (h.includes(q)) return 70;
  const tokens = q.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return 0;
  let hit = 0;
  for (const t of tokens) {
    if (h.includes(t)) hit += 1;
  }
  if (hit === 0) return 0;
  return Math.round(40 * (hit / tokens.length) + (hit === tokens.length ? 15 : 0));
}

function bestScore(fields: string[], query: string): number {
  let best = 0;
  for (const f of fields) {
    best = Math.max(best, scoreMatch(f, query));
  }
  return best;
}

function matchFeatures(query: string, limit: number): SpotlightResult[] {
  const out: SpotlightResult[] = [];
  for (const f of FEATURE_CATALOG) {
    const score = bestScore([f.title, f.subtitle, ...f.keywords], query);
    if (score <= 0) continue;
    out.push({
      id: f.id,
      category: "feature",
      title: f.title,
      subtitle: f.subtitle,
      score,
      action: f.action,
      badge: "App",
    });
  }
  out.sort((a, b) => b.score - a.score);
  return out.slice(0, limit);
}

function matchSettings(query: string, limit: number): SpotlightResult[] {
  const out: SpotlightResult[] = [];
  for (const e of SETTINGS_SEARCH_INDEX) {
    // Prefer full Marketplace screen over Settings → Marketplace.
    if (e.panel === "marketplace") {
      const score = bestScore([e.title, e.subtitle ?? "", ...e.keywords, e.pathLabel], query);
      if (score <= 0) continue;
      out.push({
        id: `settings-${e.id}`,
        category: "feature",
        title: e.title,
        subtitle: "Open Marketplace",
        score,
        action: { type: "view", view: "marketplace" },
        badge: "App",
      });
      continue;
    }
    const score = bestScore([e.title, e.subtitle ?? "", ...e.keywords, e.pathLabel], query);
    if (score <= 0) continue;
    out.push({
      id: `settings-${e.id}`,
      category: "settings",
      title: e.title,
      subtitle: e.subtitle ? `${e.pathLabel} · ${e.subtitle}` : e.pathLabel,
      score,
      action: { type: "settings", panel: e.panel },
      badge: "Settings",
    });
  }
  out.sort((a, b) => b.score - a.score);
  return out.slice(0, limit);
}

type PersonnelRow = {
  id: string;
  name: string;
  display_name?: string | null;
  division?: string;
  rank_level?: number;
};

type InventoryRow = {
  id: string;
  item_name: string;
  category?: string;
  division?: string;
  quantity?: number;
};

type PoiRow = {
  id: string;
  name?: string;
  notes?: string;
};

type MarkerRow = {
  id: string;
  title?: string;
  description?: string;
  category?: string;
};

type ChatRow = {
  id: string;
  title: string;
  description?: string | null;
};

type CatalogEntry = {
  id: string;
  name: string;
  description: string;
  kind: string;
  tags?: string[];
};

async function matchPersonnel(query: string, limit: number): Promise<SpotlightResult[]> {
  try {
    const rows = await invoke<PersonnelRow[]>("list_personnel");
    const out: SpotlightResult[] = [];
    for (const p of rows ?? []) {
      const display = (p.display_name?.trim() || p.name || "").trim();
      const score = bestScore([display, p.name, p.division ?? "", String(p.rank_level ?? "")], query);
      if (score <= 0) continue;
      out.push({
        id: `personnel-${p.id}`,
        category: "personnel",
        title: display || p.name,
        subtitle: [p.division, p.rank_level != null ? `Rank ${p.rank_level}` : null].filter(Boolean).join(" · ") || "Personnel",
        score,
        action: { type: "personnel", personId: p.id },
        badge: "Person",
      });
    }
    out.sort((a, b) => b.score - a.score);
    return out.slice(0, limit);
  } catch {
    return [];
  }
}

async function matchInventory(query: string, limit: number): Promise<SpotlightResult[]> {
  try {
    const rows = await invoke<InventoryRow[]>("list_inventory");
    const out: SpotlightResult[] = [];
    for (const it of rows ?? []) {
      const score = bestScore([it.item_name, it.category ?? "", it.division ?? ""], query);
      if (score <= 0) continue;
      out.push({
        id: `inv-${it.id}`,
        category: "inventory",
        title: it.item_name,
        subtitle: [it.category, it.division, it.quantity != null ? `qty ${it.quantity}` : null]
          .filter(Boolean)
          .join(" · "),
        score,
        action: { type: "inventory", itemId: it.id, query: it.item_name },
        badge: "Supply",
      });
    }
    out.sort((a, b) => b.score - a.score);
    return out.slice(0, limit);
  } catch {
    return [];
  }
}

async function matchMap(query: string, limit: number): Promise<SpotlightResult[]> {
  const out: SpotlightResult[] = [];
  try {
    const pois = await invoke<PoiRow[]>("get_pois");
    for (const p of pois ?? []) {
      const name = (p.name || "POI").trim();
      const score = bestScore([name, p.notes ?? ""], query);
      if (score <= 0) continue;
      out.push({
        id: `poi-${p.id}`,
        category: "map",
        title: name,
        subtitle: "Point of interest",
        score,
        action: { type: "map", poiId: p.id, label: name },
        badge: "Map",
      });
    }
  } catch {
    /* ignore */
  }
  try {
    const markers = await invoke<MarkerRow[]>("get_map_markers");
    for (const m of markers ?? []) {
      const name = (m.title || "Marker").trim();
      const score = bestScore([name, m.description ?? "", m.category ?? ""], query);
      if (score <= 0) continue;
      out.push({
        id: `marker-${m.id}`,
        category: "map",
        title: name,
        subtitle: m.category ? `Marker · ${m.category}` : "Marker",
        score,
        action: { type: "map", markerId: m.id, label: name },
        badge: "Map",
      });
    }
  } catch {
    /* ignore */
  }
  out.sort((a, b) => b.score - a.score);
  return out.slice(0, limit);
}

async function matchChats(query: string, limit: number): Promise<SpotlightResult[]> {
  try {
    const rows = await invoke<ChatRow[]>("list_chats", {});
    const out: SpotlightResult[] = [];
    for (const c of rows ?? []) {
      const score = bestScore([c.title, c.description ?? ""], query);
      if (score <= 0) continue;
      out.push({
        id: `chat-${c.id}`,
        category: "chat",
        title: c.title || "Untitled chat",
        subtitle: c.description?.trim() || "Chat",
        score,
        action: { type: "chat", chatId: c.id },
        badge: "Chat",
      });
    }
    out.sort((a, b) => b.score - a.score);
    return out.slice(0, limit);
  } catch {
    return [];
  }
}

async function matchMarketplace(query: string, limit: number): Promise<SpotlightResult[]> {
  try {
    const overview = await invoke<{ catalog: CatalogEntry[] }>("marketplace_overview");
    const out: SpotlightResult[] = [];
    for (const e of overview?.catalog ?? []) {
      const score = bestScore([e.name, e.description, e.kind, ...(e.tags ?? [])], query);
      if (score <= 0) continue;
      out.push({
        id: `mkt-${e.id}`,
        category: "marketplace",
        title: e.name,
        subtitle: e.description,
        score,
        action: { type: "marketplace", appId: e.id },
        badge: e.kind === "module" ? "Module" : "Store",
      });
    }
    out.sort((a, b) => b.score - a.score);
    return out.slice(0, limit);
  } catch {
    return [];
  }
}

function webSearchResult(query: string): SpotlightResult {
  const q = query.trim();
  const url = `https://duckduckgo.com/?q=${encodeURIComponent(q)}`;
  return {
    id: "web-search",
    category: "web",
    title: `Search the web for “${q}”`,
    subtitle: "Opens in your computer’s browser",
    score: 55,
    action: { type: "web", url, query: q },
    badge: "Web",
  };
}

/**
 * Spotlight search: always local features/settings + local data.
 * When `internetAllowed`, also offers a native-browser web search action.
 */
export async function runSpotlightSearch(opts: SpotlightSearchOptions): Promise<SpotlightResult[]> {
  const query = opts.query.trim();
  if (!query) return [];
  const per = opts.limitPerCategory ?? 6;
  const limit = opts.limit ?? 24;

  const [personnel, inventory, map, chats, marketplace] = await Promise.all([
    matchPersonnel(query, per),
    matchInventory(query, per),
    matchMap(query, per),
    matchChats(query, per),
    matchMarketplace(query, per),
  ]);

  const merged = [
    ...matchFeatures(query, per),
    ...matchSettings(query, per),
    ...personnel,
    ...inventory,
    ...map,
    ...chats,
    ...marketplace,
  ];

  if (opts.internetAllowed) {
    merged.push(webSearchResult(query));
  }

  // Dedupe by id, keep highest score
  const byId = new Map<string, SpotlightResult>();
  for (const r of merged) {
    const prev = byId.get(r.id);
    if (!prev || r.score > prev.score) byId.set(r.id, r);
  }
  const list = Array.from(byId.values()).sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    // Prefer local navigation over web when tied
    if (a.category === "web") return 1;
    if (b.category === "web") return -1;
    return a.title.localeCompare(b.title);
  });
  return list.slice(0, limit);
}

export const SPOTLIGHT_STORAGE = {
  inventoryQuery: "athena-spotlight-inventory-query",
  inventoryItemId: "athena-spotlight-inventory-item",
} as const;
