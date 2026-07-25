const TOKEN_KEY = "athena_lan_session_token";
const INVOKE_TIMEOUT_MS = 20_000;

/** Base URL for the LAN bridge API (empty = same origin, e.g. served from :8787). */
export function lanApiBase(): string {
  const fromEnv = (import.meta as ImportMeta & { env?: Record<string, string> }).env?.VITE_ATHENA_LAN_API;
  if (fromEnv && fromEnv.trim()) return fromEnv.replace(/\/$/, "");
  if (typeof window !== "undefined") {
    const { protocol, hostname, port } = window.location;
    if (port === "1420" || port === "1421") {
      return `${protocol}//${hostname}:8787`;
    }
  }
  return "";
}

export function getLanSessionToken(): string | null {
  try {
    return sessionStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setLanSessionToken(token: string | null): void {
  try {
    if (token) sessionStorage.setItem(TOKEN_KEY, token);
    else sessionStorage.removeItem(TOKEN_KEY);
  } catch {
    /* ignore */
  }
}

export type LanInvokeResponse = {
  ok: boolean;
  result?: unknown;
  error?: string;
  token?: string;
};

export async function lanInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  const base = lanApiBase();
  const url = `${base}/api/invoke`;
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  const token = getLanSessionToken();
  if (token) headers.Authorization = `Bearer ${token}`;

  const controller = new AbortController();
  const timer = window.setTimeout(() => controller.abort(), INVOKE_TIMEOUT_MS);
  let res: Response;
  try {
    res = await fetch(url, {
      method: "POST",
      headers,
      body: JSON.stringify({ cmd, args: args ?? {} }),
      signal: controller.signal,
    });
  } catch (err) {
    if (err instanceof DOMException && err.name === "AbortError") {
      throw new Error(
        `Request timed out talking to the host (${cmd}). Is Allow LAN access still enabled?`,
      );
    }
    throw err instanceof Error ? err : new Error(String(err));
  } finally {
    window.clearTimeout(timer);
  }

  const body = (await res.json().catch(() => ({}))) as LanInvokeResponse;
  if (body.token) setLanSessionToken(body.token);
  if (!res.ok || !body.ok) {
    throw new Error(body.error || `LAN invoke failed (${res.status})`);
  }
  return body.result;
}

export type LanEventMessage = { event: string; payload: unknown };

type Handler = (payload: unknown) => void;

/** One shared SSE per browser tab so the host counts each device once. */
let sharedEs: EventSource | null = null;
const handlersByEvent = new Map<string, Set<Handler>>();

function ensureSharedEventSource(): void {
  if (sharedEs) return;
  const base = lanApiBase();
  const token = getLanSessionToken();
  const qs = new URLSearchParams();
  if (token) qs.set("token", token);
  const url = `${base}/api/events?${qs.toString()}`;
  const es = new EventSource(url);
  const onMessage = (ev: MessageEvent) => {
    try {
      const data = JSON.parse(String(ev.data)) as LanEventMessage;
      const eventName = data.event || "";
      const fanout = (name: string) => {
        const set = handlersByEvent.get(name);
        if (!set) return;
        for (const h of set) h(data.payload);
      };
      fanout(eventName);
      fanout("*");
    } catch {
      /* ignore malformed */
    }
  };
  es.addEventListener("athena", onMessage);
  es.onmessage = onMessage;
  sharedEs = es;
}

function maybeCloseSharedEventSource(): void {
  if (!sharedEs) return;
  let any = false;
  for (const set of handlersByEvent.values()) {
    if (set.size > 0) {
      any = true;
      break;
    }
  }
  if (!any) {
    sharedEs.close();
    sharedEs = null;
  }
}

/**
 * Subscribe to LAN bridge events (SSE). Returns an unsubscribe function.
 */
export function lanListen(event: string, handler: Handler): () => void {
  ensureSharedEventSource();
  let set = handlersByEvent.get(event);
  if (!set) {
    set = new Set();
    handlersByEvent.set(event, set);
  }
  set.add(handler);
  return () => {
    set!.delete(handler);
    if (set!.size === 0) handlersByEvent.delete(event);
    maybeCloseSharedEventSource();
  };
}

/** Register this tab as connected (shared SSE). Safe to call often. */
export function ensureLanPresence(): void {
  if (typeof window === "undefined") return;
  ensureSharedEventSource();
  if (!handlersByEvent.has("lan:presence")) {
    handlersByEvent.set("lan:presence", new Set([() => undefined]));
  }
}
