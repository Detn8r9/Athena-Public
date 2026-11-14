import type { GoToTarget } from "./goTo";

/**
 * Every screen, tab and setting in ATHENA, in one list, so a person can always find where a thing
 * lives. Used by:
 *  - the header search ("find anything"),
 *  - Settings search and Settings → "Settings on other screens",
 * Each entry jumps straight to the screen, tab and section that holds it.
 *
 * Keep `where` in plain words matching the labels people see on screen.
 */
export type Place = {
  id: string;
  title: string;
  /** Breadcrumb as shown in the app, e.g. "Library › Vault". */
  where: string;
  detail: string;
  keywords: string[];
  target: GoToTarget;
  /** screen = a top-level screen; section = a tab or part of a screen; setting = something you configure. */
  kind: "screen" | "section" | "setting";
  /** Lowest rank that can open it (matches the header and Home rules). */
  minRank?: number;
};

const LIB = { view: "archive" as const };
/** Library › Vault, optionally one of its sections (its own tab ids: coverage, scan, packs, …). */
const VAULT = (section?: string): GoToTarget => ({ ...LIB, tab: "vault", section, anchor: section ? `vault-${section}` : undefined });
const FIELD_OPS = (tab: string): GoToTarget => ({ view: "field_ops", tab });
const COMMS = (tab: string): GoToTarget => ({ view: "comms_hub", tab });
const FILES = (tab: string): GoToTarget => ({ view: "data_acquisition", tab });
const CATALOG = (tab: string): GoToTarget => ({ view: "marketplace", tab });
const NOTES = (tab: string): GoToTarget => ({ view: "logbook", tab });

export const PLACES: Place[] = [
  // ── Screens ────────────────────────────────────────────────────────────────
  { id: "home", kind: "screen", title: "Home", where: "Home", detail: "Start here: what needs attention and every screen.", keywords: ["dashboard", "start", "launcher"], target: { view: "home" } },
  { id: "chat", kind: "screen", title: "Chat", where: "Chat", detail: "Ask the offline AI a question.", keywords: ["ai", "assistant", "ask", "question"], target: { view: "chat" } },
  { id: "map", kind: "screen", title: "Map", where: "Map", detail: "Offline maps, places and routes.", keywords: ["maps", "navigation", "gps"], target: { view: "map" }, minRank: 4 },
  { id: "comms", kind: "screen", title: "Comms", where: "Comms", detail: "Distress calls, radios, mesh messages, other stations.", keywords: ["communications", "radio", "hub", "distress"], target: COMMS("overview"), minRank: 2 },
  { id: "library", kind: "screen", title: "Library", where: "Library", detail: "Documents and files on this station, and the Vault.", keywords: ["archive", "documents", "files", "books", "kiwix", "zim"], target: { ...LIB, tab: "files" } },
  { id: "downloads", kind: "screen", title: "Downloads", where: "Downloads", detail: "Get AI models, maps, apps, courses and documents.", keywords: ["marketplace", "store", "data acquisition", "get", "install"], target: CATALOG("discover") },
  { id: "academy", kind: "screen", title: "Academy", where: "Academy", detail: "Courses and lessons.", keywords: ["learn", "courses", "lessons", "training"], target: { view: "academy" } },
  { id: "field-ops", kind: "screen", title: "Field Ops", where: "Field Ops", detail: "Group check-ins, roster, patients, supplies, records.", keywords: ["group", "team", "operations"], target: { view: "field_ops" } },
  { id: "notes", kind: "screen", title: "Notes & inventory", where: "Notes & inventory", detail: "Field notes, tactical notes and inventory.", keywords: ["logbook", "logistics", "field log", "stores"], target: NOTES("notes") },
  { id: "personnel", kind: "screen", title: "Personnel", where: "Personnel", detail: "People, ranks, sign-ins.", keywords: ["people", "users", "accounts", "roster", "pin"], target: { view: "personnel" }, minRank: 4 },
  { id: "org-chart", kind: "section", title: "Org chart", where: "Personnel › Org chart", detail: "Who reports to whom.", keywords: ["hierarchy", "structure", "chain of command"], target: { view: "org_chart" }, minRank: 4 },
  { id: "calendar", kind: "screen", title: "Calendar", where: "Calendar", detail: "Events and schedules.", keywords: ["schedule", "events", "dates"], target: { view: "calendar" } },

  // ── Library › Vault ────────────────────────────────────────────────────────
  { id: "vault", kind: "section", title: "Vault", where: "Library › Vault", detail: "Library coverage, packs, other stations, updates, alerts, Home Assistant.", keywords: ["vault", "coverage", "packs", "alerts", "fleet"], target: VAULT() },
  { id: "vault-coverage", kind: "section", title: "What the library covers", where: "Library › Vault", detail: "Which topics are well covered and which are thin.", keywords: ["coverage", "gaps", "topics"], target: VAULT("coverage") },
  { id: "vault-scan", kind: "section", title: "Find documents on this computer", where: "Library › Vault", detail: "Scan drives for documents and copy them into the library.", keywords: ["scan", "import", "find on this computer", "usb"], target: VAULT("scan") },
  { id: "vault-packs", kind: "section", title: "Packs", where: "Library › Vault", detail: "Bundle library content to carry to another station.", keywords: ["pack", "bundle", "export", "share"], target: VAULT("packs") },
  { id: "vault-stations", kind: "setting", title: "Other stations & fleet link", where: "Library › Vault", detail: "See what other stations hold; fleet relay address.", keywords: ["fleet", "fleet link", "relay", "relay address", "other stations", "sync"], target: VAULT("stations") },
  { id: "vault-updates", kind: "setting", title: "Updates", where: "Library › Vault", detail: "Newer releases of library content; daily update check.", keywords: ["update", "auto update", "autoupdate", "releases", "new version"], target: VAULT("updates") },
  { id: "vault-alerts", kind: "section", title: "Alerts", where: "Library › Vault", detail: "Earthquakes, disasters, weather, air traffic and news alerts; hunker-down.", keywords: ["alerts", "warnings", "hunker", "earthquake", "weather", "faa", "news"], target: VAULT("alerts") },
  { id: "vault-settings", kind: "setting", title: "Vault settings", where: "Library › Vault", detail: "Archive folder, alert region and position, airports, news feeds, watch words, auto-update.", keywords: ["archive folder", "external drive", "state", "latitude", "longitude", "position", "region", "airports", "faa", "news feeds", "rss", "watch words", "auto update", "mesh share", "auto import"], target: VAULT("settings") },
  { id: "vault-ha", kind: "setting", title: "Home Assistant", where: "Library › Vault", detail: "Address, access token, webhook to phones, sensors, devices to read, use HA location.", keywords: ["home assistant", "ha", "token", "webhook", "sensors", "entities", "devices", "location", "smart home"], target: VAULT("ha") },

  // ── Map ────────────────────────────────────────────────────────────────────
  { id: "map-layers", kind: "setting", title: "Map layers", where: "Map › Layers", detail: "Choose which map archives and layers are shown.", keywords: ["layers", "archives", "overlay", "basemap"], target: { view: "map", mapPanel: "layers" }, minRank: 4 },
  { id: "map-settings", kind: "setting", title: "Map style & satellite imagery", where: "Map › Map settings (gear)", detail: "Street, topo or satellite view; download satellite (USGS) imagery for a selected area.", keywords: ["satellite", "usgs", "imagery", "aerial", "basemap", "topo", "style", "gear"], target: { view: "map", mapPanel: "map_settings" }, minRank: 4 },
  { id: "map-directions", kind: "section", title: "Directions", where: "Map › Directions", detail: "Turn-by-turn routes offline.", keywords: ["route", "routing", "directions", "navigate", "drive", "walk"], target: { view: "map", mapPanel: "directions" }, minRank: 4 },
  { id: "map-routing-setup", kind: "setting", title: "Download maps for my area & build road data", where: "Map › Directions › Set up offline routing", detail: "Get the map and road network for your region so directions work offline.", keywords: ["download maps for my area", "build road data", "road data", "routing setup", "offline routing", "osm", "graph"], target: { view: "map", mapPanel: "routing_setup" }, minRank: 4 },

  // ── Comms ──────────────────────────────────────────────────────────────────
  { id: "comms-radio-plan", kind: "section", title: "Radio plan", where: "Comms › Radio plan", detail: "Radio silence times, channels and schedules.", keywords: ["radio silence", "channels", "frequencies", "schedule", "comms plan"], target: COMMS("radio_plan"), minRank: 2 },
  { id: "comms-radio-data", kind: "section", title: "Radio data & intercepts", where: "Comms › Radio data & intercepts", detail: "Send data over a voice radio; log intercepted traffic.", keywords: ["intercepts", "modem", "data over radio", "sdr"], target: COMMS("radio_data"), minRank: 2 },
  { id: "comms-stations", kind: "setting", title: "Other stations & courier", where: "Comms › Other stations", detail: "Group keys, courier bundles on USB, other stations.", keywords: ["courier", "group key", "usb", "bundle", "stations", "sync"], target: COMMS("stations"), minRank: 2 },
  { id: "mesh", kind: "setting", title: "Mesh network & radio", where: "Comms › Mesh network", detail: "Meshtastic USB radio, mesh messages, trusted radios.", keywords: ["meshtastic", "mesh", "lora", "radio", "usb radio", "trusted radios"], target: { view: "mesh" }, minRank: 2 },
  { id: "voice", kind: "section", title: "Voice comms", where: "Comms › Voice comms", detail: "Push-to-talk voice over a connected radio.", keywords: ["voice", "ptt", "push to talk"], target: { view: "radio" }, minRank: 2 },

  // ── Downloads ──────────────────────────────────────────────────────────────
  { id: "dl-models", kind: "section", title: "AI models", where: "Downloads › Browse catalog › Models", detail: "Pick and install AI models.", keywords: ["models", "llm", "gguf", "ollama", "ai"], target: CATALOG("models") },
  { id: "dl-maps", kind: "section", title: "Map downloads", where: "Downloads › Files & queue › Maps", detail: "Download map areas and tiles.", keywords: ["maps", "tiles", "pmtiles", "mbtiles", "area", "region"], target: FILES("maps") },
  { id: "dl-documents", kind: "section", title: "Documents & media downloads", where: "Downloads › Files & queue › Documents", detail: "Download documents, Wikipedia (Kiwix) and media.", keywords: ["documents", "wikipedia", "kiwix", "zim", "media", "pdf"], target: FILES("documents") },
  { id: "dl-packages", kind: "section", title: "Library packages", where: "Downloads › Files & queue › Packages", detail: "Preset bundles of library content.", keywords: ["packages", "presets", "bundles"], target: FILES("packages") },
  { id: "dl-courses", kind: "section", title: "Course downloads", where: "Downloads › Files & queue › Academy", detail: "Download Academy courses.", keywords: ["academy", "courses"], target: FILES("academy") },
  { id: "dl-queue", kind: "section", title: "Download queue", where: "Downloads › Files & queue › Queue", detail: "What is downloading and what is next.", keywords: ["queue", "progress", "downloading"], target: FILES("queue") },
  { id: "dl-apps", kind: "section", title: "Apps & modules", where: "Downloads › Browse catalog › Apps", detail: "Docker apps and voice modules.", keywords: ["apps", "docker", "modules", "whisper", "kokoro", "jellyfin"], target: CATALOG("apps") },

  // ── Field Ops ──────────────────────────────────────────────────────────────
  ...([
    ["checkins", "Check-ins", "People", ["check in", "safety", "overdue"]],
    ["roster", "Duty roster", "People", ["duty", "shifts", "watch"]],
    ["skills", "Skills", "People", ["skills", "training", "who can"]],
    ["intake", "Newcomers", "People", ["intake", "new people", "refugees"]],
    ["protocols", "Protocols", "Health", ["medical", "first aid", "protocol"]],
    ["patients", "Patients", "Health", ["patients", "medical records", "triage"]],
    ["supplies", "Supplies & power", "Supplies", ["supplies", "power", "fuel", "water", "food", "runway"]],
    ["ledger", "Ledger", "Supplies", ["ledger", "debts", "trades", "barter"]],
    ["memory", "Group memory", "Records", ["memory", "knowledge", "notes"]],
    ["decisions", "Decisions", "Records", ["decisions", "votes", "agreements"]],
    ["binder", "Paper binder", "Records", ["binder", "print", "paper"]],
    ["sensors", "Sensors & map reports", "Sensors", ["sensors", "readings", "geiger", "reports"]],
    ["tools", "Field tools", "Field tools", ["tools", "calculators", "reference"]],
  ] as const).map(
    ([tab, title, area, keywords]): Place => ({
      id: `fo-${tab}`,
      kind: "section",
      title,
      where: `Field Ops › ${area}`,
      detail: `${title} in Field Ops.`,
      keywords: [...keywords],
      target: FIELD_OPS(tab),
    }),
  ),

  // ── Notes & inventory ──────────────────────────────────────────────────────
  { id: "notes-field", kind: "section", title: "Field notes", where: "Notes & inventory › Field notes", detail: "Notes and the station log.", keywords: ["notes", "log", "journal"], target: NOTES("notes") },
  { id: "notes-tactical", kind: "section", title: "Tactical notes", where: "Notes & inventory › Tactical", detail: "Tactical notebook.", keywords: ["tactical", "notebook"], target: NOTES("tactical") },
  { id: "notes-inventory", kind: "section", title: "Inventory", where: "Notes & inventory › Inventory", detail: "Stores, quantities and expiry.", keywords: ["inventory", "stock", "stores", "quartermaster", "items"], target: NOTES("inventory") },
  { id: "notes-audit", kind: "section", title: "Logistics log", where: "Notes & inventory › Logistics log", detail: "Who changed the inventory and when.", keywords: ["audit", "logistics log", "history"], target: NOTES("audit") },
];

/** Places this rank can open. */
export function placesForRank(rank: number): Place[] {
  return PLACES.filter((p) => (p.minRank ?? 0) <= rank);
}
