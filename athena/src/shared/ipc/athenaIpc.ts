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
    return tauriListen<T>(event, handler);
  }
  const unsub = lanListen(event, (payload) => {
    handler({ event, id: 0, payload: payload as T });
  });
  return () => {
    unsub();
  };
}

export { isAthenaDesktopRuntime, isAthenaWebRuntime, canUseHostCapability } from "./athenaRuntime";
