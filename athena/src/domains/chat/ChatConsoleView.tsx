import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Event } from "@tauri-apps/api/event";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { open } from "@tauri-apps/plugin-dialog";
import { useLexicon } from "../../shared/lexicon";
import { useTauriListen } from "../../shared/hooks/useTauriListen";

type Role = "user" | "assistant" | "status";

type LibraryStreamSource = {
  filePath: string;
  displayTitle: string;
  pageNumber: number | null;
};

type ChatLine = {
  id: string;
  role: Role;
  content: string;
  sources?: LibraryStreamSource[];
};

type ModelManifest = {
  models: Array<{
    name: string;
    description: string;
    filename: string;
    url: string;
  }>;
};

type DownloadProgressPayload = {
  manifestType: string;
  downloadId: string;
  downloadedBytes: number;
  totalBytes: number | null;
  pct: number | null;
  bytesPerSecond?: number | null;
};

function normalizeLibraryStreamSources(raw: unknown): LibraryStreamSource[] {
  if (!Array.isArray(raw)) return [];
  const out: LibraryStreamSource[] = [];
  for (const item of raw) {
    if (item == null || typeof item !== "object") continue;
    const o = item as Record<string, unknown>;
    const filePath = typeof o.filePath === "string" ? o.filePath : "";
    const displayTitle = typeof o.displayTitle === "string" ? o.displayTitle : "";
    let pageNumber: number | null = null;
    if (typeof o.pageNumber === "number" && Number.isFinite(o.pageNumber)) {
      pageNumber = o.pageNumber;
    }
    if (!displayTitle && !filePath) continue;
    out.push({ filePath, displayTitle: displayTitle || filePath || "Source", pageNumber });
  }
  return out;
}

function clampPct(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.max(0, Math.min(100, v));
}

function formatThroughput(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec < 0) return "";
  if (bytesPerSec >= 1024 ** 2) return `${(bytesPerSec / 1024 ** 2).toFixed(1)} MB/s`;
  return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
}

function pctForProgress(p: DownloadProgressPayload | null): number | null {
  if (!p) return null;
  if (typeof p.pct === "number" && Number.isFinite(p.pct)) return clampPct(p.pct);
  if (typeof p.totalBytes === "number" && p.totalBytes > 0) {
    return clampPct((p.downloadedBytes / p.totalBytes) * 100);
  }
  return null;
}

function suggestedOllamaModelIdFromFilename(filename: string): string {
  const base = filename.trim().replace(/\.gguf$/i, "");
  return base || filename;
}

/**
 * Subscribe to package download terminal events first, then start `download_package` (which returns as soon as the
 * background task is spawned). Resolves when the file is renamed in place, or on error/cancel/timeout.
 */
async function invokeModelsPackageDownloadAndWait(
  args: {
    downloadUrl: string;
    destinationFolder: string;
    downloadId: string;
    fileName: string;
  },
  timeoutMs: number,
): Promise<{ ok: true } | { ok: false; message: string }> {
  const downloadId = args.downloadId;
  return await new Promise((resolve) => {
    let finished = false;
    const unsubscribers: Array<() => void> = [];
    let timer: number | undefined;
    const cleanup = () => {
      if (timer !== undefined) {
        window.clearTimeout(timer);
        timer = undefined;
      }
      unsubscribers.forEach((u) => u());
    };
    const done = (r: { ok: true } | { ok: false; message: string }) => {
      if (finished) return;
      finished = true;
      cleanup();
      resolve(r);
    };

    void (async () => {
      try {
        unsubscribers.push(
          await listen<{ manifestType: string; downloadId: string }>("package_download_complete", (e) => {
            const p = e.payload;
            if (!p || p.manifestType !== "models" || p.downloadId !== downloadId) return;
            done({ ok: true });
          }),
        );
        unsubscribers.push(
          await listen<{ manifestType: string; downloadId: string; error?: string }>("package_download_error", (e) => {
            const p = e.payload;
            if (!p || p.manifestType !== "models" || p.downloadId !== downloadId) return;
            done({ ok: false, message: String(p.error ?? "Download failed.") });
          }),
        );
        unsubscribers.push(
          await listen<{ manifestType: string; downloadId: string }>("package_download_cancelled", (e) => {
            const p = e.payload;
            if (!p || p.manifestType !== "models" || p.downloadId !== downloadId) return;
            done({ ok: false, message: "Download cancelled." });
          }),
        );
      } catch (e) {
        done({ ok: false, message: e != null ? String(e) : "Failed to subscribe to download events." });
        return;
      }

      timer = window.setTimeout(
        () => done({ ok: false, message: "Timed out waiting for download to finish." }),
        timeoutMs,
      );

      try {
        await invoke("download_package", {
          args: {
            manifestType: "models",
            downloadUrl: args.downloadUrl,
            destinationFolder: args.destinationFolder,
            downloadId: args.downloadId,
            fileName: args.fileName,
          },
        });
      } catch (e) {
        done({ ok: false, message: e != null ? String(e) : "Could not start download." });
      }
    })();
  });
}

export function ChatConsoleView() {
  const lx = useLexicon();
  const [modelManifest, setModelManifest] = useState<ModelManifest | null>(null);
  const [modelManifestErr, setModelManifestErr] = useState<string | null>(null);
  const [modelsDir, setModelsDir] = useState<string | null>(null);
  const [activeModel, setActiveModel] = useState<string | null>(null);
  const [modelDownloadingId, setModelDownloadingId] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, DownloadProgressPayload>>({});

  const [lines, setLines] = useState<ChatLine[]>([]);
  const [input, setInput] = useState<string>("");
  const [sending, setSending] = useState<boolean>(false);
  const [phase, setPhase] = useState<"idle" | "searching" | "generating">("idle");
  const assistantBufferRef = useRef<string>("");
  const transcriptEndRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [lines.length]);

  useEffect(() => {
    let cancelled = false;
    void invoke<ModelManifest>("get_model_manifest")
      .then((m) => {
        if (cancelled) return;
        setModelManifest(m);
        setModelManifestErr(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setModelManifest(null);
        setModelManifestErr(e != null ? String(e) : "Could not load model manifest.");
      });

    void invoke<string | null>("get_models_dir")
      .then((dir) => {
        if (cancelled) return;
        const v = typeof dir === "string" && dir.trim() ? dir.trim() : null;
        setModelsDir(v);
      })
      .catch(() => {
        if (cancelled) return;
        setModelsDir(null);
      });

    void invoke<{ explicitOllamaModel: string | null }>("get_user_and_inference_settings")
      .then((s) => {
        if (cancelled) return;
        const v = s?.explicitOllamaModel;
        setActiveModel(typeof v === "string" && v.trim() ? v.trim() : null);
      })
      .catch(() => {
        if (cancelled) return;
        setActiveModel(null);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const onConsolePhase = useCallback((e: Event<{ phase: string }>) => {
    const p = e.payload?.phase;
    if (p === "searching" || p === "generating") {
      setPhase(p);
    }
  }, []);

  useTauriListen<{ phase: string }>("athena:console-stream-phase", { handler: onConsolePhase });

  const onStreamSources = useCallback((e: Event<unknown>) => {
    const sources = normalizeLibraryStreamSources(e.payload);
    if (sources.length === 0) return;
    setLines((prev) => {
      const next = [...prev];
      for (let i = next.length - 1; i >= 0; i--) {
        if (next[i].role === "assistant") {
          next[i] = { ...next[i], sources };
          break;
        }
      }
      return next;
    });
  }, []);

  useTauriListen<unknown>("athena:stream-sources", { handler: onStreamSources });

  const onStreamChunk = useCallback((e: Event<string>) => {
    const chunk = e.payload ?? "";
    assistantBufferRef.current += chunk;
    setLines((prev) => {
      const next = [...prev];
      const last = next[next.length - 1];
      if (last && last.role === "assistant") {
        next[next.length - 1] = { ...last, content: assistantBufferRef.current };
        return next;
      }
      next.push({ id: crypto.randomUUID(), role: "assistant", content: assistantBufferRef.current });
      return next;
    });
  }, []);

  useTauriListen<string>("athena:console-stream-chunk", { handler: onStreamChunk });

  const onStreamDone = useCallback(() => {
    setSending(false);
    setPhase("idle");
  }, []);

  useTauriListen("athena:console-stream-done", { handler: onStreamDone });

  const onStreamError = useCallback((e: Event<string>) => {
    const msg = e.payload ?? "Unknown error";
    setLines((prev) => [...prev, { id: crypto.randomUUID(), role: "status", content: `Error: ${msg}` }]);
    setSending(false);
    setPhase("idle");
  }, []);

  useTauriListen<string>("athena:console-stream-error", { handler: onStreamError });

  useEffect(() => {
    const run = { cancelled: false };
    const unsubs: Array<() => void> = [];

    const register = async (p: Promise<() => void>) => {
      const u = await p;
      if (run.cancelled) {
        u();
      } else {
        unsubs.push(u);
      }
    };

    void (async () => {
      await register(
        listen<DownloadProgressPayload>("package_download_progress", (e) => {
          const p = e.payload;
          if (!p || p.manifestType !== "models" || typeof p.downloadId !== "string") return;
          setDownloadProgress((prev) => ({ ...prev, [p.downloadId]: p }));
        }),
      );
      await register(
        listen<{ manifestType: string; downloadId: string }>("package_download_complete", (e) => {
          const p = e.payload;
          if (!p || p.manifestType !== "models") return;
          if (typeof p.downloadId === "string") {
            setModelDownloadingId((cur) => (cur === p.downloadId ? null : cur));
            setDownloadProgress((prev) => {
              if (!(p.downloadId in prev)) return prev;
              const next = { ...prev };
              delete next[p.downloadId];
              return next;
            });
          }
        }),
      );
      await register(
        listen<{ manifestType: string; downloadId: string; error: string }>("package_download_error", (e) => {
          const p = e.payload;
          if (!p || p.manifestType !== "models") return;
          setLines((prev) => [...prev, { id: crypto.randomUUID(), role: "status", content: `Download error: ${p.error ?? "Unknown error"}` }]);
          if (typeof p.downloadId === "string") {
            setModelDownloadingId((cur) => (cur === p.downloadId ? null : cur));
            setDownloadProgress((prev) => {
              if (!(p.downloadId in prev)) return prev;
              const next = { ...prev };
              delete next[p.downloadId];
              return next;
            });
          }
        }),
      );
      await register(
        listen<{ manifestType: string; downloadId: string }>("package_download_cancelled", (e) => {
          const p = e.payload;
          if (!p || p.manifestType !== "models" || typeof p.downloadId !== "string") return;
          setLines((prev) => [...prev, { id: crypto.randomUUID(), role: "status", content: "Model download cancelled." }]);
          setModelDownloadingId((cur) => (cur === p.downloadId ? null : cur));
          setDownloadProgress((prev) => {
            if (!(p.downloadId in prev)) return prev;
            const next = { ...prev };
            delete next[p.downloadId];
            return next;
          });
        }),
      );
    })();

    return () => {
      run.cancelled = true;
      unsubs.forEach((u) => u());
    };
  }, []);

  const phaseLabel = useMemo(() => {
    if (phase === "searching") return "Searching local library…";
    if (phase === "generating") return "Generating…";
    return "";
  }, [phase]);

  async function ensureModelsDir(): Promise<string | null> {
    if (modelsDir) return modelsDir;
    const picked = await open({ directory: true, multiple: false, title: "Choose models folder" }).catch(() => null);
    const dir = typeof picked === "string" && picked.trim() ? picked.trim() : null;
    if (!dir) return null;
    await invoke("set_models_dir", { value: dir });
    setModelsDir(dir);
    return dir;
  }

  async function downloadModel(entry: { filename: string; url: string }) {
    if (modelDownloadingId) return;
    const dir = await ensureModelsDir();
    if (!dir) return;
    const downloadId = entry.filename;
    setModelDownloadingId(downloadId);
    try {
      const outcome = await invokeModelsPackageDownloadAndWait(
        {
          downloadUrl: entry.url,
          destinationFolder: dir,
          downloadId,
          fileName: entry.filename,
        },
        3 * 60 * 60 * 1000,
      );
      if (!outcome.ok) {
        setLines((prev) => [...prev, { id: crypto.randomUUID(), role: "status", content: outcome.message }]);
      }
    } finally {
      setModelDownloadingId(null);
    }
  }

  async function send(): Promise<void> {
    if (sending) return;
    const text = input.trim();
    if (!text) return;
    setInput("");
    setSending(true);
    setPhase("searching");
    assistantBufferRef.current = "";
    setLines((prev) => [
      ...prev,
      { id: crypto.randomUUID(), role: "user", content: text },
      { id: crypto.randomUUID(), role: "assistant", content: "" },
    ]);
    try {
      await invoke("chat_console_stream", { message: text });
    } catch (e) {
      setLines((prev) => [...prev, { id: crypto.randomUUID(), role: "status", content: e != null ? String(e) : "Could not start stream." }]);
      setSending(false);
      setPhase("idle");
    }
  }

  async function abortStream(): Promise<void> {
    try {
      await invoke("abort_chat_console_stream");
    } catch {
      /* ignore */
    }
  }

  return (
    <div
      className="glass-panel app-view-placeholder chat-console-root"
      style={{
        padding: "1rem",
        display: "flex",
        flexDirection: "column",
        gap: "1rem",
        flex: 1,
        minHeight: 0,
        boxSizing: "border-box",
      }}
    >
      <div className="chat-console-models-scroll">
        <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.75rem" }}>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.75rem", alignItems: "center", justifyContent: "space-between" }}>
            <div style={{ display: "grid", gap: "0.15rem" }}>
              <div style={{ fontWeight: 650 }}>Models</div>
              <div className="text-muted small">Download GGUF files and select the active chat model.</div>
            </div>
            <button type="button" className={modelsDir ? "secondary" : "glass-action-btn"} onClick={() => void ensureModelsDir()}>
              {modelsDir ? "Change models folder" : "Choose models folder"}
            </button>
          </div>

          <div className="text-muted small">Models folder: {modelsDir ? modelsDir : "Not set"}</div>

          <div className="glass-panel" style={{ padding: "0.75rem", borderRadius: "12px", display: "grid", gap: "0.6rem" }}>
            <label style={{ fontWeight: 650 }}>Active Ollama model</label>
            <select
              value={activeModel ?? ""}
              onChange={(e) => {
                const v = e.target.value;
                const next = v.trim() ? v.trim() : null;
                setActiveModel(next);
                void invoke("set_explicit_ollama_model", { value: next }).catch(() => {});
              }}
              className="settings-startup-view-select"
              aria-label="Active Ollama model"
            >
              <option value="">Auto (recommended tier)</option>
              {(modelManifest?.models ?? []).map((m) => {
                const id = suggestedOllamaModelIdFromFilename(m.filename);
                return (
                  <option key={m.filename} value={id}>
                    {m.name}
                  </option>
                );
              })}
            </select>
            <div className="text-muted small">
              This sets explicit_ollama_model. The selected value must match an installed Ollama model name to take effect.
            </div>
          </div>

          {modelManifestErr ? (
            <p className="error" role="alert" style={{ margin: 0 }}>
              {modelManifestErr}
            </p>
          ) : null}

          <div style={{ display: "grid", gap: "0.6rem" }}>
            {(modelManifest?.models ?? []).map((m) => {
              const id = m.filename;
              const p = downloadProgress[id] ?? null;
              const pct = pctForProgress(p);
              const downloadingThis = modelDownloadingId === id;
              const bps =
                p && typeof p.bytesPerSecond === "number" && Number.isFinite(p.bytesPerSecond) && p.bytesPerSecond > 0
                  ? p.bytesPerSecond
                  : null;
              return (
                <div key={m.filename} className="glass-panel" style={{ padding: "0.75rem", borderRadius: "12px", display: "grid", gap: "0.35rem" }}>
                  <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap", alignItems: "baseline", justifyContent: "space-between" }}>
                    <div style={{ fontWeight: 700 }}>{m.name}</div>
                    <div className="text-muted small">{m.filename}</div>
                  </div>
                  <div className="text-muted small" style={{ margin: 0 }}>
                    {m.description}
                  </div>
                  <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap", alignItems: "center" }}>
                    <button type="button" className="glass-action-btn" onClick={() => void downloadModel(m)} disabled={!!modelDownloadingId || !m.url}>
                      {downloadingThis ? "Downloading…" : "Download"}
                    </button>
                    {downloadingThis ? (
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          void invoke("cancel_package_download", {
                            args: { manifestType: "models", downloadId: id },
                          }).catch(() => {});
                        }}
                      >
                        Cancel
                      </button>
                    ) : null}
                    {pct !== null ? (
                      <div style={{ flex: 1, minWidth: "10rem", display: "grid", gap: "0.2rem" }}>
                        <div className="glass-panel" style={{ height: "6px", padding: 0, overflow: "hidden", borderRadius: "999px" }} aria-hidden>
                          <div
                            style={{
                              width: `${clampPct(pct)}%`,
                              height: "100%",
                              background: "color-mix(in srgb, var(--primary) 70%, var(--tertiary))",
                              transition: "width 180ms ease-out",
                            }}
                          />
                        </div>
                        {bps != null ? <div className="text-muted small">{formatThroughput(bps)}</div> : null}
                      </div>
                    ) : null}
                  </div>
                </div>
              );
            })}
            {modelManifest && modelManifest.models.length === 0 ? (
              <p className="text-muted" style={{ margin: 0 }}>
                No models listed in the manifest.
              </p>
            ) : null}
          </div>
        </div>
      </div>

      <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.5rem", minHeight: "18rem", flexShrink: 0 }}>
        <div className="text-muted small" aria-live="polite">
          {phaseLabel}
        </div>

        <div style={{ overflow: "auto", maxHeight: "40vh", paddingRight: "0.25rem" }}>
          <div style={{ display: "grid", gap: "0.6rem" }}>
            {lines.length === 0 ? (
              <div className="text-muted small">Ask a question to search the local library, then generate an answer.</div>
            ) : null}
            {lines.map((l) => (
              <div key={l.id} className="glass-panel" style={{ padding: "0.65rem 0.75rem", borderRadius: "12px" }}>
                <div className="text-muted small" style={{ marginBottom: "0.25rem" }}>
                  {l.role === "user" ? "You" : l.role === "assistant" ? "Athena" : "Status"}
                </div>
                <div style={{ whiteSpace: "pre-wrap", lineHeight: 1.35 }}>{l.content}</div>
                {l.role === "assistant" && (l.sources?.length ?? 0) > 0 ? (
                  <div style={{ marginTop: "0.5rem", display: "flex", flexWrap: "wrap", gap: "0.35rem", alignItems: "center" }} aria-label="Sources from your library">
                    <span className="text-muted small">Sources:</span>
                    {l.sources!.map((c, idx) =>
                      c.filePath ? (
                        <button
                          key={`${c.filePath}-${c.pageNumber ?? "x"}-${idx}`}
                          type="button"
                          className="glass-action-btn"
                          onClick={() => {
                            void invoke("open_library_source", { path: c.filePath, page: c.pageNumber }).catch(() => {});
                          }}
                          aria-label={`${lx.citationOpenFile}: ${c.displayTitle}`}
                        >
                          {c.displayTitle}
                          {c.pageNumber != null ? ` · ${lx.citationPageLabel} ${c.pageNumber}` : ""}
                        </button>
                      ) : (
                        <span key={`${c.displayTitle}-${idx}`} className="text-muted small">
                          {c.displayTitle}
                        </span>
                      ),
                    )}
                  </div>
                ) : null}
              </div>
            ))}
            <div ref={transcriptEndRef} />
          </div>
        </div>

        <div style={{ display: "flex", gap: "0.5rem", alignItems: "center", flexWrap: "wrap" }}>
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void send();
              }
            }}
            className="sidebar-search"
            placeholder="Ask Athena…"
            disabled={sending}
            aria-label="Chat input"
            style={{ flex: "1 1 12rem", minWidth: 0 }}
          />
          <button type="button" className="glass-action-btn" onClick={() => void send()} disabled={sending || !input.trim()}>
            {lx.chatSend}
          </button>
          <button type="button" className="secondary" onClick={() => void abortStream()} disabled={!sending}>
            Abort
          </button>
        </div>
      </div>
    </div>
  );
}
