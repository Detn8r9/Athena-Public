import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { Map } from "maplibre-gl";
import { useAuth } from "../../auth/AuthContext";
import { SoftToast } from "../../components/SoftToast";
import { Lock, Navigation2, Pencil, X } from "lucide-react";

type SavedPlaceRow = {
  id: string;
  name: string;
  lat: number;
  lng: number;
  icon: string;
  color: string;
  scope: string;
  ownerPersonnelId?: string;
  createdAt: string;
  updatedAt: string;
};

export function SavedPlacesPanel({
  open,
  onClose,
  map,
  onRouteToLatLng,
}: {
  open: boolean;
  onClose: () => void;
  map: Map | null;
  onRouteToLatLng?: (latLng: { lat: number; lng: number }) => void;
}) {
  const [places, setPlaces] = useState<SavedPlaceRow[]>([]);
  const [err, setErr] = useState("");
  const [query, setQuery] = useState("");
  const [editMode, setEditMode] = useState(false);
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);
  const { user } = useAuth();
  const clearance = user?.clearance_level ?? user?.rank_level ?? 0;

  const load = useCallback(async () => {
    try {
      const list = await invoke<SavedPlaceRow[]>("list_saved_places");
      setPlaces(Array.isArray(list) ? list : []);
      setErr("");
    } catch {
      setPlaces([]);
      setErr("Could not load saved places.");
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void load();
  }, [open, load]);

  useEffect(() => {
    if (!open) return;
    let un: (() => void) | undefined;
    void listen("peer-saved-place-rx", () => {
      void load();
    }).then((x) => {
      un = () => x();
    });
    return () => {
      if (un) un();
    };
  }, [open, load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const base = Array.isArray(places) ? places : [];
    const arr = q.length
      ? base.filter((p) => String(p.name ?? "").toLowerCase().includes(q))
      : base;
    const sortByName = (a: SavedPlaceRow, b: SavedPlaceRow) => a.name.localeCompare(b.name);
    const shared = arr.filter((p) => String(p.scope).toLowerCase() === "shared").sort(sortByName);
    const personal = arr.filter((p) => String(p.scope).toLowerCase() !== "shared").sort(sortByName);
    return { shared, personal };
  }, [places, query]);

  const flyTo = (p: { lat: number; lng: number }) => {
    if (!map) return;
    map.flyTo({ center: [p.lng, p.lat], zoom: Math.max(map.getZoom(), 15), essential: true });
  };

  if (!open) return null;

  return (
    <div
      className="map-saved-popout glass-panel"
      role="dialog"
      style={{
        position: "fixed",
        left: "var(--map-panel-left, 96px)",
        top: "var(--header-offset, 72px)",
        zIndex: 1500,
        minWidth: 300,
        maxHeight: undefined,
        padding: 12,
        pointerEvents: "auto",
      }}
    >
      <div className="map-panel__head" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
        <strong>Saved</strong>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button
            type="button"
            className="secondary small-btn"
            onClick={() => setEditMode((v) => !v)}
            aria-label="Toggle edit mode"
            title="Edit"
          >
            <Pencil size={16} />
            {editMode ? "Done" : "Edit"}
          </button>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close saved places">
            <X size={16} />
          </button>
        </div>
      </div>
      <div className="map-panel__body">
        <input
          type="text"
          className="map-inp"
          placeholder="Search saved places…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          style={{ width: "100%", marginBottom: 10 }}
        />

        {err ? (
          <p className="quartermaster-hub-err" style={{ fontSize: 12, margin: "0 0 10px" }} role="alert">
            {err}
          </p>
        ) : null}

        <div style={{ display: "grid", gap: 10 }}>
        {filtered.shared.length > 0 ? (
          <section>
            <div className="text-muted small" style={{ marginBottom: 6 }}>
              Shared
            </div>
            <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: 6 }}>
              {filtered.shared.map((p) => (
                <li key={p.id}>
                  <div
                    className="map-panel-list-row"
                    role="button"
                    tabIndex={0}
                    onClick={() => flyTo(p)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") flyTo(p);
                    }}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 10,
                      padding: "10px 10px",
                      borderRadius: 14,
                      background: "rgba(255,255,255,0.05)",
                      border: "1px solid rgba(255,255,255,0.10)",
                      cursor: "pointer",
                    }}
                  >
                    <div style={{ minWidth: 0 }}>
                      <div style={{ fontWeight: 600, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {p.name}
                      </div>
                      <div className="text-muted small">
                        {p.lat.toFixed(4)}, {p.lng.toFixed(4)}
                      </div>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <button
                        type="button"
                        className="secondary small-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          onRouteToLatLng?.({ lat: p.lat, lng: p.lng });
                        }}
                        title="Navigate"
                        aria-label="Navigate"
                      >
                        <Navigation2 size={16} />
                      </button>
                      {editMode ? (
                        clearance >= 4 ? (
                          <button
                            type="button"
                            className="secondary small-btn"
                            style={{
                              borderColor: "color-mix(in srgb, var(--warning) 55%, rgba(255,255,255,0.10))",
                              background: "color-mix(in srgb, var(--warning) 14%, rgba(255,255,255,0.06))",
                              color: "var(--warning)",
                            }}
                            onClick={(e) => {
                              e.stopPropagation();
                              void (async () => {
                                try {
                                  await invoke("delete_saved_place", { id: p.id });
                                  await load();
                                  setToast({ tone: "success", message: "Removed." });
                                } catch (e2) {
                                  setToast({ tone: "warning", message: String(e2) });
                                }
                              })();
                            }}
                            title="Remove"
                            aria-label="Remove"
                          >
                            Remove
                          </button>
                        ) : (
                          <button
                            type="button"
                            className="secondary small-btn"
                            disabled
                            onClick={(e) => {
                              e.stopPropagation();
                              setToast({
                                tone: "warning",
                                message: "Action restricted: Clearance Level 4 required to delete operational data.",
                              });
                            }}
                            title="Clearance Level 4 required"
                            aria-label="Clearance Level 4 required"
                          >
                            <Lock size={16} />
                          </button>
                        )
                      ) : null}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        {filtered.personal.length > 0 ? (
          <section>
            <div className="text-muted small" style={{ marginBottom: 6 }}>
              Personal
            </div>
            <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: 6 }}>
              {filtered.personal.map((p) => (
                <li key={p.id}>
                  <div
                    className="map-panel-list-row"
                    role="button"
                    tabIndex={0}
                    onClick={() => flyTo(p)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") flyTo(p);
                    }}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 10,
                      padding: "10px 10px",
                      borderRadius: 14,
                      background: "rgba(255,255,255,0.05)",
                      border: "1px solid rgba(255,255,255,0.10)",
                      cursor: "pointer",
                    }}
                  >
                    <div style={{ minWidth: 0 }}>
                      <div style={{ fontWeight: 600, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {p.name}
                      </div>
                      <div className="text-muted small">
                        {p.lat.toFixed(4)}, {p.lng.toFixed(4)}
                      </div>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <button
                        type="button"
                        className="secondary small-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          onRouteToLatLng?.({ lat: p.lat, lng: p.lng });
                        }}
                        title="Navigate"
                        aria-label="Navigate"
                      >
                        <Navigation2 size={16} />
                      </button>
                      {editMode ? (
                        clearance >= 4 ? (
                          <button
                            type="button"
                            className="secondary small-btn"
                            style={{
                              borderColor: "color-mix(in srgb, var(--warning) 55%, rgba(255,255,255,0.10))",
                              background: "color-mix(in srgb, var(--warning) 14%, rgba(255,255,255,0.06))",
                              color: "var(--warning)",
                            }}
                            onClick={(e) => {
                              e.stopPropagation();
                              void (async () => {
                                try {
                                  await invoke("delete_saved_place", { id: p.id });
                                  await load();
                                  setToast({ tone: "success", message: "Removed." });
                                } catch (e2) {
                                  setToast({ tone: "warning", message: String(e2) });
                                }
                              })();
                            }}
                            title="Remove"
                            aria-label="Remove"
                          >
                            Remove
                          </button>
                        ) : (
                          <button
                            type="button"
                            className="secondary small-btn"
                            disabled
                            onClick={(e) => {
                              e.stopPropagation();
                              setToast({
                                tone: "warning",
                                message: "Action restricted: Clearance Level 4 required to delete operational data.",
                              });
                            }}
                            title="Clearance Level 4 required"
                            aria-label="Clearance Level 4 required"
                          >
                            <Lock size={16} />
                          </button>
                        )
                      ) : null}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        {filtered.shared.length === 0 && filtered.personal.length === 0 ? (
          <p className="text-muted small" style={{ margin: 0 }}>
            No saved places yet.
          </p>
        ) : null}
        </div>
      </div>

      <SoftToast
        open={toast != null}
        tone={toast?.tone ?? "success"}
        message={toast?.message ?? ""}
        onClose={() => setToast(null)}
      />
    </div>
  );
}
