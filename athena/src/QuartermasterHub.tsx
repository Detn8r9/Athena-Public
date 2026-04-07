import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ClipboardList, Package } from "lucide-react";
import { useAuth } from "./auth/AuthContext";

type InventoryItemRow = {
  id: string;
  division: string;
  item_name: string;
  quantity: number;
  last_updated: number;
  category: string;
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
  const { rankLevel, division } = useAuth();
  const r = rankLevel ?? 0;
  const div = (division ?? "").trim();

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

  const [addOpen, setAddOpen] = useState(false);
  const [addDivision, setAddDivision] = useState("");
  const [addName, setAddName] = useState("");
  const [addQty, setAddQty] = useState("0");
  const [addCategory, setAddCategory] = useState<string>("general");

  const [editRow, setEditRow] = useState<InventoryItemRow | null>(null);
  const [editQty, setEditQty] = useState("");
  const [editCategory, setEditCategory] = useState("general");

  const [fpRow, setFpRow] = useState<InventoryLogRow | null>(null);
  const [fpNote, setFpNote] = useState("");

  const groupedItems = useMemo(() => {
    const m = new Map<string, InventoryItemRow[]>();
    for (const it of items) {
      const c = it.category || "general";
      if (!m.has(c)) m.set(c, []);
      m.get(c)!.push(it);
    }
    for (const arr of m.values()) {
      arr.sort((a, b) => a.item_name.localeCompare(b.item_name, undefined, { sensitivity: "base" }));
    }
    return m;
  }, [items]);

  const extraCategories = useMemo(() => {
    const s = new Set(groupedItems.keys());
    for (const c of CATEGORY_ORDER) s.delete(c);
    return Array.from(s).sort((a, b) => a.localeCompare(b));
  }, [groupedItems]);


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
  const rowWritable = (itemDivision: string) => {
    if (hideEdits) return false;
    if (r >= 4) return true;
    if (r === 3) return itemDivision.trim().toLowerCase() === div.toLowerCase();
    return false;
  };

  async function submitAdd(e: React.FormEvent) {
    e.preventDefault();
    const q = parseInt(addQty, 10);
    if (Number.isNaN(q) || q < 0) return;
    setBusy(true);
    setLoadErr(null);
    try {
      await invoke("add_inventory_item", {
        division: addDivision.trim(),
        item_name: addName.trim(),
        quantity: q,
        category: addCategory,
      });
      setAddOpen(false);
      setAddDivision("");
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
          <p className="text-muted small qm-inventory-lead">
            Stock is grouped by category (food, water, weapons, medical, and other supply classes).
          </p>
          {items.length === 0 ? (
            <p className="text-muted">No inventory rows.</p>
          ) : (
            <>
            {CATEGORY_ORDER.map((cat) => {
              const group = groupedItems.get(cat);
              if (!group || group.length === 0) return null;
              return (
                <div key={cat} className="quartermaster-category-block">
                  <h4 className="quartermaster-category-title">{categoryLabel(cat)}</h4>
                  <div className="quartermaster-table-wrap">
                    <table className="quartermaster-table">
                      <thead>
                        <tr>
                          <th>Division</th>
                          <th>Item</th>
                          <th>Qty</th>
                          <th>Updated</th>
                          {!hideEdits ? <th>Actions</th> : null}
                        </tr>
                      </thead>
                      <tbody>
                        {group.map((row) => {
                          const w = rowWritable(row.division);
                          return (
                            <tr key={row.id}>
                              <td>{row.division}</td>
                              <td>{row.item_name}</td>
                              <td>{row.quantity}</td>
                              <td className="text-muted">{new Date(row.last_updated).toLocaleString()}</td>
                              {!hideEdits ? (
                                <td>
                                  <button
                                    type="button"
                                    className="secondary small-btn"
                                    disabled={!w || busy}
                                    title={!w ? "Not authorized for this division" : "Edit line"}
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
            {extraCategories.map((cat) => {
              const group = groupedItems.get(cat);
              if (!group || group.length === 0) return null;
              return (
                <div key={cat} className="quartermaster-category-block">
                  <h4 className="quartermaster-category-title">{categoryLabel(cat)}</h4>
                  <div className="quartermaster-table-wrap">
                    <table className="quartermaster-table">
                      <thead>
                        <tr>
                          <th>Division</th>
                          <th>Item</th>
                          <th>Qty</th>
                          <th>Updated</th>
                          {!hideEdits ? <th>Actions</th> : null}
                        </tr>
                      </thead>
                      <tbody>
                        {group.map((row) => {
                          const w = rowWritable(row.division);
                          return (
                            <tr key={row.id}>
                              <td>{row.division}</td>
                              <td>{row.item_name}</td>
                              <td>{row.quantity}</td>
                              <td className="text-muted">{new Date(row.last_updated).toLocaleString()}</td>
                              {!hideEdits ? (
                                <td>
                                  <button
                                    type="button"
                                    className="secondary small-btn"
                                    disabled={!w || busy}
                                    title={!w ? "Not authorized for this division" : "Edit line"}
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
                  {r >= 5 ? <th>Annotate</th> : null}
                </tr>
              </thead>
              <tbody>
                {logs.length === 0 ? (
                  <tr>
                    <td colSpan={r >= 5 ? 7 : 6} className="text-muted">
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
                Division
                <input
                  className="lock-screen-input"
                  value={addDivision}
                  onChange={(e) => setAddDivision(e.target.value)}
                  required
                  disabled={busy}
                />
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
              {editRow.item_name} ({editRow.division})
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
