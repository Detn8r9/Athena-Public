export type LayoutConfig = {
  cols: number;
  rowHeight: number;
  maxApps: number;
  rows: number;
  widgets: Array<{
    i: string;
    x: number;
    y: number;
    w: number;
    h: number;
    minW?: number;
    minH?: number;
  }>;
};

export type HealthResponse = {
  ok: boolean;
  providers: {
    cloud_configured: boolean;
    internet: boolean;
    ollama: boolean;
    comfyui: boolean;
    preferred: string;
  };
};

export type ChatResponse = {
  content: string;
  provider: "cloud" | "local" | "error";
  model: string;
  detail?: string | null;
};

export type ModuleRow = {
  id: string;
  name: string;
  category: string;
  description: string;
  size_bytes: number;
  required: boolean;
  status: string;
  progress: number;
  source?: string | null;
  error?: string | null;
  tags: string[];
};

export type CatalogApp = {
  id: string;
  name: string;
  description: string;
  image: string;
  category: string;
  container_port: number;
  preferred_host_port?: number | null;
  tags: string[];
};

export type InstalledApp = {
  id: string;
  name: string;
  image: string;
  container_name: string;
  host_port: number;
  container_port: number;
  status: string;
  pinned: boolean;
};

async function json<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers || {}),
    },
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || res.statusText);
  }
  return res.json() as Promise<T>;
}

export const api = {
  health: () => json<HealthResponse>("/api/health"),
  getLayout: () => json<LayoutConfig>("/api/layout"),
  saveLayout: (body: LayoutConfig) =>
    json<LayoutConfig>("/api/layout", { method: "PUT", body: JSON.stringify(body) }),
  chat: (messages: { role: string; content: string }[], forceLocal = false) =>
    json<ChatResponse>("/api/chat", {
      method: "POST",
      body: JSON.stringify({ messages, force_local: forceLocal }),
    }),
  modules: () => json<ModuleRow[]>("/api/modules"),
  installModule: (module_id: string, prefer_peer = true) =>
    json("/api/modules/install", {
      method: "POST",
      body: JSON.stringify({ module_id, prefer_peer }),
    }),
  appsCatalog: () => json<CatalogApp[]>("/api/apps/catalog"),
  appsInstalled: () => json<InstalledApp[]>("/api/apps/installed"),
  installApp: (id: string) => json<InstalledApp>(`/api/apps/${id}/install`, { method: "POST" }),
  startApp: (id: string) => json<InstalledApp>(`/api/apps/${id}/start`, { method: "POST" }),
  stopApp: (id: string) => json<InstalledApp>(`/api/apps/${id}/stop`, { method: "POST" }),
  pinApp: (id: string, pinned: boolean) =>
    json<InstalledApp>(`/api/apps/${id}/pin`, {
      method: "POST",
      body: JSON.stringify({ pinned }),
    }),
};
