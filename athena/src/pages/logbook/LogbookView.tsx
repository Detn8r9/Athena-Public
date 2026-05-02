import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useLexicon } from "../../shared/lexicon";

export interface LogbookEntryRow {
  id: string;
  body: string;
  created_at: number;
  from_node_id: number | null;
  author_name: string;
  author_occupation: string;
  parent_entry_id: string | null;
  /** `general` field logbook; `tactical` restricted (no mesh). */
  notebook?: string;
}

interface LogbookAuthorProfile {
  name: string;
  occupation: string;
}

interface LogbookViewProps {
  meshConnected: boolean;
  notebookScope?: "general" | "tactical";
  onProfileSaved?: () => void;
}

type SortKey = "date" | "author" | "occupation";

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

function previewLine(body: string, maxChars: number, emptyLabel: string): string {
  const oneLine = body.trim().split(/\r?\n/)[0] ?? "";
  if (oneLine.length <= maxChars) return oneLine || emptyLabel;
  return `${oneLine.slice(0, maxChars - 1)}…`;
}

function entryNotebook(e: LogbookEntryRow): "general" | "tactical" {
  const n = (e.notebook ?? "general").trim().toLowerCase();
  return n === "tactical" ? "tactical" : "general";
}

function sortRoots(a: LogbookEntryRow, b: LogbookEntryRow, key: SortKey): number {
  if (key === "author") {
    const c = (a.author_name || "").localeCompare(b.author_name || "", undefined, { sensitivity: "base" });
    if (c !== 0) return c;
  }
  if (key === "occupation") {
    const c = (a.author_occupation || "").localeCompare(b.author_occupation || "", undefined, { sensitivity: "base" });
    if (c !== 0) return c;
  }
  return b.created_at - a.created_at;
}

export function LogbookView({ meshConnected, notebookScope = "general", onProfileSaved }: LogbookViewProps) {
  const lx = useLexicon();
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
  const [authorProfile, setAuthorProfile] = useState<LogbookAuthorProfile | null>(null);
  const [profileLoading, setProfileLoading] = useState(true);
  const [profileModalOpen, setProfileModalOpen] = useState(false);
  const [profileNameDraft, setProfileNameDraft] = useState("");
  const [profileOccDraft, setProfileOccDraft] = useState("");
  const [profileSaving, setProfileSaving] = useState(false);
  const [sortKey, setSortKey] = useState<SortKey>("date");
  const [searchQuery, setSearchQuery] = useState("");
  const [replyParentId, setReplyParentId] = useState<string | null>(null);

  const profileComplete =
    authorProfile != null &&
    authorProfile.name.trim().length > 0 &&
    authorProfile.occupation.trim().length > 0;

  const selected = useMemo(
    () => (selectedId ? entries.find((e) => e.id === selectedId) ?? null : null),
    [entries, selectedId],
  );

  const loadProfile = useCallback(async () => {
    setProfileLoading(true);
    try {
      const p = await invoke<LogbookAuthorProfile>("get_logbook_author_profile");
      setAuthorProfile(p);
      const need = !p.name.trim() || !p.occupation.trim();
      setProfileModalOpen(need);
      if (need) {
        setProfileNameDraft(p.name);
        setProfileOccDraft(p.occupation);
      }
    } catch {
      setAuthorProfile({ name: "", occupation: "" });
      setProfileModalOpen(true);
    } finally {
      setProfileLoading(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const list = await invoke<LogbookEntryRow[]>("logbook_list_entries");
      const normalized = (Array.isArray(list) ? list : []).map((e) => ({
        ...e,
        author_name: e.author_name ?? "",
        author_occupation: e.author_occupation ?? "",
        parent_entry_id: e.parent_entry_id ?? null,
        notebook: e.notebook ?? "general",
      }));
      setEntries(normalized);
    } catch (e) {
      setError(e != null ? String(e) : lx.logbookErrLoad);
    } finally {
      setLoading(false);
    }
  }, [lx]);

  useEffect(() => {
    void invoke<number>("logbook_mesh_max_body_bytes")
      .then((n) => {
        if (typeof n === "number" && n > 0) setMaxBodyBytes(n);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    void loadProfile();
  }, [loadProfile]);

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

  const filteredEntries = useMemo(() => {
    const scoped = entries.filter((e) =>
      notebookScope === "tactical" ? entryNotebook(e) === "tactical" : entryNotebook(e) !== "tactical",
    );
    const q = searchQuery.trim().toLowerCase();
    if (!q) return scoped;
    return scoped.filter((e) => e.body.toLowerCase().includes(q));
  }, [entries, searchQuery, notebookScope]);

  const sidebarRows = useMemo(() => {
    const idSet = new Set(filteredEntries.map((e) => e.id));
    const roots = filteredEntries.filter((e) => !e.parent_entry_id || !idSet.has(e.parent_entry_id));
    const sortedRoots = [...roots].sort((a, b) => sortRoots(a, b, sortKey));
    const childrenByParent = new Map<string, LogbookEntryRow[]>();
    for (const e of filteredEntries) {
      if (e.parent_entry_id) {
        const k = e.parent_entry_id;
        const arr = childrenByParent.get(k) ?? [];
        arr.push(e);
        childrenByParent.set(k, arr);
      }
    }
    for (const arr of childrenByParent.values()) {
      arr.sort((a, b) => a.created_at - b.created_at);
    }
    const out: Array<{ row: LogbookEntryRow; depth: number }> = [];
    for (const r of sortedRoots) {
      out.push({ row: r, depth: 0 });
      for (const c of childrenByParent.get(r.id) ?? []) {
        out.push({ row: c, depth: 1 });
      }
    }
    return out;
  }, [filteredEntries, sortKey]);

  const draftBytes = useMemo(() => utf8ByteLength(draft.trim()), [draft]);

  async function handleSaveProfile() {
    const n = profileNameDraft.trim();
    const o = profileOccDraft.trim();
    if (!n || !o || profileSaving) return;
    setProfileSaving(true);
    setError(null);
    try {
      await invoke("set_logbook_author_profile", { name: n, occupation: o });
      setAuthorProfile({ name: n, occupation: o });
      setProfileModalOpen(false);
      onProfileSaved?.();
    } catch (e) {
      setError(e != null ? String(e) : lx.logbookErrSaveProfile);
    } finally {
      setProfileSaving(false);
    }
  }

  async function handleAdd() {
    const body = draft.trim();
    if (!body || saving || !profileComplete) return;
    setSaving(true);
    setError(null);
    try {
      const row = await invoke<LogbookEntryRow>("logbook_add_entry", {
        body,
        parent_entry_id: replyParentId,
        notebook: notebookScope === "tactical" ? "tactical" : "general",
      });
      setDraft("");
      setReplyParentId(null);
      await refresh();
      if (row?.id) setSelectedId(row.id);
    } catch (e) {
      setError(e != null ? String(e) : lx.logbookErrSaveNote);
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(id: string) {
    setError(null);
    try {
      await invoke("logbook_delete_entry", { id });
      if (selectedId === id) setSelectedId(null);
      if (replyParentId === id) setReplyParentId(null);
      await refresh();
    } catch (e) {
      setError(e != null ? String(e) : lx.logbookErrDelete);
    }
  }

  async function handlePullFromMesh() {
    if (!meshConnected || pullingFromMesh) return;
    setPullingFromMesh(true);
    setError(null);
    try {
      await invoke("logbook_request_mesh_sync");
    } catch (e) {
      setError(e != null ? String(e) : lx.logbookErrMeshPull);
    } finally {
      setPullingFromMesh(false);
    }
  }

  const composeDisabled = !profileComplete || profileLoading || profileModalOpen;

  return (
    <div className="app-chat-workspace logbook-workspace">
      {profileModalOpen ? (
        <div className="modal-backdrop" role="presentation">
          <div
            className="library-modal glass-panel"
            role="dialog"
            aria-modal="true"
            aria-labelledby="logbook-profile-title"
            onClick={(e) => e.stopPropagation()}
            style={{ maxWidth: "22rem" }}
          >
            <div className="library-header">
              <h2 id="logbook-profile-title">{lx.logbookProfileTitle}</h2>
            </div>
            <div style={{ padding: "0 1rem 1rem", display: "grid", gap: "0.65rem" }}>
              <p className="text-muted small" style={{ margin: 0 }}>
                {lx.logbookProfileBlurb}
              </p>
              <label className="logbook-label">{lx.logbookLabelName}</label>
              <input
                className="logbook-textarea"
                style={{ minHeight: "2.25rem" }}
                value={profileNameDraft}
                onChange={(e) => setProfileNameDraft(e.target.value)}
                autoComplete="name"
              />
              <label className="logbook-label">{lx.logbookLabelOccupation}</label>
              <input
                className="logbook-textarea"
                style={{ minHeight: "2.25rem" }}
                value={profileOccDraft}
                onChange={(e) => setProfileOccDraft(e.target.value)}
                autoComplete="organization-title"
              />
              <button
                type="button"
                className="logbook-save-note"
                disabled={profileSaving || !profileNameDraft.trim() || !profileOccDraft.trim()}
                onClick={() => void handleSaveProfile()}
              >
                {profileSaving ? lx.stateSaving : lx.logbookSaveContinue}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      <aside className="chat-sidebar logbook-sidebar" aria-label={lx.logbookSidebarAria}>
        <div className="sidebar-brand">
          <h1 className="sidebar-title">
            {notebookScope === "tactical" ? lx.logbookTacticalTitle : lx.logbookTitle}
          </h1>
          <p className="sidebar-tagline text-muted">
            {notebookScope === "tactical" ? lx.logbookTacticalTagline : lx.logbookTagline}
          </p>
        </div>
        <div className="sidebar-chat-actions" style={{ flexDirection: "column", alignItems: "stretch", gap: "0.45rem" }}>
          <input
            type="search"
            className="logbook-textarea"
            style={{ minHeight: "2rem", fontSize: "0.8rem" }}
            placeholder={lx.logbookSearchPlaceholder}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            aria-label={lx.logbookSearchAria}
          />
          <label className="logbook-sort-row text-muted small">
            {lx.logbookSortLabel}
            <select
              className="logbook-sort-select"
              value={sortKey}
              onChange={(e) => setSortKey(e.target.value as SortKey)}
            >
              <option value="date">{lx.logbookSortDate}</option>
              <option value="author">{lx.logbookSortAuthor}</option>
              <option value="occupation">{lx.logbookSortOccupation}</option>
            </select>
          </label>
          <button type="button" className="sidebar-new-chat" onClick={() => setSelectedId(null)} title={lx.logbookFocusNewNote}>
            <span className="sidebar-new-chat-icon" aria-hidden>
              <span className="sidebar-new-chat-icon-mask" />
            </span>
            <span className="sidebar-new-chat-label">{lx.logbookNewNoteTitle}</span>
          </button>
        </div>
        <div className="logbook-sidebar-list-wrap">
          {loading ? (
            <p className="logbook-sidebar-empty text-muted">{lx.logbookLoading}</p>
          ) : sidebarRows.length === 0 ? (
            <p className="logbook-sidebar-empty text-muted">{lx.logbookNoMatching}</p>
          ) : (
            <ul className="sidebar-chat-list logbook-sidebar-list">
              {sidebarRows.map(({ row, depth }) => (
                <li
                  key={row.id}
                  className="sidebar-chat-item"
                  style={{ paddingLeft: depth > 0 ? `${0.65 + depth * 0.75}rem` : undefined }}
                >
                  <button
                    type="button"
                    className={`sidebar-chat-btn${selectedId === row.id ? " active" : ""}`}
                    onClick={() => setSelectedId(row.id)}
                    title={row.body}
                  >
                    <span className="sidebar-chat-title">
                      {depth > 0 ? "↳ " : ""}
                      {previewLine(row.body, 44, lx.logbookEmptyPreview)}
                    </span>
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
            <h2 className="logbook-main-title">{selected ? lx.logbookEntryHeader : lx.logbookNewNoteTitle}</h2>
            <div className="logbook-main-header-actions">
              {notebookScope === "general" ? (
                <button
                  type="button"
                  className="logbook-pull-mesh logbook-pull-mesh--header"
                  onClick={() => void handlePullFromMesh()}
                  disabled={!meshConnected || pullingFromMesh}
                  title={!meshConnected ? lx.logbookPullMeshTitleOff : lx.logbookPullMeshTitleOn}
                >
                  {pullingFromMesh ? lx.stateRequesting : lx.logbookPullMesh}
                </button>
              ) : null}
              <button
                type="button"
                className={`logbook-help-toggle${logbookHelpOpen ? " is-open" : ""}`}
                onClick={() => setLogbookHelpOpen((o) => !o)}
                aria-expanded={logbookHelpOpen}
                aria-controls={logbookHelpId}
                title={
                  notebookScope === "tactical" ? lx.logbookTacticalHelpToggleTitle : lx.logbookHelpToggleTitle
                }
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
              aria-label={notebookScope === "tactical" ? lx.logbookTacticalHelpAria : lx.logbookHelpAria}
            >
              <p className="logbook-help-desc text-muted">
                {notebookScope === "tactical" ? lx.logbookTacticalHelpBody : lx.logbookHelpBody}
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
                <div>
                  <span className="logbook-detail-author">{selected.author_name?.trim() || lx.logbookUnknownAuthor}</span>
                  {selected.author_occupation?.trim() ? (
                    <span> · {selected.author_occupation.trim()}</span>
                  ) : null}
                </div>
                <div>
                  <time dateTime={new Date(selected.created_at).toISOString()}>{formatWhen(selected.created_at)}</time>
                  <span>
                    {selected.from_node_id != null
                      ? ` · ${lx.logbookMeshNodeLabel} ${selected.from_node_id}`
                      : ` · ${lx.logbookThisDevice}`}
                  </span>
                  {selected.parent_entry_id ? <span> · {lx.logbookFollowUpTag}</span> : null}
                </div>
              </div>
              <div className="logbook-detail-body">{selected.body}</div>
              <div className="logbook-detail-actions">
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    setReplyParentId(selected.id);
                    setSelectedId(null);
                  }}
                >
                  {lx.logbookAddFollowUp}
                </button>
                <button type="button" className="secondary" onClick={() => void handleDelete(selected.id)}>
                  {lx.logbookDeleteEntry}
                </button>
              </div>
            </article>
          ) : (
            <div className="logbook-empty-main text-muted">
              <p>{lx.logbookSelectOrWrite}</p>
              {replyParentId ? <p className="text-muted small">{lx.logbookFollowUpHint}</p> : null}
            </div>
          )}
        </div>

        <div className="logbook-compose" aria-label={lx.logbookComposeAria}>
          <label htmlFor="logbook-draft" className="logbook-label">
            {lx.logbookNewNoteLabel}
          </label>
          <textarea
            id="logbook-draft"
            className="logbook-textarea"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={
              notebookScope === "tactical" ? lx.logbookTacticalDraftPlaceholder : lx.logbookDraftPlaceholder
            }
            rows={4}
            disabled={saving || composeDisabled}
          />
          {composeDisabled ? (
            <p className="text-muted small" role="status">
              {profileModalOpen ? lx.logbookProfileCompleteModal : lx.logbookProfileCompleteGeneral}
            </p>
          ) : null}
          <div className="logbook-compose-meta text-muted">
            {notebookScope === "tactical" ? (
              <span>{lx.logbookTacticalLocalOnly}</span>
            ) : (
              <>
                {lx.logbookMeshBudget} {draftBytes} / {maxBodyBytes} bytes (UTF-8)
                {!meshConnected ? lx.logbookMeshNoRadioHint : lx.logbookMeshTruncateHint}
              </>
            )}
          </div>
          <div className="logbook-compose-actions">
            {replyParentId ? (
              <button type="button" className="secondary" onClick={() => setReplyParentId(null)}>
                {lx.logbookCancelFollowUp}
              </button>
            ) : null}
            <button
              type="button"
              className="logbook-save-note"
              onClick={() => void handleAdd()}
              disabled={saving || !draft.trim() || composeDisabled}
            >
              {saving ? lx.stateSaving : lx.logbookSaveNote}
            </button>
          </div>
        </div>
      </main>
    </div>
  );
}
