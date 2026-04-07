import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAuth } from "./auth/AuthContext";
import { ASSIGNABLE_RANKS, formatRomanRank } from "./romanRanks";

type PersonnelPublic = {
  id: string;
  name: string;
  division: string;
  rank_level: number;
  created_at: number;
  display_name: string | null;
};

function rosterLabel(p: PersonnelPublic): string {
  const d = p.display_name?.trim();
  return d && d.length > 0 ? d : p.name;
}

function comparePersonnelByRank(a: PersonnelPublic, b: PersonnelPublic): number {
  if (b.rank_level !== a.rank_level) {
    return b.rank_level - a.rank_level;
  }
  return rosterLabel(a).localeCompare(rosterLabel(b), undefined, { sensitivity: "base" });
}

export function PersonnelHub() {
  const { rankLevel, user, refreshSession } = useAuth();
  const r = rankLevel ?? 0;
  if (r < 4) {
    return (
      <div className="personnel-hub home-view-scroll">
        <p className="text-muted">You do not have access to the personnel registry.</p>
      </div>
    );
  }

  const canDeleteFromRegistry = r >= 4;

  const [rows, setRows] = useState<PersonnelPublic[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [addOpen, setAddOpen] = useState(false);
  const [name, setName] = useState("");
  const [displayNameNew, setDisplayNameNew] = useState("");
  const [division, setDivision] = useState("");
  const [rank, setRank] = useState<string>(String(ASSIGNABLE_RANKS[1]));
  const [pin, setPin] = useState("");

  const [editPerson, setEditPerson] = useState<PersonnelPublic | null>(null);
  const [editDisplayDraft, setEditDisplayDraft] = useState("");

  const sortedRows = useMemo(() => [...rows].sort(comparePersonnelByRank), [rows]);

  const load = useCallback(async () => {
    setErr(null);
    try {
      const list = await invoke<PersonnelPublic[]>("list_personnel");
      setRows(Array.isArray(list) ? list : []);
    } catch (e) {
      setErr(e != null ? String(e) : "Failed to load personnel.");
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  function closeAddModal() {
    if (busy) return;
    setAddOpen(false);
  }

  async function onCreate(e: React.FormEvent) {
    e.preventDefault();
    const rl = parseInt(rank, 10);
    if (Number.isNaN(rl) || !ASSIGNABLE_RANKS.includes(rl as (typeof ASSIGNABLE_RANKS)[number])) {
      setErr("Rank must be Level 1–4 (Caesar is exclusive).");
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const dn = displayNameNew.trim();
      await invoke("add_personnel", {
        name: name.trim(),
        division: division.trim(),
        rankLevel: rl,
        pin,
        displayName: dn.length > 0 ? dn : null,
      });
      setName("");
      setDisplayNameNew("");
      setDivision("");
      setRank(String(ASSIGNABLE_RANKS[1]));
      setPin("");
      setAddOpen(false);
      await load();
    } catch (ex) {
      setErr(ex != null ? String(ex) : "Create failed.");
    } finally {
      setBusy(false);
    }
  }

  async function saveEditDisplay(e: React.FormEvent) {
    e.preventDefault();
    if (!editPerson) return;
    const targetId = editPerson.id;
    setBusy(true);
    setErr(null);
    try {
      const v = editDisplayDraft.trim();
      await invoke("set_personnel_display_name", {
        personnelId: targetId,
        displayName: v.length > 0 ? v : null,
      });
      setEditPerson(null);
      await load();
      if (user?.id === targetId) {
        await refreshSession();
      }
    } catch (ex) {
      setErr(ex != null ? String(ex) : "Update failed.");
    } finally {
      setBusy(false);
    }
  }

  async function onRemove(p: PersonnelPublic) {
    if (!canDeleteFromRegistry || user?.id === p.id) return;
    const label = rosterLabel(p);
    if (!window.confirm(`Remove ${label} from the personnel registry? This cannot be undone.`)) {
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      await invoke("remove_personnel", { personnelId: p.id });
      if (editPerson?.id === p.id) {
        setEditPerson(null);
      }
      await load();
    } catch (ex) {
      setErr(ex != null ? String(ex) : "Remove failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="personnel-hub home-view-scroll">
      <header className="personnel-hub-top">
        <div className="personnel-hub-top-text">
          <h1 className="quartermaster-hub-title">Personnel registry</h1>
          <p className="quartermaster-hub-subtitle text-muted">
            Roster sorted by rank (Level 4 — General and above may add or remove accounts)
          </p>
        </div>
        <button
          type="button"
          className="secondary personnel-hub-add-btn"
          disabled={busy}
          onClick={() => setAddOpen(true)}
        >
          Add person
        </button>
      </header>

      {err ? (
        <p className="quartermaster-hub-err" role="alert">
          {err}
        </p>
      ) : null}

      <section className="library-section quartermaster-section glass-panel">
        <h3 className="personnel-hub-section-title">Personnel</h3>
        <div className="quartermaster-table-wrap">
          <table className="quartermaster-table">
            <thead>
              <tr>
                <th>Login name</th>
                <th>Roster display</th>
                <th>Division</th>
                <th>Rank</th>
                <th>Created</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {sortedRows.length === 0 ? (
                <tr>
                  <td colSpan={6} className="text-muted">
                    No personnel rows.
                  </td>
                </tr>
              ) : (
                sortedRows.map((p) => {
                  const isSelf = user?.id === p.id;
                  return (
                    <tr key={p.id}>
                      <td>{p.name}</td>
                      <td>{rosterLabel(p)}</td>
                      <td>{p.division}</td>
                      <td>{formatRomanRank(p.rank_level)}</td>
                      <td className="text-muted">{new Date(p.created_at).toLocaleString()}</td>
                      <td>
                        <div className="personnel-hub-actions">
                          <button
                            type="button"
                            className="secondary small-btn"
                            disabled={busy}
                            onClick={() => {
                              setEditPerson(p);
                              const existing = p.display_name?.trim() ?? "";
                              setEditDisplayDraft(
                                existing.length > 0
                                  ? existing
                                  : p.rank_level === 5
                                    ? `${p.name} — Caesar`
                                    : "",
                              );
                            }}
                          >
                            Edit display
                          </button>
                          {canDeleteFromRegistry ? (
                            <button
                              type="button"
                              className="secondary small-btn personnel-hub-delete-btn"
                              disabled={busy || isSelf}
                              title={
                                isSelf
                                  ? "You cannot remove your own account while logged in."
                                  : "Remove this account from the registry"
                              }
                              onClick={() => void onRemove(p)}
                            >
                              Delete
                            </button>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </section>

      {addOpen ? (
        <div className="modal-backdrop" onClick={() => !busy && closeAddModal()}>
          <div className="qm-modal glass-panel personnel-hub-add-modal" onClick={(e) => e.stopPropagation()}>
            <h3>Add personnel</h3>
            <p className="text-muted small" style={{ marginTop: 0 }}>
              <strong>Login name</strong> is used at the lock screen with the PIN. <strong>Roster display</strong> is
              optional—often <em>login or callsign — rank title</em> (e.g. <em>Corvus — Caesar</em> for Level 5).{" "}
              <strong>Level 5 — Caesar</strong> cannot be assigned here; only one Caesar exists until passed on.
            </p>
            <form className="personnel-form" onSubmit={onCreate}>
              <label className="lock-screen-label">
                Login name
                <input
                  className="lock-screen-input"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                  disabled={busy}
                />
              </label>
              <label className="lock-screen-label">
                Roster display (optional)
                <input
                  className="lock-screen-input"
                  value={displayNameNew}
                  onChange={(e) => setDisplayNameNew(e.target.value)}
                  disabled={busy}
                  placeholder="Corvus — Caesar"
                />
              </label>
              <label className="lock-screen-label">
                Division
                <input
                  className="lock-screen-input"
                  value={division}
                  onChange={(e) => setDivision(e.target.value)}
                  required
                  disabled={busy}
                />
              </label>
              <label className="lock-screen-label">
                Rank (new accounts)
                <select
                  className="lock-screen-input"
                  value={rank}
                  onChange={(e) => setRank(e.target.value)}
                  disabled={busy}
                >
                  {ASSIGNABLE_RANKS.map((n) => (
                    <option key={n} value={String(n)}>
                      {formatRomanRank(n)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Initial PIN
                <input
                  className="lock-screen-input"
                  type="password"
                  inputMode="numeric"
                  value={pin}
                  onChange={(e) => setPin(e.target.value)}
                  required
                  disabled={busy}
                />
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={closeAddModal} disabled={busy}>
                  Cancel
                </button>
                <button type="submit" className="secondary" disabled={busy}>
                  {busy ? "Saving…" : "Create account"}
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}

      {editPerson ? (
        <div className="modal-backdrop" onClick={() => !busy && setEditPerson(null)}>
          <div className="qm-modal glass-panel" onClick={(e) => e.stopPropagation()}>
            <h3>Roster display</h3>
            <p className="text-muted small">
              Login: <strong>{editPerson.name}</strong>
            </p>
            <form onSubmit={saveEditDisplay}>
              <label className="lock-screen-label">
                Roster display name
                <input
                  className="lock-screen-input"
                  value={editDisplayDraft}
                  onChange={(e) => setEditDisplayDraft(e.target.value)}
                  disabled={busy}
                  placeholder="e.g. Corvus — Caesar, or leave blank for login only"
                />
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={() => setEditPerson(null)} disabled={busy}>
                  Cancel
                </button>
                <button type="submit" className="secondary" disabled={busy}>
                  Save
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}
    </div>
  );
}
