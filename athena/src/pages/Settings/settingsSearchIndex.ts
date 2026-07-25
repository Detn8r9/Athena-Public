/** Stable settings panel ids used by the two-pane shell and deep search. */
export type SettingsPanelId =
  | "me"
  | "general"
  | "ai"
  | "marketplace"
  | "location"
  | "apiKeys"
  | "maps"
  | "mesh"
  | "voice"
  | "calendar"
  | "dateTime"
  | "appearance"
  | "colorsThemes"
  | "themes"
  | "troubleshooting"
  | "usbExport"
  | "developer";

export type SettingsSearchEntry = {
  id: string;
  panel: SettingsPanelId;
  title: string;
  subtitle?: string;
  keywords: string[];
  /** Sidebar group when listing top-level sections. */
  group: "account" | "system" | "tactical" | "intelligence" | "appearance" | "data";
  /** Human path crumb, e.g. "General". */
  pathLabel: string;
  /** Hide from sidebar (deep-only), e.g. developer. */
  sidebarHidden?: boolean;
  /** Host-only (USB export). */
  hostOnly?: "usbExport";
};

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
  {
    id: "me-account",
    panel: "me",
    title: "Me",
    subtitle: "Operator account, login name, PIN",
    keywords: ["me", "account", "profile", "pin", "operator", "name"],
    group: "account",
    pathLabel: "Me",
  },
  {
    id: "general-about",
    panel: "general",
    title: "About ATHENA",
    subtitle: "Product name and version",
    keywords: ["about", "athena", "version", "general"],
    group: "system",
    pathLabel: "General",
  },
  {
    id: "startup-view",
    panel: "general",
    title: "Default startup page",
    subtitle: "Screen opened on launch",
    keywords: ["startup", "default", "home", "launch", "open to"],
    group: "system",
    pathLabel: "General",
  },
  {
    id: "device-overlay",
    panel: "general",
    title: "Device stats overlay",
    subtitle: "CPU, RAM, and network telemetry",
    keywords: ["device", "overlay", "cpu", "ram", "telemetry", "stats"],
    group: "system",
    pathLabel: "General",
  },
  {
    id: "lan-access",
    panel: "general",
    title: "Allow LAN access",
    subtitle: "Serve the same UI on your Wi‑Fi",
    keywords: ["lan", "wifi", "wi-fi", "network", "browser", "8787", "remote", "phone", "tablet"],
    group: "system",
    pathLabel: "General",
  },
  {
    id: "lan-port",
    panel: "general",
    title: "LAN address",
    subtitle: "This Mac’s LAN IP and bridge port",
    keywords: ["lan", "port", "8787", "bridge", "ip", "address", "wifi"],
    group: "system",
    pathLabel: "General",
  },
  {
    id: "age-catalog",
    panel: "general",
    title: "Age & catalog access",
    subtitle: "Birthdate and unrestricted catalog",
    keywords: ["age", "birthdate", "nsfw", "catalog", "18"],
    group: "system",
    pathLabel: "General",
  },
  {
    id: "date-time",
    panel: "dateTime",
    title: "Date, Time & Calendar",
    subtitle: "Clock format and calendar region",
    keywords: ["date", "time", "clock", "timezone", "calendar"],
    group: "system",
    pathLabel: "Date & Time",
  },
  {
    id: "calendar",
    panel: "calendar",
    title: "Calendar",
    subtitle: "Events, ICS, templates",
    keywords: ["calendar", "events", "ics", "schedule"],
    group: "system",
    pathLabel: "Calendar",
  },
  {
    id: "mesh",
    panel: "mesh",
    title: "Mesh",
    subtitle: "Meshtastic USB and signup",
    keywords: ["mesh", "meshtastic", "radio", "lora", "usb"],
    group: "tactical",
    pathLabel: "Mesh",
  },
  {
    id: "mesh-ai-reply",
    panel: "mesh",
    title: "Allow /ai mesh questions",
    subtitle: "Local AI auto-reply on mesh",
    keywords: ["mesh", "ai", "auto-reply", "auto reply", "/ai"],
    group: "tactical",
    pathLabel: "Mesh",
  },
  {
    id: "location",
    panel: "location",
    title: "Location & GPS",
    subtitle: "Place-based chat context",
    keywords: ["location", "gps", "address", "map"],
    group: "tactical",
    pathLabel: "Location",
  },
  {
    id: "feature-map",
    panel: "location",
    title: "Enable location-aware answers",
    subtitle: "Use address for place-based advice",
    keywords: ["location", "map", "aware", "address", "enable"],
    group: "tactical",
    pathLabel: "Location",
  },
  {
    id: "maps",
    panel: "maps",
    title: "Offline maps",
    subtitle: "Basemaps and tile archives",
    keywords: ["maps", "offline", "tiles", "basemap", "osm", "pmtiles"],
    group: "tactical",
    pathLabel: "Offline maps",
  },
  {
    id: "ai-models",
    panel: "ai",
    title: "AI & Models",
    subtitle: "Local inference and routing",
    keywords: ["ai", "models", "ollama", "llm", "inference"],
    group: "intelligence",
    pathLabel: "AI & Models",
  },
  {
    id: "ai-connectivity",
    panel: "ai",
    title: "Connectivity",
    subtitle: "Auto / Online / Offline — NOMAD dual-mode",
    keywords: ["connectivity", "offline", "online", "network", "nomad", "cloud"],
    group: "intelligence",
    pathLabel: "AI & Models",
  },
  {
    id: "marketplace-apps",
    panel: "marketplace",
    title: "Marketplace",
    subtitle: "Open Discover (Docker apps & modules)",
    keywords: ["marketplace", "docker", "jellyfin", "portainer", "whisper", "kokoro", "modules", "store"],
    group: "intelligence",
    pathLabel: "Marketplace",
  },
  {
    id: "show-sources",
    panel: "ai",
    title: "Show library sources in chat",
    subtitle: "Citations under assistant messages",
    keywords: ["sources", "citations", "library", "from your library"],
    group: "intelligence",
    pathLabel: "AI & Models",
  },
  {
    id: "api-keys",
    panel: "apiKeys",
    title: "API keys",
    subtitle: "Satellite and external keys",
    keywords: ["api", "keys", "token", "satellite", "credentials"],
    group: "intelligence",
    pathLabel: "API keys",
  },
  {
    id: "voice",
    panel: "voice",
    title: "Voice",
    subtitle: "TTS, Conversation duplex, microphone",
    keywords: ["voice", "tts", "speech", "microphone", "whisper", "kokoro", "duplex", "barge"],
    group: "intelligence",
    pathLabel: "Voice",
  },
  {
    id: "chat-surface-default",
    panel: "voice",
    title: "Default Chat surface",
    subtitle: "Chat typing UI or Conversation face mode",
    keywords: ["chat", "conversation", "face", "yes man", "default", "voice"],
    group: "intelligence",
    pathLabel: "Voice",
  },
  {
    id: "voice-duplex-status",
    panel: "voice",
    title: "Duplex modules",
    subtitle: "Whisper STT and Kokoro TTS health",
    keywords: ["duplex", "whisper", "kokoro", "stt", "conversation", "barge"],
    group: "intelligence",
    pathLabel: "Voice",
  },
  {
    id: "tts-enable",
    panel: "voice",
    title: "Enable text-to-speech",
    subtitle: "Read replies aloud",
    keywords: ["tts", "speech", "read aloud", "voice", "kokoro"],
    group: "intelligence",
    pathLabel: "Voice",
  },
  {
    id: "appearance",
    panel: "appearance",
    title: "Window & Display",
    subtitle: "Fullscreen and night vision",
    keywords: ["window", "display", "fullscreen", "appearance"],
    group: "appearance",
    pathLabel: "Window & Display",
  },
  {
    id: "fullscreen",
    panel: "appearance",
    title: "Fullscreen",
    subtitle: "Native fullscreen window",
    keywords: ["fullscreen", "full screen", "space"],
    group: "appearance",
    pathLabel: "Window & Display",
  },
  {
    id: "colors-themes",
    panel: "colorsThemes",
    title: "Colors & Themes",
    subtitle: "Presets and Theme Studio",
    keywords: ["colors", "themes", "palette", "studio"],
    group: "appearance",
    pathLabel: "Colors & Themes",
  },
  {
    id: "usb-export",
    panel: "usbExport",
    title: "USB export",
    subtitle: "Aegis Protocol portable copy",
    keywords: ["usb", "export", "aegis", "backup", "portable", "scorched"],
    group: "data",
    pathLabel: "USB export",
    hostOnly: "usbExport",
  },
  {
    id: "troubleshooting",
    panel: "troubleshooting",
    title: "Troubleshooting",
    subtitle: "Error log and diagnostics",
    keywords: ["troubleshooting", "error", "log", "diagnostics"],
    group: "data",
    pathLabel: "Troubleshooting",
  },
  {
    id: "developer",
    panel: "developer",
    title: "Developer",
    subtitle: "Hidden developer tools",
    keywords: ["developer", "debug", "overseer"],
    group: "data",
    pathLabel: "Developer",
    sidebarHidden: true,
  },
];

/** One sidebar row per panel (first index entry that is not deep-only). */
export const SETTINGS_SIDEBAR_PANELS: {
  panel: SettingsPanelId;
  title: string;
  group: SettingsSearchEntry["group"];
  hostOnly?: "usbExport";
}[] = [
  { panel: "me", title: "Me", group: "account" },
  { panel: "general", title: "General", group: "system" },
  { panel: "dateTime", title: "Date, Time & Calendar", group: "system" },
  { panel: "calendar", title: "Calendar", group: "system" },
  { panel: "mesh", title: "Mesh", group: "tactical" },
  { panel: "location", title: "Location & GPS", group: "tactical" },
  { panel: "maps", title: "Offline maps", group: "tactical" },
  { panel: "ai", title: "AI & Models", group: "intelligence" },
  { panel: "marketplace", title: "Marketplace", group: "intelligence" },
  { panel: "apiKeys", title: "API keys", group: "intelligence" },
  { panel: "voice", title: "Voice", group: "intelligence" },
  { panel: "appearance", title: "Window & Display", group: "appearance" },
  { panel: "colorsThemes", title: "Colors & Themes", group: "appearance" },
  { panel: "usbExport", title: "USB export", group: "data", hostOnly: "usbExport" },
  { panel: "troubleshooting", title: "Troubleshooting", group: "data" },
];

export const SETTINGS_GROUP_LABELS: Record<SettingsSearchEntry["group"], string> = {
  account: "Account",
  system: "System",
  tactical: "Tactical & Hardware",
  intelligence: "Intelligence",
  appearance: "Appearance",
  data: "Data & Diagnostics",
};

export function matchSettingsSearch(
  query: string,
  entries: SettingsSearchEntry[] = SETTINGS_SEARCH_INDEX,
): SettingsSearchEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const tokens = q.split(/\s+/).filter(Boolean);
  return entries.filter((e) => {
    const hay = [e.title, e.subtitle, e.pathLabel, ...e.keywords].filter(Boolean).join(" ").toLowerCase();
    return tokens.every((t) => hay.includes(t));
  });
}
