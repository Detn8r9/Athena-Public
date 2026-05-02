import type { Dispatch, SetStateAction } from "react";
import { useCallback, useRef } from "react";
import type { Event } from "@tauri-apps/api/event";
import type { UniversalHeaderArchiveExtractionState } from "../../ui/components/UniversalHeader";
import { useTauriListen } from "./useTauriListen";

/**
 * Listens for `athena:library-archive-extraction-progress` with safe cleanup and ~200ms UI throttling.
 */
export function useArchiveExtractionListen(
  setArchiveExtraction: Dispatch<SetStateAction<UniversalHeaderArchiveExtractionState | null>>,
) {
  const pendingRef = useRef<UniversalHeaderArchiveExtractionState | null>(null);
  const rafRef = useRef<number | null>(null);
  const lastUiUpdateAt = useRef(0);
  const errorClearRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const setRef = useRef(setArchiveExtraction);
  setRef.current = setArchiveExtraction;

  const flush = useCallback(() => {
    rafRef.current = null;
    const pending = pendingRef.current;
    if (!pending) return;
    pendingRef.current = null;
    lastUiUpdateAt.current = Date.now();
    setRef.current(pending);
  }, []);

  const handler = useCallback(
    (event: Event<Record<string, unknown>>) => {
      const p = event.payload;
      if (!p || typeof p !== "object") return;
      const phase = typeof p.phase === "string" ? p.phase : "";
      if (phase === "done") {
        if (errorClearRef.current) clearTimeout(errorClearRef.current);
        errorClearRef.current = undefined;
        setRef.current(null);
        return;
      }
      const archiveLabel =
        typeof p.archiveLabel === "string" && p.archiveLabel.length > 0 ? p.archiveLabel : "archive";
      const entriesProcessed = typeof p.entriesProcessed === "number" ? p.entriesProcessed : 0;
      const entriesTotal = typeof p.entriesTotal === "number" ? p.entriesTotal : null;
      const bytesWritten = typeof p.bytesWritten === "number" ? p.bytesWritten : 0;
      const message = typeof p.message === "string" ? p.message : null;
      if (phase === "error") {
        if (errorClearRef.current) clearTimeout(errorClearRef.current);
        setRef.current({
          archiveLabel,
          phase: "error",
          entriesProcessed,
          entriesTotal,
          bytesWritten,
          message,
        });
        errorClearRef.current = setTimeout(() => setRef.current(null), 10_000);
        return;
      }
      if (errorClearRef.current) {
        clearTimeout(errorClearRef.current);
        errorClearRef.current = undefined;
      }
      pendingRef.current = {
        archiveLabel,
        phase,
        entriesProcessed,
        entriesTotal,
        bytesWritten,
        message: null,
      };
      const now = Date.now();
      if (now - lastUiUpdateAt.current >= 200) {
        flush();
        return;
      }
      if (rafRef.current == null) {
        rafRef.current = window.requestAnimationFrame(flush);
      }
    },
    [flush],
  );

  const cleanup = useCallback(() => {
    if (errorClearRef.current) clearTimeout(errorClearRef.current);
    errorClearRef.current = undefined;
    if (rafRef.current != null) window.cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
  }, []);

  useTauriListen<Record<string, unknown>>("athena:library-archive-extraction-progress", {
    handler,
    cleanup,
  });
}
