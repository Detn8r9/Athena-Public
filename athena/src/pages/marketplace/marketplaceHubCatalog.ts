/**
 * Synthetic Marketplace catalog rows for Ollama models and offline map archives,
 * shaped like Docker store entries so Discover/rails/detail share one UI.
 */

export type StoreCatalogEntry = {
  id: string;
  name: string;
  tagline?: string;
  description: string;
  developer?: string;
  kind: "app" | "module" | "model" | "map";
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
  /** Ollama pull tag when kind === "model". */
  ollamaTag?: string;
  /** Offline map catalog id when kind === "map". */
  mapId?: string;
  mapDownloadUrl?: string;
  mapType?: string;
};

type OllamaCatalogJsonEntry = {
  name: string;
  label: string;
  category: string;
  nsfw?: boolean;
  sizeBytesEstimate?: number;
};

/** Approximate download sizes when the catalog omits an explicit estimate. */
const OLLAMA_SIZE_FALLBACKS: Record<string, number> = {
  "llama3.2:3b": 2_000_000_000,
  "mistral:7b": 4_100_000_000,
  "qwen2.5:7b": 4_700_000_000,
  "dolphin-mixtral:8x7b": 26_000_000_000,
};

export function formatStoreSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "Size varies";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(0)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : mb.toFixed(0)} MB`;
  const gb = mb / 1024;
  return `${gb < 10 ? gb.toFixed(1) : gb.toFixed(0)} GB`;
}

export function storeSizeLabel(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "Size varies";
  return `~${formatStoreSize(bytes)}`;
}

type MapCatalogLike = {
  id: string;
  name: string;
  region: string;
  type: string;
  size_mb: number;
  download_url: string;
};

const MODEL_ICON_COLORS: Record<string, string> = {
  general: "#3d8bfd",
  coding: "#22c55e",
  unrestricted: "#f97316",
  default: "#8b5cf6",
};

const MODEL_GLYPHS: Record<string, string> = {
  general: "🧠",
  coding: "⌨️",
  unrestricted: "🔓",
  default: "✨",
};

export function ollamaEntriesToStoreCatalog(
  entries: OllamaCatalogJsonEntry[],
  opts: { allowNsfw: boolean },
): StoreCatalogEntry[] {
  const out: StoreCatalogEntry[] = [];
  let featuredRank = 1;
  for (const e of entries) {
    if (!e?.name?.trim()) continue;
    if (e.nsfw && !opts.allowNsfw) continue;
    const cat = (e.category || "general").toLowerCase();
    const tag = e.name.trim();
    const primary = cat === "general" || featuredRank <= 2;
    const featured = tag === "llama3.2:3b" || tag.startsWith("llama3.2");
    const sizeFromCatalog =
      typeof e.sizeBytesEstimate === "number" && Number.isFinite(e.sizeBytesEstimate) && e.sizeBytesEstimate > 0
        ? e.sizeBytesEstimate
        : 0;
    const sizeBytesEstimate = sizeFromCatalog || OLLAMA_SIZE_FALLBACKS[tag] || 0;
    out.push({
      id: `model:${tag}`,
      name: e.label?.trim() || tag,
      tagline: `Ollama · ${tag} · ${storeSizeLabel(sizeBytesEstimate)}`,
      description: `Pull and run ${e.label || tag} locally through Ollama. Used for Chat, Conversation, and system routing once activated in Settings.`,
      developer: "Ollama",
      kind: "model",
      category: "Models",
      image: tag,
      sizeBytesEstimate,
      primary,
      featured,
      featuredRank: featured ? featuredRank++ : undefined,
      iconGlyph: MODEL_GLYPHS[cat] ?? MODEL_GLYPHS.default,
      iconColor: MODEL_ICON_COLORS[cat] ?? MODEL_ICON_COLORS.default,
      websiteUrl: "https://ollama.com/library",
      githubUrl: null,
      tags: ["ollama", "ai", cat, ...(e.nsfw ? ["nsfw"] : [])],
      ollamaTag: tag,
    });
  }
  return out;
}

export function mapEntriesToStoreCatalog(entries: MapCatalogLike[]): StoreCatalogEntry[] {
  return entries.map((e, i) => {
    const typeUpper = (e.type || "pmtiles").toUpperCase();
    const sizeBytes = Math.max(0, Math.round((e.size_mb || 0) * 1024 * 1024));
    return {
      id: `map:${e.id}`,
      name: e.name,
      tagline: `${e.region} · ${typeUpper} · ${storeSizeLabel(sizeBytes)}`,
      description: `Offline ${typeUpper} archive for ${e.region}. Downloads into Athena’s maps folder for use on the tactical map without a network.`,
      developer: "Athena Maps",
      kind: "map" as const,
      category: "Maps",
      image: e.id,
      sizeBytesEstimate: sizeBytes,
      primary: i < 3,
      featured: i === 0,
      featuredRank: i === 0 ? 2 : undefined,
      iconGlyph: typeUpper.includes("MB") ? "🛰️" : "🗺️",
      iconColor: "#0ea5e9",
      websiteUrl: null,
      githubUrl: null,
      tags: ["maps", "offline", e.region, e.type].filter(Boolean),
      mapId: e.id,
      mapDownloadUrl: e.download_url,
      mapType: e.type,
    };
  });
}

export function dockerEntryToStoreCatalog(raw: {
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
}): StoreCatalogEntry {
  const kind = raw.kind === "module" ? "module" : "app";
  return {
    ...raw,
    kind,
    tags: Array.isArray(raw.tags) ? raw.tags : [],
  };
}
