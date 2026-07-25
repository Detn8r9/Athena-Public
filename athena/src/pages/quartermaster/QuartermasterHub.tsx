import React, { useCallback, useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { ClipboardList, Package } from "lucide-react";
import { useAuth } from "../../domains/auth/AuthContext";
import { DIRECTORATES, DIRECTORATE_LABELS, type Directorate } from "../../shared/orgStructure";

type InventoryItemRow = {
  id: string;
  division: string;
  item_name: string;
  quantity: number;
  last_updated: number;
  category: string;
  directorate?: string | null;
};

type InventoryLogRow = {
  id: string;
  item_id: string;
  user_id: string;
  action_type: string;
  old_qty: number | null;
  new_qty: number | null;
  timestamp: number;
  caesar_fingerprint: string | null;
};

export type QuartermasterTabId = "inventory" | "log";

export type QuartermasterHubProps = {
  /** Nested in Field & logistics hub — no page chrome; tabs may be controlled by parent. */
  embedded?: boolean;
  activeTab?: QuartermasterTabId;
  onActiveTabChange?: (tab: QuartermasterTabId) => void;
};

const CATEGORY_ORDER = [
  "food",
  "water",
  "weapons",
  "medical",
  "equipment",
  "comms",
  "fuel",
  "materials",
  "general",
] as const;

const CATEGORY_LABELS: Record<string, string> = {
  food: "Food",
  water: "Water",
  weapons: "Weapons",
  medical: "Medical",
  equipment: "Equipment",
  comms: "Communications",
  fuel: "Fuel",
  materials: "Materials",
  general: "General",
};

function categoryLabel(key: string): string {
  return CATEGORY_LABELS[key] ?? key.replace(/_/g, " ");
}

export function QuartermasterHub({
  embedded = false,
  activeTab: activeTabProp,
  onActiveTabChange,
}: QuartermasterHubProps = {}) {
  const { rankLevel, division, directorate: userDir } = useAuth();
  const r = rankLevel ?? 0;
  const div = (division ?? "").trim();
  const myOrgKey = (userDir?.trim() || div).toLowerCase();

  const [tabInternal, setTabInternal] = useState<QuartermasterTabId>("inventory");
  const controlled = activeTabProp !== undefined;
  const tab = controlled ? activeTabProp : tabInternal;
  const setTab = useCallback(
    (next: QuartermasterTabId) => {
      if (controlled) onActiveTabChange?.(next);
      else setTabInternal(next);
    },
    [controlled, onActiveTabChange],
  );
  const [items, setItems] = useState<InventoryItemRow[]>([]);
  const [logs, setLogs] = useState<InventoryLogRow[]>([]);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [spotlightFilter, setSpotlightFilter] = useState<string | null>(null);

  const [addOpen, setAddOpen] = useState(false);
  /** Directorate key for new row — `add_inventory_item` `division` arg (and directorate column). */
  const [addDirectorate, setAddDirectorate] = useState<Directorate>("Infrastructure");
  const [addName, setAddName] = useState("");
  const [addQty, setAddQty] = useState("0");
  const [addCategory, setAddCategory] = useState<string>("general");

  const [editRow, setEditRow] = useState<InventoryItemRow | null>(null);
  const [editQty, setEditQty] = useState("");
  const [editCategory, setEditCategory] = useState("general");

  const [fpRow, setFpRow] = useState<InventoryLogRow | null>(null);
  const [fpNote, setFpNote] = useState("");

  useEffect(() => {
    const u = userDir?.trim();
    if (u && (DIRECTORATES as readonly string[]).includes(u)) {
      setAddDirectorate(u as Directorate);
    }
  }, [userDir]);

  useEffect(() => {
    try {
      const q = sessionStorage.getItem("athena-spotlight-inventory-query");
      if (q?.trim()) {
        setSpotlightFilter(q.trim());
        sessionStorage.removeItem("athena-spotlight-inventory-query");
        sessionStorage.removeItem("athena-spotlight-inventory-item");
        setTab("inventory");
      }
    } catch {
      /* ignore */
    }
  }, [setTab]);

  const visibleItems = useMemo(() => {
    let rows = items;
    if (r < 4) {
      if (r === 3) {
        if (myOrgKey) {
          rows = items.filter((it) => {
            const rowKey = (it.directorate?.trim() || it.division?.trim() || "").toLowerCase();
            return rowKey === myOrgKey;
          });
        }
      } else {
        rows = items;
      }
    }
    if (spotlightFilter) {
      const q = spotlightFilter.toLowerCase();
      rows = rows.filter(
        (it) =>
          it.item_name.toLowerCase().includes(q) ||
          (it.category || "").toLowerCase().includes(q) ||
          it.id === spotlightFilter,
      );
    }
    return rows;
  }, [items, r, myOrgKey, spotlightFilter]);

  /** Top-level directorate bucket */
  const bucketKey = (it: InventoryItemRow) => {
    const d = it.directorate?.trim();
    if (d) return d;
    return "Uncategorized";
  };

  const itemsByDirectorate = useMemo(() => {
    const m = new Map<string, InventoryItemRow[]>();
    for (const it of visibleItems) {
      const k = bucketKey(it);
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(it);
    }
    for (const arr of m.values()) {
      arr.sort((a, b) => a.item_name.localeCompare(b.item_name, undefined, { sensitivity: "base" }));
    }
    return m;
  }, [visibleItems]);

  const directorateSectionOrder = useMemo(() => {
    const o: string[] = DIRECTORATES.slice() as string[];
    o.push("Uncategorized");
    return o;
  }, []);

  function groupByCategory(rows: InventoryItemRow[]) {
    const m = new Map<string, InventoryItemRow[]>();
    for (const it of rows) {
      const c = it.category || "general";
      if (!m.has(c)) m.set(c, []);
      m.get(c)!.push(it);
    }
    for (const arr of m.values()) {
      arr.sort((a, b) => a.item_name.localeCompare(b.item_name, undefined, { sensitivity: "base" }));
    }
    return m;
  }

  function extraCategoriesFor(m: Map<string, InventoryItemRow[]>) {
    const s = new Set(m.keys());
    for (const c of CATEGORY_ORDER) s.delete(c);
    return Array.from(s).sort((a, b) => a.localeCompare(b));
  }


  const refreshInventory = useCallback(async () => {
    setLoadErr(null);
    try {
      const rows = await invoke<InventoryItemRow[]>("list_inventory");
      setItems(Array.isArray(rows) ? rows : []);
    } catch (e) {
      setLoadErr(e != null ? String(e) : "Failed to load inventory.");
    }
  }, []);

  const refreshLogs = useCallback(async () => {
    setLoadErr(null);
    try {
      const rows = await invoke<InventoryLogRow[]>("list_inventory_log");
      setLogs(Array.isArray(rows) ? rows : []);
    } catch (e) {
      setLoadErr(e != null ? String(e) : "Failed to load logistics log.");
    }
  }, []);

  const canRollback = r >= 4;

  useEffect(() => {
    void refreshInventory();
  }, [refreshInventory]);

  useEffect(() => {
    if (tab === "log" && r >= 4) {
      void refreshLogs();
    }
  }, [tab, r, refreshLogs]);

  const canSeeLog = r >= 4;
  const hideEdits = r <= 2;
  const canRebucket = r >= 4;
  const rowWritable = (row: InventoryItemRow) => {
    if (hideEdits) return false;
    if (r >= 4) return true;
    if (r === 3) {
      const itemKey = (row.directorate?.trim() || row.division).trim().toLowerCase();
      return itemKey === myOrgKey;
    }
    return false;
  };

  async function rebucketRow(row: InventoryItemRow, next: Directorate) {
    if (!canRebucket || busy) return;
    setBusy(true);
    setLoadErr(null);
    try {
      await invoke("update_inventory_directorate", {
        item_id: row.id,
        new_directorate: next,
      });
      await refreshInventory();
    } catch (err) {
      setLoadErr(err != null ? String(err) : "Re-bucket failed.");
    } finally {
      setBusy(false);
    }
  }

  async function submitAdd(e: React.FormEvent) {
    e.preventDefault();
    const q = parseInt(addQty, 10);
    if (Number.isNaN(q) || q < 0) return;
    setBusy(true);
    setLoadErr(null);
    try {
      await invoke("add_inventory_item", {
        division: addDirectorate,
        item_name: addName.trim(),
        quantity: q,
        category: addCategory,
      });
      setAddOpen(false);
      setAddName("");
      setAddQty("0");
      setAddCategory("general");
      await refreshInventory();
    } catch (err) {
      setLoadErr(err != null ? String(err) : "Add failed.");
    } finally {
      setBusy(false);
    }
  }

  async function submitEdit(e: React.FormEvent) {
    e.preventDefault();
    if (!editRow) return;
    const q = parseInt(editQty, 10);
    if (Number.isNaN(q) || q < 0) return;
    setBusy(true);
    setLoadErr(null);
    try {
      const id = editRow.id;
      if (q !== editRow.quantity) {
        await invoke("update_inventory_quantity", { item_id: id, new_quantity: q });
      }
      if (editCategory !== editRow.category) {
        await invoke("update_inventory_category", { item_id: id, category: editCategory });
      }
      setEditRow(null);
      await refreshInventory();
    } catch (err) {
      setLoadErr(err != null ? String(err) : "Update failed.");
    } finally {
      setBusy(false);
    }
  }

  async function submitFingerprint(e: React.FormEvent) {
    e.preventDefault();
    if (!fpRow || !fpNote.trim()) return;
    setBusy(true);
    setLoadErr(null);
    try {
      await invoke("edit_log_fingerprint", {
        log_id: fpRow.id,
        correction_note: fpNote.trim(),
      });
      setFpRow(null);
      setFpNote("");
      await refreshLogs();
    } catch (err) {
      setLoadErr(err != null ? String(err) : "Annotation failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={`quartermaster-hub${embedded ? " quartermaster-hub--embedded" : " home-view-scroll"}`}>
      {embedded ? null : (
        <header className="quartermaster-hub-header">
          <h1 className="quartermaster-hub-title">Logistics Hub</h1>
          <p className="quartermaster-hub-subtitle text-muted">Inventory, supply, and audit</p>
        </header>
      )}

      {embedded ? null : (
        <div className="quartermaster-hub-tabs glass-panel">
          <button
            type="button"
            className={tab === "inventory" ? "qm-tab qm-tab-active" : "qm-tab"}
            onClick={() => setTab("inventory")}
          >
            <Package size={18} aria-hidden />
            Inventory
          </button>
          {canSeeLog ? (
            <button
              type="button"
              className={tab === "log" ? "qm-tab qm-tab-active" : "qm-tab"}
              onClick={() => setTab("log")}
            >
              <ClipboardList size={18} aria-hidden />
              Logistics log
            </button>
          ) : null}
        </div>
      )}

      {loadErr ? (
        <p className="quartermaster-hub-err" role="alert">
          {loadErr}
        </p>
      ) : null}

      {tab === "inventory" ? (
        <section className="library-section glass-panel quartermaster-inventory-master">
          <div className="quartermaster-section-head">
            <h3>Inventory</h3>
            {!hideEdits ? (
              <button type="button" className="secondary" onClick={() => setAddOpen(true)} disabled={busy}>
                Add item
              </button>
            ) : null}
          </div>
          {spotlightFilter ? (
            <p className="text-muted small" style={{ display: "flex", gap: "0.5rem", alignItems: "center", flexWrap: "wrap" }}>
              Spotlight filter: <strong>{spotlightFilter}</strong>
              <button type="button" className="secondary" onClick={() => setSpotlightFilter(null)}>
                Clear
              </button>
            </p>
          ) : null}
          <p className="text-muted small qm-inventory-lead">
            Stock is grouped by <strong>directorate</strong>, then supply category. Rows without a directorate land in
            &nbsp;<em>Uncategorized</em> until a Level 4+ operator re-buckets them.
          </p>
          {items.length === 0 ? (
            <p className="text-muted">No inventory rows.</p>
          ) : visibleItems.length === 0 ? (
            <p className="text-muted">No items in your directorate (Level 3) or the list is empty.</p>
          ) : (
            <>
              {directorateSectionOrder.map((dirKey) => {
                const dirRows = itemsByDirectorate.get(dirKey);
                if (!dirRows || dirRows.length === 0) return null;
                const grouped = groupByCategory(dirRows);
                const extra = extraCategoriesFor(grouped);
                const dirTitle =
                  dirKey === "Uncategorized"
                    ? "Uncategorized (legacy / pending review)"
                    : DIRECTORATE_LABELS[dirKey as Directorate] ?? dirKey;
                return (
                  <div key={dirKey} className="quartermaster-directorate-block" style={{ marginBottom: "1.25rem" }}>
                    <h3 className="quartermaster-category-title" style={{ fontSize: "1.05rem" }}>
                      {dirTitle}
                    </h3>
                    {CATEGORY_ORDER.map((cat) => {
                      const group = grouped.get(cat);
                      if (!group || group.length === 0) return null;
                      return (
                        <div key={`${dirKey}-${cat}`} className="quartermaster-category-block">
                          <h4 className="quartermaster-category-title">{categoryLabel(cat)}</h4>
                          <div className="quartermaster-table-wrap">
                            <table className="quartermaster-table">
                              <thead>
                                <tr>
                                  <th>Directorate</th>
                                  <th>Item</th>
                                  <th>Category</th>
                                  <th>Qty</th>
                                  <th>Updated</th>
                                  {!hideEdits ? <th>Actions</th> : null}
                                </tr>
                              </thead>
                              <tbody>
                                {group.map((row) => {
                                  const w = rowWritable(row);
                                  const dirSelectVal =
                                    row.directorate?.trim() &&
                                    (DIRECTORATES as readonly string[]).includes(row.directorate.trim())
                                      ? row.directorate.trim()
                                      : "";
                                  return (
                                    <tr key={row.id}>
                                      <td>
                                        {canRebucket ? (
                                          <select
                                            className="lock-screen-input"
                                            style={{ fontSize: "0.8rem", minWidth: 200 }}
                                            value={dirSelectVal}
                                            onChange={(e) => {
                                              const v = e.target.value;
                                              if (v) void rebucketRow(row, v as Directorate);
                                            }}
                                            disabled={busy}
                                            aria-label="Assign directorate"
                                            title="Re-bucket to a directorate"
                                          >
                                            <option value="">(Uncategorized — assign)</option>
                                            {DIRECTORATES.map((d) => (
                                              <option key={d} value={d}>
                                                {DIRECTORATE_LABELS[d]}
                                              </option>
                                            ))}
                                          </select>
                                        ) : (
                                          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                                            <span>{row.directorate?.trim() || "—"}</span>
                                            {row.division && row.division !== row.directorate ? (
                                              <span className="text-muted" style={{ fontSize: "0.75rem" }}>
                                                Legacy: {row.division}
                                              </span>
                                            ) : null}
                                          </div>
                                        )}
                                      </td>
                                      <td>{row.item_name}</td>
                                      <td className="text-muted">{categoryLabel(row.category || "general")}</td>
                                      <td>{row.quantity}</td>
                                      <td className="text-muted">{new Date(row.last_updated).toLocaleString()}</td>
                                      {!hideEdits ? (
                                        <td>
                                          <button
                                            type="button"
                                            className="secondary small-btn"
                                            disabled={!w || busy}
                                            title={!w ? "Not authorized for this row" : "Edit line"}
                                            onClick={() => {
                                              setEditRow(row);
                                              setEditQty(String(row.quantity));
                                              setEditCategory(row.category || "general");
                                            }}
                                          >
                                            Edit
                                          </button>
                                        </td>
                                      ) : null}
                                    </tr>
                                  );
                                })}
                              </tbody>
                            </table>
                          </div>
                        </div>
                      );
                    })}
                    {extra.map((cat) => {
                      const group = grouped.get(cat);
                      if (!group || group.length === 0) return null;
                      return (
                        <div key={`${dirKey}-${cat}`} className="quartermaster-category-block">
                          <h4 className="quartermaster-category-title">{categoryLabel(cat)}</h4>
                          <div className="quartermaster-table-wrap">
                            <table className="quartermaster-table">
                              <thead>
                                <tr>
                                  <th>Directorate</th>
                                  <th>Item</th>
                                  <th>Category</th>
                                  <th>Qty</th>
                                  <th>Updated</th>
                                  {!hideEdits ? <th>Actions</th> : null}
                                </tr>
                              </thead>
                              <tbody>
                                {group.map((row) => {
                                  const w = rowWritable(row);
                                  const dirSelectVal =
                                    row.directorate?.trim() &&
                                    (DIRECTORATES as readonly string[]).includes(row.directorate.trim())
                                      ? row.directorate.trim()
                                      : "";
                                  return (
                                    <tr key={row.id}>
                                      <td>
                                        {canRebucket ? (
                                          <select
                                            className="lock-screen-input"
                                            style={{ fontSize: "0.8rem", minWidth: 200 }}
                                            value={dirSelectVal}
                                            onChange={(e) => {
                                              const v = e.target.value;
                                              if (v) void rebucketRow(row, v as Directorate);
                                            }}
                                            disabled={busy}
                                            aria-label="Assign directorate"
                                            title="Re-bucket to a directorate"
                                          >
                                            <option value="">(Uncategorized — assign)</option>
                                            {DIRECTORATES.map((d) => (
                                              <option key={d} value={d}>
                                                {DIRECTORATE_LABELS[d]}
                                              </option>
                                            ))}
                                          </select>
                                        ) : (
                                          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                                            <span>{row.directorate?.trim() || "—"}</span>
                                            {row.division && row.division !== row.directorate ? (
                                              <span className="text-muted" style={{ fontSize: "0.75rem" }}>
                                                Legacy: {row.division}
                                              </span>
                                            ) : null}
                                          </div>
                                        )}
                                      </td>
                                      <td>{row.item_name}</td>
                                      <td className="text-muted">{categoryLabel(row.category || "general")}</td>
                                      <td>{row.quantity}</td>
                                      <td className="text-muted">{new Date(row.last_updated).toLocaleString()}</td>
                                      {!hideEdits ? (
                                        <td>
                                          <button
                                            type="button"
                                            className="secondary small-btn"
                                            disabled={!w || busy}
                                            title={!w ? "Not authorized for this row" : "Edit line"}
                                            onClick={() => {
                                              setEditRow(row);
                                              setEditQty(String(row.quantity));
                                              setEditCategory(row.category || "general");
                                            }}
                                          >
                                            Edit
                                          </button>
                                        </td>
                                      ) : null}
                                    </tr>
                                  );
                                })}
                              </tbody>
                            </table>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                );
              })}
            </>
          )}
        </section>
      ) : (
        <section className="library-section glass-panel quartermaster-inventory-master">
          <h3>Logistics log</h3>
          <div className="quartermaster-table-wrap">
            <table className="quartermaster-table">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Action</th>
                  <th>Item</th>
                  <th>Old</th>
                  <th>New</th>
                  <th>Note</th>
                  {canRollback ? <th>Rollback</th> : null}
                  {r >= 5 ? <th>Annotate</th> : null}
                </tr>
              </thead>
              <tbody>
                {logs.length === 0 ? (
                  <tr>
                    <td colSpan={6 + (canRollback ? 1 : 0) + (r >= 5 ? 1 : 0)} className="text-muted">
                      No log entries.
                    </td>
                  </tr>
                ) : (
                  logs.map((log) => (
                    <tr key={log.id}>
                      <td className="text-muted">{new Date(log.timestamp).toLocaleString()}</td>
                      <td>{log.action_type}</td>
                      <td className="qm-mono">{log.item_id.slice(0, 8)}…</td>
                      <td>{log.old_qty ?? "—"}</td>
                      <td>{log.new_qty ?? "—"}</td>
                      <td className="qm-note-cell">{log.caesar_fingerprint ?? "—"}</td>
                      {canRollback ? (
                        <td>
                          <button
                            type="button"
                            className="secondary small-btn"
                            disabled={busy || log.old_qty == null}
                            title={log.old_qty == null ? "No previous value to rollback to" : "Rollback this change"}
                            onClick={() => {
                              setBusy(true);
                              setLoadErr(null);
                              void invoke("rollback_inventory_change", { log_id: log.id })
                                .then(() => refreshInventory())
                                .then(() => refreshLogs())
                                .catch((e) => setLoadErr(e != null ? String(e) : "Rollback failed."))
                                .finally(() => setBusy(false));
                            }}
                          >
                            Rollback
                          </button>
                        </td>
                      ) : null}
                      {r >= 5 ? (
                        <td>
                          <button
                            type="button"
                            className="secondary small-btn"
                            disabled={busy}
                            onClick={() => {
                              setFpRow(log);
                              setFpNote("");
                            }}
                          >
                            Note
                          </button>
                        </td>
                      ) : null}
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {addOpen ? (
        <div className="modal-backdrop" onClick={() => !busy && setAddOpen(false)}>
          <div className="qm-modal glass-panel" onClick={(e) => e.stopPropagation()}>
            <h3>Add inventory item</h3>
            <form onSubmit={submitAdd}>
              <label className="lock-screen-label">
                Category
                <select
                  className="lock-screen-input"
                  value={addCategory}
                  onChange={(e) => setAddCategory(e.target.value)}
                  disabled={busy}
                >
                  {CATEGORY_ORDER.map((c) => (
                    <option key={c} value={c}>
                      {categoryLabel(c)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Directorate
                <select
                  className="lock-screen-input"
                  value={addDirectorate}
                  onChange={(e) => setAddDirectorate(e.target.value as Directorate)}
                  required
                  disabled={busy}
                >
                  {DIRECTORATES.map((d) => (
                    <option key={d} value={d}>
                      {DIRECTORATE_LABELS[d]}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Item name
                <input
                  className="lock-screen-input"
                  value={addName}
                  onChange={(e) => setAddName(e.target.value)}
                  required
                  disabled={busy}
                />
              </label>
              <label className="lock-screen-label">
                Quantity
                <input
                  className="lock-screen-input"
                  type="number"
                  min={0}
                  value={addQty}
                  onChange={(e) => setAddQty(e.target.value)}
                  disabled={busy}
                />
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={() => setAddOpen(false)} disabled={busy}>
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

      {editRow ? (
        <div className="modal-backdrop" onClick={() => !busy && setEditRow(null)}>
          <div className="qm-modal glass-panel" onClick={(e) => e.stopPropagation()}>
            <h3>Edit line</h3>
            <p className="text-muted small">
              {editRow.item_name} — {editRow.directorate?.trim() || "Uncategorized"}
              {editRow.division && editRow.division !== editRow.directorate ? ` (legacy: ${editRow.division})` : ""}
            </p>
            <form onSubmit={submitEdit}>
              <label className="lock-screen-label">
                Category
                <select
                  className="lock-screen-input"
                  value={editCategory}
                  onChange={(e) => setEditCategory(e.target.value)}
                  disabled={busy}
                >
                  {CATEGORY_ORDER.map((c) => (
                    <option key={c} value={c}>
                      {categoryLabel(c)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="lock-screen-label">
                Quantity
                <input
                  className="lock-screen-input"
                  type="number"
                  min={0}
                  value={editQty}
                  onChange={(e) => setEditQty(e.target.value)}
                  disabled={busy}
                />
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={() => setEditRow(null)} disabled={busy}>
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

      {fpRow ? (
        <div className="modal-backdrop" onClick={() => !busy && setFpRow(null)}>
          <div className="qm-modal glass-panel" onClick={(e) => e.stopPropagation()}>
            <h3>Caesar fingerprint note</h3>
            <p className="text-muted small">Appends to the audit trail without changing quantities.</p>
            <form onSubmit={submitFingerprint}>
              <label className="lock-screen-label">
                Correction note
                <textarea
                  className="lock-screen-input qm-textarea"
                  rows={4}
                  value={fpNote}
                  onChange={(e) => setFpNote(e.target.value)}
                  disabled={busy}
                  required
                />
              </label>
              <div className="qm-modal-actions">
                <button type="button" className="secondary" onClick={() => setFpRow(null)} disabled={busy}>
                  Cancel
                </button>
                <button type="submit" className="secondary" disabled={busy}>
                  Append
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}
    </div>
  );
}
