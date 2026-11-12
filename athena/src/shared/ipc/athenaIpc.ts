import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { emit as tauriEmit, listen as tauriListen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { isAthenaDesktopRuntime } from "./athenaRuntime";
import { ensureLanPresence, lanEmitLocal, lanInvoke, lanListen, setLanSessionToken } from "./lanClient";

/**
 * Tauri 2 matches command arguments by their camelCase name (`item_id` in Rust is `itemId` in
 * JS) unless the command opts into snake_case. Many call sites pass snake_case keys, which made
 * those commands fail with "missing required key". Send both spellings for every top-level key;
 * Tauri ignores the one a command does not use. Nested objects (structs) are left alone because
 * serde decides their field names.
 */
export function withCamelCaseArgs(args?: Record<string, unknown>): Record<string, unknown> | undefined {
  if (!args) return args;
  let out: Record<string, unknown> | null = null;
  for (const key of Object.keys(args)) {
    if (!key.includes("_")) continue;
    const camel = key.replace(/_([a-z0-9])/g, (_m, c: string) => c.toUpperCase());
    if (camel === key || camel in args) continue;
    out ??= { ...args };
    out[camel] = args[key];
  }
  return out ?? args;
}

/**
 * Unified invoke: Tauri IPC on desktop, HTTP bridge on LAN/browser.
 */
export async function athenaInvoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    if (isAthenaDesktopRuntime()) {
      return await tauriInvoke<T>(cmd, withCamelCaseArgs(args));
    }
    ensureLanPresence();
    const result = await lanInvoke(cmd, args);
    if (cmd === "logout") {
      setLanSessionToken(null);
    }
    return result as T;
  } catch (e) {
    if (sessionActive && isSessionExpiredError(e)) {
      sessionActive = false;
      window.dispatchEvent(new CustomEvent(SESSION_EXPIRED_EVENT));
      throw new Error("Your session ended. Sign in again to continue.");
    }
    throw e;
  }
}

let sessionActive = false;

/** The auth layer says whether someone is signed in; only then is "not authenticated" an expiry. */
export function setSessionActive(active: boolean): void {
  sessionActive = active;
}

/** Fired on `window` when any command reports that nobody is signed in any more. */
export const SESSION_EXPIRED_EVENT = "athena:session-expired";

/** The backend answers "Not authenticated." when the session is gone (timeout, restart, lock). */
export function isSessionExpiredError(e: unknown): boolean {
  const text = typeof e === "string" ? e : e instanceof Error ? e.message : "";
  return /^not authenticated\.?$/i.test(text.trim());
}

/** Front-end event between screens: a Tauri event on desktop, local delivery in a browser. */
export async function athenaEmit(event: string, payload?: unknown): Promise<void> {
  if (isAthenaDesktopRuntime()) {
    await tauriEmit(event, payload);
    return;
  }
  lanEmitLocal(event, payload);
}

/**
 * Unified event listen: Tauri on desktop, SSE on LAN/browser.
 */
export async function athenaListen<T>(
  event: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  if (isAthenaDesktopRuntime()) {
    return safeUnlisten(await tauriListen<T>(event, handler));
  }
  const unsub = lanListen(event, (payload) => {
    handler({ event, id: 0, payload: payload as T });
  });
  return safeUnlisten(() => {
    unsub();
  });
}

/**
 * Make an unlisten function safe to call more than once and after a webview reload.
 *
 * Tauri's own unlisten throws `undefined is not an object (evaluating
 * 'listeners[eventId].handlerId')` when the listener is already gone, which happens when React
 * runs an effect cleanup twice (StrictMode, fast unmount/remount) or the page reloads before
 * cleanup. Because the call sites `void` the result, that surfaced as an unhandled rejection on
 * every navigation and flooded the error log.
 */
export function safeUnlisten(unlisten: UnlistenFn): UnlistenFn {
  let done = false;
  return () => {
    if (done) return;
    done = true;
    try {
      const out = unlisten() as unknown;
      if (out && typeof (out as Promise<unknown>).catch === "function") {
        (out as Promise<unknown>).catch(() => {});
      }
    } catch {
      /* listener was already removed */
    }
  };
}

export { isAthenaDesktopRuntime, isAthenaWebRuntime, canUseHostCapability } from "./athenaRuntime";
