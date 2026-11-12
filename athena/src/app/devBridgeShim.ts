/**
 * Development-only: lets a normal browser tab drive the running desktop app.
 *
 * Active only when all hold: Vite dev mode, not inside Tauri, and the URL has `?devbridge=1`.
 * Requires the desktop app to be running a debug build with `ATHENA_DEV_BRIDGE=1`
 * (see `src-tauri/src/domains/system/dev_bridge.rs`). Production builds drop this code.
 */

type Callback = (payload: unknown) => void;

const BRIDGE = "http://127.0.0.1:8799";

function install(): void {
  const w = window as unknown as Record<string, unknown>;
  const callbacks = new Map<number, Callback>();
  let nextCb = 1;
  // event name -> (eventId -> callback id)
  const listeners = new Map<string, Map<number, number>>();
  let nextEventId = 1;
  let source: EventSource | null = null;

  function ensureSource(): void {
    if (source) return;
    source = new EventSource(`${BRIDGE}/events`);
    source.onmessage = (m) => {
      let msg: { event: string; payload: unknown };
      try {
        msg = JSON.parse(m.data);
      } catch {
        return;
      }
      const subs = listeners.get(msg.event);
      if (!subs) return;
      for (const [eventId, cbId] of subs) {
        callbacks.get(cbId)?.({ event: msg.event, id: eventId, payload: msg.payload });
      }
    };
  }

  async function remoteInvoke(cmd: string, args: unknown): Promise<unknown> {
    const res = await fetch(`${BRIDGE}/invoke`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cmd, args: args ?? {} }),
    });
    const body = (await res.json()) as { ok: boolean; result?: unknown; error?: string };
    if (!body.ok) throw body.error ?? "dev bridge error";
    return body.result;
  }

  async function invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    if (cmd === "plugin:event|listen") {
      const event = String(args?.event ?? "");
      const handler = Number(args?.handler);
      ensureSource();
      await fetch(`${BRIDGE}/subscribe`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ event }),
      });
      const id = nextEventId++;
      if (!listeners.has(event)) listeners.set(event, new Map());
      listeners.get(event)!.set(id, handler);
      return id;
    }
    if (cmd === "plugin:event|unlisten") {
      listeners.get(String(args?.event ?? ""))?.delete(Number(args?.eventId));
      return null;
    }
    return remoteInvoke(cmd, args);
  }

  w.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback(cb: Callback, once = false): number {
      const id = nextCb++;
      callbacks.set(id, (p) => {
        if (once) callbacks.delete(id);
        cb(p);
      });
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc(path: string) {
      return path;
    },
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    plugins: {},
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event: string, eventId: number) {
      listeners.get(event)?.delete(eventId);
    },
  };
  console.info("[athena] dev bridge shim active");
}

if (
  import.meta.env.DEV &&
  typeof window !== "undefined" &&
  !("__TAURI_INTERNALS__" in window) &&
  new URLSearchParams(window.location.search).has("devbridge")
) {
  install();
}

export {};
