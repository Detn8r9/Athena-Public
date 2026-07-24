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
  provider?: string;
  resolved_image?: string;
  gpu_effective?: boolean;
  host_port?: number | null;
};

export type PeersResponse = {
  scanned_at: number | null;
  peers: Array<{
    base_url: string;
    ip?: string;
    manifest?: { modules?: Array<{ id: string; status?: string }> };
  }>;
  scanning?: boolean;
};

export type GpuStatus = {
  nvidia_runtime_available: boolean;
  effective_gpu: boolean;
  override: boolean | null;
  detail: string;
};

export type OfflineMapStatus = {
  ready: boolean;
  provider: string;
  offline_maps_dir: string | null;
  basemap_present: boolean;
  tile_count: number;
  regions: string[];
  message: string;
};

export type MeshRadioStatus = {
  connected: boolean;
  provider: string;
  node_id: string | null;
  channel: string | null;
  peers_heard: number;
  last_packet_at: number | null;
  message: string;
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
  peers: () => json<PeersResponse>("/api/modules/peers"),
  scanPeers: () => json<PeersResponse>("/api/modules/peers/scan", { method: "POST" }),
  gpuStatus: () => json<GpuStatus>("/api/settings/gpu"),
  setGpuOverride: (gpu_override: boolean | null) =>
    json<GpuStatus>("/api/settings/gpu", {
      method: "PUT",
      body: JSON.stringify({ gpu_override }),
    }),
  mapStatus: () => json<OfflineMapStatus>("/api/map/status"),
  radioStatus: () => json<MeshRadioStatus>("/api/radio/status"),
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
