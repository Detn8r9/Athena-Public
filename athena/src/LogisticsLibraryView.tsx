import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { useAuth } from "./auth/AuthContext";
import { useLexicon } from "./shared/lexicon";
import { InvNewItemPromptModal, applyInventoryProposal, type NewItemLine } from "./features/logistics/InvNewItemPromptModal";

type LogisticsSection =
  | "Inventory"
  | "Requests"
  | "Deliveries"
  | "LossDamage"
  | "Maintenance"
  | "General"
  | "Other";

type ProposalStatus = "pending" | "applied" | "rejected";

type LogisticsLibraryRow = {
  entry_id: string;
  body: string;
  created_at: number;
  author_name: string;
  author_rank_level: number | null;
  author_division: string | null;
  author_directorate?: string | null;
  author_tier?: string | null;
  author_job_title?: string | null;
  section: LogisticsSection;
  proposal_id: string | null;
  proposal_status: ProposalStatus | null;
  map_lat?: number | null;
  map_lon?: number | null;
  saved_place_id?: string | null;
  location_text?: string | null;
};

type ProposalLine = {
  id: string;
  action: string;
  division: string;
  item_name: string;
  category: string;
  quantity: number;
  confidence: number | null;
  line_location?: string | null;
};

type LineOutcome = {
  kind: string;
  lineId?: string | null;
  proposalId?: string | null;
  inventoryId?: string | null;
  delta?: number | null;
  newQty?: number | null;
  logId?: string | null;
  otherUserName?: string | null;
  priorAtMs?: number | null;
  itemName?: string | null;
  directorate?: string | null;
  message?: string | null;
};

type SubmitResult = {
  entry: LogisticsLibraryRow;
  proposalLines: ProposalLine[];
  lineOutcomes: LineOutcome[];
};

type LibraryAttach = {
  locationText: string | null;
  savedPlaceId: string | null;
  mapLat: number | null;
  mapLon: number | null;
};

type SortKey = "date" | "author" | "rank" | "section";

type SavedPlaceRow = { id: string; name: string; lat: number; lng: number };

function formatWhen(ms: number): string {
  try {
    return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" }).format(new Date(ms));
  } catch {
    return String(ms);
  }
}

export function LogisticsLibraryView({
  meshConnected,
  onProfileSaved,
}: {
  meshConnected: boolean;
  onProfileSaved?: () => void;
}) {
  const lx = useLexicon();
  const { user, rankLevel } = useAuth();
  const r = rankLevel ?? 0;

  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const [rows, setRows] = useState<LogisticsLibraryRow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("date");

  const [lastResult, setLastResult] = useState<SubmitResult | null>(null);
  const [proposalLines, setProposalLines] = useState<ProposalLine[]>([]);
  const [lineOutcomes, setLineOutcomes] = useState<LineOutcome[]>([]);
  const [newItemModal, setNewItemModal] = useState<NewItemLine[] | null>(null);

  const [locationText, setLocationText] = useState("");
  const [savedPlaceId, setSavedPlaceId] = useState<string>("");
  const [savedPlaces, setSavedPlaces] = useState<SavedPlaceRow[]>([]);

  /** Last successful submit payload (for “apply anyway” on deduped lines). */
  const lastSubmitRef = useRef<{ message: string; attached: LibraryAttach | null } | null>(null);

  const loadPlaces = useCallback(async () => {
    try {
      const list = await invoke<SavedPlaceRow[]>("list_saved_places");
      setSavedPlaces(Array.isArray(list) ? list : []);
    } catch {
      setSavedPlaces([]);
    }
  }, []);

  useEffect(() => {
    void loadPlaces();
  }, [loadPlaces]);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<LogisticsLibraryRow[]>("logistics_library_list_entries");
      setRows(Array.isArray(list) ? list : []);
    } catch {
      setRows([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    const base = !q ? rows : rows.filter((e) => e.body.toLowerCase().includes(q));
    const sorted = [...base];
    sorted.sort((a, b) => {
      if (sortKey === "author") return (a.author_name || "").localeCompare(b.author_name || "", undefined, { sensitivity: "base" });
      if (sortKey === "rank") return (b.author_rank_level ?? -1) - (a.author_rank_level ?? -1);
      if (sortKey === "section") return (a.section || "").localeCompare(b.section || "", undefined, { sensitivity: "base" });
      return b.created_at - a.created_at;
    });
    return sorted;
  }, [rows, search, sortKey]);

  const selected = useMemo(
    () => (selectedId ? (filtered.find((e) => e.entry_id === selectedId) ?? null) : null),
    [filtered, selectedId]
  );

  async function runLibrarySubmit(
    message: string,
    forceApply: boolean,
    options?: { attached: LibraryAttach | null },
  ) {
    const msg = message.trim();
    if (!msg || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const fromForm =
        locationText.trim().length > 0 || savedPlaceId.length > 0;
      const attached: LibraryAttach | null =
        options && "attached" in options
          ? options.attached
          : fromForm
            ? {
                locationText: locationText.trim() || null,
                savedPlaceId: savedPlaceId || null,
                mapLat: null,
                mapLon: null,
              }
            : null;
      if (!forceApply) {
        lastSubmitRef.current = { message: msg, attached };
      }
      const res = await invoke<SubmitResult>("logistics_library_submit_message", {
        message: msg,
        forceApply,
        attached,
      });
      setDraft("");
      setLocationText("");
      setSavedPlaceId("");
      setLastResult(res);
      setProposalLines(Array.isArray(res?.proposalLines) ? res.proposalLines : []);
      setLineOutcomes(Array.isArray(res?.lineOutcomes) ? res.lineOutcomes : []);
      onProfileSaved?.();
      await refresh();
      if (res?.entry?.entry_id) setSelectedId(res.entry.entry_id);

      const newItems: NewItemLine[] = (res.lineOutcomes || [])
        .filter((o) => o.kind === "newItemPrompt")
        .map((o) => ({
          lineId: o.lineId ?? null,
          proposalId: o.proposalId ?? res.entry.proposal_id,
          itemName: o.itemName ?? null,
          directorate: o.directorate ?? null,
          delta: o.delta ?? null,
        }));
      if (newItems.length > 0) {
        setNewItemModal(newItems);
      }
    } catch (e) {
      setErr(e != null ? String(e) : "Submit failed.");
    } finally {
      setBusy(false);
    }
  }

  async function submit() {
    await runLibrarySubmit(draft, false);
  }

  async function resubmitLastWithForceApply() {
    const p = lastSubmitRef.current;
    if (!p?.message) {
      setErr("Nothing to re-submit. Send a note first, then use Apply anyway.");
      return;
    }
    await runLibrarySubmit(p.message, true, { attached: p.attached });
  }

  async function applyProposal(proposalId: string) {
    if (!proposalId || busy) return;
    setBusy(true);
    setErr(null);
    try {
      await applyInventoryProposal(proposalId);
      await refresh();
      setProposalLines([]);
      setLineOutcomes([]);
    } catch (e) {
      setErr(e != null ? String(e) : "Apply failed.");
    } finally {
      setBusy(false);
    }
  }

  async function rejectProposal(proposalId: string) {
    if (!proposalId || busy) return;
    setBusy(true);
    setErr(null);
    try {
      await invoke("reject_inventory_proposal", { proposal_id: proposalId });
      await refresh();
      setProposalLines([]);
      setLineOutcomes([]);
    } catch (e) {
      setErr(e != null ? String(e) : "Reject failed.");
    } finally {
      setBusy(false);
    }
  }

  const composeDisabled = !user;
  const canSeeInventoryProposalActions = r >= 3;

  function authorLine(meta: LogisticsLibraryRow): string {
    const parts: string[] = [meta.author_name?.trim() || "Unknown"];
    if (meta.author_job_title?.trim()) {
      parts.push(meta.author_job_title.trim());
    } else if (meta.author_division?.trim()) {
      parts.push(meta.author_division.trim());
    }
    if (meta.author_directorate?.trim()) {
      parts.push(`[${meta.author_directorate.trim()}]`);
    }
    if (meta.author_rank_level != null) {
      parts.push(`Rank ${meta.author_rank_level}`);
    }
    return parts.join(" · ");
  }

  return (
    <div className="app-chat-workspace logbook-workspace">
      <InvNewItemPromptModal
        open={newItemModal != null && (newItemModal?.length ?? 0) > 0}
        lines={newItemModal ?? []}
        busy={busy}
        canApply={canSeeInventoryProposalActions}
        onClose={() => setNewItemModal(null)}
        onAdd={async (pid) => {
          setBusy(true);
          try {
            await applyInventoryProposal(pid);
            setNewItemModal(null);
            await refresh();
          } catch (ex) {
            setErr(ex != null ? String(ex) : "Apply failed.");
          } finally {
            setBusy(false);
          }
        }}
        onSkip={() => setNewItemModal(null)}
      />

      <aside className="chat-sidebar logbook-sidebar" aria-label="Logistics library sidebar">
        <div className="sidebar-brand">
          <h1 className="sidebar-title">Logistics Library</h1>
          <p className="sidebar-tagline text-muted">Submit notes. Propose inventory updates. Keep an auditable trail.</p>
        </div>

        <div className="sidebar-chat-actions" style={{ flexDirection: "column", alignItems: "stretch", gap: "0.45rem" }}>
          <input
            type="search"
            className="logbook-textarea"
            style={{ minHeight: "2rem", fontSize: "0.8rem" }}
            placeholder="Search notes"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            aria-label="Search logistics library entries"
          />
          <label className="logbook-sort-row text-muted small">
            Sort
            <select className="logbook-sort-select" value={sortKey} onChange={(e) => setSortKey(e.target.value as SortKey)}>
              <option value="date">Date</option>
              <option value="author">Author</option>
              <option value="rank">Rank</option>
              <option value="section">Section</option>
            </select>
          </label>
          <button type="button" className="sidebar-new-chat" onClick={() => setSelectedId(null)} title="Focus new submission">
            <span className="sidebar-new-chat-icon" aria-hidden>
              <span className="sidebar-new-chat-icon-mask" />
            </span>
            <span className="sidebar-new-chat-label">New submission</span>
          </button>
        </div>

        <div className="logbook-sidebar-list-wrap">
          {filtered.length === 0 ? (
            <p className="logbook-sidebar-empty text-muted">{rows.length === 0 ? "No submissions yet." : "No matching entries."}</p>
          ) : (
            <ul className="sidebar-chat-list logbook-sidebar-list">
              {filtered.map((row) => (
                <li key={row.entry_id} className="sidebar-chat-item">
                  <button
                    type="button"
                    className={`sidebar-chat-btn${selectedId === row.entry_id ? " active" : ""}`}
                    onClick={() => setSelectedId(row.entry_id)}
                    title={row.body}
                  >
                    <span className="sidebar-chat-title">{row.body.trim().split(/\r?\n/)[0] || "Untitled"}</span>
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
            <h2 className="logbook-main-title">{selected ? "Submission" : "New submission"}</h2>
            <div className="logbook-main-header-actions">
              <button
                type="button"
                className="logbook-pull-mesh logbook-pull-mesh--header"
                onClick={() => void refresh()}
                disabled={busy}
                title={!meshConnected ? lx.logbookPullMeshTitleOff : "Refresh list"}
              >
                Refresh
              </button>
            </div>
          </div>
        </header>

        {err ? (
          <p className="logbook-error" role="alert">
            {err}
          </p>
        ) : null}

        <div className="logbook-main-body">
          {selected ? (
            <article className="logbook-detail" aria-label="Selected logistics submission">
              <div className="logbook-detail-meta text-muted">
                <div>
                  <span className="logbook-detail-author">{authorLine(selected)}</span>
                </div>
                <div>
                  <time dateTime={new Date(selected.created_at).toISOString()}>{formatWhen(selected.created_at)}</time>
                  <span> · {selected.section}</span>
                  {selected.proposal_status ? <span> · Proposal: {selected.proposal_status}</span> : null}
                </div>
                {selected.map_lat != null && selected.map_lon != null ? (
                  <div style={{ marginTop: 6 }}>
                    <button
                      type="button"
                      className="secondary small-btn"
                      onClick={() =>
                        void emit("athena:map-focus", { lat: selected.map_lat, lng: selected.map_lon, zoom: 15 })
                      }
                    >
                      View on map
                    </button>
                    {selected.location_text?.trim() ? <span className="text-muted"> · {selected.location_text.trim()}</span> : null}
                  </div>
                ) : selected.location_text?.trim() ? (
                  <div className="text-muted small" style={{ marginTop: 4 }}>
                    Location: {selected.location_text.trim()}
                  </div>
                ) : null}
              </div>
              <div className="logbook-detail-body">{selected.body}</div>
            </article>
          ) : (
            <div className="logbook-empty-main text-muted">
              <p>Type a note or inventory change in the bar below.</p>
            </div>
          )}

          {lineOutcomes.length > 0 ? (
            <section className="library-section glass-panel" style={{ marginTop: "0.8rem" }}>
              <h3>Intake result</h3>
              <ul className="text-muted small" style={{ lineHeight: 1.7 }}>
                {lineOutcomes.map((o, i) => (
                  <li key={i}>
                    {o.kind === "autoApplied" && (
                      <>
                        Applied {o.itemName}: Δ{o.delta} → {o.newQty} in {o.directorate}
                        {o.logId && r >= 4 ? (
                          <>
                            {" "}
                            <button
                              type="button"
                              className="link-button small-btn"
                              onClick={() => {
                                void (async () => {
                                  if (!o.logId) return;
                                  setBusy(true);
                                  setErr(null);
                                  try {
                                    await invoke("rollback_inventory_change", { log_id: o.logId });
                                    setLineOutcomes((prev) => prev.filter((_, j) => j !== i));
                                    await refresh();
                                  } catch (ex) {
                                    setErr(ex != null ? String(ex) : "Rollback failed.");
                                  } finally {
                                    setBusy(false);
                                  }
                                })();
                              }}
                              title="Undo this auto-applied delta (audit log rollback)"
                            >
                              Undo
                            </button>
                          </>
                        ) : null}
                      </>
                    )}
                    {o.kind === "suppressedDuplicate" && (
                      <>
                        Suppressed duplicate of {o.otherUserName}
                        {o.priorAtMs != null
                          ? ` (${Math.max(0, (Date.now() - o.priorAtMs) / 3600000).toFixed(1)}h ago)`
                          : ""}{" "}
                        — {o.itemName} @ {o.directorate}
                        {r >= 3 ? (
                          <>
                            {" "}
                            <button
                              type="button"
                              className="link-button small-btn"
                              disabled={busy}
                              onClick={() => void resubmitLastWithForceApply()}
                              title="Re-submit the same text bypassing 24h duplicate check"
                            >
                              Apply anyway
                            </button>
                          </>
                        ) : null}
                      </>
                    )}
                    {o.kind === "pendingProposal" && <>Staged (rank &lt; 3): {o.itemName} Δ{o.delta}</>}
                    {o.kind === "newItemPrompt" && <>New item pending confirmation: {o.itemName}</>}
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          {lastResult?.entry?.proposal_id && proposalLines.length > 0 ? (
            <section className="library-section glass-panel" style={{ marginTop: "0.8rem" }}>
              <div className="library-section-head">
                <h3>AI proposal</h3>
                <div className="text-muted small">Section: {lastResult.entry.section}</div>
              </div>
              <div className="quartermaster-table-wrap">
                <table className="quartermaster-table">
                  <thead>
                    <tr>
                      <th>Action</th>
                      <th>Directorate</th>
                      <th>Item</th>
                      <th>Qty</th>
                      <th>Loc</th>
                      <th>Category</th>
                      <th>Conf.</th>
                    </tr>
                  </thead>
                  <tbody>
                    {proposalLines.map((l) => (
                      <tr key={l.id}>
                        <td>{l.action}</td>
                        <td>{l.division}</td>
                        <td>{l.item_name}</td>
                        <td>{l.quantity}</td>
                        <td className="text-muted">{l.line_location?.trim() || "—"}</td>
                        <td>{l.category}</td>
                        <td className="text-muted">{l.confidence == null ? "—" : Math.round(l.confidence * 100)}%</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.6rem", justifyContent: "flex-end" }}>
                <button
                  type="button"
                  className="secondary"
                  disabled={busy || !canSeeInventoryProposalActions}
                  title={!canSeeInventoryProposalActions ? "Insufficient rank to apply proposals" : "Reject proposal"}
                  onClick={() => void rejectProposal(lastResult.entry.proposal_id!)}
                >
                  Reject
                </button>
                <button
                  type="button"
                  className="logbook-save-note"
                  disabled={busy || !canSeeInventoryProposalActions}
                  title={!canSeeInventoryProposalActions ? "Insufficient rank to apply proposals" : "Apply proposal"}
                  onClick={() => void applyProposal(lastResult.entry.proposal_id!)}
                >
                  Apply
                </button>
              </div>
            </section>
          ) : null}
        </div>

        <div className="logbook-compose" aria-label="Logistics library composer">
          <label htmlFor="logistics-library-draft" className="logbook-label">
            Submission
          </label>
          <textarea
            id="logistics-library-draft"
            className="logbook-textarea"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Example: Found 5 bottles of water near Fairlee lake. Used 2 bandages at the clinic."
            rows={3}
            disabled={busy || composeDisabled}
          />
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", marginTop: 6 }}>
            <span className="text-muted small">Attach context:</span>
            <input
              className="logbook-textarea"
              style={{ minHeight: "2rem", flex: "1 1 160px" }}
              placeholder="Location (e.g. Fairlee)"
              value={locationText}
              onChange={(e) => setLocationText(e.target.value)}
              disabled={busy || composeDisabled}
            />
            <label className="text-muted small">
              Saved place
              <select
                className="logbook-sort-select"
                value={savedPlaceId}
                onChange={(e) => setSavedPlaceId(e.target.value)}
                style={{ marginLeft: 6 }}
                disabled={busy || composeDisabled}
              >
                <option value="">—</option>
                {savedPlaces.map((sp) => (
                  <option key={sp.id} value={sp.id}>
                    {sp.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {composeDisabled ? (
            <p className="text-muted small" role="status">
              Sign in to submit logistics notes.
            </p>
          ) : null}
          <div className="logbook-compose-actions">
            <button
              type="button"
              className="logbook-save-note"
              onClick={() => void submit()}
              disabled={busy || !draft.trim() || composeDisabled}
            >
              {busy ? lx.stateSaving : "Submit"}
            </button>
          </div>
        </div>
      </main>
    </div>
  );
}
