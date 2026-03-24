import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { PDFDocument, StandardFonts, rgb } from "pdf-lib";
import { TRIAGE_ITEMS, getTriageItem, TriageId } from "./triageData";
import { HelpModal } from "./HelpModal";
import { MeshRadioModal, type MeshRadioMessage } from "./MeshRadioModal";
import { MapView, type MapBasemap, type TacticalMapMeshNodeLocation, type TacticalMapPointOfInterest } from "./MapView";
import { defaultTacticalLayerVisibilityState, type TacticalLayerVisibilityState } from "./TacticalSidebar";
import { ErrorBoundary } from "./ErrorBoundary";
import { OfflineMapManagerDashboard } from "./OfflineMapManagerDashboard";
import { registerPmtilesMaplibreProtocolOnce } from "./maplibrePmtilesProtocolRegistration";
import { evaluateMainChatWatchdog } from "./streamWatchdog";
import { appendMeshMessageCapped } from "./meshMessageLog";
import { PhotoshopColorPicker } from "./PhotoshopColorPicker";
import { hexToRgbCsv } from "./colorUtils";
import type { AppView } from "./appView";
import { APP_VIEW_LABELS, APP_VIEWS, readStartupView, saveStartupView } from "./appView";
import { UniversalHeader } from "./UniversalHeader";
import { DashboardView, type DashboardDestination } from "./DashboardView";
import { usePageVisible } from "./usePageVisible";
import { LogbookView } from "./LogbookView";
import { AiModelManager } from "./AiModelManager";
import { RadioView } from "./RadioView";
import { RadioSettingsView } from "./RadioSettingsView";
import { KnowledgeCatalogView } from "./KnowledgeCatalogView";

type UserAndInferenceSettings = {
  user_display_name: string | null;
  user_birthdate_iso: string | null;
  allow_nsfw_unrestricted_models: boolean;
  is_at_least_18: boolean;
  is_birthday_today: boolean;
  inference_backend: string | null;
  explicit_ollama_model: string | null;
  openai_compatible_base_url: string | null;
  openai_compatible_model: string | null;
  openai_compatible_api_key_set: boolean;
};

registerPmtilesMaplibreProtocolOnce();

const ATHENA_TACTICAL_MAP_LAYER_VISIBILITY_KEY = "athena-tactical-map-layer-visibility-v3";

function readTacticalMapLayerVisibilityFromStorage(): TacticalLayerVisibilityState {
  if (typeof window === "undefined" || typeof window.localStorage === "undefined") {
    return defaultTacticalLayerVisibilityState;
  }
  try {
    let raw = window.localStorage.getItem(ATHENA_TACTICAL_MAP_LAYER_VISIBILITY_KEY);
    if (!raw) {
      raw = window.localStorage.getItem("athena-tactical-map-layer-visibility-v2");
    }
    if (!raw) {
      return defaultTacticalLayerVisibilityState;
    }
    const parsed = JSON.parse(raw) as Partial<TacticalLayerVisibilityState>;
    return {
      activeFamily: parsed.activeFamily ?? defaultTacticalLayerVisibilityState.activeFamily,
      network: {
        nodes: parsed.network?.nodes ?? defaultTacticalLayerVisibilityState.network.nodes,
        coverageCircles:
          parsed.network?.coverageCircles ?? defaultTacticalLayerVisibilityState.network.coverageCircles,
        links: parsed.network?.links ?? defaultTacticalLayerVisibilityState.network.links,
      },
      resources: {
        power: parsed.resources?.power ?? defaultTacticalLayerVisibilityState.resources.power,
        water: parsed.resources?.water ?? defaultTacticalLayerVisibilityState.resources.water,
        food: parsed.resources?.food ?? defaultTacticalLayerVisibilityState.resources.food,
        equipment:
          parsed.resources?.equipment ?? defaultTacticalLayerVisibilityState.resources.equipment,
      },
      customPins: parsed.customPins ?? defaultTacticalLayerVisibilityState.customPins,
    };
  } catch {
    return defaultTacticalLayerVisibilityState;
  }
}

interface IndexStats {
  total_files: number;
  text_count: number;
  image_count: number;
  video_count: number;
  map_count: number;
  zim_count: number;
}

interface SystemSpecs {
  ram_gb: number;
  cpu_cores: number;
  recommended_tier: string;
  /** From backend: scales with detected RAM */
  max_memory_messages: number;
}

interface MemoryEstimate {
  memory_messages: number;
  model_name: string;
  model_size_b: number;
  estimated_model_ram_gb: number;
  estimated_context_tokens: number;
  estimated_context_ram_mb: number;
  total_estimated_ram_gb: number;
}

interface OllamaSetupStatus {
  http_reachable: boolean;
  sidecar_child_active: boolean;
  managed_binary_exists: boolean;
  managed_binary_path: string;
  configured_cli_path: string | null;
  detected_ollama_paths: string[];
  unconventional_ollama_path: string | null;
}

interface BatteryState {
  percentage: number | null;
  is_charging: boolean | null;
  time_to_empty_minutes?: number | null;
}

const MODEL_TIERS = ["auto", "small", "medium", "large", "xlarge"] as const;
type ModelTier = (typeof MODEL_TIERS)[number];

const MODEL_TIER_LABELS: Record<ModelTier, string> = {
  auto: "Automatic (recommended)",
  small: "Small",
  medium: "Medium",
  large: "Large (e.g. 13B)",
  xlarge: "Extra large (34B+)",
};

/** External resource links for finding survival, technical, and reference content to ingest.
 *  Order of categories below is the display order (basics first for new users). */
const RESOURCE_LINKS: Array<{ category: string; label: string; url: string }> = [
  // 1. Survival & first aid (immediate life-safety)
  { category: "Survival & first aid", label: "Ready.gov (FEMA)", url: "https://www.ready.gov" },
  { category: "Survival & first aid", label: "CDC Emergency Preparedness", url: "https://www.cdc.gov/preparedness/" },
  { category: "Survival & first aid", label: "American Red Cross — Prepare", url: "https://www.redcross.org/get-help/how-to-prepare-for-emergencies.html" },
  { category: "Survival & first aid", label: "Hesperian Health Guides", url: "https://hesperian.org/" },
  { category: "Survival & first aid", label: "Internet Archive: Survival & preparedness", url: "https://archive.org/search?query=survival+preparedness" },
  { category: "Survival & first aid", label: "Wikipedia: Survival skills", url: "https://en.wikipedia.org/wiki/Survival_skills" },
  { category: "Survival & first aid", label: "Wikipedia: First aid", url: "https://en.wikipedia.org/wiki/First_aid" },
  // 2. Food & water
  { category: "Food & water", label: "Internet Archive: Agriculture & food", url: "https://archive.org/details/folkscanomy_food" },
  { category: "Food & water", label: "Internet Archive: Permaculture", url: "https://archive.org/search?query=permaculture" },
  { category: "Food & water", label: "Internet Archive: Homesteading", url: "https://archive.org/search?query=homesteading" },
  { category: "Food & water", label: "Internet Archive: Food storage", url: "https://archive.org/search?query=food+storage+preservation" },
  { category: "Food & water", label: "Internet Archive: Gardening", url: "https://archive.org/search?query=gardening+vegetable" },
  { category: "Food & water", label: "Wikipedia: Sustainable agriculture", url: "https://en.wikipedia.org/wiki/Sustainable_agriculture" },
  { category: "Food & water", label: "Wikipedia: Water purification", url: "https://en.wikipedia.org/wiki/Water_purification" },
  { category: "Food & water", label: "Wikipedia: Food preservation", url: "https://en.wikipedia.org/wiki/Food_preservation" },
  // 3. Shelter
  { category: "Shelter", label: "Internet Archive: Shelter & construction", url: "https://archive.org/search?query=shelter+construction+building" },
  { category: "Shelter", label: "Internet Archive: Building manual", url: "https://archive.org/search?query=building+manual+construction" },
  { category: "Shelter", label: "Wikipedia: Emergency shelter", url: "https://en.wikipedia.org/wiki/Emergency_shelter" },
  { category: "Shelter", label: "Wikipedia: Earth shelter", url: "https://en.wikipedia.org/wiki/Earth_sheltering" },
  { category: "Shelter", label: "Wikipedia: Off-the-grid", url: "https://en.wikipedia.org/wiki/Off-the-grid" },
  { category: "Shelter", label: "Internet Archive: Heating & wood stove", url: "https://archive.org/search?query=wood+stove+heating" },
  // 4. Energy independence
  { category: "Energy independence", label: "NREL (solar, batteries, off-grid)", url: "https://www.nrel.gov" },
  { category: "Energy independence", label: "DOE — Renewable energy", url: "https://www.energy.gov/eere/renewable-energy" },
  { category: "Energy independence", label: "Internet Archive: Solar power", url: "https://archive.org/search?query=solar+power+off+grid" },
  { category: "Energy independence", label: "Internet Archive: Battery bank", url: "https://archive.org/search?query=battery+bank+storage" },
  { category: "Energy independence", label: "Internet Archive: Generator", url: "https://archive.org/search?query=generator+electric" },
  { category: "Energy independence", label: "Internet Archive: Inverter", url: "https://archive.org/search?query=inverter+power" },
  { category: "Energy independence", label: "Wikipedia: Solar panel", url: "https://en.wikipedia.org/wiki/Solar_panel" },
  { category: "Energy independence", label: "Wikipedia: Charge controller", url: "https://en.wikipedia.org/wiki/Charge_controller" },
  { category: "Energy independence", label: "Wikipedia: Lead–acid battery", url: "https://en.wikipedia.org/wiki/Lead%E2%80%93acid_battery" },
  // 5. Communications & mesh
  { category: "Communications & mesh", label: "Meshtastic (LoRa mesh)", url: "https://meshtastic.org" },
  { category: "Communications & mesh", label: "Meshtastic docs", url: "https://meshtastic.org/docs/" },
  { category: "Communications & mesh", label: "ARRL (amateur radio)", url: "https://www.arrl.org" },
  { category: "Communications & mesh", label: "Wikipedia: Mesh networking", url: "https://en.wikipedia.org/wiki/Mesh_networking" },
  { category: "Communications & mesh", label: "Wikipedia: Amateur radio", url: "https://en.wikipedia.org/wiki/Amateur_radio" },
  { category: "Communications & mesh", label: "Wikipedia: APRS", url: "https://en.wikipedia.org/wiki/Automatic_Packet_Reporting_System" },
  { category: "Communications & mesh", label: "Internet Archive: Amateur radio", url: "https://archive.org/search?query=amateur+radio" },
  { category: "Communications & mesh", label: "Internet Archive: Emergency communications", url: "https://archive.org/search?query=emergency+communications" },
  { category: "Communications & mesh", label: "Wikipedia: LoRa", url: "https://en.wikipedia.org/wiki/LoRa" },
  // 6. Vehicle & equipment
  { category: "Vehicle & equipment", label: "Internet Archive: Vehicle repair", url: "https://archive.org/search?query=vehicle+repair+manual" },
  { category: "Vehicle & equipment", label: "Internet Archive: Auto service manual", url: "https://archive.org/search?query=auto+service+manual" },
  { category: "Vehicle & equipment", label: "Internet Archive: Small engine", url: "https://archive.org/search?query=small+engine+repair" },
  { category: "Vehicle & equipment", label: "Internet Archive: Tractor manual", url: "https://archive.org/search?query=tractor+manual" },
  { category: "Vehicle & equipment", label: "ManualsLib (auto & equipment)", url: "https://www.manualslib.com" },
  { category: "Vehicle & equipment", label: "Wikipedia: Automotive maintenance", url: "https://en.wikipedia.org/wiki/Automotive_maintenance" },
  { category: "Vehicle & equipment", label: "Wikipedia: Parts interchange", url: "https://en.wikipedia.org/wiki/Interchangeable_parts" },
  { category: "Vehicle & equipment", label: "Internet Archive: Chilton Haynes", url: "https://archive.org/search?query=chilton+haynes+manual" },
  { category: "Vehicle & equipment", label: "Internet Archive: Diesel generator", url: "https://archive.org/search?query=diesel+generator+manual" },
  // 7. Technical documentation
  { category: "Technical documentation", label: "iFixit (repair guides)", url: "https://www.ifixit.com" },
  { category: "Technical documentation", label: "ManualsLib (manuals)", url: "https://www.manualslib.com" },
  { category: "Technical documentation", label: "Internet Archive: Manuals", url: "https://archive.org/details/manuals" },
  { category: "Technical documentation", label: "McMaster-Carr (parts catalog)", url: "https://www.mcmaster.com" },
  { category: "Technical documentation", label: "Grainger (industrial supplies)", url: "https://www.grainger.com" },
  { category: "Technical documentation", label: "Kiwix Library (offline ZIM)", url: "https://library.kiwix.org" },
  { category: "Technical documentation", label: "Wikipedia: Repair manual", url: "https://en.wikipedia.org/wiki/Repair_manual" },
  // 8. Societal rebuilding (long-term)
  { category: "Societal rebuilding", label: "Wikipedia: Disaster recovery", url: "https://en.wikipedia.org/wiki/Disaster_recovery" },
  { category: "Societal rebuilding", label: "Internet Archive: Community development", url: "https://archive.org/search?query=community+development" },
  { category: "Societal rebuilding", label: "Wikipedia: Resilient community", url: "https://en.wikipedia.org/wiki/Community_resilience" },
  // 9. Weapons & manufacturing
  { category: "Weapons & manufacturing", label: "Internet Archive: Military manuals", url: "https://archive.org/details/militarymanuals" },
  { category: "Weapons & manufacturing", label: "Wikipedia: Gunsmithing", url: "https://en.wikipedia.org/wiki/Gunsmithing" },
  { category: "Weapons & manufacturing", label: "Wikipedia: Machining", url: "https://en.wikipedia.org/wiki/Machining" },
  { category: "Weapons & manufacturing", label: "Internet Archive: Machining", url: "https://archive.org/search?query=machining+manual" },
  { category: "Weapons & manufacturing", label: "Wikipedia: Welding", url: "https://en.wikipedia.org/wiki/Welding" },
];

interface DiscoveredZim {
  path: string;
  name: string;
  size_bytes: number;
  size_label: string;
}

interface LibraryFile {
  path: string;
  name: string;
  size_bytes: number;
}

interface RagIngestionProgressPayload {
  file_path: string;
  filename: string;
  completed: boolean;
  chunks: number;
  document_id?: string | null;
  error?: string | null;
}

interface MapCatalogEntry {
  id: string;
  name: string;
  description: string;
  region: string;
  style: string;
  url: string;
  size_bytes: number;
  size_label: string;
  for_user_region: boolean;
}

interface DownloadedMap {
  path: string;
  name: string;
  region: string;
  style: string;
}

function formatLibraryFileSizeBytes(sizeBytes: number): string {
  if (sizeBytes >= 1_000_000_000) return `${(sizeBytes / 1_000_000_000).toFixed(1)} GB`;
  if (sizeBytes >= 1_000_000) return `${(sizeBytes / 1_000_000).toFixed(1)} MB`;
  return `${(sizeBytes / 1_000).toFixed(1)} KB`;
}

/** Kiwix regional slippy-map archives (e.g. maps_en_north-america_*.zim). */
function isLikelyKiwixMapZimFilename(name: string): boolean {
  const n = name.toLowerCase();
  return n.endsWith(".zim") && n.startsWith("maps_");
}

interface TilePackInfo {
  id: string;
  name: string;
  source: string;
  root_path: string;
  min_zoom: number;
  max_zoom: number;
  center_lat: number;
  center_lon: number;
  radius_miles: number;
}

interface OfflineBasemapPaths {
  osm: string | null;
  opentopo: string | null;
  imported: string | null;
}

interface TilePackEstimate {
  count: number;
  max_tiles: number;
  within_limit: boolean;
}

function tilePackSourceLabel(source: string): string {
  switch (source) {
    case "opentopo":
      return "OpenTopoMap";
    case "mbtiles":
      return "Imported MBTiles";
    case "pmtiles":
      return "Imported PMTiles";
    default:
      return "OpenStreetMap";
  }
}

function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${Math.round(n)} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function isTileCountProgressJob(id: string): boolean {
  return (
    id.startsWith("tiles:") || id.startsWith("import:") || id.startsWith("mbtiles_export:")
  );
}

interface Poi {
  id: string;
  name: string;
  lat: number;
  lon: number;
  notes: string;
}

interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  /** Optional timestamp (ms) when message was added; used for display */
  timestamp?: number;
  /** Library doc names used to answer (citations) */
  sources?: string[];
}

interface RagSearchChunk {
  chunk_id: string;
  document_id: string;
  chunk_index: number;
  content: string;
  score: number;
  source?: string;
  content_type?: string;
}

interface RagSearchFilters {
  file_types?: string[];
  source_contains?: string;
  content_type?: string;
  tags?: string[];
}

interface RadioLogEntry {
  id: string;
  direction: string;
  message: string;
  timestamp: string;
}

interface ChatSummary {
  id: string;
  title: string;
  description?: string;
  updated_at: number;
}

interface SavedChat {
  id: string;
  title: string;
  messages: Array<{ role: string; content: string }>;
  created_at: number;
  updated_at: number;
}

interface BackendCustomTheme {
  id: string;
  name: string;
  base: string;
  bg_color?: string | null;
  primary_color?: string | null;
  text_color?: string | null;
  accent_color?: string | null;
  font_family?: string | null;
  personality_id?: string | null;
  scanlines_enabled?: boolean | null;
  scanlines_opacity?: number | null;
  vignette_enabled?: boolean | null;
  film_grain?: number | null;
  sharp_corners?: boolean | null;
}

interface SunTimes {
  sunrise_iso: string | null;
  sunset_iso: string | null;
}

type NightVisionMode = "auto" | "on" | "off";

const PERSONAS = ["default", "fallout", "fieldguide"] as const;
type PersonalityId = (typeof PERSONAS)[number];

const PERSONALITY_LABELS: Record<PersonalityId, string> = {
  default: "Calm survival guide",
  fallout: "Fallout terminal (dry, retro)",
  fieldguide: "Old survival manual (terse, instructional)",
};

const VOICE_PRESETS = ["default", "fallout", "ultron"] as const;
type VoicePresetId = (typeof VOICE_PRESETS)[number];
const VOICE_PRESET_LABELS: Record<VoicePresetId, string> = {
  default: "Default (system voice)",
  fallout: "Fallout (1950s + static)",
  ultron: "Ultron (deep, calm)",
};

/** When true, voice (TTS) and microphone (conversation / speak-to-type) UI are hidden. Code remains in place. */
const VOICE_AND_MIC_FEATURES_HIDDEN = true;

const SURVIVAL_TIPS = [
  "In cold weather, keep your head covered—you lose a lot of heat through your scalp.",
  "When lost, stay put if you have shelter and water; moving without a plan can make rescue harder.",
  "A simple way to purify water is to boil it for at least one minute at a rolling boil.",
  "Duct tape can patch gear, secure splints, and even help with blisters—keep a small roll in your kit.",
  "If someone is bleeding heavily, apply firm, direct pressure with a clean cloth; don’t remove it to peek.",
  "In a power outage, keep the fridge and freezer closed so food stays cold longer.",
  "Know how to turn off gas and water at the main shutoff in case of emergency.",
  "When using a fire extinguisher, remember PASS: Pull pin, Aim at base, Squeeze, Sweep.",
  "For minor burns, run cool (not ice) water for several minutes and cover with a clean, dry dressing.",
  "A whistle carries much farther than your voice and uses less energy if you need to signal for help.",
];

function getTipOfTheDay(): string {
  const start = new Date(new Date().getFullYear(), 0, 0);
  const now = new Date();
  const dayOfYear = Math.floor((now.getTime() - start.getTime()) / (24 * 60 * 60 * 1000));
  return SURVIVAL_TIPS[dayOfYear % SURVIVAL_TIPS.length];
}

/** Renders basic markdown in message text: **bold**, *italic*, `code` */
function formatMessageText(text: string): React.ReactNode {
  const formatCode = (s: string, keyPrefix: string): React.ReactNode[] => {
    const parts = s.split(/(`[^`]*`)/g);
    return parts.map((p, i) =>
      p.startsWith("`") && p.endsWith("`") ? (
        <code key={`${keyPrefix}-c-${i}`} className="message-inline-code">{p.slice(1, -1)}</code>
      ) : (
        <React.Fragment key={`${keyPrefix}-c-${i}`}>{p}</React.Fragment>
      )
    );
  };
  const formatItalic = (s: string, keyPrefix: string): React.ReactNode[] => {
    const parts = s.split(/(\*[^*]*\*)/g);
    const out: React.ReactNode[] = [];
    for (let i = 0; i < parts.length; i++) {
      const p = parts[i];
      if (p.startsWith("*") && p.endsWith("*") && p.length > 1) {
        out.push(<em key={`${keyPrefix}-i-${i}`}>{formatCode(p.slice(1, -1), `${keyPrefix}-i-${i}`)}</em>);
      } else {
        out.push(...formatCode(p, `${keyPrefix}-i-${i}`));
      }
    }
    return out;
  };
  const formatBold = (s: string, keyPrefix: string): React.ReactNode[] => {
    const parts = s.split(/(\*\*[\s\S]*?\*\*)/g);
    const out: React.ReactNode[] = [];
    for (let i = 0; i < parts.length; i++) {
      const p = parts[i];
      if (p.startsWith("**") && p.endsWith("**")) {
        out.push(<strong key={`${keyPrefix}-b-${i}`}>{formatItalic(p.slice(2, -2), `${keyPrefix}-b-${i}`)}</strong>);
      } else {
        out.push(...formatItalic(p, `${keyPrefix}-b-${i}`));
      }
    }
    return out;
  };
  const nodes = formatBold(text, "0");
  return nodes.length === 1 ? nodes[0] : <>{nodes}</>;
}

/** Renders message content with optional search term highlighted. */
function formatMessageTextWithHighlight(content: string, query: string): React.ReactNode {
  if (!query.trim()) return formatMessageText(content);
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(escaped, "gi");
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  let key = 0;
  let match;
  while ((match = re.exec(content)) !== null) {
    parts.push(<React.Fragment key={`h-${key++}`}>{formatMessageText(content.slice(lastIndex, match.index))}</React.Fragment>);
    parts.push(<mark key={`h-${key++}`} className="find-in-chat-highlight">{match[0]}</mark>);
    lastIndex = match.index + match[0].length;
  }
  parts.push(<React.Fragment key={`h-${key++}`}>{formatMessageText(content.slice(lastIndex))}</React.Fragment>);
  return <>{parts}</>;
}

const STORAGE_KEY_THEME = "athena-theme";
const STORAGE_KEY_CUSTOM_BG = "athena-custom-bg";
const STORAGE_KEY_CUSTOM_PRIMARY = "athena-custom-primary";
const STORAGE_KEY_CUSTOM_TEXT = "athena-custom-text";
const STORAGE_KEY_NIGHT_VISION_MODE = "athena-night-vision-mode";
const STORAGE_KEY_DEBUG_MODE = "athena-debug-mode";
const STORAGE_KEY_DEVICE_OVERLAY = "athena-device-overlay";
const STORAGE_KEY_PINNED_CHATS = "athena-pinned-chats";
const STORAGE_KEY_LAST_OPENED_CHATS = "athena-last-opened-chats";

function loadLastOpenedMap(): Record<string, number> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_LAST_OPENED_CHATS);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (typeof parsed !== "object" || parsed === null) return {};
    const out: Record<string, number> = {};
    for (const [k, v] of Object.entries(parsed)) {
      if (typeof v === "number" && v > 0) out[k] = v;
    }
    return out;
  } catch {
    return {};
  }
}

function saveLastOpened(chatId: string) {
  try {
    const map = loadLastOpenedMap();
    map[chatId] = Date.now();
    localStorage.setItem(STORAGE_KEY_LAST_OPENED_CHATS, JSON.stringify(map));
  } catch {}
}

function loadPinnedChatIds(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_PINNED_CHATS);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function savePinnedChatIds(ids: string[]) {
  try {
    localStorage.setItem(STORAGE_KEY_PINNED_CHATS, JSON.stringify(ids));
  } catch {}
}

const THEMES = [
  "blue",
  "cyan",
  "violet",
  "silver",
  "green",
  "amber",
  "rose",
  "teal",
  "indigo",
  "slate",
  "crimson",
  "orange",
  "lime",
  "emerald",
  "red",
  "yellow",
  "magenta",
  "purple",
  "pink",
  "brown",
  "navy",
] as const;
type Theme = (typeof THEMES)[number];
type ThemeOrCustom = Theme | "custom";

const PRESET_COLORS: Record<Theme, { bg: string; primary: string; text: string }> = {
  blue: { bg: "#0c1222", primary: "#38bdf8", text: "#e8ecf4" },
  cyan: { bg: "#0a1628", primary: "#22d3ee", text: "#e8ecf4" },
  violet: { bg: "#140c1c", primary: "#a78bfa", text: "#e8ecf4" },
  silver: { bg: "#0f0f12", primary: "#cbd5e1", text: "#e8ecf4" },
  green: { bg: "#0a1612", primary: "#34d399", text: "#e8ecf4" },
  amber: { bg: "#1a1508", primary: "#fbbf24", text: "#e8ecf4" },
  rose: { bg: "#1a0c12", primary: "#fb7185", text: "#e8ecf4" },
  teal: { bg: "#0a1a1c", primary: "#2dd4bf", text: "#e8ecf4" },
  indigo: { bg: "#0e1020", primary: "#818cf8", text: "#e8e9ff" },
  slate: { bg: "#0b1220", primary: "#94a3b8", text: "#f1f5f9" },
  crimson: { bg: "#1a0a10", primary: "#fb7185", text: "#ffe4e8" },
  orange: { bg: "#1a1208", primary: "#fb923c", text: "#fff7ed" },
  lime: { bg: "#0f1608", primary: "#a3e635", text: "#ecfccb" },
  emerald: { bg: "#0a1512", primary: "#34d399", text: "#d1fae5" },
  red: { bg: "#1a0c0c", primary: "#f87171", text: "#fef2f2" },
  yellow: { bg: "#1a1608", primary: "#facc15", text: "#fefce8" },
  magenta: { bg: "#1a0a16", primary: "#f472b6", text: "#fce7f3" },
  purple: { bg: "#150c1a", primary: "#c084fc", text: "#f3e8ff" },
  pink: { bg: "#1a0c14", primary: "#f9a8d4", text: "#fdf2f8" },
  brown: { bg: "#1a120c", primary: "#d4a574", text: "#faf5f0" },
  navy: { bg: "#060d1a", primary: "#60a5fa", text: "#e8f0ff" },
};

const DEFAULT_CUSTOM = PRESET_COLORS.blue;

/** RGB triples for `documentElement` CSS vars — default experience uses blue (`PRESET_COLORS.blue`). */
const DEFAULT_THEME_BG_RGB = hexToRgbCsv(PRESET_COLORS.blue.bg) ?? "12, 18, 34";
const DEFAULT_THEME_ACCENT_RGB = hexToRgbCsv(PRESET_COLORS.blue.primary) ?? "56, 189, 248";

/** Fallout CRT: green phosphor; bg slightly darker than `PRESET_COLORS.green` for screen depth. */
const FALLOUT_DEFAULT_BG_RGB = "6, 14, 10";
const FALLOUT_DEFAULT_ACCENT_RGB = hexToRgbCsv(PRESET_COLORS.green.primary) ?? "52, 211, 153";

function loadTheme(): ThemeOrCustom {
  try {
    const s = localStorage.getItem(STORAGE_KEY_THEME);
    if (s === "custom") return "custom";
    if (s && THEMES.includes(s as Theme)) return s as Theme;
  } catch (_) {}
  return "blue";
}

function loadCustomColors(): { bg: string; primary: string; text: string } {
  try {
    const bg = localStorage.getItem(STORAGE_KEY_CUSTOM_BG) || DEFAULT_CUSTOM.bg;
    const primary = localStorage.getItem(STORAGE_KEY_CUSTOM_PRIMARY) || DEFAULT_CUSTOM.primary;
    const text = localStorage.getItem(STORAGE_KEY_CUSTOM_TEXT) || DEFAULT_CUSTOM.text;
    return { bg, primary, text };
  } catch (_) {}
  return { ...DEFAULT_CUSTOM };
}

function saveTheme(preset: ThemeOrCustom) {
  try {
    localStorage.setItem(STORAGE_KEY_THEME, preset);
  } catch (_) {}
}

function saveCustomColors(colors: { bg: string; primary: string; text: string }) {
  try {
    localStorage.setItem(STORAGE_KEY_CUSTOM_BG, colors.bg);
    localStorage.setItem(STORAGE_KEY_CUSTOM_PRIMARY, colors.primary);
    localStorage.setItem(STORAGE_KEY_CUSTOM_TEXT, colors.text);
  } catch (_) {}
}

const THEME_LABELS: Record<Theme, string> = {
  blue: "Blue",
  cyan: "Cyan",
  violet: "Violet",
  silver: "Silver",
  green: "Green",
  amber: "Amber",
  rose: "Rose",
  teal: "Teal",
  indigo: "Indigo",
  slate: "Slate",
  crimson: "Crimson",
  orange: "Orange",
  lime: "Lime",
  emerald: "Emerald",
  red: "Red",
  yellow: "Yellow",
  magenta: "Magenta",
  purple: "Purple",
  pink: "Pink",
  brown: "Brown",
  navy: "Navy",
};

const EXPERIENCE_PRESETS = [
  {
    id: "default" as const,
    title: "Survival Guide (Default)",
    description: "Calm, practical instructor.",
  },
  {
    id: "fallout" as const,
    title: "Fallout terminal (Default)",
    description: "CRT / Pip-Boy style; uses your color palette below.",
  },
  {
    id: "fieldguide" as const,
    title: "Old survival manual (Default)",
    description: "Paper field-guide look (warm paper tones).",
  },
];

function normalizeCustomThemeForSave(t: BackendCustomTheme): BackendCustomTheme {
  return {
    ...t,
    scanlines_enabled: t.scanlines_enabled ?? false,
    scanlines_opacity: t.scanlines_opacity ?? 0.15,
    vignette_enabled: t.vignette_enabled ?? false,
    film_grain: t.film_grain ?? 0,
    sharp_corners: t.sharp_corners ?? false,
  };
}

function App() {
  const [theme, setTheme] = useState<ThemeOrCustom>(() => loadTheme());
  const [customBg, setCustomBg] = useState<string>(() => loadCustomColors().bg);
  const [customPrimary, setCustomPrimary] = useState<string>(() => loadCustomColors().primary);
  const [customText, setCustomText] = useState<string>(() => loadCustomColors().text);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  /** Shown when the frontend watchdog detects a stalled Ollama stream (main chat). */
  const [streamStallMessage, setStreamStallMessage] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [offlineMapManagerOpen, setOfflineMapManagerOpen] = useState(false);
  type LibraryTabId = "documents" | "maps";
  const [libraryActiveTab, setLibraryActiveTab] = useState<LibraryTabId>("documents");
  const [meshOpen, setMeshOpen] = useState(false);
  const [meshMessages, setMeshMessages] = useState<MeshRadioMessage[]>([]);
  const [kiwixSearchOpen, setKiwixSearchOpen] = useState(false);
  const [chatsOpen, setChatsOpen] = useState(true);
  const [watchFolder, setWatchFolder] = useState<string | null>(null);
  const [libraryFiles, setLibraryFiles] = useState<LibraryFile[]>([]);
  const [libraryFilesModalOpen, setLibraryFilesModalOpen] = useState(false);
  const [stats, setStats] = useState<IndexStats | null>(null);
  const [libraryError, setLibraryError] = useState<string | null>(null);
  const [_ragIngestingPaths, setRagIngestingPaths] = useState<string[]>([]);
  const [ragIngestedLocalFiles, setRagIngestedLocalFiles] = useState<LibraryFile[]>([]);
  const [systemSpecs, setSystemSpecs] = useState<SystemSpecs | null>(null);
  const [ollamaAvailable, setOllamaAvailable] = useState(false);
  const [ollamaNotRunningBannerDismissed, setOllamaNotRunningBannerDismissed] = useState(false);
  const [ollamaLaunching, setOllamaLaunching] = useState(false);
  const [ollamaSidecarManaged, setOllamaSidecarManaged] = useState(false);
  const [ollamaSetupStatus, setOllamaSetupStatus] = useState<OllamaSetupStatus | null>(null);
  const [ollamaSetupProgress, setOllamaSetupProgress] = useState<string | null>(null);
  const [ollamaSetupLastError, setOllamaSetupLastError] = useState<string | null>(null);
  const [ollamaCopyPromptDismissed, setOllamaCopyPromptDismissed] = useState(false);
  const [ollamaCopyBusy, setOllamaCopyBusy] = useState(false);
  const [ollamaInstallBusy, setOllamaInstallBusy] = useState(false);
  const [activeEngineModel, setActiveEngineModel] = useState("");
  const [userDisplayName, setUserDisplayName] = useState("");
  const [userBirthdateIso, setUserBirthdateIso] = useState("");
  const [allowNsfwCatalog, setAllowNsfwCatalog] = useState(false);
  const [userIs18, setUserIs18] = useState(false);
  const [birthdayBannerVisible, setBirthdayBannerVisible] = useState(false);
  const [inferenceBackendMode, setInferenceBackendMode] = useState<"ollama" | "openai_compatible">("ollama");
  const [openaiBaseUrl, setOpenaiBaseUrl] = useState("http://127.0.0.1:1234/v1");
  const [openaiModelId, setOpenaiModelId] = useState("");
  const [openaiApiKeyLocal, setOpenaiApiKeyLocal] = useState("");
  const [aiInferenceReady, setAiInferenceReady] = useState(false);
  const [quarantineAlert, setQuarantineAlert] = useState<{ path: string; reason: string } | null>(null);
  const [modelOverride, setModelOverride] = useState<ModelTier>("auto");
  const [memoryMessages, setMemoryMessages] = useState(20);
  const [memoryEstimate, setMemoryEstimate] = useState<MemoryEstimate | null>(null);
  const [aiTemperature, setAiTemperature] = useState(() => {
    try {
      const v = localStorage.getItem("athena_ai_temperature");
      const n = v != null ? Number.parseFloat(v) : Number.NaN;
      return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0.7;
    } catch {
      return 0.7;
    }
  });
  const [aiNumCtx, setAiNumCtx] = useState(() => {
    try {
      const v = localStorage.getItem("athena_ai_num_ctx");
      const n = v != null ? Number.parseInt(v, 10) : Number.NaN;
      return Number.isFinite(n) && n >= 512 ? Math.min(131072, n) : 4096;
    } catch {
      return 4096;
    }
  });
  const [maintenanceFeedback, setMaintenanceFeedback] = useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const [discoveredZims, setDiscoveredZims] = useState<DiscoveredZim[]>([]);
  const [kiwixScanLoading, setKiwixScanLoading] = useState(false);
  const [kiwixScanError, setKiwixScanError] = useState<string | null>(null);
  const [chatList, setChatList] = useState<ChatSummary[]>([]);
  const [currentChatId, setCurrentChatId] = useState<string | null>(null);
  const [chatSearch, setChatSearch] = useState("");
  const [findInChatOpen, setFindInChatOpen] = useState(false);
  const [findInChatQuery, setFindInChatQuery] = useState("");
  const [findInChatIndex, setFindInChatIndex] = useState(0);
  const [pinnedChatIds, setPinnedChatIds] = useState<string[]>(() => loadPinnedChatIds());
  const [lastOpenedMap, setLastOpenedMap] = useState<Record<string, number>>(() => loadLastOpenedMap());
  const findInChatInputRef = useRef<HTMLInputElement>(null);
  const messageRefsRef = useRef<(HTMLDivElement | null)[]>([]);

  function togglePinChat(chatId: string) {
    setPinnedChatIds((prev) => {
      const next = prev.includes(chatId) ? prev.filter((id) => id !== chatId) : [...prev, chatId];
      savePinnedChatIds(next);
      return next;
    });
  }
  const [errorLog, setErrorLog] = useState<string>("");
  type SettingsPanel =
    | "menu"
    | "general"
    | "ai"
    | "location"
    | "apiKeys"
    | "maps"
    | "mesh"
    | "voice"
    | "appearance"
    | "themes"
    | "troubleshooting"
    | "developer";
  const [settingsPanel, setSettingsPanel] = useState<SettingsPanel>("menu");
  const settingsBodyRef = useRef<HTMLDivElement>(null);
  const menuScrollPosRef = useRef(0);
  const [meshRadioConnected, setMeshRadioConnected] = useState<boolean | null>(null);

  function goToSettingsSubpanel(panel: SettingsPanel) {
    if (settingsPanel === "menu" && settingsBodyRef.current) {
      menuScrollPosRef.current = settingsBodyRef.current.scrollTop;
    }
    setSettingsPanel(panel);
  }
  const [settingsTitleClickCount, setSettingsTitleClickCount] = useState(0);
  const [activeThemeId, setActiveThemeId] = useState<string>("default");
  const [activePersonalityId, setActivePersonalityId] = useState<PersonalityId>("default");
  const [customThemes, setCustomThemes] = useState<BackendCustomTheme[]>([]);
  const [debugMode, setDebugMode] = useState(() => {
    try {
      return localStorage.getItem(STORAGE_KEY_DEBUG_MODE) === "1";
    } catch {
      return false;
    }
  });
  const [_rebuildVectorIndexProgress, setRebuildVectorIndexProgress] = useState<{
    processed: number;
    total: number;
    failed: number;
  } | null>(null);
  const [deviceOverlayEnabled, setDeviceOverlayEnabled] = useState(() => {
    try {
      return localStorage.getItem(STORAGE_KEY_DEVICE_OVERLAY) === "1";
    } catch {
      return false;
    }
  });
  const [deviceStats, setDeviceStats] = useState<{
    cpu_usage_pct: number;
    ram_used_mb: number;
    ram_total_mb: number;
    swap_used_mb: number;
    swap_total_mb: number;
    disk_used_gb: number;
    disk_total_gb: number;
    net_rx_bytes: number;
    net_tx_bytes: number;
    disk_read_bytes_total: number;
    disk_write_bytes_total: number;
    cpu_temp_c?: number | null;
    gpu_usage_pct?: number | null;
    battery_pct?: number | null;
    battery_charging?: boolean | null;
    battery_time_to_empty_minutes?: number | null;
  } | null>(null);
  const deviceStatsPrevNetRef = useRef<{ rx: number; tx: number; t: number } | null>(null);
  const deviceStatsPrevDiskRef = useRef<{ read: number; write: number; t: number } | null>(null);
  const [netSpeedRxKbps, setNetSpeedRxKbps] = useState<number | null>(null);
  const [netSpeedTxKbps, setNetSpeedTxKbps] = useState<number | null>(null);
  const [diskSpeedReadMbps, setDiskSpeedReadMbps] = useState<number | null>(null);
  const [diskSpeedWriteMbps, setDiskSpeedWriteMbps] = useState<number | null>(null);
  const [deviceOverlayExpanded, setDeviceOverlayExpanded] = useState(false);
  /** When false (other tab / minimized), slow telemetry polling to save CPU & battery. */
  const pageVisible = usePageVisible();
  const [selectedCustomThemeId, setSelectedCustomThemeId] = useState<string | null>(null);
  type ThemesSubView = "main" | "studio";
  const [themesSubView, setThemesSubView] = useState<ThemesSubView>("main");
  const [studioDraft, setStudioDraft] = useState<BackendCustomTheme | null>(null);
  const [featureMapEnabled, setFeatureMapEnabled] = useState(false);
  const [meshAiAutoReply, setMeshAiAutoReply] = useState(true);
  const [showSourcesInChat, setShowSourcesInChat] = useState(true);
  const [fullscreen, setFullscreen] = useState(false);
  const [userLocationAddress, setUserLocationAddress] = useState<string>("");
  const DEFAULT_SATELLITE_API_KEY_PLACEHOLDER = "MAPTILER_SATELLITE_API_KEY_HERE";
  const [satelliteApiKeyInput, setSatelliteApiKeyInput] = useState("");
  const [satelliteApiKeySaved, setSatelliteApiKeySaved] = useState(false);
  const [satelliteApiKeyStatus, setSatelliteApiKeyStatus] = useState<string | null>(null);
  const [satelliteApiKeyFocused, setSatelliteApiKeyFocused] = useState(false);
  const [showSatelliteKeyTutorial, setShowSatelliteKeyTutorial] = useState(false);
  const [currentView, setCurrentView] = useState<AppView>(() => readStartupView());
  const [startupViewPreference, setStartupViewPreference] = useState<AppView>(() => readStartupView());
  const [mapCatalog, setMapCatalog] = useState<MapCatalogEntry[]>([]);
  const [downloadedMaps, setDownloadedMaps] = useState<DownloadedMap[]>([]);
  const [pois, setPois] = useState<Poi[]>([]);
  const [tacticalMapPoints, setTacticalMapPoints] = useState<TacticalMapPointOfInterest[]>([]);
  const [tacticalMapNodes, setTacticalMapNodes] = useState<TacticalMapMeshNodeLocation[]>([]);
  const [mapChatInput, setMapChatInput] = useState("");
  const [mapChatSending, setMapChatSending] = useState(false);
  const [mapChatLastMessage, setMapChatLastMessage] = useState("");
  const [mapChatHistory, setMapChatHistory] = useState<Array<{ role: "user" | "assistant"; content: string }>>([]);
  const [_layerFilters, setLayerFilters] = useState({ pois: true, roads: true, rivers: true });
  void _layerFilters;
  void setLayerFilters;
  /** Active Kiwix / map downloads keyed by URL (matches backend progress `id`). */
  const [downloadJobs, setDownloadJobs] = useState<
    Record<string, { downloaded: number; total: number | null; label: string }>
  >({});
  const [localDataInstallOpen, setLocalDataInstallOpen] = useState(false);
  const [networkReachable, setNetworkReachable] = useState<boolean | null>(null);
  const [mapsSuggestion, setMapsSuggestion] = useState<MapCatalogEntry | null>(null);
  const [mapsSuggestionLoading, setMapsSuggestionLoading] = useState(false);
  const [tilePacks, setTilePacks] = useState<TilePackInfo[]>([]);
  const [offlineOsmRoot, setOfflineOsmRoot] = useState<string | null>(null);
  const [offlineTopoRoot, setOfflineTopoRoot] = useState<string | null>(null);
  const [offlineImportedRoot, setOfflineImportedRoot] = useState<string | null>(null);
  const [enabledArchiveRasterFileNames, setEnabledArchiveRasterFileNames] = useState<string[]>([]);
  const mapRasterLayersHydratedRef = useRef(false);
  /** Last browser GPS fix or explicit update from map chat; used for chat spatial RAG when a fresh fix is unavailable. */
  const lastKnownGpsRef = useRef<{ lat: number; lon: number } | null>(null);
  const [archiveRasterNativeZoomRangesByFile, setArchiveRasterNativeZoomRangesByFile] = useState<
    Record<string, [number, number]>
  >({});
  const [archiveRasterBoundsByFile, setArchiveRasterBoundsByFile] = useState<
    Record<string, [number, number, number, number]>
  >({});
  const [mapBasemap, setMapBasemap] = useState<MapBasemap>("online_osm");
  const [mapViewCenter, setMapViewCenter] = useState<[number, number]>([39.5, -98.35]);
  const [mapViewZoom, setMapViewZoom] = useState(4);
  const [tacticalMapLayerVisibility, setTacticalMapLayerVisibility] = useState<TacticalLayerVisibilityState>(
    readTacticalMapLayerVisibilityFromStorage,
  );
  const [tileRadiusMiles, setTileRadiusMiles] = useState(12);
  const [tileZoomMin, setTileZoomMin] = useState(10);
  const [tileZoomMax, setTileZoomMax] = useState(13);
  const [tileGeocodeQuery, setTileGeocodeQuery] = useState("");
  const [tilePackEstimate, setTilePackEstimate] = useState<TilePackEstimate | null>(null);
  const [tileEstimateBusy, setTileEstimateBusy] = useState(false);

  useEffect(() => {
    try {
      window.localStorage.setItem(ATHENA_TACTICAL_MAP_LAYER_VISIBILITY_KEY, JSON.stringify(tacticalMapLayerVisibility));
    } catch {
      /* ignore quota / private mode */
    }
  }, [tacticalMapLayerVisibility]);

  const resolveGpsForChatPayload = useCallback(async (): Promise<{ userLat: number | null; userLon: number | null }> => {
    if (typeof navigator !== "undefined" && navigator.geolocation) {
      const fresh = await new Promise<{ lat: number; lon: number } | null>((resolve) => {
        navigator.geolocation.getCurrentPosition(
          (pos) => resolve({ lat: pos.coords.latitude, lon: pos.coords.longitude }),
          () => resolve(null),
          { timeout: 8000, maximumAge: 120000, enableHighAccuracy: false },
        );
      });
      if (fresh) {
        lastKnownGpsRef.current = fresh;
        return { userLat: fresh.lat, userLon: fresh.lon };
      }
    }
    const last = lastKnownGpsRef.current;
    if (last) {
      return { userLat: last.lat, userLon: last.lon };
    }
    if (featureMapEnabled && currentView === "map") {
      return { userLat: mapViewCenter[0], userLon: mapViewCenter[1] };
    }
    return { userLat: null, userLon: null };
  }, [featureMapEnabled, currentView, mapViewCenter]);

  const headerDownloadBar = useMemo(() => {
    const entries = Object.entries(downloadJobs);
    if (entries.length === 0) return null;
    const jobs = entries.map(([, v]) => v);
    const withTotal = entries.filter(([, j]) => j.total != null && (j.total as number) > 0);
    if (withTotal.length === entries.length && withTotal.length > 0) {
      const sumD = withTotal.reduce((a, [, j]) => a + j.downloaded, 0);
      const sumT = withTotal.reduce((a, [, j]) => a + (j.total as number), 0);
      const allTileCount = entries.every(([id]) => isTileCountProgressJob(id));
      const progressDetail = allTileCount
        ? `${sumD.toLocaleString()} / ${sumT.toLocaleString()} tiles`
        : `${formatBytes(sumD)} / ${formatBytes(sumT)}`;
      return {
        mode: "determinate" as const,
        pct: Math.min(100, sumT > 0 ? Math.round((sumD / sumT) * 100) : 0),
        subtitle: jobs.length === 1 ? jobs[0].label : `${jobs.length} downloads`,
        progressDetail,
      };
    }
    const partial = entries.find(([, j]) => j.downloaded > 0);
    const partialDetail =
      partial != null
        ? isTileCountProgressJob(partial[0])
          ? `${partial[1].downloaded.toLocaleString()} tiles…`
          : `${formatBytes(partial[1].downloaded)}…`
        : null;
    return {
      mode: "indeterminate" as const,
      subtitle: jobs.length === 1 ? jobs[0].label : `${jobs.length} downloads`,
      progressDetail: partialDetail,
    };
  }, [downloadJobs]);

  const downloadedMapPaths = useMemo(() => new Set(downloadedMaps.map((m) => m.path)), [downloadedMaps]);

  const mapLibraryFiles = useMemo(
    () =>
      libraryFiles.filter((f) => downloadedMapPaths.has(f.path) || isLikelyKiwixMapZimFilename(f.name)),
    [libraryFiles, downloadedMapPaths],
  );

  const ragLibraryFiles = useMemo(
    () =>
      libraryFiles.filter((f) => !downloadedMapPaths.has(f.path) && !isLikelyKiwixMapZimFilename(f.name)),
    [libraryFiles, downloadedMapPaths],
  );

  const ragDisplayFiles = useMemo(() => {
    const byPath = new Map<string, LibraryFile>();
    for (const f of ragIngestedLocalFiles) byPath.set(f.path, f);
    for (const f of ragLibraryFiles) byPath.set(f.path, f);
    return Array.from(byPath.values()).sort((a, b) => a.name.localeCompare(b.name));
  }, [ragLibraryFiles, ragIngestedLocalFiles]);

  const mapDownloadJobs = useMemo(
    () =>
      Object.entries(downloadJobs).filter(
        ([id]) => id.startsWith("http://") || id.startsWith("https://"),
      ),
    [downloadJobs],
  );

  const mapZimsOnDiskOnly = useMemo(
    () => mapLibraryFiles.filter((f) => !downloadedMapPaths.has(f.path)),
    [mapLibraryFiles, downloadedMapPaths],
  );

  const convertFileSrcFn = useCallback((path: string) => convertFileSrc(path), []);

  const [attachments, setAttachments] = useState<Array<{ id: string; kind: "image" | "file"; name: string; path: string }>>([]);
  const [toolsOpen, setToolsOpen] = useState(false);
  const [batteryState, setBatteryState] = useState<BatteryState | null>(null);
  const [showLowBatteryBanner, setShowLowBatteryBanner] = useState(false);
  const [lowBatteryDismissed, setLowBatteryDismissed] = useState(false);
  const [exportingPdf, setExportingPdf] = useState(false);
  const [activeTriageId, setActiveTriageId] = useState<TriageId | null>(null);
  const [triageMenuOpen, setTriageMenuOpen] = useState(false);
  const [temporaryChat, setTemporaryChat] = useState(false);
  function renderTriageIcon(id: TriageId) {
    switch (id) {
      case "severe-bleeding":
        return (
          <span className="triage-icon-mask triage-icon-mask-bleeding" aria-hidden role="img" />
        );
      case "hypothermia":
        return (
          <span className="triage-icon-mask triage-icon-mask-hypothermia" aria-hidden role="img" />
        );
      case "shock":
        return (
          <span className="triage-icon-mask triage-icon-mask-shock" aria-hidden role="img" />
        );
      case "unconscious":
        return (
          <span className="triage-icon-mask triage-icon-mask-unconscious" aria-hidden role="img" />
        );
      case "poisoning":
      default:
        return (
          <span className="triage-icon-mask" aria-hidden role="img" />
        );
    }
  }
  const [ttsEnabled, setTtsEnabled] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    try {
      const raw = window.localStorage.getItem("athena-tts-enabled");
      return raw === "1";
    } catch {
      return false;
    }
  });
  const [ttsVoices, setTtsVoices] = useState<SpeechSynthesisVoice[]>([]);
  const [ttsPreferredVoiceUri, setTtsPreferredVoiceUri] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    try {
      return window.localStorage.getItem("athena-tts-voice") || null;
    } catch {
      return null;
    }
  });
  const [voicePreset, setVoicePreset] = useState<VoicePresetId>(() => {
    if (typeof window === "undefined") return "default";
    try {
      const stored = window.localStorage.getItem("athena-voice-preset");
      if (stored === "fallout" || stored === "ultron" || stored === "default") return stored;
    } catch {
      // ignore
    }
    return "default";
  });
  const [ttsSpeakingIndex, setTtsSpeakingIndex] = useState<number | null>(null);
  const [ttsPaused, setTtsPaused] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const messagesRef = useRef<ChatMessage[]>([]);
  const ttsUtteranceRef = useRef<SpeechSynthesisUtterance | null>(null);
  const staticNoiseRef = useRef<{ ctx: AudioContext; gain: GainNode; source: AudioBufferSourceNode } | null>(null);
  const ttsAudioRef = useRef<HTMLAudioElement | null>(null);
  const speechRecognitionRef = useRef<{ start(): void; abort(): void } | null>(null);
  const [speechListening, setSpeechListening] = useState(false);
  const [conversationInterimTranscript, setConversationInterimTranscript] = useState("");
  const [conversationMicStatus, setConversationMicStatus] = useState<"idle" | "requesting" | "granted" | "denied">("idle");
  const [conversationMicError, setConversationMicError] = useState<string | null>(null);
  const [conversationModeActive, setConversationModeActive] = useState(false);
  const conversationModeRef = useRef(false);
  const speakMessageAtIndexRef = useRef<(index: number, onAfterSpeak?: () => void) => void>(() => {});
  const startConversationListeningRef = useRef<() => void>(() => {});
  const logErrorRef = useRef<(msg: string) => void>(() => {});
  const mapStreamContentRef = useRef("");
  const mapStreamActiveRef = useRef(false);
  const streamUnlistenRef = useRef<Array<() => void>>([]);
  const streamAssistantContentRef = useRef("");
  /** Watchdog: main chat stream — start time (ms), last chunk time, first chunk received. */
  const streamSendStartRef = useRef(0);
  const streamLastChunkAtRef = useRef<number | null>(null);
  const streamGotFirstChunkRef = useRef(false);
  /** Watchdog: map chat stream (same thresholds as backend: 120s to first token, 30s idle). */
  const mapStreamSendStartRef = useRef(0);
  const mapStreamLastChunkAtRef = useRef<number | null>(null);
  const mapStreamGotFirstChunkRef = useRef(false);
  const currentChatIdRef = useRef<string | null>(null);
  const temporaryChatRef = useRef(false);

  useEffect(() => {
    currentChatIdRef.current = currentChatId;
  }, [currentChatId]);

  /** Local AI stream watchdog: 120s max wait for first chunk, 30s max idle after first chunk. */
  useEffect(() => {
    if (!sending && !mapChatSending) return;
    const tick = () => {
      const now = Date.now();
      if (sending) {
        const w = evaluateMainChatWatchdog({
          nowMs: now,
          sending: true,
          sendStartMs: streamSendStartRef.current,
          lastChunkAtMs: streamLastChunkAtRef.current,
          gotFirstChunk: streamGotFirstChunkRef.current,
        });
        if (w.action === "stall_first_timeout") {
          setStreamStallMessage(w.message);
          setMessages((prev) => {
            const next = [...prev];
            const i = next.length - 1;
            if (i >= 0 && next[i].role === "assistant") {
              next[i] = {
                ...next[i],
                content: w.message,
              };
            }
            return next;
          });
          setSending(false);
        } else if (w.action === "stall_idle") {
          setStreamStallMessage(w.message);
          setMessages((prev) => {
            const next = [...prev];
            const i = next.length - 1;
            if (i >= 0 && next[i].role === "assistant") {
              next[i] = {
                ...next[i],
                content: w.message,
              };
            }
            return next;
          });
          setSending(false);
        }
      }
      if (mapChatSending) {
        const start = mapStreamSendStartRef.current;
        const last = mapStreamLastChunkAtRef.current;
        const gotFirst = mapStreamGotFirstChunkRef.current;
        if (!gotFirst) {
          if (now - start > 120_000) {
            mapStreamActiveRef.current = false;
            mapStreamContentRef.current = "";
            setMapChatLastMessage("The local AI did not respond in time (120s).");
            setMapChatSending(false);
          }
        } else if (last != null && now - last > 30_000) {
          mapStreamActiveRef.current = false;
          mapStreamContentRef.current = "";
          setMapChatLastMessage("The local AI stalled (no output for 30s).");
          setMapChatSending(false);
        }
      }
    };
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [sending, mapChatSending]);

  useEffect(() => {
    conversationModeRef.current = conversationModeActive;
  }, [conversationModeActive]);
  const appRef = useRef<HTMLDivElement>(null);
  const triageMenuRef = useRef<HTMLDivElement>(null);
  const [nightVisionMode, setNightVisionMode] = useState<NightVisionMode>(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY_NIGHT_VISION_MODE) as NightVisionMode | null;
      if (stored === "auto" || stored === "on" || stored === "off") return stored;
    } catch {
      // ignore
    }
    return "auto";
  });
  const [isNight, setIsNight] = useState(false);

  function resolveBaseColors(base: string | null | undefined): { bg: string; primary: string; text: string } {
    if (base && (THEMES as readonly string[]).includes(base)) {
      return PRESET_COLORS[base as Theme];
    }
    if (base === "fallout") {
      return PRESET_COLORS.blue;
    }
    if (base === "fieldguide") {
      return PRESET_COLORS.amber;
    }
    return PRESET_COLORS.blue;
  }

  function resolveFontStack(fontToken: string | null | undefined, persona: PersonalityId): string | null {
    const token = fontToken ?? "";
    if (token === "mono-crt") {
      return '"SF Mono", ui-monospace, Menlo, Monaco, "Courier New", monospace';
    }
    if (token === "system") {
      return 'system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif';
    }
    if (persona === "fallout") {
      return '"SF Mono", ui-monospace, Menlo, Monaco, "Courier New", monospace';
    }
    if (persona === "fieldguide") {
      return '"Georgia", "Times New Roman", Times, serif';
    }
    return null;
  }

  function handlePresetChange(e: React.ChangeEvent<HTMLSelectElement>) {
    const value = e.target.value as ThemeOrCustom;
    if (value === "custom") {
      setTheme("custom");
      saveTheme("custom");
      return;
    }
    if (THEMES.includes(value)) {
      setTheme(value);
      saveTheme(value);
    }
  }

  function handleCustomColorChange(
    which: "bg" | "primary" | "text",
    value: string
  ) {
    setTheme("custom");
    saveTheme("custom");
    if (which === "bg") {
      setCustomBg(value);
      saveCustomColors({ bg: value, primary: customPrimary, text: customText });
    } else if (which === "primary") {
      setCustomPrimary(value);
      saveCustomColors({ bg: customBg, primary: value, text: customText });
    } else {
      setCustomText(value);
      saveCustomColors({ bg: customBg, primary: customPrimary, text: value });
    }
  }

  function applyThemePreset(id: string) {
    setActiveThemeId(id);
    setSelectedCustomThemeId(null);
    const persona: PersonalityId = (id === "fallout" || id === "fieldguide" || id === "default" ? id : "default") as PersonalityId;
    setActivePersonalityId(persona);
    if (id === "fieldguide") {
      setTheme("amber");
      saveTheme("amber");
      const c = PRESET_COLORS.amber;
      setCustomBg(c.bg);
      setCustomPrimary(c.primary);
      setCustomText(c.text);
      saveCustomColors(c);
    } else if (id === "default") {
      setTheme("blue");
      saveTheme("blue");
      const c = PRESET_COLORS.blue;
      setCustomBg(c.bg);
      setCustomPrimary(c.primary);
      setCustomText(c.text);
      saveCustomColors(c);
    } else {
      const palette: Theme =
        theme !== "custom" && THEMES.includes(theme as Theme) ? (theme as Theme) : "blue";
      setTheme(palette);
      saveTheme(palette);
      const c = PRESET_COLORS[palette];
      setCustomBg(c.bg);
      setCustomPrimary(c.primary);
      setCustomText(c.text);
      saveCustomColors(c);
    }
    invoke("set_theme_config", {
      themeId: id,
      personalityId: persona,
      customThemes,
    }).catch((e) => logError(String(e)));
  }

  function applyCustomTheme(id: string) {
    const themeDef = customThemes.find((t) => t.id === id);
    if (!themeDef) return;
    setActiveThemeId(id);
    setSelectedCustomThemeId(id);
    const persona: PersonalityId =
      (themeDef.personality_id === "fallout" || themeDef.personality_id === "fieldguide" || themeDef.personality_id === "default"
        ? (themeDef.personality_id as PersonalityId)
        : "default");
    setActivePersonalityId(persona);
    const baseColors = resolveBaseColors(themeDef.base);
    const bg = themeDef.bg_color ?? baseColors.bg;
    const primary = themeDef.primary_color ?? baseColors.primary;
    const text = themeDef.text_color ?? baseColors.text;
    setTheme("custom");
    saveTheme("custom");
    setCustomBg(bg);
    setCustomPrimary(primary);
    setCustomText(text);
    saveCustomColors({ bg, primary, text });
    invoke("set_theme_config", {
      themeId: id,
      personalityId: persona,
      customThemes,
    }).catch((e) => logError(String(e)));
  }

  function handlePersonalityChange(next: PersonalityId) {
    setActivePersonalityId(next);
    if (!selectedCustomThemeId && next === "fieldguide") {
      setTheme("amber");
      saveTheme("amber");
    }
    const updatedCustomThemes =
      selectedCustomThemeId && customThemes.length
        ? customThemes.map((t) =>
            t.id === selectedCustomThemeId
              ? {
                  ...t,
                  personality_id: next,
                }
              : t
          )
        : customThemes;
    if (updatedCustomThemes !== customThemes) {
      setCustomThemes(updatedCustomThemes);
    }
    invoke("set_theme_config", {
      themeId: activeThemeId,
      personalityId: next,
      customThemes: updatedCustomThemes,
    }).catch((e) => logError(String(e)));
  }

  function openThemeStudioNew() {
    const id = `custom-${Date.now().toString(36)}`;
    const isPreset = theme !== "custom";
    const baseColors = isPreset ? PRESET_COLORS[theme as Theme] : { bg: customBg, primary: customPrimary, text: customText };
    const baseId = isPreset ? (theme as string) : activeThemeId;
    setStudioDraft(
      normalizeCustomThemeForSave({
        id,
        name: "My theme",
        base: baseId === "default" || baseId === "fallout" || baseId === "fieldguide" ? baseId : "default",
        bg_color: baseColors.bg,
        primary_color: baseColors.primary,
        text_color: baseColors.text,
        accent_color: null,
        font_family: null,
        personality_id: activePersonalityId,
        scanlines_enabled: false,
        scanlines_opacity: 0.15,
        vignette_enabled: false,
        film_grain: 0,
        sharp_corners: false,
      })
    );
    setThemesSubView("studio");
  }

  function openThemeStudioEdit(themeId: string) {
    const t = customThemes.find((x) => x.id === themeId);
    if (!t) return;
    setStudioDraft(normalizeCustomThemeForSave({ ...t }));
    setThemesSubView("studio");
  }

  function saveThemeStudio() {
    if (!studioDraft) return;
    const normalized = normalizeCustomThemeForSave(studioDraft);
    const exists = customThemes.some((x) => x.id === normalized.id);
    const nextThemes = exists
      ? customThemes.map((x) => (x.id === normalized.id ? normalized : x))
      : [...customThemes, normalized];
    setCustomThemes(nextThemes);
    setActiveThemeId(normalized.id);
    setSelectedCustomThemeId(normalized.id);
    const persona: PersonalityId =
      normalized.personality_id === "fallout" || normalized.personality_id === "fieldguide" || normalized.personality_id === "default"
        ? (normalized.personality_id as PersonalityId)
        : "default";
    setActivePersonalityId(persona);
    setTheme("custom");
    saveTheme("custom");
    const bg = normalized.bg_color ?? resolveBaseColors(normalized.base).bg;
    const primary = normalized.primary_color ?? resolveBaseColors(normalized.base).primary;
    const text = normalized.text_color ?? resolveBaseColors(normalized.base).text;
    setCustomBg(bg);
    setCustomPrimary(primary);
    setCustomText(text);
    saveCustomColors({ bg, primary, text });
    invoke("set_theme_config", {
      themeId: normalized.id,
      personalityId: persona,
      customThemes: nextThemes,
    }).catch((e) => logError(String(e)));
    setStudioDraft(null);
    setThemesSubView("main");
  }

  function cancelThemeStudio() {
    setStudioDraft(null);
    setThemesSubView("main");
  }

  function updateStudioDraft(patch: Partial<BackendCustomTheme>) {
    setStudioDraft((d) => (d ? { ...d, ...patch } : d));
  }

  function handleDeleteCustomTheme(id: string) {
    if (studioDraft?.id === id) {
      cancelThemeStudio();
    }
    const nextThemes = customThemes.filter((t) => t.id !== id);
    setCustomThemes(nextThemes);
    let nextThemeId = activeThemeId;
    if (activeThemeId === id) {
      nextThemeId = "default";
      setActiveThemeId("default");
      setSelectedCustomThemeId(null);
      setTheme("blue");
      saveTheme("blue");
    }
    invoke("set_theme_config", {
      themeId: nextThemeId,
      personalityId: activePersonalityId,
      customThemes: nextThemes,
    }).catch((e) => logError(String(e)));
  }

  function scrollToBottom() {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  useEffect(() => {
    temporaryChatRef.current = temporaryChat;
  }, [temporaryChat]);

  useEffect(() => {
    logErrorRef.current = logError;
  }, [logError]);

  useEffect(() => {
    const root = appRef.current;
    if (!root) return;
    const active = nightVisionMode === "on" || (nightVisionMode === "auto" && isNight);
    if (active) {
      root.setAttribute("data-nightvision", "true");
    } else {
      root.removeAttribute("data-nightvision");
    }
  }, [nightVisionMode, isNight]);

  useEffect(() => {
    if (typeof window === "undefined" || !("speechSynthesis" in window)) return;

    function chooseDefaultVoice(voices: SpeechSynthesisVoice[]): SpeechSynthesisVoice | null {
      if (!voices.length) return null;
      if (ttsPreferredVoiceUri) {
        const byStored = voices.find((v) => v.voiceURI === ttsPreferredVoiceUri);
        if (byStored) return byStored;
      }
      const isEnglish = (v: SpeechSynthesisVoice) => v.lang.toLowerCase().startsWith("en");
      const en = voices.filter(isEnglish);
      const list = en.length ? en : voices;
      if (voicePreset === "ultron") {
        return list.find((v) => /daniel|ralph|fred|alex|tom/i.test(v.name)) || list[list.length - 1] || list[0] || voices[0];
      }
      if (voicePreset === "fallout") {
        const appleVoices = list.filter((v) => /apple|alex|fred/i.test(v.name));
        return appleVoices.find((v) => /alex|fred/i.test(v.name)) || appleVoices[0] || list[0] || voices[0];
      }
      const english = voices.find(isEnglish);
      return english || voices[0];
    }

    function loadVoices() {
      const list = window.speechSynthesis.getVoices();
      if (!list || list.length === 0) return;
      setTtsVoices(list);
      const chosen = chooseDefaultVoice(list);
      if (chosen && !ttsPreferredVoiceUri) {
        setTtsPreferredVoiceUri(chosen.voiceURI);
        try {
          window.localStorage.setItem("athena-tts-voice", chosen.voiceURI);
        } catch {
          // ignore
        }
      }
    }

    loadVoices();
    window.speechSynthesis.addEventListener("voiceschanged", loadVoices);
    return () => {
      window.speechSynthesis.removeEventListener("voiceschanged", loadVoices);
    };
  }, [activePersonalityId, ttsPreferredVoiceUri, voicePreset]);

  function stopFalloutStatic() {
    const prev = staticNoiseRef.current;
    if (prev) {
      try {
        prev.source.stop();
      } catch {
        // already stopped
      }
      staticNoiseRef.current = null;
    }
  }

  function startFalloutStatic() {
    if (typeof window === "undefined" || !("AudioContext" in window)) return;
    stopFalloutStatic();
    try {
      const ctx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
      const gain = ctx.createGain();
      gain.gain.value = 0.03;
      gain.connect(ctx.destination);
      const buf = ctx.createBuffer(1, ctx.sampleRate * 2, ctx.sampleRate);
      const channel = buf.getChannelData(0);
      for (let i = 0; i < channel.length; i++) channel[i] = (Math.random() * 2 - 1) * 0.3;
      const source = ctx.createBufferSource();
      source.buffer = buf;
      source.loop = true;
      source.connect(gain);
      source.start(0);
      staticNoiseRef.current = { ctx, gain, source };
    } catch {
      // ignore
    }
  }

  function stopTts() {
    if (typeof window !== "undefined" && "speechSynthesis" in window) {
      window.speechSynthesis.cancel();
    }
    stopFalloutStatic();
    const audio = ttsAudioRef.current;
    if (audio) {
      audio.pause();
      audio.src = "";
      ttsAudioRef.current = null;
    }
    ttsUtteranceRef.current = null;
    setTtsSpeakingIndex(null);
    setTtsPaused(false);
  }

  function toggleTtsPause() {
    if (typeof window === "undefined" || !("speechSynthesis" in window)) return;
    const synth = window.speechSynthesis;
    if (!synth.speaking) return;
    if (synth.paused) {
      synth.resume();
      setTtsPaused(false);
    } else {
      synth.pause();
      setTtsPaused(true);
    }
  }

  function chooseSystemVoiceForPreset(voices: SpeechSynthesisVoice[]): SpeechSynthesisVoice | null {
    if (!voices.length) return null;
    const isEnglish = (v: SpeechSynthesisVoice) => v.lang.toLowerCase().startsWith("en");
    const en = voices.filter(isEnglish);
    const list = en.length ? en : voices;
    if (voicePreset === "ultron") {
      const deep = list.find((v) => /daniel|ralph|fred|alex|tom/i.test(v.name)) || list[list.length - 1];
      return deep ?? list[0];
    }
    if (voicePreset === "fallout") {
      const retro = list.find((v) => /alex|fred/i.test(v.name)) || list[0];
      return retro ?? voices[0];
    }
    if (ttsPreferredVoiceUri) {
      const byStored = list.find((v) => v.voiceURI === ttsPreferredVoiceUri);
      if (byStored) return byStored;
    }
    return list[0] ?? voices[0];
  }

  function speakMessageAtIndex(index: number, onAfterSpeak?: () => void) {
    if (!ttsEnabled) {
      onAfterSpeak?.();
      return;
    }
    const msg = messages[index];
    if (!msg || msg.role !== "assistant" || !msg.content.trim()) {
      onAfterSpeak?.();
      return;
    }

    stopTts();
    setTtsSpeakingIndex(index);
    setTtsPaused(false);

    const applyEnd = () => {
      stopFalloutStatic();
      ttsUtteranceRef.current = null;
      ttsAudioRef.current = null;
      setTtsSpeakingIndex((current) => (current === index ? null : current));
      setTtsPaused(false);
      onAfterSpeak?.();
    };

    if ((voicePreset === "fallout" || voicePreset === "ultron") && typeof invoke !== "undefined") {
      invoke<string>("synthesize_speech", { text: msg.content, voiceId: voicePreset })
        .then((base64Wav) => {
          const binary = atob(base64Wav);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
          const blob = new Blob([bytes], { type: "audio/wav" });
          const url = URL.createObjectURL(blob);
          const audio = new Audio(url);
          ttsAudioRef.current = audio;
          audio.onended = () => {
            URL.revokeObjectURL(url);
            applyEnd();
          };
          audio.onerror = () => {
            URL.revokeObjectURL(url);
            applyEnd();
          };
          audio.play().catch(() => applyEnd());
        })
        .catch(() => {
          setTtsSpeakingIndex(null);
          speakWithSystemTts(index, msg.content, applyEnd);
        });
      return;
    }

    speakWithSystemTts(index, msg.content, applyEnd);
  }

  function speakWithSystemTts(
    _index: number,
    text: string,
    onEnd: () => void
  ) {
    if (typeof window === "undefined" || !("speechSynthesis" in window)) {
      onEnd();
      return;
    }
    const synth = window.speechSynthesis;
    let didSpeak = false;

    const doSpeak = () => {
      if (didSpeak) return;
      didSpeak = true;
      const voices = ttsVoices.length ? ttsVoices : synth.getVoices();
      const voice = chooseSystemVoiceForPreset(voices);
      const utterance = new SpeechSynthesisUtterance(text);
      if (voice) utterance.voice = voice;
      utterance.onend = onEnd;
      utterance.onerror = onEnd;
      ttsUtteranceRef.current = utterance;
      requestAnimationFrame(() => {
        synth.speak(utterance);
      });
    };

    const voices = ttsVoices.length ? ttsVoices : synth.getVoices();
    if (voices.length > 0) {
      if (voicePreset === "fallout") startFalloutStatic();
      doSpeak();
      return;
    }
    const onVoicesChanged = () => {
      synth.removeEventListener("voiceschanged", onVoicesChanged);
      if (timeoutId !== null) window.clearTimeout(timeoutId);
      if (voicePreset === "fallout") startFalloutStatic();
      doSpeak();
    };
    let timeoutId: ReturnType<typeof setTimeout> | null = setTimeout(() => {
      timeoutId = null;
      synth.removeEventListener("voiceschanged", onVoicesChanged);
      if (voicePreset === "fallout") startFalloutStatic();
      doSpeak();
    }, 800);
    synth.addEventListener("voiceschanged", onVoicesChanged);
  }

  function startConversationListening() {
    if (!conversationModeRef.current) return;
    const SpeechRecognitionAPI =
      typeof window !== "undefined" &&
      (window.SpeechRecognition || window.webkitSpeechRecognition);
    if (!SpeechRecognitionAPI) {
      logErrorRef.current?.("Speech recognition not supported.");
      setConversationMicStatus("denied");
      return;
    }
    speechRecognitionRef.current?.abort();
    setConversationInterimTranscript("");
    let hadResult = false;
    const tryStart = () => {
      const rec = new SpeechRecognitionAPI();
      rec.continuous = false;
      rec.interimResults = true;
      rec.lang = "en-US";
      rec.maxAlternatives = 2;
      rec.onresult = (e: SpeechRecognitionEvent) => {
        const results = Array.from(e.results);
        const transcript = results
          .map((r) => r[0]?.transcript)
          .filter(Boolean)
          .join(" ")
          .trim();
        const lastIsFinal = results[results.length - 1]?.isFinal;
        if (lastIsFinal) {
          setConversationInterimTranscript("");
          if (transcript) {
            hadResult = true;
            setSpeechListening(false);
            handleSend(transcript);
          }
        } else {
          setConversationInterimTranscript(transcript);
        }
      };
      rec.onend = () => {
        setConversationInterimTranscript("");
        if (conversationModeRef.current && !hadResult) {
          setTimeout(() => startConversationListening(), 500);
        } else {
          setSpeechListening(false);
        }
      };
      rec.onerror = (e: SpeechRecognitionErrorEvent) => {
        setConversationInterimTranscript("");
        setSpeechListening(false);
        const permissionDenied = e.error === "not-allowed" || e.error === "audio-capture";
        if (permissionDenied) {
          setConversationMicError(`Microphone denied (${e.error}). Allow in System Settings → Privacy & Security → Microphone.`);
          setConversationMicStatus("denied");
          logErrorRef.current?.("Microphone access denied. Allow microphone to use conversation mode.");
          return;
        }
        if (conversationModeRef.current) {
          setTimeout(() => startConversationListening(), 800);
        }
      };
      speechRecognitionRef.current = rec;
      rec.start();
      setSpeechListening(true);
    };
    if (typeof navigator !== "undefined" && navigator.mediaDevices?.getUserMedia) {
      setConversationMicStatus("requesting");
      setConversationMicError(null);
      (async () => {
        try {
          const { requestMicrophonePermission } = await import("tauri-plugin-macos-permissions-api");
          await requestMicrophonePermission();
        } catch {
          /* Not Tauri, or not macOS; continue with getUserMedia */
        }
        if (!conversationModeRef.current) return;
        const timeout = new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("Request timed out. Check System Settings → Privacy & Security → Microphone for Athena.")), 10000)
        );
        try {
          const stream = await Promise.race([
            navigator.mediaDevices.getUserMedia({ audio: true }),
            timeout,
          ]);
          if (!conversationModeRef.current) return;
          stream.getTracks().forEach((t) => t.stop());
          setConversationMicStatus("granted");
          setConversationMicError(null);
          tryStart();
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          setConversationMicError(msg);
          setConversationMicStatus("denied");
          logErrorRef.current?.(`Microphone: ${msg}`);
        }
      })();
    } else {
      setConversationMicStatus("granted");
      tryStart();
    }
  }

  speakMessageAtIndexRef.current = speakMessageAtIndex;
  startConversationListeningRef.current = startConversationListening;

  // Load theme + personality config from backend so persona matches the selected preset.
  useEffect(() => {
    invoke<[string | null, string | null, BackendCustomTheme[]]>("get_theme_config")
      .then(([themeId, personalityId, customs]) => {
        const id = themeId ?? "default";
        const personaRaw = personalityId ?? id;
        const persona: PersonalityId =
          personaRaw === "fallout" || personaRaw === "fieldguide" || personaRaw === "default"
            ? (personaRaw as PersonalityId)
            : "default";
        const safeId = id === "jarvis" || id === "lastofus" ? "default" : id;
        setActiveThemeId(safeId);
        setActivePersonalityId(persona);
        const backendCustoms = customs || [];
        setCustomThemes(backendCustoms);
        const activeCustom = backendCustoms.find((t) => t.id === safeId);
        if (activeCustom) {
          setSelectedCustomThemeId(activeCustom.id);
          const baseColors = resolveBaseColors(activeCustom.base);
          const bg = activeCustom.bg_color ?? baseColors.bg;
          const primary = activeCustom.primary_color ?? baseColors.primary;
          const text = activeCustom.text_color ?? baseColors.text;
          setTheme("custom");
          saveTheme("custom");
          setCustomBg(bg);
          setCustomPrimary(primary);
          setCustomText(text);
          saveCustomColors({ bg, primary, text });
        } else {
          if (safeId === "fieldguide") {
            setTheme("amber");
            saveTheme("amber");
            const c = PRESET_COLORS.amber;
            setCustomBg(c.bg);
            setCustomPrimary(c.primary);
            setCustomText(c.text);
            saveCustomColors(c);
          } else if (safeId === "default") {
            setTheme("blue");
            saveTheme("blue");
            const c = PRESET_COLORS.blue;
            setCustomBg(c.bg);
            setCustomPrimary(c.primary);
            setCustomText(c.text);
            saveCustomColors(c);
          } else {
            const local = loadTheme();
            const pal: Theme =
              local !== "custom" && THEMES.includes(local as Theme) ? (local as Theme) : "blue";
            setTheme(pal);
            saveTheme(pal);
            const c = PRESET_COLORS[pal];
            setCustomBg(c.bg);
            setCustomPrimary(c.primary);
            setCustomText(c.text);
            saveCustomColors(c);
          }
        }
      })
      .catch(() => {
        // Ignore; fall back to local theme only.
      });
  }, []);

  async function loadWatchFolder() {
    try {
      const folder = await invoke<string | null>("get_watch_folder");
      setWatchFolder(folder);
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  async function loadStats() {
    try {
      const s = await invoke<IndexStats>("get_index_stats");
      setStats(s);
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  async function loadModelSettings() {
    try {
      const [specs, available, sidecarManaged, override, mem, estimate, uis, inferReady] = await Promise.all([
        invoke<SystemSpecs>("get_system_specs"),
        invoke<boolean>("is_ollama_available"),
        invoke<boolean>("get_sidecar_status"),
        invoke<string | null>("get_model_override"),
        invoke<number>("get_memory_messages"),
        invoke<MemoryEstimate>("get_memory_estimate"),
        invoke<UserAndInferenceSettings>("get_user_and_inference_settings").catch(() => null),
        invoke<boolean>("is_inference_ready").catch(() => false),
      ]);
      setSystemSpecs(specs);
      setOllamaAvailable(available);
      setOllamaSidecarManaged(sidecarManaged);
      if (available) setOllamaNotRunningBannerDismissed(false);
      setAiInferenceReady(inferReady);
      {
        const raw = (override ?? "auto").toLowerCase();
        setModelOverride(
          (MODEL_TIERS as readonly string[]).includes(raw) ? (raw as ModelTier) : "auto",
        );
      }
      setMemoryMessages(mem);
      setMemoryEstimate(estimate);
      if (uis) {
        setUserDisplayName(uis.user_display_name ?? "");
        setUserBirthdateIso(uis.user_birthdate_iso ?? "");
        setAllowNsfwCatalog(uis.allow_nsfw_unrestricted_models);
        setUserIs18(uis.is_at_least_18);
        setInferenceBackendMode(uis.inference_backend === "openai_compatible" ? "openai_compatible" : "ollama");
        setOpenaiBaseUrl(uis.openai_compatible_base_url?.trim() || "http://127.0.0.1:1234/v1");
        setOpenaiModelId(uis.openai_compatible_model ?? "");
        setActiveEngineModel(uis.explicit_ollama_model ?? "");
        if (!uis.openai_compatible_api_key_set) {
          setOpenaiApiKeyLocal("");
        }
        const todayKey = `athena_birthday_shown_${new Date().toISOString().slice(0, 10)}`;
        if (uis.is_birthday_today && !sessionStorage.getItem(todayKey)) {
          setBirthdayBannerVisible(true);
        } else {
          setBirthdayBannerVisible(false);
        }
      }
    } catch (_) {
      setSystemSpecs(null);
      setOllamaAvailable(false);
      setOllamaSidecarManaged(false);
      setMemoryEstimate(null);
      setAiInferenceReady(false);
    }
  }

  async function handleMemoryMessagesChange(value: number) {
    const cap = systemSpecs?.max_memory_messages ?? 80;
    const clamped = Math.min(cap, Math.max(5, value));
    setMemoryMessages(clamped);
    try {
      await invoke("set_memory_messages", { value: clamped });
      const estimate = await invoke<MemoryEstimate>("get_memory_estimate");
      setMemoryEstimate(estimate);
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  useEffect(() => {
    try {
      localStorage.setItem("athena_ai_temperature", String(aiTemperature));
    } catch {
      /* ignore */
    }
  }, [aiTemperature]);

  useEffect(() => {
    try {
      localStorage.setItem("athena_ai_num_ctx", String(aiNumCtx));
    } catch {
      /* ignore */
    }
  }, [aiNumCtx]);

  async function handleBackupDatabase() {
    setMaintenanceFeedback(null);
    try {
      const destPath = await save({
        title: "Backup database",
        defaultPath: "athena_backup.db",
        filters: [{ name: "SQLite database", extensions: ["db"] }],
      });
      if (destPath == null) return;
      await invoke("backup_database", { destPath });
      setMaintenanceFeedback({ kind: "ok", text: "Database backup saved." });
    } catch (e) {
      setMaintenanceFeedback({ kind: "err", text: e != null ? String(e) : "Backup failed." });
    }
  }

  async function handleRestoreDatabase() {
    setMaintenanceFeedback(null);
    try {
      const selectedResult = await open({
        title: "Restore database",
        multiple: false,
        filters: [{ name: "SQLite database", extensions: ["db"] }],
      });
      if (selectedResult == null) return;
      const sourcePath = Array.isArray(selectedResult) ? selectedResult[0] : selectedResult;
      await invoke("restore_database", { sourcePath });
      setMaintenanceFeedback({
        kind: "ok",
        text: "Database restored from file. If anything looks stale, restart Athena.",
      });
    } catch (e) {
      setMaintenanceFeedback({ kind: "err", text: e != null ? String(e) : "Restore failed." });
    }
  }

  async function handleExportCommsLogs() {
    setMaintenanceFeedback(null);
    try {
      const destPath = await save({
        title: "Export comms logs",
        defaultPath: "athena_comms_export.csv",
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (destPath == null) return;
      await invoke("export_radio_logs", { destPath });
      setMaintenanceFeedback({ kind: "ok", text: "Comms log export saved." });
    } catch (e) {
      setMaintenanceFeedback({ kind: "err", text: e != null ? String(e) : "Export failed." });
    }
  }

  async function loadChatList() {
    try {
      const list = await invoke<ChatSummary[]>("list_chats");
      setChatList(list);
      setPinnedChatIds((prev) => {
        const next = prev.filter((id) => list.some((c) => c.id === id));
        if (next.length !== prev.length) savePinnedChatIds(next);
        return next;
      });
    } catch (e) {
      logError(String(e));
    }
  }

  async function loadChat(id: string) {
    try {
      setTemporaryChat(false);
      const chat = await invoke<SavedChat | null>("get_chat", { id });
      if (chat?.messages) {
        setMessages(chat.messages as ChatMessage[]);
        setCurrentChatId(chat.id);
        saveLastOpened(id);
        setLastOpenedMap((prev) => ({ ...prev, [id]: Date.now() }));
      }
    } catch (e) {
      logError(String(e));
    }
  }

  async function newChat() {
    try {
      const msgs = messages;
      if (msgs.length > 0 && currentChatId && !temporaryChat) {
        await invoke("save_chat", {
          id: currentChatId,
          title: chatTitle(msgs),
          messages: msgs.map((m) => ({ role: m.role, content: m.content })),
          description: null,
        });
      }
      setMessages([]);
      setCurrentChatId(null);
      setTemporaryChat(false);
      loadChatList();
    } catch (e) {
      logError(String(e));
    }
  }

  function chatTitle(msgs: ChatMessage[]): string {
    const first = msgs.find((m) => m.role === "user");
    if (first) {
      const t = first.content.trim().slice(0, 50);
      return t + (first.content.length > 50 ? "…" : "");
    }
    return "New chat";
  }

  async function saveCurrentChat(msgs: ChatMessage[]): Promise<string | null> {
    if (msgs.length === 0 || temporaryChat) return null;
    try {
      const payload: {
        id?: string;
        title: string;
        messages: { role: string; content: string }[];
        description: string | null;
      } = {
        title: chatTitle(msgs),
        messages: msgs.map((m) => ({ role: m.role, content: m.content })),
        description: null,
      };
      if (currentChatIdRef.current) {
        payload.id = currentChatIdRef.current;
      }
      const id = await invoke<string>("save_chat", payload);
      currentChatIdRef.current = id;
      setCurrentChatId(id);
      loadChatList();

      // Generate a short title for new chats (first exchange) and description for longer chats
      const msgCount = msgs.length;
      if (msgCount >= 2) {
        const firstUser = msgs.find((m) => m.role === "user")?.content?.trim();
        if (msgCount === 2 && firstUser) {
          invoke<string>("generate_chat_title", { firstMessage: firstUser })
            .then((title) => {
              if (title) invoke("update_chat_metadata", { id, title, description: null });
              loadChatList();
            })
            .catch(() => {});
        }
        if (msgCount >= 4) {
          invoke<string>("generate_chat_description", {
            messages: msgs.map((m) => ({ role: m.role, content: m.content })),
          })
            .then((description) => {
              if (description) invoke("update_chat_metadata", { id, title: null, description });
              loadChatList();
            })
            .catch(() => {});
        }
      }
      return id;
    } catch (_) {
      return null;
    }
  }

  async function deleteChat(id: string) {
    try {
      setLibraryError(null);
      await invoke("delete_chat", { id });
      if (currentChatId === id) {
        setMessages([]);
        setCurrentChatId(null);
      }
      loadChatList();
    } catch (e) {
      const msg = e != null ? String(e) : "Could not delete chat";
      setLibraryError(msg);
      logError(msg);
    }
  }

  function logError(message: string) {
    const entry = `${new Date().toISOString()} ${message}`;
    setErrorLog((prev) => (prev ? prev + "\n" + entry : entry));
    invoke("append_error_log", { message }).catch(() => {});
    try {
      const stored = localStorage.getItem("athena_last_errors");
      const arr = stored ? JSON.parse(stored) : [];
      arr.push(entry);
      localStorage.setItem("athena_last_errors", JSON.stringify(arr.slice(-5)));
    } catch (_) {}
    // #region agent log
    try {
      fetch("http://127.0.0.1:7625/ingest/8fd099f6-5880-4d68-82fc-1df05f97fa81", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "035879",
        },
        body: JSON.stringify({
          sessionId: "035879",
          runId: "pre-fix",
          hypothesisId: "H-general",
          location: "App.tsx:logError",
          message: "logError invoked",
          data: { message },
          timestamp: Date.now(),
        }),
      }).catch(() => {});
    } catch {
      // ignore instrumentation failures
    }
    // #endregion agent log
  }

  async function handleScanKiwixZim() {
    setKiwixScanError(null);
    setKiwixScanLoading(true);
    try {
      const list = await invoke<DiscoveredZim[]>("scan_kiwix_zim_files");
      setDiscoveredZims(list);
      await loadLibraryFiles();
      await loadStats();
    } catch (e) {
      const errMsg = e != null ? String(e) : "Scan failed";
      setKiwixScanError(errMsg);
      setDiscoveredZims([]);
      logError(`Kiwix scan: ${errMsg}`);
    } finally {
      setKiwixScanLoading(false);
    }
  }

  async function handleRemoveLibraryFile(path: string) {
    setLibraryError(null);
    try {
      await invoke("remove_library_file", { path });
      await loadLibraryFiles();
      await loadStats();
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  const applyOfflineBasemapPaths = useCallback((r: OfflineBasemapPaths) => {
    setOfflineOsmRoot(r.osm ?? null);
    setOfflineTopoRoot(r.opentopo ?? null);
    setOfflineImportedRoot(r.imported ?? null);
  }, []);

  async function handlePreviewTileEstimate() {
    const q = (tileGeocodeQuery.trim() || userLocationAddress.trim()).trim();
    if (!q) {
      setLibraryError("Set a location under Location settings, or enter a place name below.");
      return;
    }
    setLibraryError(null);
    setTileEstimateBusy(true);
    try {
      const geo = await invoke<[number, number]>("geocode_place_query", { query: q });
      const [lat, lon] = geo;
      const est = await invoke<TilePackEstimate>("estimate_tile_pack_tiles", {
        centerLat: lat,
        centerLon: lon,
        radiusMiles: tileRadiusMiles,
        zoomMin: Math.min(tileZoomMin, tileZoomMax),
        zoomMax: Math.max(tileZoomMin, tileZoomMax),
      });
      setTilePackEstimate(est);
    } catch (e) {
      setTilePackEstimate(null);
      setLibraryError(e != null ? String(e) : "Could not estimate tiles");
    } finally {
      setTileEstimateBusy(false);
    }
  }

  async function handleStartRasterTileDownload(source: "osm" | "opentopo") {
    const q = (tileGeocodeQuery.trim() || userLocationAddress.trim()).trim();
    if (!q) {
      setLibraryError("Set a location under Location settings, or enter a place name below.");
      return;
    }
    setLibraryError(null);
    try {
      const geo = await invoke<[number, number]>("geocode_place_query", { query: q });
      const [lat, lon] = geo;
      const packId = await invoke<string>("start_tile_pack_download", {
        source,
        centerLat: lat,
        centerLon: lon,
        radiusMiles: tileRadiusMiles,
        zoomMin: Math.min(tileZoomMin, tileZoomMax),
        zoomMax: Math.max(tileZoomMin, tileZoomMax),
        name: `${source === "opentopo" ? "Terrain" : "Street"} ~${tileRadiusMiles}mi`,
      });
      setDownloadJobs((prev) => ({
        ...prev,
        [`tiles:${packId}`]: {
          downloaded: 0,
          total: null,
          label: source === "opentopo" ? "Offline terrain tiles" : "Offline street tiles",
        },
      }));
    } catch (e) {
      setLibraryError(e != null ? String(e) : "Tile download failed");
    }
  }

  async function handleExportPackToMbtiles(packId: string, packName: string) {
    setLibraryError(null);
    try {
      const safe = packName.replace(/[^\w\-]+/g, "_").slice(0, 48) || "tiles";
      const destPath = await save({
        filters: [{ name: "MBTiles", extensions: ["mbtiles"] }],
        defaultPath: `${safe}.mbtiles`,
      });
      if (destPath == null) return;
      await invoke("export_tile_pack_to_mbtiles_file", { packId, destPath });
      setDownloadJobs((prev) => ({
        ...prev,
        [`mbtiles_export:${packId}`]: {
          downloaded: 0,
          total: null,
          label: `Export “${packName}” to .mbtiles`,
        },
      }));
    } catch (e) {
      setLibraryError(e != null ? String(e) : "Export failed");
    }
  }

  async function handleImportMbtiles() {
    setLibraryError(null);
    try {
      const selectedResult = await open({
        directory: false,
        multiple: false,
        filters: [{ name: "MBTiles", extensions: ["mbtiles"] }],
      });
      if (selectedResult === null) return;
      const sourcePath = Array.isArray(selectedResult) ? selectedResult[0] : selectedResult;
      const base = sourcePath.split(/[\\/]/).pop() ?? "Imported";
      const displayName = base.replace(/\.mbtiles$/i, "") || "Imported MBTiles";
      const jobId = await invoke<string>("start_mbtiles_import", { sourcePath, displayName });
      setDownloadJobs((prev) => ({
        ...prev,
        [`import:${jobId}`]: {
          downloaded: 0,
          total: null,
          label: `Import MBTiles “${displayName}”`,
        },
      }));
    } catch (e) {
      setLibraryError(e != null ? String(e) : "Import failed");
    }
  }

  async function handleImportPmtiles() {
    setLibraryError(null);
    try {
      const selectedResult = await open({
        directory: false,
        multiple: false,
        filters: [{ name: "PMTiles", extensions: ["pmtiles"] }],
      });
      if (selectedResult === null) return;
      const sourcePath = Array.isArray(selectedResult) ? selectedResult[0] : selectedResult;
      const base = sourcePath.split(/[\\/]/).pop() ?? "Imported";
      const displayName = base.replace(/\.pmtiles$/i, "") || "Imported PMTiles";
      const jobId = await invoke<string>("start_pmtiles_import", { sourcePath, displayName });
      setDownloadJobs((prev) => ({
        ...prev,
        [`import:${jobId}`]: {
          downloaded: 0,
          total: null,
          label: `Import PMTiles “${displayName}”`,
        },
      }));
    } catch (e) {
      setLibraryError(e != null ? String(e) : "PMTiles import failed");
    }
  }

  async function handleDownloadMap(entry: MapCatalogEntry) {
    setLibraryError(null);
    setDownloadJobs((prev) => ({
      ...prev,
      [entry.url]: { downloaded: 0, total: entry.size_bytes, label: entry.name },
    }));
    try {
      await invoke("download_map", { url: entry.url, name: entry.name, region: entry.region, style: entry.style });
      invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    } catch (e) {
      setDownloadJobs((prev) => {
        const next = { ...prev };
        delete next[entry.url];
        return next;
      });
      setLibraryError(String(e));
    }
  }

  async function handleRemoveDownloadedMap(path: string) {
    setLibraryError(null);
    try {
      await invoke("remove_downloaded_map", { path });
      setDownloadedMaps((prev) => prev.filter((m) => m.path !== path));
      if (currentView === "map") invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  async function handleRemoveTilePack(id: string) {
    setLibraryError(null);
    try {
      await invoke("remove_tile_pack_cmd", { id });
      const packs = await invoke<TilePackInfo[]>("list_tile_packs_cmd");
      setTilePacks(packs);
      const roots = await invoke<OfflineBasemapPaths>("offline_basemap_paths_cmd");
      applyOfflineBasemapPaths(roots);
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  async function handleAddPoi(name: string, lat: number, lon: number, notes: string) {
    try {
      setLibraryError(null);
      const poi = await invoke<Poi>("add_poi", { name, lat, lon, notes });
      setPois((prev) => [...prev, poi]);
    } catch (e) {
      const msg = e != null ? String(e) : "Could not add POI";
      setLibraryError(msg);
      logError(msg);
    }
  }

  async function handleRemovePoi(id: string) {
    try {
      setLibraryError(null);
      await invoke("remove_poi", { id });
      setPois((prev) => prev.filter((p) => p.id !== id));
    } catch (e) {
      const msg = e != null ? String(e) : "Could not remove POI";
      setLibraryError(msg);
      logError(msg);
    }
  }

  async function handleModelOverrideChange(e: React.ChangeEvent<HTMLSelectElement>) {
    const value = e.target.value as ModelTier;
    if (!MODEL_TIERS.includes(value)) return;
    await updateModelOverride(value);
  }

  async function handleFeatureMapToggle(enabled: boolean) {
    setFeatureMapEnabled(enabled);
    try {
      await invoke("set_feature_map_enabled", { value: enabled });
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  async function handleMeshAiAutoReplyToggle(enabled: boolean) {
    setMeshAiAutoReply(enabled);
    try {
      await invoke("set_mesh_ai_auto_reply", { value: enabled });
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  /** Save location and offer local data install when online + location feature is on. */
  async function handleUserLocationBlur(value: string) {
    const trimmed = value.trim();
    setUserLocationAddress(value);
    try {
      await invoke("set_user_location", { value: trimmed || null });
      if (featureMapEnabled && trimmed.length >= 3) {
        const online = await invoke<boolean>("check_network_reachable").catch(() => false);
        setNetworkReachable(online);
        setLocalDataInstallOpen(true);
      }
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  async function handleSaveSatelliteApiKey() {
    const trimmed = satelliteApiKeyInput.trim();
    const isGenericPlaceholder = trimmed === DEFAULT_SATELLITE_API_KEY_PLACEHOLDER;
    try {
      await invoke("set_satellite_api_key", { value: trimmed.length > 0 && !isGenericPlaceholder ? trimmed : null });
      if (trimmed.length > 0 && !isGenericPlaceholder) {
        setSatelliteApiKeySaved(true);
        setSatelliteApiKeyStatus("Satellite API key saved locally on this device.");
      } else {
        setSatelliteApiKeySaved(false);
        setSatelliteApiKeyStatus("Satellite API key cleared on this device.");
      }
    } catch (e) {
      const msg = e != null ? String(e) : "Could not save satellite API key.";
      setSatelliteApiKeyStatus(msg);
      setLibraryError(msg);
    }
  }

  async function openLocalDataInstallMenu() {
    invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    const online = await invoke<boolean>("check_network_reachable").catch(() => false);
    setNetworkReachable(online);
    setLocalDataInstallOpen(true);
  }

  async function updateModelOverride(value: ModelTier) {
    setModelOverride(value);
    try {
      await invoke("set_model_override", {
        value: value === "auto" ? null : value,
      });
    } catch (e) {
      setLibraryError(String(e));
    }
  }

  /* Check Ollama on app load so the banner reflects real state (not just initial false) */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void invoke<boolean>("get_sidecar_status")
      .then(setOllamaSidecarManaged)
      .catch(() => setOllamaSidecarManaged(false));
    invoke<boolean>("is_ollama_available")
      .then((available) => {
        setOllamaAvailable(available);
        if (available) setOllamaNotRunningBannerDismissed(false);
      })
      .catch(() => setOllamaAvailable(false));
    invoke<boolean>("is_inference_ready")
      .then((ok) => setAiInferenceReady(ok))
      .catch(() => setAiInferenceReady(false));
    void invoke<OllamaSetupStatus>("get_ollama_setup_status")
      .then(setOllamaSetupStatus)
      .catch(() => setOllamaSetupStatus(null));
    void listen<Record<string, unknown>>("ollama-setup-progress", (event) => {
      const phase = event.payload.phase;
      if (phase === "downloading") {
        setOllamaSetupLastError(null);
        setOllamaSetupProgress("Downloading the Ollama CLI for Athena (first launch can take a minute)…");
      } else if (phase === "installed") {
        setOllamaSetupProgress(null);
        void invoke<boolean>("is_ollama_available")
          .then((ok) => {
            setOllamaAvailable(ok);
            if (ok) setOllamaNotRunningBannerDismissed(false);
          })
          .catch(() => {});
        void invoke<OllamaSetupStatus>("get_ollama_setup_status")
          .then(setOllamaSetupStatus)
          .catch(() => setOllamaSetupStatus(null));
      } else if (phase === "error") {
        setOllamaSetupProgress(null);
        const msg =
          typeof event.payload.message === "string" ? event.payload.message : "Unknown error";
        setOllamaSetupLastError(msg);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  /* Hydrate persisted map/location toggles on app launch so header icons render immediately. */
  useEffect(() => {
    invoke<boolean>("get_feature_map_enabled")
      .then(setFeatureMapEnabled)
      .catch(() => setFeatureMapEnabled(false));
    invoke<string | null>("get_user_location")
      .then((value) => setUserLocationAddress(value ?? ""))
      .catch(() => setUserLocationAddress(""));
    invoke<string[]>("get_enabled_main_map_raster_archive_file_names")
      .then((names) => {
        const normalized = [...new Set(names.map((name) => name.trim()).filter((name) => name.length > 0))].sort((a, b) =>
          a.localeCompare(b),
        );
        setEnabledArchiveRasterFileNames(normalized);
      })
      .catch(() => setEnabledArchiveRasterFileNames([]))
      .finally(() => {
        mapRasterLayersHydratedRef.current = true;
      });
  }, []);

  useEffect(() => {
    if (typeof navigator === "undefined" || !navigator.geolocation) return;
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        lastKnownGpsRef.current = { lat: pos.coords.latitude, lon: pos.coords.longitude };
      },
      () => {},
      { timeout: 20000, maximumAge: 600000, enableHighAccuracy: false },
    );
  }, []);

  useEffect(() => {
    if (!mapRasterLayersHydratedRef.current) return;
    void invoke("set_enabled_main_map_raster_archive_file_names", {
      archiveFileNames: enabledArchiveRasterFileNames,
    }).catch(() => {});
  }, [enabledArchiveRasterFileNames]);

  /* When AI backend banner is visible, re-check inference (slower when window in background). */
  useEffect(() => {
    if (aiInferenceReady || ollamaNotRunningBannerDismissed) return;
    const pollMs = pageVisible ? 10_000 : 60_000;
    const id = window.setInterval(() => {
      void invoke<boolean>("is_inference_ready")
        .then((ok) => {
          setAiInferenceReady(ok);
          if (ok) setOllamaNotRunningBannerDismissed(false);
        })
        .catch(() => {});
      void invoke<boolean>("is_ollama_available")
        .then((available) => {
          setOllamaAvailable(available);
        })
        .catch(() => {});
    }, pollMs);
    return () => window.clearInterval(id);
  }, [aiInferenceReady, ollamaNotRunningBannerDismissed, pageVisible]);

  useEffect(() => {
    if (settingsOpen) {
      menuScrollPosRef.current = 0;
      setLibraryError(null);
      setSettingsPanel("menu");
      setSettingsTitleClickCount(0);
      loadModelSettings();
      invoke<string>("get_error_log").then(setErrorLog).catch(() => setErrorLog(""));
      invoke<boolean>("get_feature_map_enabled").then(setFeatureMapEnabled).catch(() => setFeatureMapEnabled(false));
      invoke<boolean>("get_mesh_ai_auto_reply").then(setMeshAiAutoReply).catch(() => setMeshAiAutoReply(true));
      invoke<boolean>("get_show_sources_in_chat").then(setShowSourcesInChat).catch(() => setShowSourcesInChat(true));
      invoke<string | null>("get_user_location").then((v) => setUserLocationAddress(v ?? "")).catch(() => setUserLocationAddress(""));
      invoke<string | null>("get_satellite_api_key")
        .then((v) => {
          const key = v ?? "";
          setSatelliteApiKeyInput(key);
          setSatelliteApiKeySaved(key.trim().length > 0 && key.trim() !== DEFAULT_SATELLITE_API_KEY_PLACEHOLDER);
          setSatelliteApiKeyStatus(null);
          setSatelliteApiKeyFocused(false);
          setShowSatelliteKeyTutorial(false);
        })
        .catch(() => {
          setSatelliteApiKeyInput("");
          setSatelliteApiKeySaved(false);
          setSatelliteApiKeyStatus(null);
          setSatelliteApiKeyFocused(false);
          setShowSatelliteKeyTutorial(false);
        });
      invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    }
  }, [settingsOpen]);

  useEffect(() => {
    if (!settingsOpen) setSettingsTitleClickCount(0);
  }, [settingsOpen]);

  useEffect(() => {
    if (!settingsOpen) {
      setThemesSubView("main");
      setStudioDraft(null);
    }
  }, [settingsOpen]);

  useLayoutEffect(() => {
    const el = settingsBodyRef.current;
    if (!el) return;
    if (settingsPanel === "menu") {
      el.scrollTop = menuScrollPosRef.current;
    } else {
      el.scrollTop = 0;
    }
  }, [settingsPanel]);

  useLayoutEffect(() => {
    if (settingsPanel !== "themes") return;
    const el = settingsBodyRef.current;
    if (!el) return;
    if (themesSubView === "studio") {
      el.scrollTop = 0;
    }
  }, [themesSubView, settingsPanel]);

  useEffect(() => {
    if (settingsOpen && settingsPanel === "appearance") {
      invoke<boolean>("is_fullscreen").then(setFullscreen).catch(() => {});
    }
  }, [settingsOpen, settingsPanel]);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY_DEBUG_MODE, debugMode ? "1" : "0");
    } catch {
      // ignore
    }
  }, [debugMode]);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY_DEVICE_OVERLAY, deviceOverlayEnabled ? "1" : "0");
    } catch {
      // ignore
    }
  }, [deviceOverlayEnabled]);

  useEffect(() => {
    if (!deviceOverlayEnabled) {
      setDeviceStats(null);
      deviceStatsPrevNetRef.current = null;
      deviceStatsPrevDiskRef.current = null;
      setNetSpeedRxKbps(null);
      setNetSpeedTxKbps(null);
      setDiskSpeedReadMbps(null);
      setDiskSpeedWriteMbps(null);
      return;
    }
    const fetchStats = () => {
      invoke<{
        cpu_usage_pct: number;
        ram_used_mb: number;
        ram_total_mb: number;
        swap_used_mb: number;
        swap_total_mb: number;
        disk_used_gb: number;
        disk_total_gb: number;
        net_rx_bytes: number;
        net_tx_bytes: number;
        disk_read_bytes_total: number;
        disk_write_bytes_total: number;
        cpu_temp_c?: number | null;
        gpu_usage_pct?: number | null;
        battery_pct?: number | null;
        battery_charging?: boolean | null;
        battery_time_to_empty_minutes?: number | null;
      }>("get_device_stats")
        .then((s) => {
          const now = Date.now();
          const prev = deviceStatsPrevNetRef.current;
          if (prev !== null && now > prev.t) {
            const dtSec = (now - prev.t) / 1000;
            if (dtSec > 0) {
              setNetSpeedRxKbps((s.net_rx_bytes - prev.rx) / 1024 / dtSec);
              setNetSpeedTxKbps((s.net_tx_bytes - prev.tx) / 1024 / dtSec);
            }
          }
          deviceStatsPrevNetRef.current = { rx: s.net_rx_bytes, tx: s.net_tx_bytes, t: now };

          const prevDisk = deviceStatsPrevDiskRef.current;
          if (prevDisk !== null && now > prevDisk.t) {
            const dtSec = (now - prevDisk.t) / 1000;
            if (dtSec > 0) {
              const dr = s.disk_read_bytes_total - prevDisk.read;
              const dw = s.disk_write_bytes_total - prevDisk.write;
              if (dr >= 0 && dw >= 0) {
                setDiskSpeedReadMbps(dr / (1024 * 1024) / dtSec);
                setDiskSpeedWriteMbps(dw / (1024 * 1024) / dtSec);
              }
            }
          }
          deviceStatsPrevDiskRef.current = {
            read: s.disk_read_bytes_total,
            write: s.disk_write_bytes_total,
            t: now,
          };

          setDeviceStats(s);
        })
        .catch(() => setDeviceStats(null));
    };
    fetchStats();
    const pollMs = pageVisible ? 2000 : 20000;
    const interval = setInterval(fetchStats, pollMs);
    return () => clearInterval(interval);
  }, [deviceOverlayEnabled, pageVisible]);

  useEffect(() => {
    loadChatList();
  }, []);

  useEffect(() => {
    const unlisten = listen("athena:mesh-chats-updated", () => {
      loadChatList();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<RagIngestionProgressPayload>("rag-ingestion-progress", (event) => {
      const p = event.payload;
      if (!p || !p.file_path) return;
      if (!p.completed) {
        setRagIngestingPaths((prev) => (prev.includes(p.file_path) ? prev : [...prev, p.file_path]));
        return;
      }
      setRagIngestingPaths((prev) => prev.filter((path) => path !== p.file_path));
      if (p.error) {
        setLibraryError(`Ingest failed for ${p.filename}: ${p.error}`);
        return;
      }
      setRagIngestedLocalFiles((prev) => {
        const next = prev.filter((f) => f.path !== p.file_path);
        next.unshift({ path: p.file_path, name: p.filename, size_bytes: 0 });
        return next;
      });
      void invoke<LibraryFile[]>("list_library_contents")
        .then((list) => setLibraryFiles(Array.isArray(list) ? list : []))
        .catch(() => {});
      void loadStats();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    function onError(ev: ErrorEvent) {
      logErrorRef.current(`[window error] ${ev.message} ${ev.filename ?? ""}:${ev.lineno ?? ""}`);
    }
    function onRejection(ev: PromiseRejectionEvent) {
      logErrorRef.current(`[unhandled rejection] ${String(ev.reason)}`);
    }
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ path: string; reason: string; quarantine_path: string }>(
      "athena:file-quarantined",
      (event) => {
        const payload = event.payload;
        if (payload && typeof payload.path === "string" && typeof payload.reason === "string") {
          setQuarantineAlert({ path: payload.path, reason: payload.reason });
        }
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ processed: number; total: number; failed: number }>(
      "athena:rebuild-progress",
      (event) => {
        const p = event.payload;
        if (p && typeof p.processed === "number" && typeof p.total === "number" && typeof p.failed === "number") {
          setRebuildVectorIndexProgress({ processed: p.processed, total: p.total, failed: p.failed });
        }
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const syncMeshRadioFromRust = async () => {
      try {
        const c = await invoke<boolean>("is_mesh_radio_connected");
        if (!cancelled) setMeshRadioConnected(c === true);
      } catch {
        if (!cancelled) setMeshRadioConnected(false);
      }
    };

    const setup = (async () => {
      await syncMeshRadioFromRust();
      const unlisten = await listen<{ connected: boolean }>("athena:mesh-connection", (event) => {
        if (cancelled) return;
        const p = event.payload;
        if (p && typeof p.connected === "boolean") {
          setMeshRadioConnected(p.connected === true);
        }
      });
      // Tauri `listen` is async; mesh may emit before the handler is registered—re-read atomic.
      await syncMeshRadioFromRust();
      return unlisten;
    })();

    return () => {
      cancelled = true;
      void setup.then(
        (unlisten) => unlisten(),
        () => {},
      );
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ sender_id: number; text: string }>("athena:mesh-text", (event) => {
      const p = event.payload;
      if (p && typeof p.sender_id === "number" && typeof p.text === "string") {
        if (p.text.trimStart().startsWith("ATHLB1:")) {
          return;
        }
        const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
        setMeshMessages((prev) =>
          appendMeshMessageCapped(prev, { id, senderId: p.sender_id, text: p.text, at: Date.now() }),
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (libraryOpen) {
      setLibraryError(null);
      loadWatchFolder();
      loadStats();
      loadLibraryFiles();
      invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    }
  }, [libraryOpen]);

  useEffect(() => {
    if (!libraryOpen || libraryActiveTab !== "maps") return;
    invoke<MapCatalogEntry[]>("get_map_catalog")
      .then(setMapCatalog)
      .catch(() => setMapCatalog([]));
    void invoke<boolean>("check_network_reachable")
      .then((r) => setNetworkReachable(r))
      .catch(() => setNetworkReachable(false));
    void invoke<MapCatalogEntry | null>("get_smallest_kiwix_map_suggestion")
      .then((s) => setMapsSuggestion(s ?? null))
      .catch(() => setMapsSuggestion(null));
  }, [libraryOpen, libraryActiveTab, featureMapEnabled, userLocationAddress]);

  useEffect(() => {
    let cancelled = false;

    function getPosition(): Promise<GeolocationPosition | null> {
      if (typeof navigator === "undefined" || !navigator.geolocation) return Promise.resolve(null);
      return new Promise((resolve) => {
        navigator.geolocation.getCurrentPosition(
          (pos) => resolve(pos),
          () => resolve(null),
          { timeout: 10000, maximumAge: 10 * 60 * 1000 }
        );
      });
    }

    function computeIsNight(sun: SunTimes | null): boolean {
      if (!sun || !sun.sunrise_iso || !sun.sunset_iso) return false;
      const now = new Date();
      const sunrise = new Date(sun.sunrise_iso);
      const sunset = new Date(sun.sunset_iso);
      if (Number.isNaN(sunrise.getTime()) || Number.isNaN(sunset.getTime())) return false;
      return now < sunrise || now > sunset;
    }

    async function refresh() {
      if (nightVisionMode === "off") {
        if (!cancelled) setIsNight(false);
        return;
      }
      const pos = await getPosition();
      if (!pos) return;
      try {
        const sun = await invoke<SunTimes>("get_sun_times", {
          lat: pos.coords.latitude,
          lon: pos.coords.longitude,
        });
        if (!cancelled) {
          setIsNight(computeIsNight(sun));
        }
      } catch {
        if (!cancelled) setIsNight(false);
      }
    }

    refresh();
    const id = window.setInterval(refresh, 10 * 60 * 1000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [nightVisionMode]);

  useEffect(() => {
    let cancelled = false;

    async function fetchBattery() {
      try {
        const state = await invoke<BatteryState>("get_battery_state");
        if (cancelled) return;
        setBatteryState(state);
      } catch (_) {
        if (!cancelled) {
          setBatteryState(null);
        }
      }
    }

    fetchBattery();
    const id = window.setInterval(fetchBattery, 3 * 60 * 1000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  useEffect(() => {
    if (!batteryState || lowBatteryDismissed) {
      setShowLowBatteryBanner(false);
      return;
    }
    const { percentage, is_charging } = batteryState;
    if (
      percentage !== null &&
      percentage < 20 &&
      is_charging === false &&
      modelOverride !== "small"
    ) {
      setShowLowBatteryBanner(true);
    } else {
      setShowLowBatteryBanner(false);
    }
  }, [batteryState, lowBatteryDismissed, modelOverride]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (activeTriageId) {
        setActiveTriageId(null);
        e.preventDefault();
      } else if (triageMenuOpen) {
        setTriageMenuOpen(false);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activeTriageId, triageMenuOpen]);

  /* Focus message input: Cmd/Ctrl+K or / (when not already in an input) */
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as Node;
      const inInput = target && (
        (target as HTMLElement).tagName === "INPUT" ||
        (target as HTMLElement).tagName === "TEXTAREA" ||
        (target as HTMLElement).isContentEditable
      );
      const isShortcut = (e.metaKey || e.ctrlKey) && e.key === "k";
      const isSlash = e.key === "/" && !inInput;
      if (!isShortcut && !isSlash) return;
      e.preventDefault();
      inputRef.current?.focus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  /* Find in chat: Cmd/Ctrl+F */
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        e.preventDefault();
        setFindInChatOpen((open) => !open);
        if (!findInChatOpen) setTimeout(() => findInChatInputRef.current?.focus(), 0);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [findInChatOpen]);

  const findMatchIndices = useMemo(() => {
    const q = findInChatQuery.trim().toLowerCase();
    if (!q) return [];
    return messages.map((m, i) => m.content.toLowerCase().includes(q) ? i : -1).filter((i) => i >= 0);
  }, [messages, findInChatQuery]);

  const findCurrentMessageIndex = findMatchIndices[Math.min(findInChatIndex, findMatchIndices.length - 1)] ?? null;

  useEffect(() => {
    if (findInChatQuery.trim() && findMatchIndices.length > 0) {
      setFindInChatIndex((i) => Math.min(i, findMatchIndices.length - 1));
    }
  }, [findInChatQuery, findMatchIndices.length]);

  useEffect(() => {
    if (findCurrentMessageIndex == null) return;
    const el = messageRefsRef.current[findCurrentMessageIndex];
    el?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [findCurrentMessageIndex, findInChatIndex]);

  useEffect(() => {
    if (!triageMenuOpen) return;
    const onPointerDown = (e: PointerEvent) => {
      const el = e.target as Node;
      const inHeader = triageMenuRef.current?.contains(el);
      const inPortal = document.body.querySelector(".triage-portal-root")?.contains(el);
      const inTriageTrigger = (e.target as HTMLElement | null)?.closest?.("[data-triage-trigger]");
      if (!inHeader && !inPortal && !inTriageTrigger) {
        setTriageMenuOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [triageMenuOpen]);

  async function loadLibraryFiles() {
    try {
      const list = await invoke<LibraryFile[]>("list_library_contents");
      setLibraryFiles(list);
    } catch (_) {
      setLibraryFiles([]);
    }
  }

  useEffect(() => {
    const run = { cancelled: false };
    const pushUnlisten = (u: () => void) => {
      if (run.cancelled) {
        u();
        return;
      }
      streamUnlistenRef.current.push(u);
    };
    listen<string>("athena:stream-chunk", (e) => {
      const now = Date.now();
      if (mapStreamActiveRef.current) {
        mapStreamContentRef.current += e.payload ?? "";
        mapStreamLastChunkAtRef.current = now;
        mapStreamGotFirstChunkRef.current = true;
      } else {
        const chunk = e.payload ?? "";
        streamLastChunkAtRef.current = now;
        streamGotFirstChunkRef.current = true;
        streamAssistantContentRef.current += chunk;
        setMessages((prev) => {
          const next = [...prev];
          const i = next.length - 1;
          if (i >= 0 && next[i].role === "assistant") {
            next[i] = { ...next[i], content: streamAssistantContentRef.current };
          }
          return next;
        });
      }
    }).then(pushUnlisten);
    listen<string[]>("athena:stream-sources", (e) => {
      const sources = e.payload;
      if (Array.isArray(sources) && sources.length > 0 && !mapStreamActiveRef.current) {
        setMessages((prev) => {
          const next = [...prev];
          const i = next.length - 1;
          if (i >= 0 && next[i].role === "assistant") {
            next[i] = { ...next[i], sources };
          }
          return next;
        });
      }
    }).then(pushUnlisten);
    listen("athena:stream-done", () => {
      if (mapStreamActiveRef.current) {
        mapStreamActiveRef.current = false;
        const content = mapStreamContentRef.current;
        mapStreamContentRef.current = "";
        setMapChatHistory((prev) => [...prev, { role: "assistant", content }]);
        setMapChatLastMessage(content);
        setMapChatSending(false);
      } else {
        setStreamStallMessage(null);
        setSending(false);
        setTimeout(() => {
          if (temporaryChatRef.current) return;
          const latest = messagesRef.current;
          if (latest.length > 0) {
            saveCurrentChat(latest).then(async (id) => {
              if (id && latest.length >= 2) {
                const lastUser = latest.filter((m) => m.role === "user").pop()?.content ?? "";
                const lastAssistant = latest.filter((m) => m.role === "assistant").pop()?.content ?? "";
                if (lastUser || lastAssistant) {
                  await invoke("update_chat_context_summary", {
                    id,
                    lastUserMsg: lastUser,
                    lastAssistantMsg: lastAssistant,
                  }).catch(() => {});
                }
              }
            });
          }
        }, 0);
        inputRef.current?.focus();
        if (conversationModeRef.current) {
          setTimeout(() => {
            if (!conversationModeRef.current) return;
            const lastIndex = messagesRef.current.length - 1;
            if (lastIndex >= 0) {
              const lastMsg = messagesRef.current[lastIndex];
              if (lastMsg?.role === "assistant" && lastMsg.content?.trim()) {
                speakMessageAtIndexRef.current?.(lastIndex, () => {
                  if (conversationModeRef.current) startConversationListeningRef.current?.();
                });
              } else {
                startConversationListeningRef.current?.();
              }
            } else {
              startConversationListeningRef.current?.();
            }
          }, 150);
        }
      }
    }).then(pushUnlisten);
    listen<string>("athena:stream-error", (e) => {
      const errMsg = e.payload ?? "Unknown error";
      logErrorRef.current(`Stream error: ${errMsg}`);
      if (mapStreamActiveRef.current) {
        mapStreamActiveRef.current = false;
        mapStreamContentRef.current = "";
        setMapChatLastMessage(`Error: ${errMsg}`);
        setMapChatSending(false);
      } else {
        setStreamStallMessage(null);
        setMessages((prev) => {
          const next = [...prev];
          const i = next.length - 1;
          if (i >= 0 && next[i].role === "assistant") {
            next[i] = { ...next[i], content: `Error: ${errMsg}` };
          }
          return next;
        });
        setSending(false);
      }
      inputRef.current?.focus();
      if (conversationModeRef.current) {
        setTimeout(() => startConversationListeningRef.current?.(), 500);
      }
    }).then(pushUnlisten);
    return () => {
      run.cancelled = true;
      streamUnlistenRef.current.forEach((u) => u());
      streamUnlistenRef.current = [];
    };
  }, []);

  useEffect(() => {
    const unlistens: Array<() => void> = [];
    listen<{ id: string; downloaded_bytes: number; total_bytes: number | null }>("athena:kiwix-progress", (e) => {
      const p = e.payload;
      if (p == null || typeof p.id !== "string") return;
      setDownloadJobs((prev) => {
        const cur = prev[p.id];
        const label = cur?.label ?? p.id.split("/").pop() ?? "Download";
        const total = p.total_bytes ?? cur?.total ?? null;
        return {
          ...prev,
          [p.id]: { downloaded: p.downloaded_bytes, total, label },
        };
      });
    }).then((u) => unlistens.push(u));
    listen<{ id: string; path: string }>("athena:kiwix-done", (e) => {
      const { id } = e.payload ?? {};
      if (id != null) {
        setDownloadJobs((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
        loadLibraryFiles();
        if (id.includes("/zim/maps")) {
          invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
        }
        if (id.startsWith("tiles:") || id.startsWith("import:") || id.startsWith("mbtiles_export:")) {
          invoke<OfflineBasemapPaths>("offline_basemap_paths_cmd").then(applyOfflineBasemapPaths).catch(() => {});
          invoke<TilePackInfo[]>("list_tile_packs_cmd").then(setTilePacks).catch(() => {});
        }
      }
    }).then((u) => unlistens.push(u));
    listen<{ id: string; error: string }>("athena:kiwix-error", (e) => {
      const { id, error } = e.payload ?? {};
      if (id != null) {
        setDownloadJobs((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
      }
      if (error != null) {
        const msg = `Download failed: ${error}`;
        setLibraryError(msg);
        logError(msg);
      }
    }).then((u) => unlistens.push(u));
    return () => unlistens.forEach((u) => u());
  }, [applyOfflineBasemapPaths]);

  useEffect(() => {
    invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    invoke<OfflineBasemapPaths>("offline_basemap_paths_cmd").then(applyOfflineBasemapPaths).catch(() => {});
    invoke<TilePackInfo[]>("list_tile_packs_cmd").then(setTilePacks).catch(() => {});
  }, [applyOfflineBasemapPaths]);

  useEffect(() => {
    if (settingsOpen && settingsPanel === "general") {
      setStartupViewPreference(readStartupView());
    }
  }, [settingsOpen, settingsPanel]);

  useEffect(() => {
    if (currentView !== "map") {
      mapStreamActiveRef.current = false;
      mapStreamContentRef.current = "";
      setMapChatSending(false);
    }
  }, [currentView]);

  useEffect(() => {
    if (mapBasemap === "offline_osm" && !offlineOsmRoot) setMapBasemap("online_osm");
    if (mapBasemap === "offline_topo" && !offlineTopoRoot) setMapBasemap("online_osm");
    if (mapBasemap === "offline_imported" && !offlineImportedRoot) setMapBasemap("online_osm");
    if (mapBasemap === "offline_archive_raster" && enabledArchiveRasterFileNames.length === 0) setMapBasemap("online_osm");
  }, [mapBasemap, offlineOsmRoot, offlineTopoRoot, offlineImportedRoot, enabledArchiveRasterFileNames]);

  useEffect(() => {
    if (enabledArchiveRasterFileNames.length === 0) {
      setArchiveRasterNativeZoomRangesByFile({});
      return;
    }
    let cancelled = false;
    void Promise.all(
      enabledArchiveRasterFileNames.map(async (fileName) => {
        const range = await invoke<[number, number] | null>("get_offline_mbtiles_zoom_range", { archiveFileName: fileName });
        return { fileName, range };
      }),
    )
      .then((rows) => {
        if (cancelled) return;
        const next: Record<string, [number, number]> = {};
        for (const row of rows) {
          const range = row.range;
          if (!Array.isArray(range) || range.length !== 2) continue;
          const minZoom = Number(range[0]);
          const maxZoom = Number(range[1]);
          if (!Number.isFinite(minZoom) || !Number.isFinite(maxZoom)) continue;
          next[row.fileName] = [minZoom, maxZoom];
        }
        setArchiveRasterNativeZoomRangesByFile(next);
      })
      .catch(() => {
        if (cancelled) return;
        setArchiveRasterNativeZoomRangesByFile({});
      });
    return () => {
      cancelled = true;
    };
  }, [enabledArchiveRasterFileNames]);

  useEffect(() => {
    if (enabledArchiveRasterFileNames.length === 0) {
      setArchiveRasterBoundsByFile({});
      return;
    }
    let cancelled = false;
    void Promise.all(
      enabledArchiveRasterFileNames.map(async (fileName) => {
        const bounds = await invoke<[number, number, number, number] | null>("get_offline_mbtiles_bounds", {
          archiveFileName: fileName,
        });
        return { fileName, bounds };
      }),
    )
      .then((rows) => {
        if (cancelled) return;
        const next: Record<string, [number, number, number, number]> = {};
        for (const row of rows) {
          const bounds = row.bounds;
          if (!Array.isArray(bounds) || bounds.length !== 4) continue;
          const minLon = Number(bounds[0]);
          const minLat = Number(bounds[1]);
          const maxLon = Number(bounds[2]);
          const maxLat = Number(bounds[3]);
          if (![minLon, minLat, maxLon, maxLat].every((v) => Number.isFinite(v))) continue;
          next[row.fileName] = [minLon, minLat, maxLon, maxLat];
        }
        setArchiveRasterBoundsByFile(next);
      })
      .catch(() => {
        if (cancelled) return;
        setArchiveRasterBoundsByFile({});
      });
    return () => {
      cancelled = true;
    };
  }, [enabledArchiveRasterFileNames]);

  const handleMapSatelliteDownloaded = useCallback((mapId: string) => {
    const normalized = mapId.trim();
    if (!normalized) return;
    const fileName = `${normalized}.mbtiles`;
    setEnabledArchiveRasterFileNames((previous) => (previous.includes(fileName) ? previous : [...previous, fileName]));
    setMapBasemap("offline_archive_raster");
    void invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
  }, []);

  const handleMainMapArchiveRasterLayersChange = useCallback((archiveFileNames: string[]) => {
    const normalized = [...new Set(archiveFileNames.map((name) => name.trim()).filter((name) => name.length > 0))].sort(
      (a, b) => a.localeCompare(b),
    );
    setEnabledArchiveRasterFileNames((previous) => {
      const previousNormalized = [...previous].sort((a, b) => a.localeCompare(b));
      if (
        previousNormalized.length === normalized.length &&
        previousNormalized.every((value, index) => value === normalized[index])
      ) {
        return previous;
      }
      return normalized;
    });
  }, []);

  const reloadTacticalMapLayers = useCallback(() => {
    void invoke<{ points_of_interest: TacticalMapPointOfInterest[]; mesh_node_locations: TacticalMapMeshNodeLocation[] }>(
      "get_tactical_map_layers",
    )
      .then((payload) => {
        setTacticalMapPoints(payload.points_of_interest);
        setTacticalMapNodes(payload.mesh_node_locations);
      })
      .catch(() => {
        setTacticalMapPoints([]);
        setTacticalMapNodes([]);
      });
  }, []);

  const handleAddTacticalPoiPin = useCallback(
    async (payload: {
      name: string;
      description: string;
      lucideIconName: string;
      pinLayer: "resource" | "network";
      lat: number;
      lng: number;
    }) => {
      await invoke<TacticalMapPointOfInterest>("add_tactical_poi", {
        name: payload.name,
        lat: payload.lat,
        lon: payload.lng,
        lucideIconName: payload.lucideIconName.trim().length > 0 ? payload.lucideIconName : null,
        description: payload.description.trim().length > 0 ? payload.description : null,
        pinLayer: payload.pinLayer,
      });
      reloadTacticalMapLayers();
    },
    [reloadTacticalMapLayers],
  );

  useEffect(() => {
    if (currentView !== "map") return;
    let cancelled = false;
    void invoke<Poi[]>("get_pois")
      .then((list) => {
        if (cancelled) return;
        setPois(list);
        if (list.length > 0) {
          setMapViewCenter([list[0].lat, list[0].lon]);
          setMapViewZoom(12);
        } else {
          setMapViewCenter([39.5, -98.35]);
          setMapViewZoom(4);
        }
      })
      .catch(() => {});
    void invoke<OfflineBasemapPaths>("offline_basemap_paths_cmd")
      .then((r) => {
        if (cancelled) return;
        applyOfflineBasemapPaths(r);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [currentView, applyOfflineBasemapPaths]);

  useEffect(() => {
    if (currentView !== "map") return;
    reloadTacticalMapLayers();
  }, [currentView, reloadTacticalMapLayers]);

  useEffect(() => {
    setTilePackEstimate(null);
  }, [tileRadiusMiles, tileZoomMin, tileZoomMax, tileGeocodeQuery]);

  useEffect(() => {
    if (!settingsOpen || settingsPanel !== "location") return;
    void invoke<boolean>("check_network_reachable")
      .then((r) => setNetworkReachable(r))
      .catch(() => setNetworkReachable(false));
  }, [settingsOpen, settingsPanel]);

  useEffect(() => {
    if (!settingsOpen || settingsPanel !== "maps") return;
    invoke<MapCatalogEntry[]>("get_map_catalog")
      .then(setMapCatalog)
      .catch(() => setMapCatalog([]));
    invoke<DownloadedMap[]>("get_downloaded_maps").then(setDownloadedMaps).catch(() => {});
    void invoke<boolean>("check_network_reachable")
      .then((r) => setNetworkReachable(r))
      .catch(() => setNetworkReachable(false));
    void invoke<MapCatalogEntry | null>("get_smallest_kiwix_map_suggestion")
      .then((s) => setMapsSuggestion(s ?? null))
      .catch(() => setMapsSuggestion(null));
    void invoke<TilePackInfo[]>("list_tile_packs_cmd").then(setTilePacks).catch(() => {});
    void invoke<OfflineBasemapPaths>("offline_basemap_paths_cmd").then(applyOfflineBasemapPaths).catch(() => {});
  }, [settingsOpen, settingsPanel, featureMapEnabled, userLocationAddress, applyOfflineBasemapPaths]);

  useEffect(() => {
    if (!localDataInstallOpen) return;
    setMapsSuggestionLoading(true);
    setMapsSuggestion(null);
    void invoke<boolean>("check_network_reachable")
      .then((r) => setNetworkReachable(r))
      .catch(() => setNetworkReachable(false));
    void invoke<MapCatalogEntry | null>("get_smallest_kiwix_map_suggestion")
      .then((s) => setMapsSuggestion(s ?? null))
      .catch(() => setMapsSuggestion(null))
      .finally(() => setMapsSuggestionLoading(false));
  }, [localDataInstallOpen]);

  useEffect(() => {
    const previewThemeStudio = Boolean(
      settingsOpen && settingsPanel === "themes" && themesSubView === "studio" && studioDraft
    );
    const displayTheme: ThemeOrCustom = previewThemeStudio ? "custom" : theme;
    const studioBaseColors = studioDraft ? resolveBaseColors(studioDraft.base) : null;
    const displayCustomBg =
      previewThemeStudio && studioDraft
        ? studioDraft.bg_color ?? studioBaseColors!.bg
        : customBg;
    const displayCustomPrimary =
      previewThemeStudio && studioDraft
        ? studioDraft.primary_color ?? studioBaseColors!.primary
        : customPrimary;
    const resolvedThemeBgHex =
      displayTheme === "custom" ? displayCustomBg : PRESET_COLORS[displayTheme as Theme].bg;
    const resolvedThemeAccentHex =
      displayTheme === "custom" ? displayCustomPrimary : PRESET_COLORS[displayTheme as Theme].primary;

    const previewFalloutPersona =
      previewThemeStudio && studioDraft?.personality_id === "fallout";
    const isFalloutCrt = activeThemeId === "fallout" || previewFalloutPersona || activePersonalityId === "fallout";

    const previewSurvivalManual =
      previewThemeStudio &&
      studioDraft &&
      (studioDraft.base === "fieldguide" || studioDraft.base === "survival-manual");
    // Only the explicit "survival manual" book preset uses paper/vellum chrome. Do not tie `fieldguide`
    // here — that left `body.theme-survival-manual` stuck when switching back to the default guide look.
    const isSurvivalManualTheme = activeThemeId === "survival-manual" || previewSurvivalManual;

    const root = document.documentElement;
    let bgRgb =
      hexToRgbCsv(resolvedThemeBgHex) ?? hexToRgbCsv(DEFAULT_CUSTOM.bg) ?? DEFAULT_THEME_BG_RGB;
    let accentRgb =
      hexToRgbCsv(resolvedThemeAccentHex) ?? hexToRgbCsv(DEFAULT_CUSTOM.primary) ?? DEFAULT_THEME_ACCENT_RGB;
    if (isFalloutCrt) {
      bgRgb = FALLOUT_DEFAULT_BG_RGB;
      accentRgb = FALLOUT_DEFAULT_ACCENT_RGB;
    }
    root.style.setProperty("--theme-bg-rgb", bgRgb);
    root.style.setProperty("--theme-accent-rgb", accentRgb);
    if (isFalloutCrt) {
      document.body.classList.add("theme-fallout");
    } else {
      document.body.classList.remove("theme-fallout");
    }

    if (isSurvivalManualTheme) {
      document.body.classList.add("theme-survival-manual");
    } else {
      document.body.classList.remove("theme-survival-manual");
    }

    return () => {
      document.body.classList.remove("theme-fallout");
      document.body.classList.remove("theme-survival-manual");
    };
  }, [
    theme,
    customBg,
    customPrimary,
    settingsOpen,
    settingsPanel,
    themesSubView,
    studioDraft,
    activeThemeId,
    activePersonalityId,
  ]);

  async function handleSend(overrideText?: string) {
    const text = (overrideText !== undefined ? overrideText : input).trim();
    if (!text || sending) return;
    const attachNote =
      attachments.length > 0
        ? `\n\n[Attached files for this question: ${attachments
            .map((a) => a.name)
            .join(", ")}. Use them if helpful.]`
        : "";
    let ragContextString = "";
    let ragSources: string[] = [];
    try {
      let ragFilters: RagSearchFilters | undefined;
      try {
        const raw = window.localStorage.getItem("athena-rag-search-filters-v1");
        if (raw) {
          ragFilters = JSON.parse(raw) as RagSearchFilters;
        }
      } catch {
        ragFilters = undefined;
      }
      const ragResults = await invoke<RagSearchChunk[]>("search_rag_chunks", {
        query: text,
        limit: 3,
        filters: ragFilters,
      });
      if (Array.isArray(ragResults) && ragResults.length > 0) {
        ragContextString = ragResults
          .map((r) => (typeof r.content === "string" ? r.content.trim() : ""))
          .filter((s) => s.length > 0)
          .join("\n\n");
        ragSources = Array.from(
          new Set(
            ragResults
              .map((r) =>
                typeof r.source === "string" && r.source.trim().length > 0
                  ? r.source.trim()
                  : typeof r.document_id === "string"
                    ? r.document_id.trim()
                    : "",
              )
              .filter((s) => s.length > 0),
          ),
        );
      }
    } catch (_) {
      // Graceful fallback: continue without RAG context.
    }

    let spatialContextString = "";
    try {
      const spatial = await invoke<string>("get_spatial_context");
      spatialContextString = typeof spatial === "string" ? spatial.trim() : "";
    } catch (_) {
      // Graceful fallback: continue without spatial context.
    }

    let radioTrafficContextString = "";
    try {
      const recentRadio = await invoke<RadioLogEntry[]>("get_recent_radio_logs");
      if (Array.isArray(recentRadio) && recentRadio.length > 0) {
        radioTrafficContextString = recentRadio
          .map((r) => {
            const ts = typeof r.timestamp === "string" ? r.timestamp : "";
            const dir = typeof r.direction === "string" ? r.direction : "";
            const msg = typeof r.message === "string" ? r.message : "";
            return `Radio Log [${ts}] [${dir}]: ${msg}`;
          })
          .join("\n");
      }
    } catch (_) {
      // Graceful fallback: continue without radio traffic context.
    }

    const promptForModel =
      ragContextString || spatialContextString || radioTrafficContextString
        ? `Use the following document context, spatial map context, and recent radio traffic context to answer the user's question.\n\nDocument Context: ${ragContextString || "(none)"}\n\nSpatial Map Context: ${spatialContextString || "(none)"}\n\nRecent Radio Traffic Context: ${radioTrafficContextString || "(none)"}\n\nQuestion: ${text}`
        : text;

    const localSourcesLabel =
      ragSources.length > 0
        ? ragSources
        : ragContextString || spatialContextString || radioTrafficContextString
          ? ["Local Library"]
          : [];
    const userMessage: ChatMessage = {
      role: "user",
      content: text + attachNote,
      timestamp: Date.now(),
      ...(localSourcesLabel.length > 0 ? { sources: localSourcesLabel } : {}),
    };
    const currentMessages = messages;
    if (text.toLowerCase() === "/rag-count") {
      setInput("");
      setMessages((prev) => [...prev, userMessage, { role: "assistant", content: "", timestamp: Date.now() }]);
      setSending(true);
      streamSendStartRef.current = Date.now();
      streamGotFirstChunkRef.current = true;
      streamLastChunkAtRef.current = Date.now();
      try {
        const stats = await invoke<{ document_count: number; chunk_count: number }>("get_rag_stats");
        const assistantMessage: ChatMessage = {
          role: "assistant",
          content: `RAG index currently has ${stats.document_count} documents and ${stats.chunk_count} semantic chunks.`,
          timestamp: Date.now(),
        };
        setMessages((prev) => {
          const next = [...prev];
          if (next.length > 0 && next[next.length - 1].role === "assistant" && next[next.length - 1].content === "") {
            next[next.length - 1] = assistantMessage;
            return next;
          }
          return [...prev, assistantMessage];
        });
      } catch (e) {
        logError(String(e));
        setMessages((prev) => {
          const next = [...prev];
          const i = next.length - 1;
          if (i >= 0 && next[i].role === "assistant") next[i] = { ...next[i], content: `Error: ${e}` };
          return next;
        });
      }
      setSending(false);
      return;
    }
    // If we have prior messages but no chat id yet (e.g. user sent follow-up before stream-done save finished),
    // save now and update context summary so the backend can use conversation context for retrieval.
    let chatIdToUse: string | null = currentChatIdRef.current ?? null;
    if (currentMessages.length >= 2 && !chatIdToUse) {
      const id = await saveCurrentChat(currentMessages);
      if (id) {
        const lastUser = currentMessages.filter((m) => m.role === "user").pop()?.content ?? "";
        const lastAssistant = currentMessages.filter((m) => m.role === "assistant").pop()?.content ?? "";
        if (lastUser || lastAssistant) {
          await invoke("update_chat_context_summary", {
            id,
            lastUserMsg: lastUser,
            lastAssistantMsg: lastAssistant,
          }).catch(() => {});
        }
        chatIdToUse = id;
      }
    }

    setInput("");
    streamAssistantContentRef.current = "";
    setMessages((prev) => [
      ...prev,
      userMessage,
      {
        role: "assistant",
        content: "",
        timestamp: Date.now(),
        ...(localSourcesLabel.length > 0 ? { sources: localSourcesLabel } : {}),
      },
    ]);
    setAttachments([]);
    streamSendStartRef.current = Date.now();
    streamLastChunkAtRef.current = null;
    streamGotFirstChunkRef.current = false;
    setStreamStallMessage(null);
    setSending(true);
    const { userLat, userLon } = await resolveGpsForChatPayload();
    invoke("send_message_stream", {
      message: promptForModel,
      history: currentMessages.map((m) => ({ role: m.role, content: m.content })),
      userLat,
      userLon,
      chatId: chatIdToUse,
      modelOptions: { temperature: aiTemperature, num_ctx: aiNumCtx },
    }).catch((e) => {
      const errMsg = e != null ? String(e) : "Send failed";
      logError(errMsg);
      setStreamStallMessage(null);
      setLibraryError(`Message failed: ${errMsg}`);
      setMessages((prev) => {
        const next = [...prev];
        const i = next.length - 1;
        if (i >= 0 && next[i].role === "assistant") {
          next[i] = { ...next[i], content: `Error: ${e}` };
        }
        return next;
      });
      setSending(false);
      inputRef.current?.focus();
    });
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  async function handleAttachImages() {
    setToolsOpen(false);
    const selected = await open({
      directory: false,
      multiple: true,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    const items = paths.map((p) => {
      const name = p.split(/[\\/]/).pop() || p;
      return { id: `${Date.now()}-${name}-${Math.random().toString(36).slice(2)}`, kind: "image" as const, name, path: p };
    });
    setAttachments((prev) => [...prev, ...items]);
    try {
      setLibraryError(null);
      await invoke("ingest_files", { paths });
    } catch (e) {
      const msg = e != null ? String(e) : "Ingest failed";
      setLibraryError(msg);
      logError(msg);
    }
  }

  async function handleAttachFiles() {
    setToolsOpen(false);
    const selected = await open({
      directory: false,
      multiple: true,
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    const items = paths.map((p) => {
      const name = p.split(/[\\/]/).pop() || p;
      return { id: `${Date.now()}-${name}-${Math.random().toString(36).slice(2)}`, kind: "file" as const, name, path: p };
    });
    setAttachments((prev) => [...prev, ...items]);
    try {
      setLibraryError(null);
      await invoke("ingest_files", { paths });
    } catch (e) {
      const msg = e != null ? String(e) : "Ingest failed";
      setLibraryError(msg);
      logError(msg);
    }
  }

  async function handleMapChatSend() {
    const text = mapChatInput.trim();
    if (!text || mapChatSending) return;
    const userMsg = { role: "user" as const, content: text };
    const newHistory = [...mapChatHistory, userMsg];
    setMapChatHistory(newHistory);
    setMapChatInput("");
    setMapChatSending(true);
    mapStreamSendStartRef.current = Date.now();
    mapStreamLastChunkAtRef.current = null;
    mapStreamGotFirstChunkRef.current = false;
    mapStreamActiveRef.current = true;
    mapStreamContentRef.current = "";

    let userLat: number | null = null;
    let userLon: number | null = null;
    const lower = text.toLowerCase();
    const wantsMarkLocation =
      (lower.includes("mark") && (lower.includes("location") || lower.includes("my location") || lower.includes("current"))) ||
      lower.includes("add my location") ||
      lower.includes("where am i");

    if (wantsMarkLocation && typeof navigator !== "undefined" && navigator.geolocation) {
      try {
        const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
          navigator.geolocation.getCurrentPosition(resolve, reject, { timeout: 10000, maximumAge: 60000 });
        });
        userLat = pos.coords.latitude;
        userLon = pos.coords.longitude;
        lastKnownGpsRef.current = { lat: userLat, lon: userLon };
        await handleAddPoi("My location", userLat, userLon, "Current GPS position");
      } catch (_) {
        /* ignore; send without coords */
      }
    } else if (typeof navigator !== "undefined" && navigator.geolocation) {
      // Short timeout so Enter/send is not blocked for seconds on every message (Leaflet map is usable without GPS).
      try {
        const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
          navigator.geolocation.getCurrentPosition(resolve, reject, { timeout: 1200, maximumAge: 120000 });
        });
        userLat = pos.coords.latitude;
        userLon = pos.coords.longitude;
        lastKnownGpsRef.current = { lat: userLat, lon: userLon };
      } catch (_) {
        /* optional; send without coords */
      }
    }

    if (userLat === null && lastKnownGpsRef.current) {
      userLat = lastKnownGpsRef.current.lat;
      userLon = lastKnownGpsRef.current.lon;
    }
    if (userLat === null && featureMapEnabled && currentView === "map") {
      userLat = mapViewCenter[0];
      userLon = mapViewCenter[1];
    }

    try {
      setLibraryError(null);
      await invoke("send_message_stream", {
        message: text,
        history: newHistory.slice(0, -1).map((m) => ({ role: m.role, content: m.content })),
        userLat,
        userLon,
        chatId: null,
        modelOptions: { temperature: aiTemperature, num_ctx: aiNumCtx },
      });
    } catch (e) {
      const errMsg = e != null ? String(e) : "Send failed";
      mapStreamActiveRef.current = false;
      setMapChatLastMessage(errMsg);
      setMapChatSending(false);
      setLibraryError(`Map chat: ${errMsg}`);
      logError(`Map chat send: ${errMsg}`);
    }
  }
  async function exportChatToPdf() {
    if (!messages.length || exportingPdf) return;
    try {
      setExportingPdf(true);
      const pdfDoc = await PDFDocument.create();
      const pageMargin = 50;
      const page = pdfDoc.addPage();
      const { width, height } = page.getSize();
      const fontSize = 11;
      const contentWidth = width - pageMargin * 2;
      const font = await pdfDoc.embedFont(StandardFonts.Helvetica);

      let cursorY = height - pageMargin;

      const addLine = (text: string, isHeader = false) => {
        const usedFontSize = isHeader ? 14 : fontSize;
        const usedLineHeight = usedFontSize * 1.5;
        if (cursorY - usedLineHeight < pageMargin) {
          const newPage = pdfDoc.addPage();
          cursorY = newPage.getSize().height - pageMargin;
        }
        const currentPage = pdfDoc.getPages()[pdfDoc.getPages().length - 1];
        currentPage.drawText(text, {
          x: pageMargin,
          y: cursorY,
          size: usedFontSize,
          font,
          color: rgb(0, 0, 0),
        });
        cursorY -= usedLineHeight;
      };

      const wrapText = (text: string): string[] => {
        const words = text.split(/\s+/);
        const lines: string[] = [];
        let line = "";
        for (const word of words) {
          const test = line ? `${line} ${word}` : word;
          const width = font.widthOfTextAtSize(test, fontSize);
          if (width > contentWidth && line) {
            lines.push(line);
            line = word;
          } else {
            line = test;
          }
        }
        if (line) lines.push(line);
        return lines;
      };

      const titleText = currentChatId ? chatList.find((c) => c.id === currentChatId)?.title || "Chat export" : chatTitle(messages);
      addLine("Athena chat export", true);
      addLine(titleText);
      addLine(new Date().toLocaleString());
      addLine("");

      for (const msg of messages) {
        addLine(msg.role === "user" ? "You:" : "Athena:", true);
        const paragraphs = msg.content.split(/\n{2,}/);
        for (const para of paragraphs) {
          const lines = wrapText(para.replace(/\s+/g, " ").trim());
          for (const ln of lines) {
            addLine(ln);
          }
          addLine("");
        }
      }

      const pdfBytes = await pdfDoc.save();
      const blob = new Blob([pdfBytes], { type: "application/pdf" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      const safeTitle = titleText.replace(/[^a-z0-9\-]+/gi, "_").slice(0, 60) || "athena_chat";
      a.href = url;
      a.download = `${safeTitle}.pdf`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (e) {
      logError(String(e));
    } finally {
      setExportingPdf(false);
    }
  }

  const filteredChatList = chatSearch.trim()
    ? chatList.filter(
        (c) =>
          c.title.toLowerCase().includes(chatSearch.trim().toLowerCase()) ||
          (c.description ?? "").toLowerCase().includes(chatSearch.trim().toLowerCase())
      )
    : chatList;

  const sortedChatList = [...filteredChatList].sort((a, b) => {
    const aPinned = pinnedChatIds.includes(a.id);
    const bPinned = pinnedChatIds.includes(b.id);
    if (aPinned && !bPinned) return -1;
    if (!aPinned && bPinned) return 1;
    const aOpened = lastOpenedMap[a.id] ?? 0;
    const bOpened = lastOpenedMap[b.id] ?? 0;
    if (bOpened !== aOpened) return bOpened - aOpened;
    return b.updated_at - a.updated_at;
  });

  const activeCustomTheme = selectedCustomThemeId
    ? customThemes.find((t) => t.id === selectedCustomThemeId) ?? null
    : null;

  const previewThemeStudio = Boolean(
    settingsOpen && settingsPanel === "themes" && themesSubView === "studio" && studioDraft
  );
  const displayTheme: ThemeOrCustom = previewThemeStudio ? "custom" : theme;
  const studioBaseColors = studioDraft ? resolveBaseColors(studioDraft.base) : null;
  const displayCustomBg =
    previewThemeStudio && studioDraft
      ? studioDraft.bg_color ?? studioBaseColors!.bg
      : customBg;
  const displayCustomPrimary =
    previewThemeStudio && studioDraft
      ? studioDraft.primary_color ?? studioBaseColors!.primary
      : customPrimary;
  const displayCustomText =
    previewThemeStudio && studioDraft
      ? studioDraft.text_color ?? studioBaseColors!.text
      : customText;

  const displayPersona: PersonalityId = (() => {
    if (previewThemeStudio && studioDraft?.personality_id) {
      const p = studioDraft.personality_id;
      if (p === "fallout" || p === "fieldguide" || p === "default") return p;
    }
    return activePersonalityId;
  })();

  const customThemeForFx: BackendCustomTheme | null = previewThemeStudio && studioDraft
    ? studioDraft
    : activeCustomTheme;

  const appStyle: (React.CSSProperties & Record<string, string>) =
    displayTheme === "custom"
      ? ({
          "--custom-bg": displayCustomBg,
          "--custom-primary": displayCustomPrimary,
          "--custom-text": displayCustomText,
        } as React.CSSProperties & Record<string, string>)
      : ({} as React.CSSProperties & Record<string, string>);

  const fontStackOverride = resolveFontStack(
    previewThemeStudio ? studioDraft?.font_family ?? null : activeCustomTheme?.font_family,
    displayPersona
  );
  if (fontStackOverride) {
    appStyle["--font"] = fontStackOverride;
  }

  if (customThemeForFx?.scanlines_opacity != null) {
    appStyle["--fx-scanline-opacity"] = String(customThemeForFx.scanlines_opacity);
  }
  if (customThemeForFx != null && (customThemeForFx.film_grain ?? 0) > 0) {
    appStyle["--fx-grain"] = String(customThemeForFx.film_grain);
  }

  return (
    <div
      ref={appRef}
      className="app"
      data-theme={displayTheme}
      data-persona={displayPersona}
      data-fx-scanlines={customThemeForFx?.scanlines_enabled ? "1" : undefined}
      data-fx-vignette={customThemeForFx?.vignette_enabled ? "1" : undefined}
      data-fx-grain={(customThemeForFx?.film_grain ?? 0) > 0 ? "1" : undefined}
      data-fx-sharp={customThemeForFx?.sharp_corners ? "1" : undefined}
      style={Object.keys(appStyle).length ? appStyle : undefined}
    >
      <div className="app-shell">
        <UniversalHeader
          currentView={currentView}
          setCurrentView={setCurrentView}
          featureMapEnabled={featureMapEnabled}
          meshRadioConnected={meshRadioConnected === true}
          onOpenLibrary={() => setLibraryOpen(true)}
          onOpenSettings={() => setSettingsOpen(true)}
          onOpenHelp={() => setHelpOpen(true)}
          onOpenMesh={() => setMeshOpen(true)}
          batteryState={batteryState}
          headerDownloadBar={headerDownloadBar}
        />
        {(triageMenuOpen || activeTriageId) &&
          createPortal(
            <div
              className="app triage-portal-root"
              data-theme={displayTheme}
              data-persona={displayPersona}
              style={appStyle}
            >
              {activeTriageId ? (() => {
                const item = getTriageItem(activeTriageId);
                if (!item) return null;
                return (
                  <div className="triage-overlay" role="dialog" aria-modal="true" aria-label={`Triage checklist: ${item.title}`}>
                    <div className="triage-overlay-backdrop" aria-hidden onClick={() => setActiveTriageId(null)} />
                    <div className="triage-overlay-panel">
                      {displayPersona === "fieldguide" && (
                        <div className="fieldguide-hole-punches fieldguide-hole-punches-triage" aria-hidden>
                          <span /><span /><span />
                        </div>
                      )}
                      <header className="triage-panel-header triage-panel-header-sticky">
                        <div>
                          <div className="triage-panel-tag">Emergency checklist</div>
                          <h2 className="triage-panel-title">{item.title}</h2>
                          <p className="triage-panel-summary">{item.summary}</p>
                        </div>
                        <button type="button" className="triage-panel-close" onClick={() => setActiveTriageId(null)} aria-label="Close triage checklist and return to chat">×</button>
                      </header>
                      <div className="triage-panel-body">
                        <section className="triage-panel" aria-label={`Triage checklist: ${item.title}`}>
                          <div className="triage-column">
                            <h3>Do this first</h3>
                            <ol className="triage-steps">{item.steps.map((step, idx) => <li key={idx}>{step}</li>)}</ol>
                          </div>
                          <div className="triage-column">
                            <div className="triage-do-dont">
                              <h3>Do</h3>
                              <ul>{item.do.map((d, idx) => <li key={idx}>{d}</li>)}</ul>
                            </div>
                            <div className="triage-do-dont triage-do-not">
                              <h3>Do NOT</h3>
                              <ul>{item.doNot.map((d, idx) => <li key={idx}>{d}</li>)}</ul>
                            </div>
                            {item.notes && <p className="triage-notes"><strong>Notes:</strong> {item.notes}</p>}
                          </div>
                        </section>
                      </div>
                    </div>
                  </div>
                );
              })() : (
                <>
                  <div className="triage-menu-backdrop" aria-hidden onClick={() => setTriageMenuOpen(false)} />
                  <div className="triage-menu triage-menu-portal" role="menu" aria-label="Emergency triage shortcuts">
                    {TRIAGE_ITEMS.map((item) => (
                      <button
                        key={item.id}
                        type="button"
                        className="triage-menu-item"
                        onClick={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setActiveTriageId(item.id);
                          setTriageMenuOpen(false);
                        }}
                        role="menuitem"
                      >
                        <span className="triage-menu-icon" aria-hidden>{renderTriageIcon(item.id)}</span>
                        <span className="triage-menu-text">
                          <span className="triage-menu-title">{item.title}</span>
                          <span className="triage-menu-summary">{item.summary}</span>
                        </span>
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>,
            document.body
          )}
        <div className="app-content-stage">
          {currentView === "home" ? (
            <div className="home-view-scroll">
              <DashboardView
                onOpenTriage={() => setTriageMenuOpen(true)}
                onNavigate={(destination: DashboardDestination) => {
                  if (destination === "library") {
                    setLibraryOpen(true);
                    return;
                  }
                  if (destination === "settings") {
                    setSettingsOpen(true);
                    return;
                  }
                  setCurrentView(destination);
                }}
              />
            </div>
          ) : null}
          {currentView === "chat" ? (
            <div className="app-chat-workspace">
      <aside className={`chat-sidebar ${chatsOpen ? "" : "chat-sidebar-collapsed"}`}>
        <div className="sidebar-head">
          <button
            type="button"
            className="sidebar-collapse-btn"
            onClick={() => setChatsOpen((open) => !open)}
            title={chatsOpen ? "Collapse sidebar" : "Expand sidebar"}
            aria-label={chatsOpen ? "Collapse sidebar" : "Expand sidebar"}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="M4 6h16" />
              <path d="M4 12h10" />
              <path d="M4 18h8" />
            </svg>
          </button>
        </div>
        <div className="sidebar-brand">
          <h1 className="sidebar-title">Athena</h1>
          <p className="sidebar-tagline">Local offline survival & repair guide</p>
        </div>
        <div className="sidebar-chat-actions">
          <button
            type="button"
            className="sidebar-new-chat"
            onClick={newChat}
            title="New chat"
          >
            <span className="sidebar-new-chat-icon" aria-hidden>
              <span className="sidebar-new-chat-icon-mask" />
            </span>
            <span className="sidebar-new-chat-label">New chat</span>
          </button>
          <button
            type="button"
            className={`sidebar-temp-chat ${temporaryChat ? "sidebar-temp-chat-active" : ""}`}
            onClick={() => {
              setTemporaryChat(true);
              setMessages([]);
              setCurrentChatId(null);
            }}
            title="Temporary chat (not saved)"
            aria-label="Temporary chat (not saved)"
          >
            <span className="sidebar-temp-chat-icon" aria-hidden>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M4 4h12v12H4z" />
                <path d="M9 9h7" />
                <path d="M9 13h5" />
              </svg>
            </span>
            <span className="sidebar-temp-chat-label">Temporary chat</span>
          </button>
        </div>
        <div className="sidebar-search-wrap">
          <input
            type="search"
            className="sidebar-search"
            placeholder="Search chats…"
            value={chatSearch}
            onChange={(e) => setChatSearch(e.target.value)}
            aria-label="Search previous chats"
          />
        </div>
        <ul className="sidebar-chat-list">
          {sortedChatList.length === 0 ? (
            <li className="sidebar-chat-empty text-muted">
              {chatList.length === 0 ? "No chats yet" : "No matching chats"}
            </li>
          ) : (
            sortedChatList.map((c, index) => (
              <li key={c.id} className={`sidebar-chat-item ${pinnedChatIds.includes(c.id) ? "sidebar-chat-item-pinned" : ""}`}>
                <button
                  type="button"
                  className={`sidebar-chat-btn ${currentChatId === c.id ? "active" : ""}`}
                  onClick={() => loadChat(c.id)}
                  title={c.description ? `${c.title}\n\n${c.description}` : c.title}
                >
                  <span className="sidebar-chat-title">{c.title}</span>
                  {activePersonalityId === "fieldguide" && (
                    <>
                      <span className="sidebar-chat-index-dots" aria-hidden />
                      <span className="sidebar-chat-index-page">{index + 1}</span>
                    </>
                  )}
                </button>
                <button
                  type="button"
                  className={`sidebar-chat-pin ${pinnedChatIds.includes(c.id) ? "sidebar-chat-pin-active" : ""}`}
                  onClick={(ev) => {
                    ev.stopPropagation();
                    togglePinChat(c.id);
                  }}
                  title={pinnedChatIds.includes(c.id) ? "Unpin" : "Pin to top"}
                  aria-label={pinnedChatIds.includes(c.id) ? "Unpin chat" : "Pin chat to top"}
                >
                  📌
                </button>
                <button
                  type="button"
                  className="sidebar-chat-delete"
                  onClick={(ev) => {
                    ev.stopPropagation();
                    deleteChat(c.id);
                  }}
                  title="Delete chat"
                  aria-label="Delete chat"
                >
                  ×
                </button>
              </li>
            ))
          )}
        </ul>
      </aside>

      <main className="chat-main glass-panel">
        {activePersonalityId === "fieldguide" && (
          <div className="fieldguide-hole-punches" aria-hidden>
            <span /><span /><span />
          </div>
        )}
        <header className="main-header">
          <div className="main-header-toolbar">
          <button
            type="button"
            className="main-header-icon-btn"
            onClick={exportChatToPdf}
            title="Export current chat as PDF"
            aria-label="Export current chat as PDF"
            disabled={!messages.length || exportingPdf}
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M6 2h9l5 5v15H6z" />
              <path d="M15 2v5h5" />
              <path d="M9 13h6" />
              <path d="M9 17h3" />
            </svg>
          </button>
          <div className="main-header-spacer" />
          <div className="main-header-triage" ref={triageMenuRef} data-triage-trigger>
            <button
              type="button"
              className="main-header-icon-btn"
              onClick={() => setTriageMenuOpen((open) => !open)}
              title="Emergency triage checklists"
              aria-label="Emergency triage checklists"
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <rect x="5" y="6" width="14" height="14" rx="2" />
                <path d="M7 9h10" />
                <path d="M12 11v6" />
                <path d="M9 14h6" />
              </svg>
            </button>
          </div>
          </div>
        </header>
        {!aiInferenceReady && !ollamaNotRunningBannerDismissed && (
          <div className="ollama-not-running-banner" role="status" aria-live="polite">
            {inferenceBackendMode === "openai_compatible" ? (
              <>
                <span className="ollama-not-running-message">
                  Local OpenAI-compatible inference isn&apos;t reachable on <code>127.0.0.1</code> / <code>localhost</code>.
                  Start LM Studio, llama.cpp server, or similar, then set the base URL under <strong>Settings → AI</strong>.
                </span>
                <div className="ollama-not-running-actions">
                  <button
                    type="button"
                    className="ollama-not-running-dismiss"
                    onClick={() => setOllamaNotRunningBannerDismissed(true)}
                    aria-label="Dismiss"
                  >
                    Dismiss
                  </button>
                </div>
              </>
            ) : (
              <>
            {ollamaSetupProgress ? (
              <span className="ollama-not-running-message ollama-setup-progress-line">{ollamaSetupProgress}</span>
            ) : null}
            {ollamaSetupLastError ? (
              <div className="ollama-setup-error-block" role="alert">
                <span className="ollama-not-running-message ollama-setup-error-line">
                  Could not download Ollama automatically: {ollamaSetupLastError}
                </span>
                {!ollamaSidecarManaged ? (
                  <button
                    type="button"
                    className="ollama-retry-download"
                    disabled={ollamaInstallBusy || ollamaSetupProgress != null}
                    onClick={async () => {
                      setOllamaInstallBusy(true);
                      setOllamaSetupLastError(null);
                      try {
                        await invoke<string>("install_ollama_cli_for_athena");
                        await invoke("try_launch_ollama");
                        await new Promise((r) => setTimeout(r, 2000));
                        const ok = await invoke<boolean>("is_ollama_available");
                        setOllamaAvailable(ok);
                        if (ok) setOllamaNotRunningBannerDismissed(false);
                        const st = await invoke<OllamaSetupStatus>("get_ollama_setup_status");
                        setOllamaSetupStatus(st);
                      } catch (e) {
                        setOllamaSetupLastError(String(e));
                      } finally {
                        setOllamaInstallBusy(false);
                      }
                    }}
                  >
                    {ollamaInstallBusy ? "Retrying…" : "Retry download"}
                  </button>
                ) : null}
              </div>
            ) : null}
            <span className="ollama-not-running-message">
              Ollama is not running. Athena can install the Ollama CLI into its app data folder automatically if nothing
              is listening on port 11434, or you can start it yourself (e.g. <code>ollama run llama3.2:3b</code>).
              {" "}
              <a
                href="https://ollama.com/download"
                target="_blank"
                rel="noopener noreferrer"
                className="ollama-not-running-install-link"
              >
                Install from ollama.com
              </a>
            </span>
            {ollamaSetupStatus?.unconventional_ollama_path &&
            !ollamaCopyPromptDismissed &&
            !ollamaSidecarManaged ? (
              <div className="ollama-unconventional-prompt" role="region" aria-label="Ollama location">
                <p className="ollama-unconventional-text">
                  Ollama was found at <code>{ollamaSetupStatus.unconventional_ollama_path}</code>, which is outside a
                  typical install location. Copy it into Athena&apos;s managed folder so the app can run{" "}
                  <code>ollama serve</code> reliably? Your original file stays untouched.
                </p>
                <div className="ollama-unconventional-actions">
                  <button
                    type="button"
                    className="ollama-unconventional-copy"
                    disabled={ollamaCopyBusy}
                    onClick={async () => {
                      const src = ollamaSetupStatus.unconventional_ollama_path;
                      if (!src) return;
                      setOllamaCopyBusy(true);
                      try {
                        await invoke<string>("copy_ollama_into_athena_managed", { sourcePath: src });
                        await invoke("try_launch_ollama");
                        await new Promise((r) => setTimeout(r, 2000));
                        const ok = await invoke<boolean>("is_ollama_available");
                        setOllamaAvailable(ok);
                        if (ok) setOllamaNotRunningBannerDismissed(false);
                        const st = await invoke<OllamaSetupStatus>("get_ollama_setup_status");
                        setOllamaSetupStatus(st);
                      } catch (e) {
                        setLibraryError(String(e));
                      } finally {
                        setOllamaCopyBusy(false);
                      }
                    }}
                  >
                    {ollamaCopyBusy ? "Copying…" : "Copy into Athena folder"}
                  </button>
                  <button
                    type="button"
                    className="ollama-unconventional-dismiss"
                    disabled={ollamaCopyBusy}
                    onClick={() => setOllamaCopyPromptDismissed(true)}
                  >
                    Not now
                  </button>
                </div>
              </div>
            ) : null}
            <div className="ollama-not-running-actions">
              {ollamaSidecarManaged ? (
                <span className="ollama-system-managed-badge" role="status">
                  System managed
                </span>
              ) : (
                <button
                  type="button"
                  className="ollama-not-running-try-launch"
                  disabled={ollamaLaunching}
                  onClick={async () => {
                    setOllamaLaunching(true);
                    try {
                      await invoke("try_launch_ollama");
                      await new Promise((r) => setTimeout(r, 3000));
                      const available = await invoke<boolean>("is_ollama_available");
                      setOllamaAvailable(available);
                      if (available) setOllamaNotRunningBannerDismissed(false);
                      const st = await invoke<OllamaSetupStatus>("get_ollama_setup_status").catch(() => null);
                      if (st) setOllamaSetupStatus(st);
                    } catch {
                      // ignore
                    } finally {
                      setOllamaLaunching(false);
                    }
                  }}
                  aria-label="Try to start Ollama"
                >
                  {ollamaLaunching ? "Starting…" : "Try to start Ollama"}
                </button>
              )}
              <button
                type="button"
                className="ollama-not-running-dismiss"
                onClick={() => setOllamaNotRunningBannerDismissed(true)}
                aria-label="Dismiss"
              >
                Dismiss
              </button>
            </div>
              </>
            )}
          </div>
        )}
        {birthdayBannerVisible && (
          <div className="ollama-not-running-banner" role="status" style={{ background: "var(--surface-solid)" }}>
            <span className="ollama-not-running-message">
              Happy birthday{userDisplayName.trim() ? `, ${userDisplayName.trim()}` : ""} — glad you&apos;re here.
            </span>
            <div className="ollama-not-running-actions">
              <button
                type="button"
                className="ollama-not-running-dismiss"
                onClick={() => {
                  const todayKey = `athena_birthday_shown_${new Date().toISOString().slice(0, 10)}`;
                  try {
                    sessionStorage.setItem(todayKey, "1");
                  } catch {
                    /* ignore */
                  }
                  setBirthdayBannerVisible(false);
                }}
                aria-label="Dismiss birthday message"
              >
                Thanks
              </button>
            </div>
          </div>
        )}
        {quarantineAlert && (
          <div className="quarantine-banner" role="alert" aria-live="polite">
            <span className="quarantine-message">
              A file could not be indexed and was moved to quarantine: <strong>{quarantineAlert.path.split(/[/\\]/).pop() ?? quarantineAlert.path}</strong>. {quarantineAlert.reason}
            </span>
            <button
              type="button"
              className="quarantine-dismiss"
              onClick={() => setQuarantineAlert(null)}
              aria-label="Dismiss"
            >
              Dismiss
            </button>
          </div>
        )}
        {showLowBatteryBanner && batteryState && batteryState.percentage !== null && (
          <div className="low-battery-banner" role="status" aria-live="polite">
            <span className="low-battery-message">
              Battery is low ({Math.round(batteryState.percentage)}%). Switch Athena to a smaller model to conserve power?
            </span>
            <div className="low-battery-actions">
              <button
                type="button"
                className="low-battery-primary"
                onClick={() => {
                  updateModelOverride("small");
                  setLowBatteryDismissed(true);
                }}
              >
                Use small model
              </button>
              <button
                type="button"
                className="low-battery-secondary"
                onClick={() => setLowBatteryDismissed(true)}
              >
                Dismiss
              </button>
            </div>
          </div>
        )}
        {findInChatOpen && (
          <div className="find-in-chat-bar" role="search">
            <input
              ref={findInChatInputRef}
              type="search"
              className="find-in-chat-input"
              placeholder="Find in chat…"
              value={findInChatQuery}
              onChange={(e) => {
                setFindInChatQuery(e.target.value);
                setFindInChatIndex(0);
              }}
              aria-label="Find in current chat"
            />
            {findInChatQuery.trim() && findMatchIndices.length > 0 && (
              <span className="find-in-chat-count" aria-live="polite">
                {findInChatIndex + 1} of {findMatchIndices.length}
              </span>
            )}
            {findInChatQuery.trim() && findMatchIndices.length === 0 && (
              <span className="find-in-chat-count find-in-chat-no-match">No matches</span>
            )}
            <button
              type="button"
              className="find-in-chat-btn"
              onClick={() => {
                setFindInChatIndex((i) => (i - 1 + findMatchIndices.length) % Math.max(findMatchIndices.length, 1));
              }}
              disabled={findMatchIndices.length === 0}
              aria-label="Previous match"
            >
              ↑
            </button>
            <button
              type="button"
              className="find-in-chat-btn"
              onClick={() => {
                setFindInChatIndex((i) => (i + 1) % Math.max(findMatchIndices.length, 1));
              }}
              disabled={findMatchIndices.length === 0}
              aria-label="Next match"
            >
              ↓
            </button>
            <button
              type="button"
              className="find-in-chat-btn find-in-chat-close"
              onClick={() => {
                setFindInChatOpen(false);
                setFindInChatQuery("");
              }}
              aria-label="Close find in chat"
            >
              Done
            </button>
          </div>
        )}
        <div key={currentChatId ?? "new"} className="chat-messages content-transition">
        <ErrorBoundary sectionLabel="chat messages">
        {messages.length === 0 && (
          <div className="welcome">
            <p>Ask me anything about survival, repair, or practical skills.</p>
            <p className="welcome-hint">
              I work fully offline. Add Kiwix or other documents in <strong>Library</strong> so I can reference them in my answers.
            </p>
            <div className="welcome-tip" aria-label="Tip of the day">
              <span className="welcome-tip-label">Tip of the day</span>
              <p className="welcome-tip-text">{getTipOfTheDay()}</p>
            </div>
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={`message message-${msg.role}`}>
            <div className="message-role">{msg.role === "user" ? "You" : "Athena"}</div>
            <div className="message-content" ref={(el) => { messageRefsRef.current[i] = el; }}>
              {formatMessageTextWithHighlight(msg.content, findInChatQuery)}
              {sending && i === messages.length - 1 && msg.role === "assistant" && msg.content === "" && (
                <span className="message-loading-dots" aria-hidden>
                  <span className="message-loading-dot" />
                  <span className="message-loading-dot" />
                  <span className="message-loading-dot" />
                </span>
              )}
              {msg.role === "assistant" && showSourcesInChat && msg.sources && msg.sources.length > 0 && (
                <p className="message-sources" aria-label="Sources from your library">
                  Sources: {msg.sources.map((s, idx) => (
                    <React.Fragment key={s}>
                      {idx ? ", " : null}
                      <strong>{s}</strong>
                    </React.Fragment>
                  ))}
                </p>
              )}
            </div>
            {msg.timestamp != null && (
              <div className="message-timestamp" aria-hidden>
                {new Date(msg.timestamp).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}
              </div>
            )}
            {msg.role === "assistant" && msg.content.trim() && (
              <div className="message-actions">
                {!VOICE_AND_MIC_FEATURES_HIDDEN && (
                  <>
                    <button
                      type="button"
                      className="message-tts-btn"
                      onClick={() => {
                        if (ttsSpeakingIndex === i) {
                          toggleTtsPause();
                        } else {
                          speakMessageAtIndex(i);
                        }
                      }}
                      disabled={!ttsEnabled}
                      aria-label={
                        !ttsEnabled
                          ? "Text-to-speech disabled"
                          : ttsSpeakingIndex === i
                          ? ttsPaused
                            ? "Resume reading message"
                            : "Pause reading message"
                          : "Read this message aloud"
                      }
                    >
                      {ttsSpeakingIndex === i ? (ttsPaused ? "▶" : "⏸") : "🔈"}
                    </button>
                    <button
                      type="button"
                      className="message-tts-btn"
                      onClick={stopTts}
                      disabled={!ttsEnabled || ttsSpeakingIndex === null}
                      aria-label="Stop reading"
                    >
                      ⏹
                    </button>
                  </>
                )}
                <button
                  type="button"
                  className="message-tts-btn message-copy-btn"
                  onClick={() => {
                    navigator.clipboard.writeText(msg.content).catch(() => {});
                  }}
                  aria-label="Copy to clipboard"
                  title="Copy to clipboard"
                >
                  ⎘
                </button>
              </div>
            )}
          </div>
        ))}
          <div ref={messagesEndRef} />
        </ErrorBoundary>
        </div>

        <div className="chat-input-wrap">
          {streamStallMessage ? (
            <div className="chat-stream-stall-banner" role="alert">
              {streamStallMessage}
            </div>
          ) : null}
          {attachments.length > 0 && (
            <div className="chat-attachments-row">
              {attachments.map((a) => (
                <span key={a.id} className="chat-attachment-pill">
                  {a.kind === "image" ? "🖼️" : "📄"} {a.name}
                </span>
              ))}
            </div>
          )}
          {!VOICE_AND_MIC_FEATURES_HIDDEN && conversationModeActive && (
            <div className="conversation-hearing-bar" aria-live="polite">
              {conversationMicStatus === "requesting" ? (
                <>
                  <span className="conversation-hearing-dot" aria-hidden />
                  <span className="conversation-hearing-label">Requesting microphone… Allow when prompted.</span>
                </>
              ) : conversationMicStatus === "denied" ? (
                <span className="conversation-hearing-label conversation-hearing-denied">
                  {conversationMicError || "Microphone access denied. Allow microphone in System Settings → Privacy & Security → Microphone for Athena."}
                </span>
              ) : speechListening ? (
                <>
                  <span className="conversation-hearing-dot" aria-hidden />
                  <span className="conversation-hearing-label">Listening…</span>
                  {conversationInterimTranscript ? (
                    <span className="conversation-hearing-transcript">"{conversationInterimTranscript}"</span>
                  ) : null}
                </>
              ) : (
                <span className="conversation-hearing-label conversation-hearing-idle">Ready for your message</span>
              )}
            </div>
          )}
          <div className="chat-input-row">
            <div className="chat-input-left-buttons">
              {!VOICE_AND_MIC_FEATURES_HIDDEN && (
                <button
                  type="button"
                  className={`chat-tools-btn conversation-mode-btn${conversationModeActive ? " conversation-mode-btn-active" : ""}`}
                  onClick={() => {
                    if (conversationModeActive) {
                      setConversationModeActive(false);
                      speechRecognitionRef.current?.abort();
                      stopTts();
                      setSpeechListening(false);
                      setConversationInterimTranscript("");
                      setConversationMicStatus("idle");
                      setConversationMicError(null);
                    } else {
                      setConversationModeActive(true);
                      startConversationListening();
                    }
                  }}
                  title={conversationModeActive ? "Exit active conversation" : "Active conversation: have one long continuous back-and-forth with Athena. Speak, get a reply, then speak again."}
                  aria-label={conversationModeActive ? "Exit active conversation" : "Start active conversation"}
                >
                  <span className="chat-tools-icon" aria-hidden>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
                      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                      <line x1="12" x2="12" y1="19" y2="22" />
                    </svg>
                  </span>
                  <span className="chat-tools-label">{conversationModeActive ? (speechListening ? "Listening…" : "On") : "Conversation"}</span>
                </button>
              )}
              <div className="chat-tools">
                <button
                  type="button"
                  className="chat-tools-btn"
                  onClick={() => setToolsOpen((v) => !v)}
                  aria-expanded={toolsOpen}
                >
                  <span className="chat-tools-icon">＋</span>
                  <span className="chat-tools-label">Tools</span>
                </button>
                {toolsOpen && (
                  <div className="chat-tools-menu">
                    {!VOICE_AND_MIC_FEATURES_HIDDEN && (
                      <button
                        type="button"
                        className={`chat-tools-item${speechListening ? " chat-tools-item-active" : ""}`}
                        onClick={() => {
                          setToolsOpen(false);
                          const SpeechRecognitionAPI = typeof window !== "undefined" && (window.SpeechRecognition || window.webkitSpeechRecognition);
                          if (!SpeechRecognitionAPI) {
                            logErrorRef.current?.("Speech recognition not supported in this browser.");
                            return;
                          }
                          if (speechListening) {
                            speechRecognitionRef.current?.abort();
                            return;
                          }
                          const rec = new SpeechRecognitionAPI();
                          rec.continuous = false;
                          rec.interimResults = true;
                          rec.lang = "en-US";
                          rec.onresult = (e: SpeechRecognitionEvent) => {
                            const transcript = Array.from(e.results)
                              .map((r) => r[0].transcript)
                              .join(" ")
                              .trim();
                            if (transcript) setInput((prev) => (prev ? `${prev} ${transcript}` : transcript));
                          };
                          rec.onend = () => setSpeechListening(false);
                          rec.onerror = () => setSpeechListening(false);
                          speechRecognitionRef.current = rec;
                          rec.start();
                          setSpeechListening(true);
                        }}
                        aria-label={speechListening ? "Stop listening" : "Speak to type"}
                        title={speechListening ? "Stop listening" : "Speak to type (browser speech recognition)"}
                      >
                        <span className="chat-tools-item-icon">🎤</span>
                        <span className="chat-tools-item-text">{speechListening ? "Listening…" : "Speak"}</span>
                      </button>
                    )}
                    <button
                      type="button"
                      className="chat-tools-item"
                      onClick={handleAttachImages}
                    >
                      <span className="chat-tools-item-icon">🖼️</span>
                      <span className="chat-tools-item-text">Upload images</span>
                    </button>
                    <button
                      type="button"
                      className="chat-tools-item"
                      onClick={handleAttachFiles}
                    >
                      <span className="chat-tools-item-icon">📄</span>
                      <span className="chat-tools-item-text">Upload files</span>
                    </button>
                  </div>
                )}
              </div>
            </div>
            <textarea
              ref={inputRef}
              className="chat-input"
              placeholder={
                conversationModeActive
                  ? "Active conversation: speak, get a reply, then speak again for a continuous back-and-forth…"
                  : temporaryChat
                    ? "Temporary chat…"
                    : "Message Athena…"
              }
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              disabled={sending}
              rows={1}
            />
            <button
              type="button"
              className={`send-btn${input.trim() ? " send-btn-active" : ""}`}
              onClick={() => handleSend()}
              disabled={sending || !input.trim()}
              title="Send (Enter)"
            >
              Send
            </button>
          </div>
        </div>
      </main>
            </div>
          ) : null}
          {currentView === "map" && featureMapEnabled ? (
            <ErrorBoundary sectionLabel="map viewer">
              <MapView
                embedded
                pois={pois}
                onAddPoi={handleAddPoi}
                onRemovePoi={handleRemovePoi}
                mapChatInput={mapChatInput}
                setMapChatInput={setMapChatInput}
                onMapChatSend={() => void handleMapChatSend()}
                mapChatSending={mapChatSending}
                mapChatMessage={mapChatLastMessage}
                layerFilters={_layerFilters}
                setLayerFilters={setLayerFilters}
                convertFileSrcFn={convertFileSrcFn}
                basemap={mapBasemap}
                onBasemapChange={setMapBasemap}
                offlineOsmRoot={offlineOsmRoot}
                offlineTopoRoot={offlineTopoRoot}
                offlineImportedRoot={offlineImportedRoot}
                archiveRasterFileNames={enabledArchiveRasterFileNames}
                archiveRasterNativeZoomRangesByFile={archiveRasterNativeZoomRangesByFile}
                archiveRasterBoundsByFile={archiveRasterBoundsByFile}
                savedUserLocationAddress={userLocationAddress}
                onSatelliteRegionDownloaded={handleMapSatelliteDownloaded}
                mapCenter={mapViewCenter}
                mapZoom={mapViewZoom}
                onOpenOfflineMapManager={() => setOfflineMapManagerOpen(true)}
                tacticalPointsOfInterest={tacticalMapPoints}
                tacticalMeshNodeLocations={tacticalMapNodes}
                onReloadTacticalMapLayers={reloadTacticalMapLayers}
                onAddTacticalPoiPin={handleAddTacticalPoiPin}
                tacticalLayerVisibility={tacticalMapLayerVisibility}
                onTacticalLayerVisibilityChange={setTacticalMapLayerVisibility}
              />
            </ErrorBoundary>
          ) : null}
          {currentView === "map" && !featureMapEnabled ? (
            <div className="glass-panel app-view-placeholder">
              <h2 className="app-view-placeholder-title">Maps are off</h2>
              <p className="text-muted app-view-placeholder-body">
                Enable Maps under <strong>Location</strong> in Settings to open the map viewer.
              </p>
              <div className="app-view-placeholder-actions">
                <button
                  type="button"
                  onClick={() => {
                    setSettingsOpen(true);
                    goToSettingsSubpanel("location");
                  }}
                >
                  Open Location settings
                </button>
                <button type="button" className="secondary" onClick={() => setCurrentView("home")}>
                  Back to Home
                </button>
              </div>
            </div>
          ) : null}
          {currentView === "radio" ? (
            <RadioView />
          ) : null}
          {currentView === "logbook" ? <LogbookView meshConnected={meshRadioConnected === true} /> : null}
        </div>
      </div>

      {deviceOverlayEnabled && (
        <div className="device-details-overlay" aria-live="polite" aria-label="Device stats">
          <button
            type="button"
            className={`device-details-overlay-inner${deviceOverlayExpanded ? " device-details-overlay-expanded" : ""}`}
            onClick={() => setDeviceOverlayExpanded((e) => !e)}
            title={deviceOverlayExpanded ? "Collapse device stats" : "Expand for more metrics"}
          >
            {deviceStats !== null ? (
              <>
                <div className="device-details-row device-details-row-compact">
                  <span>CPU</span>
                  <span>{Math.round(deviceStats.cpu_usage_pct)}%</span>
                </div>
                <div className="device-details-row device-details-row-compact">
                  <span>RAM</span>
                  <span>
                    {deviceStats.ram_total_mb >= 1024
                      ? `${(deviceStats.ram_used_mb / 1024).toFixed(1)}/${(deviceStats.ram_total_mb / 1024).toFixed(1)} GB`
                      : `${deviceStats.ram_used_mb}/${deviceStats.ram_total_mb} MB`}
                  </span>
                </div>
                {deviceOverlayExpanded ? (
                  <>
                    <div className="device-details-row">
                      <span>Swap</span>
                      <span>
                        {deviceStats.swap_total_mb > 0
                          ? `${(deviceStats.swap_used_mb / 1024).toFixed(2)}/${(deviceStats.swap_total_mb / 1024).toFixed(2)} GB`
                          : "—"}
                      </span>
                    </div>
                    <div className="device-details-row">
                      <span>Storage</span>
                      <span>
                        {deviceStats.disk_total_gb > 0
                          ? `${deviceStats.disk_used_gb.toFixed(1)} / ${deviceStats.disk_total_gb.toFixed(1)} GB`
                          : "—"}
                      </span>
                    </div>
                    <div className="device-details-row">
                      <span>Disk R</span>
                      <span>
                        {diskSpeedReadMbps != null && diskSpeedReadMbps > 0
                          ? `${diskSpeedReadMbps.toFixed(2)} MB/s`
                          : "—"}
                      </span>
                    </div>
                    <div className="device-details-row">
                      <span>Disk W</span>
                      <span>
                        {diskSpeedWriteMbps != null && diskSpeedWriteMbps > 0
                          ? `${diskSpeedWriteMbps.toFixed(2)} MB/s`
                          : "—"}
                      </span>
                    </div>
                    <div className="device-details-row">
                      <span>Net ↓</span>
                      <span>{netSpeedRxKbps != null ? `${netSpeedRxKbps.toFixed(0)} kB/s` : "—"}</span>
                    </div>
                    <div className="device-details-row">
                      <span>Net ↑</span>
                      <span>{netSpeedTxKbps != null ? `${netSpeedTxKbps.toFixed(0)} kB/s` : "—"}</span>
                    </div>
                    <div className="device-details-row">
                      <span>Battery</span>
                      <span>
                        {deviceStats.battery_pct != null && deviceStats.battery_pct !== undefined
                          ? `${Math.round(deviceStats.battery_pct)}%${
                              deviceStats.battery_charging ? " (AC)" : ""
                            }${
                              deviceStats.battery_time_to_empty_minutes != null &&
                              deviceStats.battery_time_to_empty_minutes !== undefined &&
                              deviceStats.battery_charging !== true
                                ? ` · ~${deviceStats.battery_time_to_empty_minutes}m to 0%`
                                : ""
                            }`
                          : "—"}
                      </span>
                    </div>
                    <div className="device-details-row">
                      <span>GPU</span>
                      <span>{deviceStats.gpu_usage_pct != null ? `${Math.round(deviceStats.gpu_usage_pct)}%` : "—"}</span>
                    </div>
                    <div className="device-details-row">
                      <span>CPU °C</span>
                      <span>{deviceStats.cpu_temp_c != null ? `${Math.round(deviceStats.cpu_temp_c)}°C` : "—"}</span>
                    </div>
                  </>
                ) : (
                  <>
                    <div className="device-details-row device-details-row-compact">
                      <span>SSD</span>
                      <span>
                        {deviceStats.disk_total_gb > 0
                          ? `${deviceStats.disk_used_gb.toFixed(1)}/${deviceStats.disk_total_gb.toFixed(1)} GB`
                          : "—"}
                      </span>
                    </div>
                    <div className="device-details-row device-details-row-compact">
                      <span>Net ↓</span>
                      <span>{netSpeedRxKbps != null ? `${netSpeedRxKbps.toFixed(0)} kB/s` : "—"}</span>
                    </div>
                    <div className="device-details-row device-details-row-compact">
                      <span>Net ↑</span>
                      <span>{netSpeedTxKbps != null ? `${netSpeedTxKbps.toFixed(0)} kB/s` : "—"}</span>
                    </div>
                    <div className="device-details-row device-details-row-compact">
                      <span>GPU</span>
                      <span>{deviceStats.gpu_usage_pct != null ? `${Math.round(deviceStats.gpu_usage_pct)}%` : "—"}</span>
                    </div>
                    <div className="device-details-row device-details-row-compact">
                      <span>Temp</span>
                      <span>{deviceStats.cpu_temp_c != null ? `${Math.round(deviceStats.cpu_temp_c)}°C` : "—"}</span>
                    </div>
                  </>
                )}
                <div className="device-details-hint">{deviceOverlayExpanded ? "Tap to collapse" : "Tap for more"}</div>
              </>
            ) : (
              <span className="device-details-loading">…</span>
            )}
          </button>
        </div>
      )}

      {settingsOpen && (
        <div className="modal-backdrop" onClick={() => setSettingsOpen(false)}>
          <div className="library-modal settings-modal glass-panel" onClick={(e) => e.stopPropagation()}>
            <div className="library-header library-header-with-back">
              <div className="library-header-left">
                {settingsPanel !== "menu" ? (
                  <button
                    type="button"
                    className="settings-back-btn"
                    onClick={() => {
                      if (settingsPanel === "themes" && themesSubView === "studio") {
                        cancelThemeStudio();
                        return;
                      }
                      setSettingsPanel("menu");
                    }}
                    aria-label="Back to settings menu"
                  >
                    ← Back
                  </button>
                ) : null}
              </div>
              <h2
                className={settingsPanel === "menu" ? "settings-title-tappable" : undefined}
                onClick={
                  settingsPanel === "menu"
                    ? () => {
                        const next = settingsTitleClickCount + 1;
                        setSettingsTitleClickCount(next);
                        if (next >= 7) goToSettingsSubpanel("developer");
                      }
                    : undefined
                }
                role={settingsPanel === "menu" ? "button" : undefined}
                aria-label={settingsPanel === "menu" ? "Settings" : undefined}
              >
                {settingsPanel === "menu"
                  ? "Settings"
                  : settingsPanel === "general"
                    ? "General"
                    : settingsPanel === "ai"
                      ? "AI & Models"
                      : settingsPanel === "location"
                        ? "Location"
                        : settingsPanel === "apiKeys"
                          ? "API keys"
                        : settingsPanel === "maps"
                          ? "Offline maps"
                          : settingsPanel === "mesh"
                          ? "Mesh"
                          : settingsPanel === "voice"
                            ? "Voice"
                            : settingsPanel === "appearance"
                              ? "Window & Display"
                              : settingsPanel === "themes"
                                ? themesSubView === "studio"
                                  ? "Theme Studio"
                                  : "Themes & Colors"
                                : settingsPanel === "troubleshooting"
                                  ? "Troubleshooting"
                                  : "Developer"}
              </h2>
              <button type="button" className="close-btn" onClick={() => setSettingsOpen(false)}>
                ×
              </button>
            </div>
            <div className="settings-modal-body" ref={settingsBodyRef}>
            {settingsPanel === "menu" && (
              <nav className="settings-nav" aria-label="Settings sections">
                <p className="settings-nav-intro text-muted">
                  Choose a category. Library is opened from the header.
                  {meshRadioConnected === true
                    ? " The mesh radio button appears when a Meshtastic device is connected."
                    : meshRadioConnected === false
                      ? " No Meshtastic radio detected—mesh options are unavailable until a device is connected."
                      : null}
                </p>
                <div className="settings-nav-group">
                  <span className="settings-nav-group-label">Basics</span>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("general")}>
                    <span className="settings-nav-label">General</span>
                    <span className="text-muted">Help, shortcuts, optional CPU/RAM overlay</span>
                  </button>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("ai")}>
                    <span className="settings-nav-label">AI &amp; Models</span>
                    <span className="text-muted">Ollama, personality, model size, memory, citations</span>
                  </button>
                </div>
                <div className="settings-nav-group">
                  <span className="settings-nav-group-label">Context &amp; Devices</span>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("location")}>
                    <span className="settings-nav-label">Location</span>
                    <span className="text-muted">Place-aware answers and local data downloads</span>
                  </button>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("apiKeys")}>
                    <span className="settings-nav-label">API keys</span>
                    <span className="text-muted">Paste and store provider keys for advanced features</span>
                  </button>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("maps")}>
                    <span className="settings-nav-label">Offline maps</span>
                    <span className="text-muted">
                      Kiwix regional maps, tile basemaps, and imports
                      {featureMapEnabled ? (
                        <span className="settings-build-note"> · Use the Map icon in the top bar when Maps is enabled</span>
                      ) : (
                        <span className="settings-build-note"> · Turn on Maps under Location to show the map button</span>
                      )}
                    </span>
                  </button>
                  <button
                    type="button"
                    className={`settings-nav-item ${meshRadioConnected !== true ? "settings-nav-item-disabled" : ""}`}
                    disabled={meshRadioConnected !== true}
                    onClick={() => meshRadioConnected === true && goToSettingsSubpanel("mesh")}
                    aria-disabled={meshRadioConnected !== true}
                  >
                    <span className="settings-nav-label">Mesh</span>
                    <span className="text-muted">
                      {meshRadioConnected === null
                        ? "Checking for USB radio…"
                        : meshRadioConnected === false
                          ? "No Meshtastic radio connected"
                          : "Meshtastic auto-reply and mesh traffic"}
                    </span>
                  </button>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("voice")}>
                    <span className="settings-nav-label">Voice</span>
                    <span className="text-muted">
                      Read-aloud (TTS)
                      {VOICE_AND_MIC_FEATURES_HIDDEN ? (
                        <span className="settings-build-note"> · Hidden in this build</span>
                      ) : null}
                    </span>
                  </button>
                </div>
                <div className="settings-nav-group">
                  <span className="settings-nav-group-label">Look &amp; Support</span>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("appearance")}>
                    <span className="settings-nav-label">Window &amp; Display</span>
                    <span className="text-muted">Fullscreen and night vision</span>
                  </button>
                  <button
                    type="button"
                    className="settings-nav-item"
                    onClick={() => {
                      setThemesSubView("main");
                      setStudioDraft(null);
                      goToSettingsSubpanel("themes");
                    }}
                  >
                    <span className="settings-nav-label">Themes &amp; Colors</span>
                    <span className="text-muted">Experience style, color palette, custom themes</span>
                  </button>
                  <button type="button" className="settings-nav-item" onClick={() => goToSettingsSubpanel("troubleshooting")}>
                    <span className="settings-nav-label">Troubleshooting</span>
                    <span className="text-muted">Error log and advanced diagnostics</span>
                  </button>
                </div>
              </nav>
            )}
            {settingsPanel === "general" && (
              <>
                <section className="library-section">
                  <h3>User profile</h3>
                  <p className="text-muted small">
                    Optional. Your name is added to AI context; birthdate stays on this device and unlocks 18+ options when
                    applicable.
                  </p>
                  <div className="glass-panel" style={{ padding: "0.9rem", borderRadius: "12px", display: "grid", gap: "0.65rem" }}>
                    <label htmlFor="user-display-name" className="text-muted small">
                      Preferred name
                    </label>
                    <input
                      id="user-display-name"
                      className="theme-select"
                      value={userDisplayName}
                      onChange={(e) => setUserDisplayName(e.target.value)}
                      onBlur={() => {
                        void invoke("set_user_display_name", {
                          value: userDisplayName.trim() || null,
                        }).then(() => loadModelSettings());
                      }}
                      placeholder="How Athena should address you"
                    />
                    <label htmlFor="user-birthdate" className="text-muted small">
                      Birthdate (YYYY-MM-DD)
                    </label>
                    <input
                      id="user-birthdate"
                      type="date"
                      className="theme-select"
                      value={userBirthdateIso}
                      onChange={(e) => setUserBirthdateIso(e.target.value)}
                      onBlur={() => {
                        void invoke("set_user_birthdate_iso", {
                          value: userBirthdateIso.trim() || null,
                        }).then(() => loadModelSettings());
                      }}
                    />
                    {userIs18 ? (
                      <label style={{ display: "flex", gap: "0.5rem", alignItems: "flex-start" }}>
                        <input
                          type="checkbox"
                          checked={allowNsfwCatalog}
                          onChange={(e) => {
                            const v = e.target.checked;
                            setAllowNsfwCatalog(v);
                            void invoke("set_allow_nsfw_unrestricted_models", { value: v }).then(() => loadModelSettings());
                          }}
                        />
                        <span className="text-muted small">
                          Show <strong>unrestricted / NSFW</strong> model catalog entries (local runtimes only; you are
                          responsible for compliance). Uncensored models run only on your machine—useful for detailed
                          survival/medical/engineering topics when ordinary guardrails block necessary detail.
                        </span>
                      </label>
                    ) : (
                      <p className="text-muted small" style={{ margin: 0 }}>
                        Set a valid birthdate to verify age 18+ to unlock the unrestricted catalog toggle.
                      </p>
                    )}
                  </div>
                </section>
                <section className="library-section">
                  <h3>Help &amp; shortcuts</h3>
                  <p className="text-muted">
                    Open the full <strong>Help</strong> guide from the <strong>?</strong> button in the header for every feature. Quick
                    reminders:
                  </p>
                  <ul className="settings-tips-list text-muted">
                    <li>
                      <kbd className="settings-kbd">⌘</kbd>/<kbd className="settings-kbd">Ctrl</kbd>+<kbd className="settings-kbd">K</kbd> or{" "}
                      <kbd className="settings-kbd">/</kbd> — focus the chat input
                    </li>
                    <li>
                      <kbd className="settings-kbd">⌘</kbd>/<kbd className="settings-kbd">Ctrl</kbd>+<kbd className="settings-kbd">F</kbd> — find
                      text in the current chat
                    </li>
                    <li>
                      <code>/athena</code> — ask how the app works (uses the built-in project reference)
                    </li>
                    <li>
                      <code>/rag-count</code> — show how many library documents and chunks are indexed
                    </li>
                  </ul>
                  <div className="settings-inline-actions">
                    <button
                      type="button"
                      onClick={() => {
                        setSettingsOpen(false);
                        setHelpOpen(true);
                      }}
                    >
                      Open Help guide
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => {
                        setSettingsOpen(false);
                        setLibraryOpen(true);
                      }}
                    >
                      Open Library
                    </button>
                  </div>
                </section>
                <section className="library-section">
                  <h3>Device stats overlay</h3>
                  <p className="text-muted small">
                    Show a small CPU/RAM/network readout in the corner for debugging or curiosity. Does not affect answers.
                  </p>
                  <div className="feature-checkbox-wrap">
                    <input
                      id="settings-device-overlay"
                      type="checkbox"
                      className="feature-checkbox"
                      checked={deviceOverlayEnabled}
                      onChange={(e) => setDeviceOverlayEnabled(e.target.checked)}
                      aria-label="Show device stats overlay"
                    />
                    <label htmlFor="settings-device-overlay" className="feature-checkbox-label">
                      Show device stats overlay
                    </label>
                  </div>
                </section>
                <section className="library-section">
                  <h3>Default startup page</h3>
                  <p className="text-muted small">Choose which screen appears when you open Athena. The setting is saved on this device.</p>
                  <div className="settings-startup-view-row">
                    <label htmlFor="settings-startup-view" className="settings-startup-view-label">
                      Open to
                    </label>
                    <select
                      id="settings-startup-view"
                      className="settings-startup-view-select"
                      value={startupViewPreference}
                      onChange={(e) => {
                        const v = e.target.value;
                        const next = APP_VIEWS.find((x) => x === v);
                        if (!next) return;
                        saveStartupView(next);
                        setStartupViewPreference(next);
                      }}
                      aria-label="Default startup page"
                    >
                      {APP_VIEWS.map((v) => (
                        <option key={v} value={v}>
                          {APP_VIEW_LABELS[v]}
                        </option>
                      ))}
                    </select>
                  </div>
                </section>
                <section className="library-section">
                  <h3>System maintenance</h3>
                  <p className="text-muted small">
                    Backup or restore the local Athena index database, or export radio serial logs as CSV. Paths are chosen with the system file dialog.
                  </p>
                  <div className="glass-panel" style={{ padding: "0.9rem", borderRadius: "12px", display: "grid", gap: "0.65rem" }}>
                    <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem" }}>
                      <button type="button" onClick={() => void handleBackupDatabase()}>
                        Backup database
                      </button>
                      <button type="button" className="secondary" onClick={() => void handleRestoreDatabase()}>
                        Restore database
                      </button>
                      <button type="button" className="secondary" onClick={() => void handleExportCommsLogs()}>
                        Export comms logs
                      </button>
                    </div>
                    {maintenanceFeedback ? (
                      <p
                        className={maintenanceFeedback.kind === "err" ? "error" : "text-muted small"}
                        role={maintenanceFeedback.kind === "err" ? "alert" : "status"}
                        style={{ margin: 0 }}
                      >
                        {maintenanceFeedback.text}
                      </p>
                    ) : null}
                  </div>
                </section>
              </>
            )}
            {settingsPanel === "ai" && (
              <>
                <section className="library-section">
                  <h3>Inference backend</h3>
                  <p className="text-muted small">
                    Choose <strong>Ollama</strong> (default) or a local <strong>OpenAI-compatible</strong> HTTP API (LM Studio,
                    llama.cpp server, text-generation-webui, etc.). OpenAI-compatible URLs are restricted to localhost for
                    safety.
                  </p>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "1.25rem", marginTop: "0.5rem" }}>
                    <label style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
                      <input
                        type="radio"
                        name="athena-inference-backend"
                        checked={inferenceBackendMode === "ollama"}
                        onChange={() => {
                          void invoke("set_inference_backend", { value: null }).then(() => {
                            setInferenceBackendMode("ollama");
                            void loadModelSettings();
                          });
                        }}
                      />
                      Ollama
                    </label>
                    <label style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
                      <input
                        type="radio"
                        name="athena-inference-backend"
                        checked={inferenceBackendMode === "openai_compatible"}
                        onChange={() => {
                          void invoke("set_inference_backend", { value: "openai_compatible" }).then(() => {
                            setInferenceBackendMode("openai_compatible");
                            void loadModelSettings();
                          });
                        }}
                      />
                      OpenAI-compatible (local)
                    </label>
                  </div>
                  {inferenceBackendMode === "openai_compatible" ? (
                    <div className="glass-panel" style={{ padding: "0.9rem", borderRadius: "12px", marginTop: "0.75rem", display: "grid", gap: "0.65rem" }}>
                      <label className="text-muted small" htmlFor="openai-base-url">
                        Base URL (must be localhost)
                      </label>
                      <input
                        id="openai-base-url"
                        className="theme-select"
                        style={{ maxWidth: "100%" }}
                        value={openaiBaseUrl}
                        onChange={(e) => setOpenaiBaseUrl(e.target.value)}
                        onBlur={() => {
                          void invoke("set_openai_compatible_base_url", {
                            value: openaiBaseUrl.trim() || null,
                          }).then(() => loadModelSettings());
                        }}
                        placeholder="http://127.0.0.1:1234/v1"
                      />
                      <label className="text-muted small" htmlFor="openai-model-id">
                        Model id
                      </label>
                      <input
                        id="openai-model-id"
                        className="theme-select"
                        value={openaiModelId}
                        onChange={(e) => setOpenaiModelId(e.target.value)}
                        onBlur={() => {
                          void invoke("set_openai_compatible_model", {
                            value: openaiModelId.trim() || null,
                          }).then(() => loadModelSettings());
                        }}
                        placeholder="e.g. local-model"
                      />
                      <label className="text-muted small" htmlFor="openai-api-key">
                        API key (optional; stored locally)
                      </label>
                      <input
                        id="openai-api-key"
                        type="password"
                        autoComplete="off"
                        className="theme-select"
                        value={openaiApiKeyLocal}
                        onChange={(e) => setOpenaiApiKeyLocal(e.target.value)}
                        onBlur={() => {
                          void invoke("set_openai_compatible_api_key", {
                            value: openaiApiKeyLocal.trim() || null,
                          }).then(() => loadModelSettings());
                        }}
                        placeholder={openaiApiKeyLocal ? "••••" : "Often empty for localhost"}
                      />
                      <div className="settings-inline-actions">
                        <button
                          type="button"
                          onClick={() => {
                            void invoke<boolean>("probe_openai_compatible_connection", { baseUrl: openaiBaseUrl }).then((ok) => {
                              setLibraryError(ok ? null : "Could not reach OpenAI-compatible server.");
                            });
                          }}
                        >
                          Test connection
                        </button>
                      </div>
                      <p className="text-muted small" style={{ margin: 0 }}>
                        AI status: {aiInferenceReady ? "ready" : "not ready"} · See Help for LM Studio / llama.cpp tips.
                      </p>
                    </div>
                  ) : null}
                </section>
                <section className="library-section">
                  <h3>Ollama &amp; AI Model</h3>
                  {inferenceBackendMode === "openai_compatible" ? (
                    <p className="text-muted small">
                      Ollama is not used while OpenAI-compatible mode is active. Switch back to Ollama to manage local
                      models here.
                    </p>
                  ) : (
                    <>
                  <p className="model-status">
                    {ollamaAvailable ? "Ollama is running" : "Ollama is not running. Start Ollama and install a model (e.g. ollama run llama3.2:3b)."}
                    {ollamaSidecarManaged ? (
                      <>
                        {" "}
                        <span className="ollama-system-managed-badge" role="status">
                          System managed
                        </span>
                      </>
                    ) : null}
                  </p>
                  {!ollamaAvailable && (
                    <div className="settings-inline-actions" style={{ marginTop: "0.75rem" }}>
                      {ollamaSidecarManaged ? (
                        <span className="ollama-system-managed-settings-hint text-muted small">
                          Athena is running the bundled Ollama server; manual start is disabled.
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={async () => {
                            await invoke("try_launch_ollama");
                            invoke<boolean>("is_ollama_available").then(setOllamaAvailable).catch(() => {});
                            invoke<OllamaSetupStatus>("get_ollama_setup_status")
                              .then(setOllamaSetupStatus)
                              .catch(() => {});
                          }}
                        >
                          Try to start Ollama
                        </button>
                      )}
                    </div>
                  )}
                  {!ollamaSidecarManaged && ollamaSetupStatus != null && (
                    <div className="ollama-managed-path-block text-muted small" style={{ marginTop: "1rem" }}>
                      <p>
                        <strong>Athena-managed Ollama binary</strong>
                        {ollamaSetupStatus.managed_binary_exists ? " (present)" : " (not installed yet)"}
                      </p>
                      <code className="ollama-managed-path-code">{ollamaSetupStatus.managed_binary_path}</code>
                      <div className="settings-inline-actions" style={{ marginTop: "0.5rem" }}>
                        <button
                          type="button"
                          disabled={ollamaInstallBusy}
                          onClick={async () => {
                            setOllamaInstallBusy(true);
                            try {
                              await invoke<string>("install_ollama_cli_for_athena");
                              await invoke("try_launch_ollama");
                              const ok = await invoke<boolean>("is_ollama_available");
                              setOllamaAvailable(ok);
                              const st = await invoke<OllamaSetupStatus>("get_ollama_setup_status");
                              setOllamaSetupStatus(st);
                            } catch (e) {
                              setLibraryError(String(e));
                            } finally {
                              setOllamaInstallBusy(false);
                            }
                          }}
                        >
                          {ollamaInstallBusy ? "Working…" : "Download / refresh Ollama CLI here"}
                        </button>
                      </div>
                      <p className="small" style={{ marginTop: "0.5rem" }}>
                        On first launch, Athena may download the official CLI from GitHub if nothing responds on port
                        11434. Set <code>ollama_disable_auto_cli_download</code> in config JSON to skip that behavior.
                      </p>
                    </div>
                  )}
                  {systemSpecs !== null && (
                    <>
                      <p className="model-specs">
                        This device: {systemSpecs.ram_gb} GB RAM, {systemSpecs.cpu_cores} CPU cores — recommended: <strong>{systemSpecs.recommended_tier}</strong>
                      </p>
                      <div className="model-select-wrap">
                        <label htmlFor="model-tier-select">Model size</label>
                        <select
                          id="model-tier-select"
                          className="theme-select"
                          value={modelOverride}
                          onChange={handleModelOverrideChange}
                          aria-label="Model size (automatic or manual)"
                        >
                          {MODEL_TIERS.map((t) => (
                            <option key={t} value={t}>
                              {MODEL_TIER_LABELS[t]}
                            </option>
                          ))}
                        </select>
                      </div>
                      {modelOverride !== "auto" && (
                        <p className="model-warning">
                          Choosing a larger model may slow down responses. Use Automatic for the best balance on this device.
                          {modelOverride === "xlarge" && systemSpecs !== null && systemSpecs.ram_gb < 32 && (
                            <span>
                              {" "}
                              <strong>Extra large</strong> is best with <strong>32GB+ RAM</strong>; this device reports{" "}
                              {systemSpecs.ram_gb}GB.
                            </span>
                          )}
                        </p>
                      )}
                    </>
                  )}
                  <AiModelManager
                    initialActiveModel={activeEngineModel}
                    onActiveModelChange={(model) => setActiveEngineModel(model)}
                    showUnrestrictedCatalog={allowNsfwCatalog}
                    isAdult={userIs18}
                  />
                    </>
                  )}
                </section>
                <section className="library-section">
                  <h3>Personality</h3>
                  <p className="text-muted small">
                    How Athena speaks (tone and style). Choosing an <strong>experience</strong> under Settings → Themes &amp; Colors
                    still updates this to match (Default, Fallout, or Field guide) unless you override it here.
                  </p>
                  <div className="ui-persona-row ui-persona-row-ai">
                    <div className="ui-persona-field">
                      <label htmlFor="persona-select-ai">Personality</label>
                      <select
                        id="persona-select-ai"
                        className="theme-select"
                        value={activePersonalityId}
                        onChange={(e) => handlePersonalityChange(e.target.value as PersonalityId)}
                        aria-label="Athena personality"
                      >
                        {PERSONAS.map((p) => (
                          <option key={p} value={p}>
                            {PERSONALITY_LABELS[p]}
                          </option>
                        ))}
                      </select>
                    </div>
                    <p className="text-muted small ui-persona-help">
                      Field guide personality pairs with the paper manual look; switch experience in Themes to change both at once.
                    </p>
                  </div>
                </section>
                <section className="library-section">
                  <h3>Conversation memory</h3>
                  <p className="text-muted">
                    How many previous messages to send as context. More memory helps long conversations but uses more RAM.
                  </p>
                  <div className="memory-slider-wrap">
                    <label htmlFor="memory-slider">
                      Context: <strong>{memoryMessages}</strong> messages
                      {systemSpecs !== null && (
                        <span className="text-muted small">
                          {" "}
                          (max {systemSpecs.max_memory_messages} for this system&apos;s RAM)
                        </span>
                      )}
                    </label>
                    <input
                      id="memory-slider"
                      type="range"
                      min={5}
                      max={systemSpecs?.max_memory_messages ?? 80}
                      step={5}
                      value={memoryMessages}
                      onChange={(e) => handleMemoryMessagesChange(Number(e.target.value))}
                      className="memory-slider"
                      aria-label="Conversation memory (messages)"
                    />
                  </div>
                  {memoryEstimate !== null && (
                    <div className="memory-estimate" aria-live="polite">
                      <p className="memory-estimate-title">Estimated hardware usage</p>
                      <ul className="memory-estimate-list">
                        <li>Model: <strong>{memoryEstimate.model_name}</strong> ({memoryEstimate.model_size_b}B params) — ~{memoryEstimate.estimated_model_ram_gb.toFixed(1)} GB RAM</li>
                        <li>Context: {memoryEstimate.memory_messages} messages (~{memoryEstimate.estimated_context_tokens.toLocaleString()} tokens) — ~{memoryEstimate.estimated_context_ram_mb} MB</li>
                        <li>Total estimated: <strong>~{memoryEstimate.total_estimated_ram_gb.toFixed(2)} GB</strong> RAM</li>
                      </ul>
                    </div>
                  )}
                </section>
                <section className="library-section">
                  <h3>Advanced inference</h3>
                  <p className="text-muted small">
                    Controls passed to the local Ollama API for each chat request (<code>options.temperature</code>,{" "}
                    <code>options.num_ctx</code>).
                  </p>
                  <div className="glass-panel" style={{ padding: "0.9rem", borderRadius: "12px", display: "grid", gap: "0.75rem" }}>
                    <label style={{ display: "grid", gap: "0.35rem" }}>
                      <span className="text-muted small">
                        Temperature: <strong>{aiTemperature.toFixed(2)}</strong>
                      </span>
                      <input
                        type="range"
                        min={0}
                        max={1}
                        step={0.01}
                        value={aiTemperature}
                        onChange={(e) => setAiTemperature(Number(e.target.value))}
                        className="memory-slider"
                        aria-label="Model temperature"
                      />
                    </label>
                    <label style={{ display: "grid", gap: "0.35rem" }}>
                      <span className="text-muted small">Context window (tokens)</span>
                      <input
                        type="number"
                        min={512}
                        max={131072}
                        step={256}
                        value={aiNumCtx}
                        onChange={(e) => {
                          const n = Number.parseInt(e.target.value, 10);
                          if (!Number.isFinite(n)) return;
                          setAiNumCtx(Math.min(131072, Math.max(512, n)));
                        }}
                        style={{
                          minHeight: "38px",
                          borderRadius: "10px",
                          border: "1px solid var(--border)",
                          background: "var(--surface-solid)",
                          color: "var(--text)",
                          padding: "0 0.75rem",
                          fontFamily: "var(--font)",
                          maxWidth: "12rem",
                        }}
                        aria-label="Ollama context window num_ctx"
                      />
                    </label>
                  </div>
                </section>
                <section className="library-section">
                  <h3>Answers &amp; citations</h3>
                  <p className="text-muted small">
                    When your Library is used to build a reply, you can show which files contributed.
                  </p>
                  <div className="feature-checkbox-wrap">
                    <input
                      id="show-sources-in-chat-ai"
                      type="checkbox"
                      className="feature-checkbox"
                      checked={showSourcesInChat}
                      onChange={(e) => {
                        const checked = e.target.checked;
                        setShowSourcesInChat(checked);
                        invoke("set_show_sources_in_chat", { value: checked }).catch(() => {});
                      }}
                      aria-label="Show library sources in chat"
                    />
                    <label htmlFor="show-sources-in-chat-ai" className="feature-checkbox-label">
                      Show &quot;From your library&quot; under assistant messages
                    </label>
                  </div>
                </section>
              </>
            )}
            {settingsPanel === "location" && (
              <>
                <section className="library-section">
                  <h3>Location</h3>
                  <p className="text-muted">
                    When enabled, the AI can use your location to give place-based advice (e.g. where to find spare parts, auto parts stores, or parking lots with similar vehicles). It will ask for your address if you haven’t set one.
                  </p>
                  {downloadedMaps.length === 0 && (
                    <p className="text-muted small settings-build-banner">
                      <strong>Tip:</strong> Download a regional map pack under <strong>Offline maps</strong> to unlock the map button in the header. Location-aware answers work without it.
                    </p>
                  )}
                  <div className="settings-network-row text-muted small" aria-live="polite">
                    Network:{" "}
                    {networkReachable === null
                      ? "—"
                      : networkReachable
                        ? "Reachable (internet check passed)"
                        : "Not reachable — connect to Wi‑Fi or Ethernet to download data"}
                    <button
                      type="button"
                      className="secondary settings-network-refresh"
                      onClick={() => {
                        void invoke<boolean>("check_network_reachable")
                          .then((r) => setNetworkReachable(r))
                          .catch(() => setNetworkReachable(false));
                      }}
                    >
                      Check again
                    </button>
                  </div>
                  <div className="feature-checkbox-wrap">
                    <input
                      id="feature-map-checkbox"
                      type="checkbox"
                      className="feature-checkbox"
                      checked={featureMapEnabled}
                      onChange={(e) => handleFeatureMapToggle(e.target.checked)}
                      aria-label="Enable location-aware answers"
                    />
                    <label htmlFor="feature-map-checkbox" className="feature-checkbox-label">
                      Enable location-aware answers
                    </label>
                  </div>
                  {featureMapEnabled && (
                    <div className="library-section location-field-wrap">
                      <label htmlFor="user-location-input">Your location (address or area)</label>
                      <input
                        id="user-location-input"
                        type="text"
                        className="settings-input"
                        placeholder="e.g. downtown Austin, TX or 123 Main St"
                        value={userLocationAddress}
                        onChange={(e) => setUserLocationAddress(e.target.value)}
                        onBlur={(e) => void handleUserLocationBlur(e.target.value)}
                        aria-label="Your current location for location-specific advice"
                      />
                      <p className="text-muted small">
                        Stored locally. When you finish editing this field, the <strong>Install local data</strong> sheet opens so you can download the smallest offline map pack that matches your area (Kiwix). Use Settings → Offline maps for more layers.
                      </p>
                      <div className="settings-inline-actions" style={{ marginTop: "0.75rem" }}>
                        <button type="button" onClick={() => void openLocalDataInstallMenu()}>
                          Install local data…
                        </button>
                        <button
                          type="button"
                          className="secondary"
                          onClick={() => {
                            setLocalDataInstallOpen(false);
                            goToSettingsSubpanel("maps");
                          }}
                        >
                          Open offline maps
                        </button>
                      </div>
                    </div>
                  )}
                </section>
              </>
            )}
            {settingsPanel === "apiKeys" && (
              <>
                <section className="library-section">
                  <h3>API keys</h3>
                  <p className="text-muted">
                    Add provider API keys once, directly inside Athena. Keys are stored locally in your app data settings and are
                    automatically used by features that require them.
                  </p>
                  <p className="text-muted small">
                    Default generic placeholders are shown until you replace them with real values. Generic placeholders are safe to
                    keep in source control.
                  </p>
                </section>
                <section className="library-section">
                  <div className="settings-api-key-head">
                    <h3>MapTiler satellite imagery key</h3>
                    <button
                      type="button"
                      className="settings-api-key-help-btn"
                      aria-label="Open MapTiler API key setup tutorial"
                      title="How to get this key"
                      onClick={() => setShowSatelliteKeyTutorial((previous) => !previous)}
                    >
                      ?
                    </button>
                  </div>
                  <p className="text-muted small">
                    Used by Offline Map Manager to download high-resolution satellite MBTiles for your saved location.
                  </p>
                  <input
                    id="satellite-api-key-input"
                    type={satelliteApiKeyFocused ? "text" : "password"}
                    className="settings-input"
                    placeholder={DEFAULT_SATELLITE_API_KEY_PLACEHOLDER}
                    value={satelliteApiKeyInput}
                    onChange={(e) => {
                      setSatelliteApiKeyInput(e.target.value);
                      setSatelliteApiKeyStatus(null);
                    }}
                    onFocus={() => {
                      setSatelliteApiKeyFocused(true);
                      if (satelliteApiKeyInput.trim() === DEFAULT_SATELLITE_API_KEY_PLACEHOLDER) {
                        setSatelliteApiKeyInput("");
                      }
                    }}
                    onBlur={() => {
                      setSatelliteApiKeyFocused(false);
                    }}
                    aria-label="Satellite imagery API key"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <div className="settings-inline-actions" style={{ marginTop: "0.75rem" }}>
                    <button type="button" onClick={() => void handleSaveSatelliteApiKey()}>
                      Save key
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => {
                        setSatelliteApiKeyInput("");
                        setSatelliteApiKeyStatus(
                          "Satellite API key reset to generic placeholder. Click Save to clear stored key.",
                        );
                        setSatelliteApiKeyFocused(false);
                      }}
                    >
                      Reset to generic
                    </button>
                  </div>
                  <p className="text-muted small" aria-live="polite" style={{ marginTop: "0.5rem" }}>
                    {satelliteApiKeyStatus ??
                      (satelliteApiKeySaved
                        ? "A real satellite API key is saved for this device."
                        : "No real satellite API key is saved yet.")}
                  </p>

                  {showSatelliteKeyTutorial && (
                    <div className="settings-api-key-tutorial">
                      <h4>Detailed setup tutorial</h4>
                      <ol>
                        <li>
                          Open <a href="https://www.maptiler.com/cloud/" target="_blank" rel="noopener noreferrer">maptiler.com/cloud</a> and sign in
                          (or create an account and verify email first).
                        </li>
                        <li>
                          In the dashboard, open <strong>Keys</strong>, click <strong>Create key</strong>, and name it
                          <code> Athena Satellite</code>.
                        </li>
                        <li>
                          In key capabilities, enable raster tile access and confirm satellite tiles are included for your plan.
                        </li>
                        <li>
                          Save the key and copy the full key value exactly as shown.
                        </li>
                        <li>
                          Return to Athena, open <strong>Settings → API keys</strong>, paste the key in this field, then click
                          <strong> Save key</strong>.
                        </li>
                        <li>
                          Open <strong>Offline Map Manager</strong>, go to <strong>Local Region</strong>, then click
                          <strong> Download satellite (MBTiles)</strong>.
                        </li>
                        <li>
                          If you receive an authorization error, regenerate a key in MapTiler, save the new key here, and retry.
                        </li>
                        <li>
                          Keep this key private. Never commit the real value to git or share screenshots that expose it.
                        </li>
                      </ol>
                    </div>
                  )}
                </section>
              </>
            )}
            {settingsPanel === "maps" && (
              <>
                <section className="library-section">
                  <h3>Offline map packs (Kiwix)</h3>
                  {libraryError !== null && (
                    <p className="error settings-maps-error-banner" role="alert">
                      {libraryError}
                    </p>
                  )}
                  <p className="text-muted small">
                    <strong>Kiwix</strong> regional map files for the built-in map. The app can suggest a small pack for your saved location; pick others from the list. Use Wi‑Fi for large downloads.
                  </p>
                  {mapCatalog.length === 0 && (
                    <p className="text-muted small settings-map-catalog-empty-note" role="status">
                      <strong>No Kiwix map packs listed right now.</strong> Use <strong>Offline raster basemaps</strong> below to add MBTiles/PMTiles, or get <code>.zim</code> files from{" "}
                      <a href="https://library.kiwix.org" target="_blank" rel="noopener noreferrer">
                        library.kiwix.org
                      </a>{" "}
                      and put them where the app can scan them (see Library).
                    </p>
                  )}
                  {mapsSuggestion !== null && (
                    <div className="settings-map-suggestion-banner">
                      <strong>Suggested for your location:</strong> {mapsSuggestion.name} ({mapsSuggestion.size_label})
                      {downloadedMaps.some(
                        (m) => m.region === mapsSuggestion.region && m.style === mapsSuggestion.style,
                      ) ? (
                        <span className="text-muted"> — already downloaded</span>
                      ) : (
                        <button
                          type="button"
                          className="settings-map-suggestion-dl"
                          onClick={() => void handleDownloadMap(mapsSuggestion)}
                          disabled={networkReachable === false}
                        >
                          Download
                        </button>
                      )}
                    </div>
                  )}
                  <ul className="settings-map-catalog-list">
                    {mapCatalog.map((entry) => {
                      const acquired = downloadedMaps.some(
                        (m) => m.region === entry.region && m.style === entry.style,
                      );
                      return (
                        <li key={entry.id} className="settings-map-catalog-item">
                          <div className="settings-map-catalog-main">
                            <span className="settings-map-catalog-name">
                              {entry.name}
                              {entry.for_user_region ? (
                                <span className="settings-map-catalog-badge">Matches location</span>
                              ) : null}
                            </span>
                            <span className="text-muted settings-map-catalog-meta">
                              {entry.size_label} · {entry.region}
                            </span>
                          </div>
                          {acquired ? (
                            <span className="text-muted small">Downloaded</span>
                          ) : (
                            <button
                              type="button"
                              className="secondary"
                              onClick={() => void handleDownloadMap(entry)}
                              disabled={networkReachable === false}
                            >
                              Download
                            </button>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                  {mapCatalog.length === 0 && (
                    <p className="text-muted small">Enable location-aware answers and set a location to highlight matching regions.</p>
                  )}
                </section>
                <section className="library-section">
                  <h3>Downloaded maps</h3>
                  {downloadedMaps.length === 0 ? (
                    <p className="text-muted small">No map ZIMs in app storage yet.</p>
                  ) : (
                    <ul className="settings-downloaded-maps-list">
                      {downloadedMaps.map((m) => (
                        <li key={m.path} className="settings-downloaded-map-row">
                          <span>
                            <strong>{m.name}</strong>
                            <span className="text-muted small"> · {m.region}</span>
                          </span>
                          <button
                            type="button"
                            className="secondary"
                            onClick={() => void handleRemoveDownloadedMap(m.path)}
                          >
                            Remove
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
                <section className="library-section settings-map-phase2">
                  <h3>Offline raster basemaps</h3>
                  <p className="text-muted small">
                    Download map <strong>tiles</strong> (not satellite) for offline use in the in-app map: <strong>OpenStreetMap</strong> street style and{" "}
                    <strong>OpenTopoMap</strong> terrain/hillshade. Requires internet during download; respects OSM tile policy (throttled). Use a smaller radius or zoom range if the job is too large.
                  </p>
                  <p className="text-muted small">
                    <strong>Interchange:</strong> export any pack to <code>.mbtiles</code>, or import <strong>raster</strong>{" "}
                    <code>.mbtiles</code> / <code>.pmtiles</code> (vector MVT archives are not supported). Latest user import wins in the map picker.{" "}
                    <strong>Satellite / aerial:</strong> not bundled—use your own legally obtained raster MBTiles or PMTiles (e.g. from imagery you’re licensed to cache offline).
                  </p>
                  <p className="text-muted small">
                    Place to geocode defaults to your saved location; override below. Attribution is shown on the map.
                  </p>
                  <div className="settings-inline-actions settings-tile-actions settings-tile-import-row" style={{ marginTop: "0.5rem" }}>
                    <button type="button" className="secondary" onClick={() => void handleImportMbtiles()}>
                      Import .mbtiles…
                    </button>
                    <button type="button" className="secondary" onClick={() => void handleImportPmtiles()}>
                      Import .pmtiles…
                    </button>
                  </div>
                  <div className="settings-tile-options">
                    <label className="settings-tile-field">
                      <span>Geocode query (optional)</span>
                      <input
                        type="text"
                        className="settings-input"
                        placeholder={userLocationAddress.trim() ? `Using: ${userLocationAddress.slice(0, 48)}…` : "City, region, country"}
                        value={tileGeocodeQuery}
                        onChange={(e) => setTileGeocodeQuery(e.target.value)}
                        aria-label="Override place name for tile download"
                      />
                    </label>
                    <div className="settings-tile-field">
                      <label htmlFor="tile-radius">
                        Radius: <strong>{tileRadiusMiles}</strong> mi
                      </label>
                      <input
                        id="tile-radius"
                        type="range"
                        min={3}
                        max={28}
                        step={1}
                        value={tileRadiusMiles}
                        onChange={(e) => setTileRadiusMiles(Number(e.target.value))}
                        className="memory-slider"
                        aria-label="Tile download radius in miles"
                      />
                    </div>
                    <div className="settings-tile-presets" role="group" aria-label="Radius and zoom presets">
                      <span className="settings-tile-preset-label">Quick radius</span>
                      <button type="button" className="secondary" onClick={() => setTileRadiusMiles(6)}>
                        6 mi
                      </button>
                      <button type="button" className="secondary" onClick={() => setTileRadiusMiles(12)}>
                        12 mi
                      </button>
                      <button type="button" className="secondary" onClick={() => setTileRadiusMiles(20)}>
                        20 mi
                      </button>
                      <span className="settings-tile-preset-label">Zoom presets</span>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          setTileZoomMin(10);
                          setTileZoomMax(12);
                        }}
                      >
                        Area z10–12
                      </button>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          setTileZoomMin(12);
                          setTileZoomMax(14);
                        }}
                      >
                        Detail z12–14
                      </button>
                    </div>
                    <div className="settings-tile-zoom-row">
                      <label className="settings-tile-field">
                        <span>Min zoom</span>
                        <input
                          type="number"
                          className="settings-input settings-input-narrow"
                          min={8}
                          max={14}
                          value={tileZoomMin}
                          onChange={(e) => setTileZoomMin(Number(e.target.value))}
                          aria-label="Minimum zoom level"
                        />
                      </label>
                      <label className="settings-tile-field">
                        <span>Max zoom</span>
                        <input
                          type="number"
                          className="settings-input settings-input-narrow"
                          min={10}
                          max={16}
                          value={tileZoomMax}
                          onChange={(e) => setTileZoomMax(Number(e.target.value))}
                          aria-label="Maximum zoom level"
                        />
                      </label>
                    </div>
                    {tilePackEstimate != null && (
                      <p className={tilePackEstimate.within_limit ? "text-muted small" : "settings-tile-estimate-warn"}>
                        Preview: ~{tilePackEstimate.count.toLocaleString()} tiles (limit {tilePackEstimate.max_tiles.toLocaleString()}).
                        {!tilePackEstimate.within_limit && " Shrink radius or zoom before downloading."}
                      </p>
                    )}
                    <div className="settings-inline-actions settings-tile-actions">
                      <button type="button" className="secondary" disabled={tileEstimateBusy} onClick={() => void handlePreviewTileEstimate()}>
                        {tileEstimateBusy ? "Estimating…" : "Preview tile count"}
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleStartRasterTileDownload("osm")}
                        disabled={networkReachable === false}
                      >
                        Download street (OSM) tiles
                      </button>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => void handleStartRasterTileDownload("opentopo")}
                        disabled={networkReachable === false}
                      >
                        Download terrain (OpenTopoMap)
                      </button>
                    </div>
                  </div>
                  {(() => {
                    const tileJobs = Object.entries(downloadJobs).filter(
                      ([k]) => k.startsWith("tiles:") || k.startsWith("import:") || k.startsWith("mbtiles_export:"),
                    );
                    if (tileJobs.length === 0) return null;
                    return (
                      <>
                        <h4 className="settings-tile-packs-heading">Active tile jobs</h4>
                        <ul className="settings-tile-active-jobs">
                          {tileJobs.map(([id, job]) => (
                            <li key={id}>
                              <span>{job.label}</span>
                              <span className="text-muted small">
                                {job.total != null && job.total > 0
                                  ? isTileCountProgressJob(id)
                                    ? `${job.downloaded.toLocaleString()} / ${job.total.toLocaleString()} tiles · ${Math.min(
                                        100,
                                        Math.round((job.downloaded / job.total) * 100),
                                      )}%`
                                    : `${formatBytes(job.downloaded)} / ${formatBytes(job.total)} · ${Math.min(
                                        100,
                                        Math.round((job.downloaded / job.total) * 100),
                                      )}%`
                                  : job.downloaded > 0
                                    ? isTileCountProgressJob(id)
                                      ? `${job.downloaded.toLocaleString()} tiles…`
                                      : `${formatBytes(job.downloaded)}…`
                                    : "…"}
                              </span>
                            </li>
                          ))}
                        </ul>
                      </>
                    );
                  })()}
                  <h4 className="settings-tile-packs-heading">Installed tile packs</h4>
                  {tilePacks.length === 0 ? (
                    <p className="text-muted small">None yet.</p>
                  ) : (
                    <ul className="settings-downloaded-maps-list">
                      {tilePacks.map((p) => (
                        <li key={p.id} className="settings-downloaded-map-row">
                          <span>
                            <strong>{p.name}</strong>
                            <span className="text-muted small">
                              {" "}
                              · {tilePackSourceLabel(p.source)} · z{p.min_zoom}–{p.max_zoom} · ~{p.radius_miles}mi
                            </span>
                          </span>
                          <div className="settings-downloaded-map-actions">
                            <button type="button" className="secondary" onClick={() => void handleExportPackToMbtiles(p.id, p.name)}>
                              Export .mbtiles
                            </button>
                            <button type="button" className="secondary" onClick={() => void handleRemoveTilePack(p.id)}>
                              Remove
                            </button>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                  <p className="text-muted small settings-phase2-note">
                    True satellite imagery is licensing-heavy for bulk offline cache; not included here. Terrain view uses OpenTopoMap (SRTM + OSM). User-supplied MBTiles may be any compatible raster source—comply with its license.
                  </p>
                </section>
              </>
            )}
            {settingsPanel === "mesh" && (
              <>
                {meshRadioConnected === null ? (
                  <section className="library-section">
                    <h3>Mesh</h3>
                    <p className="text-muted">Checking for a Meshtastic USB radio…</p>
                  </section>
                ) : meshRadioConnected === false ? (
                  <section className="library-section">
                    <h3>Mesh</h3>
                    <p className="text-muted">
                      No Meshtastic USB radio was detected. Connect a device and wait a few seconds—the header radio button and this menu will activate when serial is available.
                    </p>
                  </section>
                ) : (
                  <section className="library-section">
                    <h3>Meshtastic Mesh</h3>
                    <p className="text-muted">
                      When a USB Meshtastic radio is connected, inbound text from other nodes (broadcast or DM to you) can start a saved chat per sender and reply over the mesh using Ollama. Requires Ollama running. Turn off to only listen without AI replies.
                    </p>
                    <div className="feature-checkbox-wrap">
                      <input
                        id="mesh-ai-auto-reply-checkbox"
                        type="checkbox"
                        className="feature-checkbox"
                        checked={meshAiAutoReply}
                        onChange={(e) => void handleMeshAiAutoReplyToggle(e.target.checked)}
                        aria-label="Auto-reply to Meshtastic messages with AI"
                      />
                      <label htmlFor="mesh-ai-auto-reply-checkbox" className="feature-checkbox-label">
                        Auto-reply to mesh (Primary broadcast → DM; private chat per sender)
                      </label>
                    </div>
                    <RadioSettingsView />
                  </section>
                )}
              </>
            )}
            {settingsPanel === "voice" && (
              <>
                {VOICE_AND_MIC_FEATURES_HIDDEN && (
                  <section className="library-section">
                    <h3>Voice</h3>
                    <p className="text-muted small settings-build-banner">
                      <strong>Build note:</strong> Text-to-speech and microphone conversation controls are hidden in this build. The code remains in the app for a future release.
                    </p>
                  </section>
                )}
                {!VOICE_AND_MIC_FEATURES_HIDDEN && (
                  <section className="library-section">
                    <h3>Text-to-Speech</h3>
                    <p className="text-muted">
                      Have Athena read responses aloud using your browser&apos;s built-in voices. Works offline on most systems.
                    </p>
                    <div className="feature-checkbox-wrap">
                      <input
                        id="feature-tts-checkbox"
                        type="checkbox"
                        className="feature-checkbox"
                        checked={ttsEnabled}
                        onChange={(e) => {
                          const enabled = e.target.checked;
                          setTtsEnabled(enabled);
                          if (!enabled) {
                            stopTts();
                          }
                          if (typeof window !== "undefined") {
                            try {
                              window.localStorage.setItem("athena-tts-enabled", enabled ? "1" : "0");
                            } catch {
                              // ignore
                            }
                          }
                        }}
                        aria-label="Enable text-to-speech for Athena's replies"
                      />
                      <label htmlFor="feature-tts-checkbox" className="feature-checkbox-label">
                        Enable text-to-speech
                      </label>
                    </div>
                    {ttsEnabled && (
                      <>
                        <div className="tts-voice-row">
                          <label htmlFor="voice-preset-select">Athena voice</label>
                          <select
                            id="voice-preset-select"
                            className="theme-select"
                            value={voicePreset}
                            onChange={(e) => {
                              const v = e.target.value as VoicePresetId;
                              if (v === "default" || v === "fallout" || v === "ultron") {
                                setVoicePreset(v);
                                try {
                                  window.localStorage.setItem("athena-voice-preset", v);
                                } catch {
                                  // ignore
                                }
                              }
                            }}
                            aria-label="Character voice preset"
                          >
                            {VOICE_PRESETS.map((id) => (
                              <option key={id} value={id}>
                                {VOICE_PRESET_LABELS[id]}
                              </option>
                            ))}
                          </select>
                          <p className="text-muted small">
                            Default uses your chosen system voice. Fallout adds 1950s-style static; Ultron uses a deeper voice. For custom Piper voices, add models to app data.
                          </p>
                        </div>
                        <div className="tts-voice-row">
                          <label htmlFor="tts-voice-select">Preferred system voice</label>
                          <select
                            id="tts-voice-select"
                            className="theme-select"
                            value={ttsPreferredVoiceUri ?? ""}
                            onChange={(e) => {
                              const uri = e.target.value || null;
                              setTtsPreferredVoiceUri(uri);
                              if (typeof window !== "undefined") {
                                try {
                                  if (uri) {
                                    window.localStorage.setItem("athena-tts-voice", uri);
                                  } else {
                                    window.localStorage.removeItem("athena-tts-voice");
                                  }
                                } catch {
                                  // ignore
                                }
                              }
                            }}
                            disabled={!ttsVoices.length}
                            aria-label="Preferred voice for text-to-speech"
                          >
                            {ttsVoices.length === 0 ? (
                              <option value="">Loading voices…</option>
                            ) : (
                              <>
                                <option value="">System default</option>
                                {ttsVoices.map((v) => (
                                  <option key={v.voiceURI} value={v.voiceURI}>
                                    {v.name} ({v.lang})
                                  </option>
                                ))}
                              </>
                            )}
                          </select>
                          <p className="text-muted small">
                            Voice availability depends on your operating system and browser. On Apple devices, look for the Apple voices for the best match.
                          </p>
                        </div>
                      </>
                    )}
                  </section>
                )}
              </>
            )}
            {settingsPanel === "appearance" && (
              <section className="library-section ui-colors-section">
                <h3>Window</h3>
                <div className="feature-checkbox-wrap" style={{ marginBottom: "1rem" }}>
                  <input
                    id="fullscreen-toggle"
                    type="checkbox"
                    className="feature-checkbox"
                    checked={fullscreen}
                    onChange={(e) => {
                      const checked = e.target.checked;
                      setFullscreen(checked);
                      void (async () => {
                        try {
                          await invoke("set_fullscreen", { fullscreen: checked });
                        } catch {
                          /* Native transition may fail; resync from window below */
                        }
                        const actual = await invoke<boolean>("is_fullscreen").catch(() => false);
                        setFullscreen(actual);
                      })();
                    }}
                    aria-label="Fullscreen (macOS: native fullscreen)"
                  />
                  <label htmlFor="fullscreen-toggle" className="feature-checkbox-label">
                    Fullscreen
                  </label>
                </div>
                <p className="text-muted small" style={{ marginTop: "-0.5rem", marginBottom: "1rem" }}>
                  On macOS, uses native fullscreen (dedicated space, menu bar and dock hide). Press Escape or toggle again to exit.
                </p>
                <h3>Night vision</h3>
                <div className="nightvision-mode-wrap">
                  <label htmlFor="nightvision-mode-select">Night vision</label>
                  <select
                    id="nightvision-mode-select"
                    className="theme-select"
                    value={nightVisionMode}
                    onChange={(e) => {
                      const mode = e.target.value as NightVisionMode;
                      setNightVisionMode(mode);
                      try {
                        localStorage.setItem(STORAGE_KEY_NIGHT_VISION_MODE, mode);
                      } catch {
                        // ignore
                      }
                      // #region agent log
                      try {
                        fetch("http://127.0.0.1:7625/ingest/8fd099f6-5880-4d68-82fc-1df05f97fa81", {
                          method: "POST",
                          headers: {
                            "Content-Type": "application/json",
                            "X-Debug-Session-Id": "035879",
                          },
                          body: JSON.stringify({
                            sessionId: "035879",
                            runId: "pre-fix",
                            hypothesisId: "H5-nightvision",
                            location: "App.tsx:nightvision-select",
                            message: "Night vision mode changed",
                            data: { mode },
                            timestamp: Date.now(),
                          }),
                        }).catch(() => {});
                      } catch {
                        // ignore instrumentation failures
                      }
                      // #endregion agent log
                    }}
                    aria-label="Night vision mode"
                  >
                    <option value="auto">Automatic (based on sunset/sunrise)</option>
                    <option value="on">Always on</option>
                    <option value="off">Off</option>
                  </select>
                  <p className="text-muted small">
                    Automatic uses your approximate location and local time to enable a deep red night-vision palette after sunset.
                  </p>
                </div>
              </section>
            )}
            {settingsPanel === "themes" && themesSubView === "studio" && studioDraft && (
              <section className="library-section ui-colors-section theme-studio-section">
                <p className="text-muted small">
                  Changes preview live on the app behind this window. Save to keep this theme alongside the built-in options.
                </p>
                <div className="theme-studio-field">
                  <label htmlFor="theme-studio-name">Theme name</label>
                  <input
                    id="theme-studio-name"
                    type="text"
                    className="settings-input"
                    value={studioDraft.name}
                    onChange={(e) => updateStudioDraft({ name: e.target.value })}
                    aria-label="Custom theme name"
                  />
                </div>
                <h3>Base layout</h3>
                <p className="text-muted small">Which built-in look this theme extends.</p>
                <div className="color-preset-wrap">
                  <label htmlFor="theme-studio-base">Starting point</label>
                  <select
                    id="theme-studio-base"
                    className="theme-select"
                    value={studioDraft.base}
                    onChange={(e) => updateStudioDraft({ base: e.target.value })}
                  >
                    {EXPERIENCE_PRESETS.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.title}
                      </option>
                    ))}
                  </select>
                </div>
                <h3>Personality</h3>
                <div className="color-preset-wrap">
                  <label htmlFor="theme-studio-persona">Personality</label>
                  <select
                    id="theme-studio-persona"
                    className="theme-select"
                    value={
                      studioDraft.personality_id === "fallout" || studioDraft.personality_id === "fieldguide" || studioDraft.personality_id === "default"
                        ? studioDraft.personality_id
                        : "default"
                    }
                    onChange={(e) => updateStudioDraft({ personality_id: e.target.value as PersonalityId })}
                  >
                    {PERSONAS.map((p) => (
                      <option key={p} value={p}>
                        {PERSONALITY_LABELS[p]}
                      </option>
                    ))}
                  </select>
                </div>
                <h3>Font</h3>
                <div className="color-preset-wrap">
                  <label htmlFor="theme-studio-font">Font override</label>
                  <select
                    id="theme-studio-font"
                    className="theme-select"
                    value={studioDraft.font_family ?? ""}
                    onChange={(e) => updateStudioDraft({ font_family: e.target.value || null })}
                  >
                    <option value="">Match base experience</option>
                    <option value="system">System UI</option>
                    <option value="mono-crt">Monospace / CRT</option>
                  </select>
                </div>
                <h3>Colors</h3>
                <div className="colors-editor">
                  <div className="color-fields color-fields-grid">
                    {(["bg", "primary", "text"] as const).map((key) => {
                      const label = key === "bg" ? "Background" : key === "primary" ? "Highlight" : "Text";
                      const baseC = resolveBaseColors(studioDraft.base);
                      const value =
                        key === "bg"
                          ? studioDraft.bg_color ?? baseC.bg
                          : key === "primary"
                            ? studioDraft.primary_color ?? baseC.primary
                            : studioDraft.text_color ?? baseC.text;
                      return (
                        <div key={key} className="color-field color-field-card">
                          <label htmlFor={`studio-color-${key}`}>{label}</label>
                          <div className="color-card-preview" style={{ background: value }} aria-hidden />
                          <div className="color-input-row color-input-row-stacked">
                            <input
                              id={`studio-color-${key}`}
                              type="color"
                              className="color-swatch color-swatch-large"
                              value={value}
                              onChange={(e) =>
                                updateStudioDraft(
                                  key === "bg"
                                    ? { bg_color: e.target.value }
                                    : key === "primary"
                                      ? { primary_color: e.target.value }
                                      : { text_color: e.target.value }
                                )
                              }
                              aria-label={`${label} color`}
                            />
                            <input
                              type="text"
                              className="color-hex settings-input"
                              value={value}
                              onChange={(e) =>
                                updateStudioDraft(
                                  key === "bg"
                                    ? { bg_color: e.target.value }
                                    : key === "primary"
                                      ? { primary_color: e.target.value }
                                      : { text_color: e.target.value }
                                )
                              }
                              aria-label={`${label} color (hex)`}
                            />
                            <PhotoshopColorPicker
                              label={label}
                              value={value}
                              onChange={(hex) =>
                                updateStudioDraft(
                                  key === "bg"
                                    ? { bg_color: hex }
                                    : key === "primary"
                                      ? { primary_color: hex }
                                      : { text_color: hex }
                                )
                              }
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
                <h3>Visual effects</h3>
                <p className="text-muted small">Saved with this theme only. Combine scanlines, vignette, and grain for a unique look.</p>
                <div className="theme-studio-effects">
                  <label className="theme-studio-check">
                    <input
                      type="checkbox"
                      checked={studioDraft.scanlines_enabled ?? false}
                      onChange={(e) => updateStudioDraft({ scanlines_enabled: e.target.checked })}
                    />
                    Scanlines (CRT)
                  </label>
                  <div className="theme-studio-slider-row">
                    <label htmlFor="studio-scan-opacity">Scanline strength</label>
                    <input
                      id="studio-scan-opacity"
                      type="range"
                      min={0}
                      max={1}
                      step={0.01}
                      value={studioDraft.scanlines_opacity ?? 0.15}
                      onChange={(e) => updateStudioDraft({ scanlines_opacity: Number(e.target.value) })}
                    />
                  </div>
                  <label className="theme-studio-check">
                    <input
                      type="checkbox"
                      checked={studioDraft.vignette_enabled ?? false}
                      onChange={(e) => updateStudioDraft({ vignette_enabled: e.target.checked })}
                    />
                    Edge vignette
                  </label>
                  <div className="theme-studio-slider-row">
                    <label htmlFor="studio-grain">Film grain</label>
                    <input
                      id="studio-grain"
                      type="range"
                      min={0}
                      max={1}
                      step={0.01}
                      value={studioDraft.film_grain ?? 0}
                      onChange={(e) => updateStudioDraft({ film_grain: Number(e.target.value) })}
                    />
                  </div>
                  <label className="theme-studio-check">
                    <input
                      type="checkbox"
                      checked={studioDraft.sharp_corners ?? false}
                      onChange={(e) => updateStudioDraft({ sharp_corners: e.target.checked })}
                    />
                    Sharp square corners (terminal style)
                  </label>
                </div>
                <div className="theme-studio-actions">
                  <button type="button" onClick={saveThemeStudio}>
                    Save theme
                  </button>
                  <button type="button" className="secondary" onClick={cancelThemeStudio}>
                    Cancel
                  </button>
                </div>
              </section>
            )}
            {settingsPanel === "themes" && themesSubView === "main" && (
              <section className="library-section ui-colors-section">
                <h3>Experience</h3>
                <p className="text-muted">
                  Built-in looks and your saved themes share this menu. Pick colors below for any non–field-guide experience.
                </p>
                <div className="color-preset-wrap">
                  <label htmlFor="experience-preset-select">Overall look</label>
                  <select
                    id="experience-preset-select"
                    className="theme-select"
                    value={activeThemeId}
                    onChange={(e) => {
                      const v = e.target.value;
                      if (v === "default" || v === "fallout" || v === "fieldguide") applyThemePreset(v);
                      else applyCustomTheme(v);
                    }}
                    aria-label="Overall experience theme"
                  >
                    <optgroup label="Built-in">
                      {EXPERIENCE_PRESETS.map((preset) => (
                        <option key={preset.id} value={preset.id}>
                          {preset.title}
                        </option>
                      ))}
                    </optgroup>
                    {customThemes.length > 0 ? (
                      <optgroup label="Your saved themes">
                        {customThemes.map((t) => (
                          <option key={t.id} value={t.id}>
                            {t.name}
                          </option>
                        ))}
                      </optgroup>
                    ) : null}
                  </select>
                </div>
                <p className="text-muted small" style={{ marginTop: "0.5rem" }}>
                  {EXPERIENCE_PRESETS.find((p) => p.id === activeThemeId)?.description ??
                    (customThemes.some((t) => t.id === activeThemeId) ? "Your saved theme (effects and colors from Theme studio)." : "")}
                </p>
                <p className="text-muted small">
                  Voice and tone are under <strong>AI &amp; Models → Personality</strong>. Choosing a built-in experience here still
                  updates personality to match.
                </p>
                <h3>Colors</h3>
                <p className="text-muted small">
                  {activeThemeId === "fieldguide" ? (
                    <>Field guide uses fixed warm paper tones (amber). Switch experience above to edit palette colors.</>
                  ) : (
                    <>
                      Palette applies to built-in looks and Fallout accents—even before you save a custom theme. Choose{" "}
                      <strong>Custom</strong> for full control.
                    </>
                  )}
                </p>
                <div className="colors-editor">
                  <div className="color-preset-wrap color-palette-bar">
                    <label htmlFor="theme-preset-select">Palette</label>
                    <select
                      id="theme-preset-select"
                      className="theme-select"
                      value={theme}
                      onChange={handlePresetChange}
                      aria-label="Color palette preset"
                      disabled={activeThemeId === "fieldguide"}
                    >
                      {THEMES.map((t) => (
                        <option key={t} value={t}>
                          {THEME_LABELS[t]}
                        </option>
                      ))}
                      <option value="custom">Custom</option>
                    </select>
                  </div>
                  <div className="color-fields color-fields-grid">
                    {(["bg", "primary", "text"] as const).map((key) => {
                      const label = key === "bg" ? "Background" : key === "primary" ? "Highlight" : "Text";
                      const value =
                        theme === "custom"
                          ? (key === "bg" ? customBg : key === "primary" ? customPrimary : customText)
                          : PRESET_COLORS[theme][key];
                      return (
                        <div key={key} className="color-field color-field-card">
                          <label htmlFor={`color-${key}`}>{label}</label>
                          <div className="color-card-preview" style={{ background: value }} aria-hidden />
                          <div className="color-input-row color-input-row-stacked">
                            <input
                              id={`color-${key}`}
                              type="color"
                              className="color-swatch color-swatch-large"
                              value={value}
                              onChange={(e) => handleCustomColorChange(key, e.target.value)}
                              aria-label={`${label} color`}
                              disabled={activeThemeId === "fieldguide"}
                            />
                            <input
                              type="text"
                              className="color-hex settings-input"
                              value={value}
                              onChange={(e) => handleCustomColorChange(key, e.target.value)}
                              aria-label={`${label} color (hex)`}
                              disabled={activeThemeId === "fieldguide"}
                            />
                            <PhotoshopColorPicker
                              label={label}
                              value={value}
                              onChange={(hex) => handleCustomColorChange(key, hex)}
                              disabled={activeThemeId === "fieldguide"}
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
                <section className="custom-themes-section theme-studio-entry" aria-label="Custom themes">
                  <h3>My themes</h3>
                  <p className="text-muted small">
                    Open <strong>Theme studio</strong> to name a theme, set colors, scanlines, vignette, grain, and sharp corners.
                    Saved themes appear under Experience above.
                  </p>
                  <div className="theme-studio-entry-actions">
                    <button type="button" onClick={openThemeStudioNew}>
                      New theme in Theme studio…
                    </button>
                  </div>
                  {customThemes.length === 0 ? (
                    <p className="text-muted small">No saved themes yet.</p>
                  ) : (
                    <ul className="custom-themes-list custom-themes-list-compact">
                      {customThemes.map((t) => (
                        <li key={t.id} className={`custom-theme-row ${activeThemeId === t.id ? "active" : ""}`}>
                          <button
                            type="button"
                            className="custom-theme-select"
                            onClick={() => applyCustomTheme(t.id)}
                            aria-pressed={activeThemeId === t.id}
                          >
                            <span className="custom-theme-name">{t.name}</span>
                          </button>
                          <button type="button" className="secondary" onClick={() => openThemeStudioEdit(t.id)}>
                            Edit
                          </button>
                          <button
                            type="button"
                            className="custom-theme-delete"
                            onClick={() => handleDeleteCustomTheme(t.id)}
                            aria-label={`Delete theme ${t.name}`}
                          >
                            Delete
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              </section>
            )}
            {settingsPanel === "troubleshooting" && (
              <>
                <section className="library-section">
                  <h3>Error log</h3>
                  <p className="text-muted">
                    If something fails, copy this log when reporting an issue. The log is stored on this device only.
                  </p>
                  <div className="error-log-wrap">
                    <textarea
                      readOnly
                      className="error-log-text"
                      value={errorLog || "(No errors logged yet)"}
                      aria-label="Error log"
                      rows={8}
                    />
                    <button
                      type="button"
                      className="error-log-copy"
                      onClick={() => {
                        const t = errorLog || "";
                        if (t) navigator.clipboard.writeText(t);
                      }}
                      disabled={!errorLog}
                    >
                      Copy to clipboard
                    </button>
                  </div>
                </section>
                <section className="library-section">
                  <h3>Diagnostics &amp; library</h3>
                  <p className="text-muted small">
                    For <strong>indexed documents</strong> and rebuilding search, use <strong>Library</strong> in the header. For{" "}
                    <strong>full app documentation</strong>, use <strong>Help</strong> or type <code>/athena</code> in chat.
                  </p>
                  <div className="settings-inline-actions">
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => {
                        setSettingsOpen(false);
                        setLibraryOpen(true);
                      }}
                    >
                      Open Library
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => {
                        setSettingsOpen(false);
                        setHelpOpen(true);
                      }}
                    >
                      Open Help
                    </button>
                  </div>
                </section>
                <section className="library-section">
                  <h3>Developer tools</h3>
                  <p className="text-muted small">
                    Go back to the main Settings menu, then tap the <strong>Settings</strong> title in the header seven times to
                    open raw diagnostics (model, index, theme state).
                  </p>
                </section>
              </>
            )}
            {settingsPanel === "developer" && (
              <section className="library-section dev-panel" aria-label="Developer debug info">
                <div className="dev-toggles">
                  <label className="dev-toggle-row">
                    <span className="dev-toggle-label">Debug mode</span>
                    <input
                      type="checkbox"
                      checked={debugMode}
                      onChange={(e) => setDebugMode(e.target.checked)}
                      aria-label="Enable debug mode"
                    />
                  </label>
                </div>
                <p className="text-muted small">Debug mode may show extra UI in chat. Device stats overlay is under Settings → General.</p>
                <h3>System</h3>
                <pre className="dev-pre">
                  {systemSpecs !== null
                    ? `RAM: ${systemSpecs.ram_gb} GB | CPU: ${systemSpecs.cpu_cores} cores | Recommended tier: ${systemSpecs.recommended_tier}`
                    : "Loading…"}
                  {"\n"}Ollama: {ollamaAvailable ? "running" : "not running"}
                </pre>
                <h3>Model & memory</h3>
                <pre className="dev-pre">
                  Tier: {modelOverride} | Context: {memoryMessages} messages
                  {memoryEstimate !== null
                    ? `\nModel: ${memoryEstimate.model_name} (${memoryEstimate.model_size_b}B) | ~${memoryEstimate.estimated_model_ram_gb.toFixed(1)} GB RAM\nContext tokens: ~${memoryEstimate.estimated_context_tokens.toLocaleString()} | ~${memoryEstimate.estimated_context_ram_mb} MB\nTotal estimated: ~${memoryEstimate.total_estimated_ram_gb.toFixed(2)} GB RAM`
                    : ""}
                </pre>
                <h3>Theme & UI</h3>
                <pre className="dev-pre">
                  activeThemeId: {activeThemeId} | activePersonalityId: {activePersonalityId}
                  {"\n"}theme (preset): {theme} | selectedCustomThemeId: {selectedCustomThemeId ?? "null"}
                  {"\n"}Custom themes: {customThemes.length} ({customThemes.map((t) => t.id).join(", ") || "none"})
                </pre>
                <h3>Index & library</h3>
                <pre className="dev-pre">
                  {stats !== null
                    ? `Files: ${stats.total_files} (text: ${stats.text_count}, image: ${stats.image_count}, video: ${stats.video_count}, map: ${stats.map_count}, zim: ${stats.zim_count})`
                    : "Loading…"}
                  {"\n"}Watch folder: {watchFolder ?? "null"}
                </pre>
                <h3>Chats</h3>
                <pre className="dev-pre">
                  Chats: {chatList.length} | currentChatId: {currentChatId ?? "null"}
                </pre>
                <h3>Features</h3>
                <pre className="dev-pre">
                  Map enabled: {featureMapEnabled} | User location: {userLocationAddress || "(not set)"}
                </pre>
                <h3>Error log</h3>
                <pre className="dev-pre dev-log">{errorLog || "(empty)"}</pre>
              </section>
            )}
            {libraryError !== null && <p className="error">{libraryError}</p>}
            </div>
            {localDataInstallOpen && (
              <div
                className="settings-nested-backdrop"
                role="presentation"
                onClick={() => setLocalDataInstallOpen(false)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setLocalDataInstallOpen(false);
                }}
              >
                <div
                  className="settings-nested-modal"
                  role="dialog"
                  aria-modal="true"
                  aria-labelledby="local-data-install-title"
                  onClick={(e) => e.stopPropagation()}
                >
                  <div className="settings-nested-header">
                    <h3 id="local-data-install-title">Install local data</h3>
                    <button
                      type="button"
                      className="close-btn"
                      onClick={() => setLocalDataInstallOpen(false)}
                      aria-label="Close"
                    >
                      ×
                    </button>
                  </div>
                  <div className="settings-nested-body">
                    <p className="text-muted small">
                      Downloads run in the background. Progress appears in the main header. Use a reliable connection for multi‑gigabyte map packs.
                    </p>
                    <p className="settings-network-row text-muted small">
                      Network:{" "}
                      {networkReachable === null
                        ? "Checking…"
                        : networkReachable
                          ? "OK — ready to download"
                          : "Offline or blocked — connect to the internet first"}
                      <button
                        type="button"
                        className="secondary settings-network-refresh"
                        onClick={() => {
                          setNetworkReachable(null);
                          void invoke<boolean>("check_network_reachable")
                            .then((r) => setNetworkReachable(r))
                            .catch(() => setNetworkReachable(false));
                        }}
                      >
                        Check again
                      </button>
                    </p>
                    {mapsSuggestionLoading ? (
                      <p className="text-muted small">Finding the smallest map pack for your location…</p>
                    ) : mapsSuggestion ? (
                      <div className="settings-local-data-primary">
                        <p>
                          <strong>Recommended:</strong> {mapsSuggestion.name}{" "}
                          <span className="text-muted">({mapsSuggestion.size_label})</span>
                        </p>
                        <p className="text-muted small">
                          Smallest Kiwix regional file that matches your saved location text (OpenStreetMap style).
                        </p>
                        {downloadedMaps.some(
                          (m) => m.region === mapsSuggestion.region && m.style === mapsSuggestion.style,
                        ) ? (
                          <p className="text-muted small">This pack is already downloaded.</p>
                        ) : (
                          <button
                            type="button"
                            onClick={() => void handleDownloadMap(mapsSuggestion)}
                            disabled={networkReachable === false}
                          >
                            Download recommended map
                          </button>
                        )}
                      </div>
                    ) : (
                      <p className="text-muted small">
                        Could not match your location to a regional pack. Try a clearer area (e.g. city and country) or open{" "}
                        <strong>Offline maps</strong> to choose manually.
                      </p>
                    )}
                    <div className="settings-inline-actions settings-nested-actions">
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          setLocalDataInstallOpen(false);
                          goToSettingsSubpanel("maps");
                        }}
                      >
                        All map options…
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      <HelpModal open={helpOpen} onClose={() => setHelpOpen(false)} />

      <OfflineMapManagerDashboard
        open={offlineMapManagerOpen}
        onClose={() => setOfflineMapManagerOpen(false)}
        savedUserLocationAddress={userLocationAddress}
        tileGeocodeOverride={tileGeocodeQuery}
        tileRadiusMiles={tileRadiusMiles}
        enabledMainMapRasterArchiveFileNames={enabledArchiveRasterFileNames}
        onMainMapArchiveRasterLayersChange={handleMainMapArchiveRasterLayersChange}
      />

      {meshOpen && (
        <MeshRadioModal open={meshOpen} onClose={() => setMeshOpen(false)} messages={meshMessages} />
      )}

      {libraryOpen && (
        <div className="modal-backdrop" onClick={() => setLibraryOpen(false)}>
          <div className="library-modal library-modal-tabs glass-panel" onClick={(e) => e.stopPropagation()}>
            <div className="library-header">
              <h2>Library</h2>
              <button type="button" className="close-btn" onClick={() => setLibraryOpen(false)}>
                ×
              </button>
            </div>
            <div className="library-tabs-scroll">
              <p className="library-desc">
                Use the tabs below like a browser: <strong>Documents</strong> for RAG files and Kiwix content for search,{" "}
                <strong>Maps</strong> for Kiwix map ZIMs, tile packs, and the <strong>map archive manager</strong> (PMTiles / MBTiles catalog). More library types can be added as new tabs later.
              </p>
              <div className="library-tablist" role="tablist" aria-label="Library sections">
                <button
                  type="button"
                  role="tab"
                  id="library-tab-documents"
                  aria-selected={libraryActiveTab === "documents"}
                  aria-controls="library-panel-documents"
                  className={"library-tab" + (libraryActiveTab === "documents" ? " library-tab-active" : "")}
                  onClick={() => setLibraryActiveTab("documents")}
                >
                  Documents
                </button>
                <button
                  type="button"
                  role="tab"
                  id="library-tab-maps"
                  aria-selected={libraryActiveTab === "maps"}
                  aria-controls="library-panel-maps"
                  className={"library-tab" + (libraryActiveTab === "maps" ? " library-tab-active" : "")}
                  onClick={() => setLibraryActiveTab("maps")}
                >
                  Maps
                </button>
              </div>
              <div className="library-tab-panels">
              {libraryActiveTab === "documents" && (
                <div
                  className="library-tab-panel"
                  id="library-panel-documents"
                  role="tabpanel"
                  aria-labelledby="library-tab-documents"
                >
                  <KnowledgeCatalogView />
                </div>
              )}
              {libraryActiveTab === "maps" && (
                <div
                  className="library-tab-panel"
                  id="library-panel-maps"
                  role="tabpanel"
                  aria-labelledby="library-tab-maps"
                >
                  <section className="library-section library-section-map-archives">
                    <h3>Map files (PMTiles / MBTiles)</h3>
                    <p className="text-muted small library-section-lead">
                      <strong>Download or import</strong> map tile archives for the viewer. Same tool as <strong>Map archives…</strong> on the map.
                    </p>
                    <div className="library-maps-action-row">
                      <button
                        type="button"
                        onClick={() => {
                          setLibraryOpen(false);
                          setOfflineMapManagerOpen(true);
                        }}
                      >
                        Open map file manager…
                      </button>
                    </div>
                  </section>
                  <section className="library-section library-section-maps">
                    <h3>Offline map packs (Kiwix)</h3>
                    <p className="text-muted small library-section-lead">
                      <strong>Regional map ZIMs</strong> for the map viewer. Downloads also show in Settings → Offline maps.
                    </p>
                    <div className="library-maps-action-row">
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          setLibraryOpen(false);
                          setSettingsOpen(true);
                          goToSettingsSubpanel("maps");
                        }}
                      >
                        Offline map settings…
                      </button>
                      <button type="button" onClick={() => void openUrl("https://library.kiwix.org")}>
                        Kiwix Library (browse ZIMs)
                      </button>
                    </div>
                    <div className="settings-network-row text-muted small library-maps-network" aria-live="polite">
                      Network:{" "}
                      {networkReachable === null
                        ? "—"
                        : networkReachable
                          ? "Reachable"
                          : "Not reachable — connect to download packs"}
                      <button
                        type="button"
                        className="secondary settings-network-refresh"
                        onClick={() => {
                          void invoke<boolean>("check_network_reachable")
                            .then((r) => setNetworkReachable(r))
                            .catch(() => setNetworkReachable(false));
                        }}
                      >
                        Check again
                      </button>
                    </div>
                    {mapCatalog.length === 0 && (
                      <p className="text-muted small settings-map-catalog-empty-note" role="status">
                        <strong>No regional map ZIMs are listed right now.</strong> The official Kiwix mirror&apos;s{" "}
                        <code>zim/maps/</code> folder is sometimes empty, so direct links can 404. For the <strong>in-app map</strong>, use{" "}
                        <strong>Offline map settings</strong> for raster basemaps and tile imports. For other Kiwix content, use the{" "}
                        <strong>Documents</strong> tab → <strong>Download more data</strong> and place <code>.zim</code> files in your library folder.
                      </p>
                    )}
                    {mapsSuggestion !== null && (
                      <div className="settings-map-suggestion-banner">
                        <strong>Suggested for your location:</strong> {mapsSuggestion.name} ({mapsSuggestion.size_label})
                        {downloadedMaps.some(
                          (m) => m.region === mapsSuggestion.region && m.style === mapsSuggestion.style,
                        ) ? (
                          <span className="text-muted"> — already downloaded</span>
                        ) : (
                          <button
                            type="button"
                            className="settings-map-suggestion-dl"
                            onClick={() => void handleDownloadMap(mapsSuggestion)}
                            disabled={networkReachable === false}
                          >
                            Download
                          </button>
                        )}
                      </div>
                    )}
                    {mapCatalog.length > 0 && (
                      <>
                        <h4 className="library-maps-subheading">Regional map packs</h4>
                        <ul className="settings-map-catalog-list">
                          {mapCatalog.map((entry) => {
                            const acquired = downloadedMaps.some(
                              (m) => m.region === entry.region && m.style === entry.style,
                            );
                            return (
                              <li key={entry.id} className="settings-map-catalog-item">
                                <div className="settings-map-catalog-main">
                                  <span className="settings-map-catalog-name">
                                    {entry.name}
                                    {entry.for_user_region ? (
                                      <span className="settings-map-catalog-badge">Matches location</span>
                                    ) : null}
                                  </span>
                                  <span className="text-muted settings-map-catalog-meta">
                                    {entry.size_label} · {entry.region}
                                  </span>
                                </div>
                                {acquired ? (
                                  <span className="text-muted small">Downloaded</span>
                                ) : (
                                  <button
                                    type="button"
                                    className="secondary"
                                    onClick={() => void handleDownloadMap(entry)}
                                    disabled={networkReachable === false}
                                  >
                                    Download
                                  </button>
                                )}
                              </li>
                            );
                          })}
                        </ul>
                      </>
                    )}
                    {mapDownloadJobs.length > 0 && (
                      <div className="library-maps-downloads" role="status" aria-live="polite">
                        <h4 className="library-maps-subheading">Downloading</h4>
                        <ul className="library-map-download-list">
                          {mapDownloadJobs.map(([id, job]) => (
                            <li key={id} className="library-map-download-item">
                              <div className="library-map-download-head">
                                <span className="library-map-download-label">{job.label}</span>
                                <span className="text-muted small">
                                  {job.total != null && job.total > 0
                                    ? `${formatBytes(job.downloaded)} / ${formatBytes(job.total)} · ${Math.min(
                                        100,
                                        Math.round((job.downloaded / job.total) * 100),
                                      )}%`
                                    : job.downloaded > 0
                                      ? `${formatBytes(job.downloaded)}…`
                                      : "Starting…"}
                                </span>
                              </div>
                              {job.total != null && job.total > 0 && (
                                <div className="library-map-download-bar" aria-hidden>
                                  <div
                                    className="library-map-download-bar-fill"
                                    style={{
                                      width: `${Math.min(100, Math.round((job.downloaded / job.total) * 100))}%`,
                                    }}
                                  />
                                </div>
                              )}
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {downloadedMaps.length > 0 && (
                      <>
                        <h4 className="library-maps-subheading">Downloaded (map viewer)</h4>
                        <ul className="library-file-list library-map-file-list">
                          {downloadedMaps.map((m) => (
                            <li key={m.path} className="library-file-item">
                              <span className="library-file-name" title={m.path}>
                                <strong>{m.name}</strong>
                                <span className="text-muted small"> · {m.region}</span>
                              </span>
                              <button
                                type="button"
                                className="library-file-remove"
                                onClick={() => void handleRemoveDownloadedMap(m.path)}
                                title="Remove map pack"
                                aria-label={`Remove map ${m.name}`}
                              >
                                Remove
                              </button>
                            </li>
                          ))}
                        </ul>
                      </>
                    )}
                    {mapZimsOnDiskOnly.length > 0 && (
                      <>
                        <h4 className="library-maps-subheading">Map ZIMs on disk (indexed, not in viewer)</h4>
                        <p className="text-muted small">
                          These look like Kiwix map archives but are not registered for the map tab yet. They are still indexed for search if they sit in your library folder.
                        </p>
                        <ul className="library-file-list library-map-file-list">
                          {mapZimsOnDiskOnly.map((file) => (
                            <li key={file.path} className="library-file-item">
                              <span className="library-file-name" title={file.path}>
                                {file.name}
                              </span>
                              <span className="library-file-size">{formatLibraryFileSizeBytes(file.size_bytes)}</span>
                              <button
                                type="button"
                                className="library-file-remove"
                                onClick={() => handleRemoveLibraryFile(file.path)}
                                title="Remove file"
                                aria-label={"Remove " + file.name}
                              >
                                Remove
                              </button>
                            </li>
                          ))}
                        </ul>
                      </>
                    )}
                    {mapDownloadJobs.length === 0 &&
                      downloadedMaps.length === 0 &&
                      mapZimsOnDiskOnly.length === 0 &&
                      mapCatalog.length === 0 &&
                      mapsSuggestion === null && (
                        <p className="text-muted">
                          No map downloads in progress and no map packs on disk yet. When mirrors list regional ZIMs, download from the catalog above; otherwise use{" "}
                          <strong>Offline map settings</strong> for raster basemaps, or add a <code>maps_*</code> ZIM where the app can see it.
                        </p>
                      )}
                  </section>
                  {stats !== null && (
                    <section className="library-section library-stats library-stats-maps">
                      <div className="stats">
                        <span className="stat">Map ZIM files: {mapLibraryFiles.length}</span>
                        <span className="stat">Active downloads: {mapDownloadJobs.length}</span>
                      </div>
                    </section>
                  )}
                </div>
              )}
              </div>
            </div>
            {libraryError !== null && <p className="error library-tab-error">{libraryError}</p>}
            {libraryFilesModalOpen && (
              <div className="modal-backdrop" onClick={() => setLibraryFilesModalOpen(false)}>
                <div className="library-modal library-files-modal" onClick={(e) => e.stopPropagation()}>
                  <div className="library-header">
                    <h2>All documents (AI search)</h2>
                    <button
                      type="button"
                      className="close-btn"
                      onClick={() => setLibraryFilesModalOpen(false)}
                      aria-label="Close"
                    >
                      ×
                    </button>
                  </div>
                  <p className="text-muted small" style={{ padding: "0 1.25rem" }}>
                    Map ZIMs are not listed here — open Library and use the <strong>Maps</strong> tab.
                  </p>
                  {ragDisplayFiles.length === 0 ? (
                    <p className="text-muted" style={{ padding: "0 1.25rem 1rem" }}>
                      No document files in your library folder.
                    </p>
                  ) : (
                    <ul className="library-file-list library-file-list-full">
                      {ragDisplayFiles.map((file) => (
                        <li key={file.path} className="library-file-item">
                          <span className="library-file-name" title={file.path}>
                            {file.name}
                          </span>
                          <span className="library-file-size">{formatLibraryFileSizeBytes(file.size_bytes)}</span>
                          <button
                            type="button"
                            className="library-file-remove"
                            onClick={() => {
                              handleRemoveLibraryFile(file.path);
                              if (ragDisplayFiles.length <= 1) setLibraryFilesModalOpen(false);
                            }}
                            title="Remove from library"
                            aria-label={"Remove " + file.name}
                          >
                            Remove
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {kiwixSearchOpen && (
        <div className="modal-backdrop" onClick={() => setKiwixSearchOpen(false)}>
          <div className="library-modal kiwix-modal" onClick={(e) => e.stopPropagation()}>
            <div className="library-header">
              <h2>Offline ZIM (Kiwix)</h2>
              <button type="button" className="close-btn" onClick={() => setKiwixSearchOpen(false)}>
                ×
              </button>
            </div>
            <div className="kiwix-setup-content">
              <p className="kiwix-instructions">
                Download ZIM files from the Kiwix Library in your browser. Place them anywhere inside your designated library folder (the folder you chose in Library settings). Then click <strong>Scan for Local Files</strong> to find them here.
              </p>
              <div className="kiwix-setup-actions">
                <button
                  type="button"
                  className="kiwix-open-library-btn"
                  onClick={() => openUrl("https://library.kiwix.org")}
                >
                  Open Kiwix Library
                </button>
                <button
                  type="button"
                  className={`kiwix-scan-btn${kiwixScanLoading ? " kiwix-scan-btn-loading" : ""}`}
                  onClick={handleScanKiwixZim}
                  disabled={kiwixScanLoading}
                  aria-busy={kiwixScanLoading}
                >
                  {kiwixScanLoading ? (
                    <>
                      <span className="kiwix-scan-spinner" aria-hidden />
                      <span>Scanning…</span>
                    </>
                  ) : (
                    "Scan for Local Files"
                  )}
                </button>
              </div>
              {kiwixScanError != null && (
                <p className="kiwix-scan-error" role="alert">
                  {kiwixScanError}
                </p>
              )}
              {discoveredZims.length > 0 ? (
                <div className="kiwix-discovered-list-wrap">
                  <p className="kiwix-discovered-count text-muted">
                    Found {discoveredZims.length} ZIM file{discoveredZims.length !== 1 ? "s" : ""} in your library folder.
                  </p>
                  <ul className="kiwix-list kiwix-discovered-list">
                    {discoveredZims.map((z) => (
                      <li key={z.path} className="kiwix-item">
                        <div className="kiwix-item-head">
                          <strong title={z.path}>{z.name}</strong>
                          <span className="kiwix-size">{z.size_label}</span>
                        </div>
                        <p className="kiwix-path-muted text-muted" title={z.path}>
                          {z.path}
                        </p>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : !kiwixScanLoading && !kiwixScanError ? (
                <p className="text-muted">No ZIM files found yet. Download from the Kiwix Library and put them in your library folder, then scan.</p>
              ) : null}
              <section className="resource-links-section" aria-label="External resource links">
                <h3 className="resource-links-title">Find more content to ingest</h3>
                <p className="resource-links-intro text-muted">
                  Visit these sites for survival, societal rebuilding, manufacturing, and technical docs (repair manuals, parts catalogs). Download or save content, then add it to your library folder.
                </p>
                <div className="resource-links-grid">
                  {Array.from(
                    new Set(RESOURCE_LINKS.map((r) => r.category))
                  ).map((category) => (
                    <div key={category} className="resource-links-category">
                      <h4 className="resource-links-category-title">{category}</h4>
                      <ul className="resource-links-list">
                        {RESOURCE_LINKS.filter((r) => r.category === category).map((r) => (
                          <li key={r.url}>
                            <button
                              type="button"
                              className="resource-link-btn"
                              onClick={() => openUrl(r.url)}
                            >
                              {r.label}
                            </button>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              </section>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
