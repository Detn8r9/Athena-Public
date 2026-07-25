import React, { useCallback, useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { useAuth } from "../../domains/auth/AuthContext";
import { ASSIGNABLE_RANKS, formatRomanRank } from "../../shared/lib/romanRanks";
import {
  LEVEL1_ROLES,
  LEVEL2_SECTOR_LEADERS,
  LEVEL3_DIRECTORATES,
  LEVEL4_EXECUTIVE,
  PERSONNEL_DOMAINS,
  type PersonnelDomain,
  suggestHierarchyForLevel1,
} from "../../shared/personnelHierarchy";

type RoleLevelKey = "l1" | "l2" | "l3" | "l4";

type RoleCatalogEntry = {
  id: string;
  domain: PersonnelDomain;
  level: RoleLevelKey;
  title: string;
};

type PersonnelPublic = {
  id: string;
  name: string;
  division: string;
  rank_level: number;
  created_at: number;
  display_name: string | null;
  operational_status: "active" | "on_mission" | "mia" | "kia";
  domain?: string | null;
  role_l1?: string | null;
  role_l2?: string | null;
  role_l3?: string | null;
  role_l4?: string | null;
  /** Designated ATHENA architect — not implied by Level V alone. */
  athena_architect?: boolean;
};

function normalizeOperationalStatus(v: string): PersonnelPublic["operational_status"] {
  const t = v.trim().toLowerCase();
  if (t === "on_mission") return "on_mission";
  if (t === "mia") return "mia";
  if (t === "kia") return "kia";
  return "active";
}

function statusLabel(v: PersonnelPublic["operational_status"]): string {
  if (v === "on_mission") return "On mission";
  if (v === "mia") return "MIA";
  if (v === "kia") return "KIA";
  return "Active";
}

function rosterLabel(p: PersonnelPublic): string {
  const d = p.display_name?.trim();
  return d && d.length > 0 ? d : p.name;
}

function primaryRoleForRow(p: PersonnelPublic): string {
  if (p.rank_level >= 5) return "Caesar";
  if (p.rank_level === 4) return (p.role_l4?.trim() || "Executive") as string;
  if (p.rank_level === 3) return (p.role_l3?.trim() || "Directorate") as string;
  if (p.rank_level === 2) return (p.role_l2?.trim() || "Sector leader") as string;
  return (p.role_l1?.trim() || "Role") as string;
}

function comparePersonnelByRank(a: PersonnelPublic, b: PersonnelPublic): number {
  if (b.rank_level !== a.rank_level) {
    return b.rank_level - a.rank_level;
  }
  return rosterLabel(a).localeCompare(rosterLabel(b), undefined, { sensitivity: "base" });
}

export function PersonnelHub({
  onOpenOrgChart,
  onOpenProfile,
}: {
  onOpenOrgChart?: () => void;
  onOpenProfile?: (personnelId: string) => void;
} = {}) {
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
  const canEditTarget = useCallback(
    (target: PersonnelPublic): boolean => {
      if (r < 4) return false;
      if (r === 4 && target.rank_level >= 5) return false;
      return true;
    },
    [r],
  );
  const canDeleteTarget = useCallback(
    (target: PersonnelPublic): boolean => {
      if (!canDeleteFromRegistry) return false;
      if (r === 4 && target.rank_level >= 5) return false;
      if (target.rank_level >= 5) return r >= 5;
      return true;
    },
    [canDeleteFromRegistry, r],
  );

  const [rows, setRows] = useState<PersonnelPublic[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [addOpen, setAddOpen] = useState(false);
  const [name, setName] = useState("");
  const [displayNameNew, setDisplayNameNew] = useState("");
  const [rank, setRank] = useState<string>("1");
  const [pin, setPin] = useState("");
  const [addDomain, setAddDomain] = useState<PersonnelDomain>(PERSONNEL_DOMAINS[0]!);
  const [addRoleL1, setAddRoleL1] = useState("");
  const [addRoleL2, setAddRoleL2] = useState("");
  const [addRoleL3, setAddRoleL3] = useState("");
  const [addRoleL4, setAddRoleL4] = useState("");

  const [editPerson, setEditPerson] = useState<PersonnelPublic | null>(null);
  const [editLoginDraft, setEditLoginDraft] = useState("");
  const [editDisplayDraft, setEditDisplayDraft] = useState("");
  const [editRankDraft, setEditRankDraft] = useState("");
  const [editStatusDraft, setEditStatusDraft] = useState<PersonnelPublic["operational_status"]>("active");
  const [editDomain, setEditDomain] = useState<PersonnelDomain>(PERSONNEL_DOMAINS[0]!);
  const [editRoleL1, setEditRoleL1] = useState("");
  const [editRoleL2, setEditRoleL2] = useState("");
  const [editRoleL3, setEditRoleL3] = useState("");
  const [editRoleL4, setEditRoleL4] = useState("");

  const [customRoleEntries, setCustomRoleEntries] = useState<RoleCatalogEntry[]>([]);
  const [newJobOpen, setNewJobOpen] = useState(false);
  const [newJobTitle, setNewJobTitle] = useState("");
  const [newJobLevel, setNewJobLevel] = useState<RoleLevelKey>("l1");
  const [newJobDomain, setNewJobDomain] = useState<PersonnelDomain>(PERSONNEL_DOMAINS[0]!);
  const [newJobContext, setNewJobContext] = useState<"add" | "edit">("add");

  const addRoleSuggestions = useMemo(() => {
    const rl = parseInt(rank, 10);
    if (rl === 4) return LEVEL4_EXECUTIVE;
    if (rl === 3) return LEVEL3_DIRECTORATES;
    if (rl === 2) return LEVEL2_SECTOR_LEADERS[addDomain] ?? [];
    return LEVEL1_ROLES[addDomain] ?? [];
  }, [rank, addDomain]);

  const editRoleSuggestions = useMemo(() => {
    const rl = parseInt(editRankDraft, 10);
    if (rl === 4) return LEVEL4_EXECUTIVE;
    if (rl === 3) return LEVEL3_DIRECTORATES;
    if (rl === 2) return LEVEL2_SECTOR_LEADERS[editDomain] ?? [];
    return LEVEL1_ROLES[editDomain] ?? [];
  }, [editRankDraft, editDomain]);

  const addRoleOptions = useMemo(() => {
    const rl = parseInt(rank, 10);
    const lvl: RoleLevelKey = rl === 4 ? "l4" : rl === 3 ? "l3" : rl === 2 ? "l2" : "l1";
    const suggested = addRoleSuggestions;
    const custom = customRoleEntries
      .filter((e) => e.domain === addDomain && e.level === lvl)
      .map((e) => e.title);
    const all = [...suggested, ...custom]
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    return Array.from(new Set(all)).sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
  }, [rank, addDomain, addRoleSuggestions, customRoleEntries]);

  const editRoleOptions = useMemo(() => {
    const rl = parseInt(editRankDraft, 10);
    const lvl: RoleLevelKey = rl === 4 ? "l4" : rl === 3 ? "l3" : rl === 2 ? "l2" : "l1";
    const suggested = editRoleSuggestions;
    const custom = customRoleEntries
      .filter((e) => e.domain === editDomain && e.level === lvl)
      .map((e) => e.title);
    const all = [...suggested, ...custom]
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    return Array.from(new Set(all)).sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
  }, [editRankDraft, editDomain, editRoleSuggestions, customRoleEntries]);

  const loadRoleCatalog = useCallback(async () => {
    try {
      const rows = await invoke<unknown>("list_role_catalog");
      const arr = Array.isArray(rows) ? (rows as any[]) : [];
      setCustomRoleEntries(
        arr
          .map((r) => ({
            id: String(r.id ?? ""),
            domain: String(r.domain ?? PERSONNEL_DOMAINS[0]) as PersonnelDomain,
            level: String(r.level ?? "l1") as RoleLevelKey,
            title: String(r.title ?? ""),
          }))
          .filter((r) => r.id && r.title.trim() && (r.level === "l1" || r.level === "l2" || r.level === "l3" || r.level === "l4")),
      );
    } catch {
      setCustomRoleEntries([]);
    }
  }, []);

  useEffect(() => {
    void loadRoleCatalog();
  }, [loadRoleCatalog]);

  const sortedRows = useMemo(() => [...rows].sort(comparePersonnelByRank), [rows]);

  useEffect(() => {
    // Only auto-suggest the *chain* when creating a Level 1 role (superiors).
    if (parseInt(rank, 10) !== 1) return;
    if (addRoleL2.trim() || addRoleL3.trim() || addRoleL4.trim()) return;
    const s = suggestHierarchyForLevel1(addDomain);
    setAddRoleL2(s.role_l2);
    setAddRoleL3(s.role_l3);
    setAddRoleL4(s.role_l4);
  }, [rank, addDomain, addRoleL2, addRoleL3, addRoleL4]);

  useEffect(() => {
    if (!editPerson) return;
    // Only auto-suggest the *chain* when editing a Level 1 role (superiors).
    if (parseInt(editRankDraft, 10) !== 1) return;
    if (editRoleL2.trim() || editRoleL3.trim() || editRoleL4.trim()) return;
    const s = suggestHierarchyForLevel1(editDomain);
    setEditRoleL2(s.role_l2);
    setEditRoleL3(s.role_l3);
    setEditRoleL4(s.role_l4);
  }, [editPerson, editRankDraft, editDomain, editRoleL2, editRoleL3, editRoleL4]);

  function openNewJob(context: "add" | "edit") {
    const rl = context === "add" ? parseInt(rank, 10) : parseInt(editRankDraft, 10);
    const level: RoleLevelKey = rl === 4 ? "l4" : rl === 3 ? "l3" : rl === 2 ? "l2" : "l1";
    setNewJobContext(context);
    setNewJobLevel(level);
    setNewJobDomain(context === "add" ? addDomain : editDomain);
    setNewJobTitle("");
    setNewJobOpen(true);
  }

  async function saveNewJob() {
    const title = newJobTitle.trim();
    if (!title) return;
    try {
      await invoke("add_role_catalog_entry", {
        domain: newJobDomain,
        level: newJobLevel,
        title,
      });
      await loadRoleCatalog();
    } catch (e) {
      setErr(e != null ? String(e) : "Failed to add job title.");
      return;
    }

    // Set the selected role immediately.
    if (newJobContext === "add") {
      const rl = parseInt(rank, 10);
      if (rl === 4) setAddRoleL4(title);
      else if (rl === 3) setAddRoleL3(title);
      else if (rl === 2) setAddRoleL2(title);
      else setAddRoleL1(title);
    } else {
      const rl = parseInt(editRankDraft, 10);
      if (rl === 4) setEditRoleL4(title);
      else if (rl === 3) setEditRoleL3(title);
      else if (rl === 2) setEditRoleL2(title);
      else setEditRoleL1(title);
    }

    setNewJobOpen(false);
  }

  const load = useCallback(async () => {
    setErr(null);
    try {
      const list = await invoke<unknown>("list_personnel");
      const rows = Array.isArray(list) ? (list as any[]) : [];
      setRows(
        rows.map((p) => ({
          id: String(p.id ?? ""),
          name: String(p.name ?? ""),
          division: String(p.division ?? ""),
          rank_level: Number.isFinite(p.rank_level) ? Number(p.rank_level) : Number(p.rankLevel ?? 1),
          created_at: Number(p.created_at ?? p.createdAt ?? Date.now()),
          display_name: (p.display_name ?? p.displayName ?? null) as string | null,
          operational_status: normalizeOperationalStatus(String(p.operational_status ?? p.operationalStatus ?? "active")),
          domain: (p.domain ?? null) as string | null,
          role_l1: (p.role_l1 ?? p.roleL1 ?? null) as string | null,
          role_l2: (p.role_l2 ?? p.roleL2 ?? null) as string | null,
          role_l3: (p.role_l3 ?? p.roleL3 ?? null) as string | null,
          role_l4: (p.role_l4 ?? p.roleL4 ?? null) as string | null,
          athena_architect: Boolean(p.athena_architect ?? p.athenaArchitect ?? false),
        })),
      );
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
      const divStr = addDomain;
      const s = suggestHierarchyForLevel1(addDomain);
      const roleL2 = addRoleL2.trim() || s.role_l2;
      const roleL3 = addRoleL3.trim() || s.role_l3;
      const roleL4 = addRoleL4.trim() || s.role_l4;
      await invoke("add_personnel", {
        name: name.trim(),
        division: divStr,
        rankLevel: rl,
        pin,
        displayName: dn.length > 0 ? dn : null,
        domain: addDomain,
        roleL1: rl === 1 ? (addRoleL1.trim() || null) : null,
        roleL2: rl <= 2 ? (roleL2 || null) : null,
        roleL3: rl <= 3 ? (roleL3 || null) : null,
        roleL4: rl <= 4 ? (roleL4 || null) : null,
      });
      setName("");
      setDisplayNameNew("");
      setRank("1");
      setPin("");
      setAddRoleL1("");
      setAddRoleL2("");
      setAddRoleL3("");
      setAddRoleL4("");
      setAddOpen(false);
      await load();
    } catch (ex) {
      setErr(ex != null ? String(ex) : "Create failed.");
    } finally {
      setBusy(false);
    }
  }

  async function saveEditPersonnel(e: React.FormEvent) {
    e.preventDefault();
    if (!editPerson) return;
    const targetId = editPerson.id;
    setBusy(true);
    setErr(null);
    try {
      const loginName = editLoginDraft.trim();
      const div = editDomain;
      const rl = parseInt(editRankDraft, 10);
      if (!loginName) {
        setErr("Login name is required.");
        return;
      }
      if (!Number.isFinite(rl)) {
        setErr("Rank level is required.");
        return;
      }
      const disp = editDisplayDraft.trim();
      const s = suggestHierarchyForLevel1(editDomain);
      const roleL2 = editRoleL2.trim() || s.role_l2;
      const roleL3 = editRoleL3.trim() || s.role_l3;
      const roleL4 = editRoleL4.trim() || s.role_l4;
      await invoke("admin_update_personnel", {
        personnelId: targetId,
        loginName,
        displayName: disp.length > 0 ? disp : null,
        division: div,
        rankLevel: rl,
        operationalStatus: editStatusDraft,
        domain: editDomain,
        roleL1: rl === 1 ? (editRoleL1.trim() || null) : null,
        roleL2: rl <= 2 ? (roleL2 || null) : null,
        roleL3: rl <= 3 ? (roleL3 || null) : null,
        roleL4: rl <= 4 ? (roleL4 || null) : null,
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
      {newJobOpen ? (
        <div className="modal-backdrop" onClick={() => !busy && setNewJobOpen(false)}>
          <div className="qm-modal glass-panel personnel-hub-add-modal" onClick={(e) => e.stopPropagation()}>
            <h3>New job title</h3>
            <p className="text-muted small" style={{ marginTop: 0 }}>
              Adds a role to the selectable list. Defaults to the currently selected domain.
            </p>
            <div className="personnel-form" style={{ display: "grid", gap: "0.75rem" }}>
              <label className="lock-screen-label">
                Domain (field)
                <select
                  className="lock-screen-input"
                  value={newJobDomain}
                  onChange={(e) => setNewJobDomain(e.target.value as PersonnelDomain)}
                  disabled={busy}
                >
                  {PERSONNEL_DOMAINS.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Level
                <select
                  className="lock-screen-input"
                  value={newJobLevel}
                  onChange={(e) => setNewJobLevel(e.target.value as "l1" | "l2" | "l3" | "l4")}
                  disabled={busy}
                >
                  <option value="l1">Level 1 — Role</option>
                  <option value="l2">Level 2 — Sector leader</option>
                  <option value="l3">Level 3 — Directorate</option>
                  <option value="l4">Level 4 — Executive</option>
                </select>
              </label>
              <label className="lock-screen-label">
                Title
                <input
                  className="lock-screen-input"
                  value={newJobTitle}
                  onChange={(e) => setNewJobTitle(e.target.value)}
                  placeholder="e.g. Water Systems Overseer"
                  disabled={busy}
                  autoFocus
                />
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={() => setNewJobOpen(false)} disabled={busy}>
                  Cancel
                </button>
                <button type="button" className="glass-action-btn" onClick={() => void saveNewJob()} disabled={busy || !newJobTitle.trim()}>
                  Add job
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      <header className="personnel-hub-top">
        <div className="personnel-hub-top-text">
          <h1 className="quartermaster-hub-title">Personnel registry</h1>
          <p className="quartermaster-hub-subtitle text-muted">
            Roster sorted by rank (Level 4 — General and above may add or remove accounts). At most two Level V
            (Command) operators; the ATHENA architect badge is separate from rank and cannot be reassigned here.
          </p>
        </div>
        {onOpenOrgChart ? (
          <button
            type="button"
            className="secondary personnel-hub-add-btn"
            disabled={busy}
            onClick={onOpenOrgChart}
            title="Visual chain of command"
          >
            Org chart
          </button>
        ) : null}
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
                <th>Domain</th>
                <th>Role</th>
                <th>Rank</th>
                <th>Notes</th>
                <th>Status</th>
                <th>Created</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {sortedRows.length === 0 ? (
                <tr>
                  <td colSpan={9} className="text-muted">
                    No personnel rows.
                  </td>
                </tr>
              ) : (
                sortedRows.map((p) => {
                  const isSelf = user?.id === p.id;
                  const st = normalizeOperationalStatus(p.operational_status);
                  const canEdit = canEditTarget(p);
                  const canDelete = canDeleteTarget(p);
                  return (
                    <tr key={p.id}>
                      <td>
                        <button
                          type="button"
                          className="secondary small-btn"
                          onClick={() => onOpenProfile?.(p.id)}
                          title="Open profile"
                        >
                          {p.name}
                        </button>
                      </td>
                      <td>
                        <button
                          type="button"
                          className="secondary small-btn"
                          onClick={() => onOpenProfile?.(p.id)}
                          title="Open profile"
                        >
                          {rosterLabel(p)}
                        </button>
                      </td>
                      <td>{(p.domain?.trim() || p.division || "—") as string}</td>
                      <td className="text-muted" style={{ fontSize: "0.86rem" }}>
                        {primaryRoleForRow(p)}
                      </td>
                      <td>{formatRomanRank(p.rank_level)}</td>
                      <td className="text-muted" style={{ fontSize: "0.8rem", maxWidth: "12rem" }}>
                        {p.athena_architect ? (
                          <span className="personnel-architect-badge" title="Designated ATHENA architect (designer / creator seat)">
                            <span className="personnel-architect-badge__icon" aria-hidden>
                              ⟁
                            </span>
                            Architect
                          </span>
                        ) : (
                          "—"
                        )}
                      </td>
                      <td>
                        <span className={`personnel-status-badge personnel-status-badge--${st}`}>
                          {statusLabel(st)}
                        </span>
                      </td>
                      <td className="text-muted">{new Date(p.created_at).toLocaleString()}</td>
                      <td>
                        <div className="personnel-hub-actions">
                          <button
                            type="button"
                            className="secondary small-btn"
                            disabled={busy || !canEdit}
                            title={
                              canEdit
                                ? "Edit this personnel record"
                                : "You do not have permission to modify this personnel record."
                            }
                            onClick={() => {
                              if (!canEdit) return;
                              setEditPerson(p);
                              setEditLoginDraft(p.name ?? "");
                              setEditDisplayDraft(p.display_name ?? "");
                              setEditRankDraft(String(p.rank_level ?? 1));
                              setEditStatusDraft(normalizeOperationalStatus(p.operational_status ?? "active"));
                              {
                                const dom = (p.domain && PERSONNEL_DOMAINS.includes(p.domain as any)
                                  ? (p.domain as PersonnelDomain)
                                  : PERSONNEL_DOMAINS[0]!) as PersonnelDomain;
                                setEditDomain(dom);
                                setEditRoleL1(String(p.role_l1 ?? ""));
                                setEditRoleL2(String(p.role_l2 ?? ""));
                                setEditRoleL3(String(p.role_l3 ?? ""));
                                setEditRoleL4(String(p.role_l4 ?? ""));
                                if (!String(p.role_l2 ?? "").trim() || !String(p.role_l3 ?? "").trim() || !String(p.role_l4 ?? "").trim()) {
                                  const s = suggestHierarchyForLevel1(dom);
                                  if (!String(p.role_l2 ?? "").trim()) setEditRoleL2(s.role_l2);
                                  if (!String(p.role_l3 ?? "").trim()) setEditRoleL3(s.role_l3);
                                  if (!String(p.role_l4 ?? "").trim()) setEditRoleL4(s.role_l4);
                                }
                              }
                            }}
                          >
                            Edit personnel
                          </button>
                          {canDeleteFromRegistry ? (
                            <button
                              type="button"
                              className="secondary small-btn personnel-hub-delete-btn"
                              disabled={busy || isSelf || !canDelete}
                              title={
                                isSelf
                                  ? "You cannot remove your own account while logged in."
                                  : canDelete
                                    ? "Remove this account from the registry"
                                    : "You do not have permission to remove this personnel record."
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
              <strong>Level 5 — Command</strong> cannot be assigned here; up to two Level V operators exist on the
              roster, promoted only by an incumbent Level V in Personnel edit. The <strong>Architect</strong> badge is
              reserved for the ATHENA designer seat.
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
                Rank (new accounts)
                <select
                  className="lock-screen-input"
                  value={rank}
                  onChange={(e) => {
                    setRank(e.target.value);
                  }}
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
                Domain (field)
                <select
                  className="lock-screen-input"
                  value={addDomain}
                  onChange={(e) => setAddDomain(e.target.value as PersonnelDomain)}
                  disabled={busy}
                >
                  {PERSONNEL_DOMAINS.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>

              <label className="lock-screen-label">
                Role title (at selected rank)
                <select
                  className="lock-screen-input"
                  value={
                    parseInt(rank, 10) === 4
                      ? addRoleL4
                      : parseInt(rank, 10) === 3
                        ? addRoleL3
                        : parseInt(rank, 10) === 2
                          ? addRoleL2
                          : addRoleL1
                  }
                  onChange={(e) => {
                    const v = e.target.value;
                    const rl = parseInt(rank, 10);
                    if (rl === 4) setAddRoleL4(v);
                    else if (rl === 3) setAddRoleL3(v);
                    else if (rl === 2) setAddRoleL2(v);
                    else setAddRoleL1(v);
                  }}
                  disabled={busy}
                >
                  <option value="">Select a role…</option>
                  {addRoleOptions.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
                <div style={{ marginTop: 8 }}>
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy}
                    onClick={() => openNewJob("add")}
                  >
                    New job…
                  </button>
                </div>
              </label>

              <details className="text-muted small" style={{ marginTop: "-0.25rem" }}>
                <summary style={{ cursor: "pointer" }}>Show hierarchy chain (editable)</summary>
                <div style={{ display: "grid", gap: "0.5rem", marginTop: "0.5rem" }}>
                  <label className="lock-screen-label">
                    Level 2 — Sector leader
                    <input
                      className="lock-screen-input"
                      list="personnel-add-l2"
                      value={addRoleL2}
                      onChange={(e) => setAddRoleL2(e.target.value)}
                      disabled={busy}
                    />
                    <datalist id="personnel-add-l2">
                      {(LEVEL2_SECTOR_LEADERS[addDomain] ?? []).map((t) => (
                        <option key={t} value={t} />
                      ))}
                    </datalist>
                  </label>
                  <label className="lock-screen-label">
                    Level 3 — Directorate
                    <input
                      className="lock-screen-input"
                      list="personnel-add-l3"
                      value={addRoleL3}
                      onChange={(e) => setAddRoleL3(e.target.value)}
                      disabled={busy}
                    />
                    <datalist id="personnel-add-l3">
                      {LEVEL3_DIRECTORATES.map((t) => (
                        <option key={t} value={t} />
                      ))}
                    </datalist>
                  </label>
                  <label className="lock-screen-label">
                    Level 4 — Executive
                    <input
                      className="lock-screen-input"
                      list="personnel-add-l4"
                      value={addRoleL4}
                      onChange={(e) => setAddRoleL4(e.target.value)}
                      disabled={busy}
                    />
                    <datalist id="personnel-add-l4">
                      {LEVEL4_EXECUTIVE.map((t) => (
                        <option key={t} value={t} />
                      ))}
                    </datalist>
                  </label>
                </div>
              </details>
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
                <button type="submit" className="glass-action-btn" disabled={busy}>
                  {busy ? "Saving…" : "Create account"}
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}

      {editPerson ? (
        <div className="modal-backdrop" onClick={() => !busy && setEditPerson(null)}>
          <div className="qm-modal glass-panel personnel-hub-edit-modal" onClick={(e) => e.stopPropagation()}>
            <h3>Edit personnel</h3>
            <p className="text-muted small" style={{ marginTop: 0 }}>
              Update login name, division, rank, roster display, and operational status. Level V is only assignable by
              an incumbent Level V operator, and at most two Level V rows may exist. The Architect badge is not granted
              by rank.
            </p>
            <form className="personnel-form" onSubmit={saveEditPersonnel}>
              <label className="lock-screen-label">
                Login name
                <input
                  className="lock-screen-input"
                  value={editLoginDraft}
                  onChange={(e) => setEditLoginDraft(e.target.value)}
                  disabled={busy}
                  required
                />
              </label>
              <label className="lock-screen-label">
                Domain (field)
                <select
                  className="lock-screen-input"
                  value={editDomain}
                  onChange={(e) => setEditDomain(e.target.value as PersonnelDomain)}
                  disabled={busy}
                >
                  {PERSONNEL_DOMAINS.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Display name (optional)
                <input
                  className="lock-screen-input"
                  value={editDisplayDraft}
                  onChange={(e) => setEditDisplayDraft(e.target.value)}
                  disabled={busy}
                  placeholder="e.g. Corvus — Caesar, or leave blank for login only"
                />
              </label>
              <label className="lock-screen-label">
                Rank level
                <select
                  className="lock-screen-input"
                  value={editRankDraft}
                  onChange={(e) => setEditRankDraft(e.target.value)}
                  disabled={busy}
                >
                  {(r >= 5 ? ([1, 2, 3, 4, 5] as const) : ([1, 2, 3, 4] as const)).map((n) => (
                    <option key={n} value={String(n)}>
                      {formatRomanRank(n)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Role title (at selected rank)
                <select
                  className="lock-screen-input"
                  value={
                    parseInt(editRankDraft, 10) === 4
                      ? editRoleL4
                      : parseInt(editRankDraft, 10) === 3
                        ? editRoleL3
                        : parseInt(editRankDraft, 10) === 2
                          ? editRoleL2
                          : editRoleL1
                  }
                  onChange={(e) => {
                    const v = e.target.value;
                    const rl = parseInt(editRankDraft, 10);
                    if (rl === 4) setEditRoleL4(v);
                    else if (rl === 3) setEditRoleL3(v);
                    else if (rl === 2) setEditRoleL2(v);
                    else setEditRoleL1(v);
                  }}
                  disabled={busy}
                >
                  <option value="">Select a role…</option>
                  {editRoleOptions.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
                <div style={{ marginTop: 8 }}>
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy}
                    onClick={() => openNewJob("edit")}
                  >
                    New job…
                  </button>
                </div>
              </label>

              <details className="text-muted small" style={{ marginTop: "-0.25rem" }}>
                <summary style={{ cursor: "pointer" }}>Show hierarchy chain (editable)</summary>
                <div style={{ display: "grid", gap: "0.5rem", marginTop: "0.5rem" }}>
                  <label className="lock-screen-label">
                    Level 2 — Sector leader
                    <input
                      className="lock-screen-input"
                      list="personnel-edit-l2"
                      value={editRoleL2}
                      onChange={(e) => setEditRoleL2(e.target.value)}
                      disabled={busy}
                    />
                    <datalist id="personnel-edit-l2">
                      {(LEVEL2_SECTOR_LEADERS[editDomain] ?? []).map((t) => (
                        <option key={t} value={t} />
                      ))}
                    </datalist>
                  </label>
                  <label className="lock-screen-label">
                    Level 3 — Directorate
                    <input
                      className="lock-screen-input"
                      list="personnel-edit-l3"
                      value={editRoleL3}
                      onChange={(e) => setEditRoleL3(e.target.value)}
                      disabled={busy}
                    />
                    <datalist id="personnel-edit-l3">
                      {LEVEL3_DIRECTORATES.map((t) => (
                        <option key={t} value={t} />
                      ))}
                    </datalist>
                  </label>
                  <label className="lock-screen-label">
                    Level 4 — Executive
                    <input
                      className="lock-screen-input"
                      list="personnel-edit-l4"
                      value={editRoleL4}
                      onChange={(e) => setEditRoleL4(e.target.value)}
                      disabled={busy}
                    />
                    <datalist id="personnel-edit-l4">
                      {LEVEL4_EXECUTIVE.map((t) => (
                        <option key={t} value={t} />
                      ))}
                    </datalist>
                  </label>
                </div>
              </details>
              <label className="lock-screen-label">
                Operational status
                <select
                  className="lock-screen-input"
                  value={editStatusDraft}
                  onChange={(e) => setEditStatusDraft(normalizeOperationalStatus(e.target.value))}
                  disabled={busy}
                >
                  <option value="active">Active</option>
                  <option value="on_mission">On mission</option>
                  <option value="mia">MIA</option>
                  <option value="kia">KIA</option>
                </select>
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={() => setEditPerson(null)} disabled={busy}>
                  Cancel
                </button>
                <button type="submit" className="glass-action-btn" disabled={busy}>
                  {busy ? "Saving…" : "Save"}
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}
    </div>
  );
}
