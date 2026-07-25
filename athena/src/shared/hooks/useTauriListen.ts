import { useEffect } from "react";
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import { athenaListen } from "../ipc/athenaIpc";

type Cleanup = void | (() => void) | Promise<void> | UnlistenFn;

type ListenOptions<T> = {
  enabled?: boolean;
  handler: EventCallback<T>;
  onError?: (error: unknown) => void;
  cleanup?: Cleanup;
};

/**
 * Register a Tauri (or LAN SSE) event listener with strict unmount cleanup.
 * Handles the race where `listen(...)` resolves after unmount.
 */
export function useTauriListen<T>(
  event: string,
  { enabled = true, handler, onError, cleanup }: ListenOptions<T>,
) {
  useEffect(() => {
    if (!enabled) return;

    let cancelled = false;
    let unlisten: UnlistenFn | null = null;

    const safeCleanup = (c: Cleanup) => {
      try {
        if (typeof c === "function") {
          const out = (c as () => void | Promise<void>)();
          if (out && typeof (out as Promise<void>).then === "function") void out;
        } else if (c && typeof (c as Promise<void>).then === "function") {
          void (c as Promise<void>);
        }
      } catch (e) {
        onError?.(e);
      }
    };

    void athenaListen<T>(event, (ev: Event<T>) => handler(ev))
      .then((fn) => {
        if (cancelled) {
          void fn();
          return;
        }
        unlisten = fn;
      })
      .catch((e) => onError?.(e));

    return () => {
      cancelled = true;
      if (unlisten) void unlisten();
      safeCleanup(cleanup);
    };
  }, [event, enabled, handler, onError, cleanup]);
}
