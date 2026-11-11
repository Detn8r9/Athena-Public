import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { isAthenaDesktopRuntime } from "./athenaRuntime";
import { ensureLanPresence, lanInvoke, lanListen, setLanSessionToken } from "./lanClient";

/**
 * Unified invoke: Tauri IPC on desktop, HTTP bridge on LAN/browser.
 */
export async function athenaInvoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isAthenaDesktopRuntime()) {
    return tauriInvoke<T>(cmd, args);
  }
  ensureLanPresence();
  const result = await lanInvoke(cmd, args);
  if (cmd === "logout") {
    setLanSessionToken(null);
  }
  return result as T;
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
