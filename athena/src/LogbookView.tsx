import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface LogbookEntryRow {
  id: string;
  body: string;
  created_at: number;
  from_node_id: number | null;
}

interface LogbookViewProps {
  meshConnected: boolean;
}

function formatWhen(ms: number): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeStyle: "medium",
    }).format(new Date(ms));
  } catch {
    return String(ms);
  }
}

function utf8ByteLength(s: string): number {
  return new TextEncoder().encode(s).length;
}

function previewLine(body: string, maxChars: number): string {
  const oneLine = body.trim().split(/\r?\n/)[0] ?? "";
  if (oneLine.length <= maxChars) return oneLine || "(empty)";
  return `${oneLine.slice(0, maxChars - 1)}…`;
}

export function LogbookView({ meshConnected }: LogbookViewProps) {
  const [entries, setEntries] = useState<LogbookEntryRow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [maxBodyBytes, setMaxBodyBytes] = useState(160);
  const [pullingFromMesh, setPullingFromMesh] = useState(false);
  const [logbookHelpOpen, setLogbookHelpOpen] = useState(false);
  const logbookHelpId = useId();

  const selected = useMemo(
    () => (selectedId ? entries.find((e) => e.id === selectedId) ?? null : null),
    [entries, selectedId],
  );

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const list = await invoke<LogbookEntryRow[]>("logbook_list_entries");
      setEntries(Array.isArray(list) ? list : []);
    } catch (e) {
      setError(e != null ? String(e) : "Could not load logbook.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void invoke<number>("logbook_mesh_max_body_bytes")
      .then((n) => {
        if (typeof n === "number" && n > 0) setMaxBodyBytes(n);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const unlisten = listen("athena:logbook-updated", () => {
      void refresh();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  useEffect(() => {
    if (selectedId && !entries.some((e) => e.id === selectedId)) {
      setSelectedId(null);
    }
  }, [entries, selectedId]);

  const draftBytes = useMemo(() => utf8ByteLength(draft.trim()), [draft]);

  async function handleAdd() {
    const body = draft.trim();
    if (!body || saving) return;
    setSaving(true);
    setError(null);
    try {
      const row = await invoke<LogbookEntryRow>("logbook_add_entry", { body });
      setDraft("");
      await refresh();
      if (row?.id) setSelectedId(row.id);
    } catch (e) {
      setError(e != null ? String(e) : "Could not save note.");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(id: string) {
    setError(null);
    try {
      await invoke("logbook_delete_entry", { id });
      if (selectedId === id) setSelectedId(null);
      await refresh();
    } catch (e) {
      setError(e != null ? String(e) : "Could not delete entry.");
    }
  }

  async function handlePullFromMesh() {
    if (!meshConnected || pullingFromMesh) return;
    setPullingFromMesh(true);
    setError(null);
    try {
      await invoke("logbook_request_mesh_sync");
    } catch (e) {
      setError(e != null ? String(e) : "Could not request notes from the mesh.");
    } finally {
      setPullingFromMesh(false);
    }
  }

  return (
    <div className="app-chat-workspace logbook-workspace">
      <aside className="chat-sidebar logbook-sidebar" aria-label="Logbook entries">
        <div className="sidebar-brand">
          <h1 className="sidebar-title">Logbook</h1>
          <p className="sidebar-tagline text-muted">Notes on device · syncs over mesh</p>
        </div>
        <div className="sidebar-chat-actions">
          <button
            type="button"
            className="sidebar-new-chat"
            onClick={() => setSelectedId(null)}
            title="Focus new note"
          >
            <span className="sidebar-new-chat-icon" aria-hidden>
              <span className="sidebar-new-chat-icon-mask" />
            </span>
            <span className="sidebar-new-chat-label">New note</span>
          </button>
        </div>
        <div className="logbook-sidebar-list-wrap">
          {loading ? (
            <p className="logbook-sidebar-empty text-muted">Loading…</p>
          ) : entries.length === 0 ? (
            <p className="logbook-sidebar-empty text-muted">No saved entries yet.</p>
          ) : (
            <ul className="sidebar-chat-list logbook-sidebar-list">
              {entries.map((row) => (
                <li key={row.id} className="sidebar-chat-item">
                  <button
                    type="button"
                    className={`sidebar-chat-btn${selectedId === row.id ? " active" : ""}`}
                    onClick={() => setSelectedId(row.id)}
                    title={row.body}
                  >
                    <span className="sidebar-chat-title">{previewLine(row.body, 48)}</span>
                    <span className="logbook-sidebar-item-date text-muted">{formatWhen(row.created_at)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>

      <main className="chat-main glass-panel logbook-main">
        <header className="logbook-main-header">
          <div className="logbook-main-header-row">
            <h2 className="logbook-main-title">{selected ? "Entry" : "New note"}</h2>
            <div className="logbook-main-header-actions">
              <button
                type="button"
                className="logbook-pull-mesh logbook-pull-mesh--header"
                onClick={() => void handlePullFromMesh()}
                disabled={!meshConnected || pullingFromMesh}
                title={
                  !meshConnected
                    ? "Connect a Meshtastic USB radio on Primary channel"
                    : "Broadcast a sync request; other Athena devices reply with recent notes"
                }
              >
                {pullingFromMesh ? "Requesting…" : "Pull from mesh"}
              </button>
              <button
                type="button"
                className={`logbook-help-toggle${logbookHelpOpen ? " is-open" : ""}`}
                onClick={() => setLogbookHelpOpen((o) => !o)}
                aria-expanded={logbookHelpOpen}
                aria-controls={logbookHelpId}
                title="How logbook sync works on the mesh"
              >
                ?
              </button>
            </div>
          </div>
          {logbookHelpOpen ? (
            <div
              id={logbookHelpId}
              className="logbook-help-panel"
              role="region"
              aria-label="Logbook mesh sync help"
            >
              <p className="logbook-help-desc text-muted">
                Saving sends new notes on the <strong>Primary</strong> channel when a Meshtastic radio is connected.
                Use <strong>Pull from mesh</strong> to ask other Athena devices to share recent entries. Entries use the
                same id everywhere, so you won&apos;t get duplicates.
              </p>
            </div>
          ) : null}
        </header>

        {error ? (
          <p className="logbook-error" role="alert">
            {error}
          </p>
        ) : null}

        <div className="logbook-main-body">
          {selected ? (
            <article className="logbook-detail" aria-label="Selected log entry">
              <div className="logbook-detail-meta text-muted">
                <time dateTime={new Date(selected.created_at).toISOString()}>{formatWhen(selected.created_at)}</time>
                <span>
                  {selected.from_node_id != null ? ` · Mesh node ${selected.from_node_id}` : " · This device"}
                </span>
              </div>
              <div className="logbook-detail-body">{selected.body}</div>
              <div className="logbook-detail-actions">
                <button type="button" className="secondary" onClick={() => void handleDelete(selected.id)}>
                  Delete entry
                </button>
              </div>
            </article>
          ) : (
            <div className="logbook-empty-main text-muted">
              <p>Select an entry on the left, or write a new note below.</p>
            </div>
          )}
        </div>

        <div className="logbook-compose" aria-label="Compose new log entry">
          <label htmlFor="logbook-draft" className="logbook-label">
            New note
          </label>
          <textarea
            id="logbook-draft"
            className="logbook-textarea"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Weather, inventory, contacts, route changes…"
            rows={4}
            disabled={saving}
          />
          <div className="logbook-compose-meta text-muted">
            One mesh packet budget (auto-send on save): {draftBytes} / {maxBodyBytes} bytes (UTF-8)
            {!meshConnected
              ? " · Connect a Meshtastic USB radio to auto-send and pull"
              : " · Longer notes are saved locally only until shortened"}
          </div>
          <div className="logbook-compose-actions">
            <button
              type="button"
              className="logbook-save-note"
              onClick={() => void handleAdd()}
              disabled={saving || !draft.trim()}
            >
              {saving ? "Saving…" : "Save note"}
            </button>
          </div>
        </div>
      </main>
    </div>
  );
}
