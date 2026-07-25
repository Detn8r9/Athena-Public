export type AppView =
  | "home"
  | "chat"
  | "map"
  | "comms_hub"
  | "radio"
  | "mesh"
  | "logbook"
  | "calendar"
  | "academy"
  | "archive"
  | "data_acquisition"
  | "personnel"
  | "org_chart"
  | "marketplace";

const STORAGE_KEY_STARTUP_VIEW = "athena-startup-view";

export const APP_VIEWS: readonly AppView[] = [
  "home",
  "chat",
  "map",
  "comms_hub",
  "radio",
  "mesh",
  "logbook",
  "calendar",
  "academy",
  "archive",
  "data_acquisition",
  "personnel",
  "org_chart",
  "marketplace",
];

function isAppView(value: string): value is AppView {
  return (APP_VIEWS as readonly string[]).includes(value);
}

export const DEFAULT_STARTUP_VIEW: AppView = "home";

export const APP_VIEW_LABELS: Record<AppView, string> = {
  home: "Home",
  chat: "Chat",
  map: "Map",
  comms_hub: "Communications Hub",
  radio: "Voice comms",
  mesh: "Mesh network",
  logbook: "Field log & logistics",
  calendar: "Calendar",
  academy: "Academy",
  archive: "The Archive",
  data_acquisition: "Data acquisition",
  personnel: "Personnel",
  org_chart: "Org chart",
  marketplace: "Marketplace",
};

export function readStartupView(): AppView {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_STARTUP_VIEW);
    if (raw === "installer") return "archive";
    if (raw === "radio") return "comms_hub";
    if (raw === "quartermaster") return "logbook";
    if (raw && isAppView(raw)) return raw;
  } catch {
    /* ignore */
  }
  return DEFAULT_STARTUP_VIEW;
}

export function saveStartupView(view: AppView): void {
  try {
    localStorage.setItem(STORAGE_KEY_STARTUP_VIEW, view);
  } catch {
    /* ignore quota / private mode */
  }
}
