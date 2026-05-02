import { invoke } from "@tauri-apps/api/core";
import { Hexagon, Lock, Plus, Trash2, Eye, EyeOff } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useAuth } from "../../../domains/auth/AuthContext";
import { SoftToast } from "../../../ui/components/SoftToast";

export type TerritoryRow = {
  id: string;
  name: string;
  status: string;
  color: string;
  geometryJson: string;
};

const STATUS_ORDER = ["friendly", "enemy", "disputed", "unexplored"] as const;
const STATUS_LABEL: Record<string, string> = {
  friendly: "Friendly",
  enemy: "Enemy",
  disputed: "Disputed",
  unexplored: "Unexplored",
};

export function TerritoriesPanel({
  open,
  onClose,
  rows,
  onRefresh,
  territoriesVisible,
  onTerritoriesVisibleChange,
  onStartDraw,
  onFlyTo,
}: {
  open: boolean;
  onClose: () => void;
  rows: TerritoryRow[];
  onRefresh: () => void;
  territoriesVisible: boolean;
  onTerritoriesVisibleChange: (v: boolean) => void;
  onStartDraw: () => void;
  onFlyTo: (id: string) => void;
}) {
  const { user } = useAuth();
  const clearance = user?.clearance_level ?? user?.rank_level ?? 0;
  const canWrite = clearance >= 4;
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);

  const byStatus = useMemo(() => {
    const m: Record<string, TerritoryRow[]> = {
      friendly: [],
      enemy: [],
      disputed: [],
      unexplored: [],
    };
    for (const t of rows) {
      const k = t.status in m ? t.status : "unexplored";
      (m[k] ?? m.unexplored)!.push(t);
    }
    return m;
  }, [rows]);

  const onDelete = useCallback(
    async (id: string) => {
      if (!canWrite) {
        setToast({
          tone: "warning",
          message: "Action restricted: Clearance Level 4 required to delete operational data.",
        });
        return;
      }
      try {
        await invoke("delete_territory", { id });
        onRefresh();
      } catch (e) {
        setToast({ tone: "warning", message: String(e) });
      }
    },
    [canWrite, onRefresh],
  );

  if (!open) return null;

  return (
    <div className="map-territories-popout glass-panel" role="dialog" aria-label="Territories" style={{ pointerEvents: "auto" }}>
      <div className="map-panel__head">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 8, marginBottom: 8 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Hexagon size={18} />
            <strong>Territories</strong>
          </div>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, alignItems: "center" }}>
          <button
            type="button"
            className="secondary small-btn"
            onClick={() => onTerritoriesVisibleChange(!territoriesVisible)}
            title={territoriesVisible ? "Hide on map" : "Show on map"}
          >
            {territoriesVisible ? <Eye size={14} style={{ marginRight: 6 }} /> : <EyeOff size={14} style={{ marginRight: 6 }} />}
            {territoriesVisible ? "Visible" : "Hidden"}
          </button>
          <button
            type="button"
            className="primary small-btn"
            onClick={() => {
              onStartDraw();
              onClose();
            }}
          >
            <Plus size={14} style={{ marginRight: 6, verticalAlign: "middle" }} />
            Draw new
          </button>
        </div>
      </div>
      <div className="map-panel__body">
        <p className="text-muted small" style={{ margin: "0 0 10px" }}>
          Click the map to add corners; double-click to finish and name the area. Routing avoids <strong>enemy</strong> zones when Valhalla
          is running.
        </p>
        {STATUS_ORDER.map((st) => {
          const list = byStatus[st] ?? [];
          if (list.length === 0) return null;
          return (
            <div key={st} style={{ marginBottom: 14 }}>
              <div className="text-muted small" style={{ marginBottom: 6, fontWeight: 600 }}>
                {STATUS_LABEL[st] ?? st}
              </div>
              <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: 8 }}>
                {list.map((t) => (
                  <li
                    key={t.id}
                    className="map-territory-row"
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      padding: "8px 10px",
                      borderRadius: 12,
                      border: "1px solid rgba(255,255,255,0.1)",
                      background: "rgba(255,255,255,0.04)",
                      cursor: "pointer",
                    }}
                    onClick={() => onFlyTo(t.id)}
                  >
                    <span
                      title={t.status}
                      style={{
                        width: 12,
                        height: 12,
                        borderRadius: 4,
                        background: t.color,
                        flexShrink: 0,
                        boxShadow: "0 0 0 1px rgba(0,0,0,0.35)",
                      }}
                    />
                    <span style={{ flex: 1, minWidth: 0, fontSize: 13 }} className="ellipsis">
                      {t.name}
                    </span>
                    {canWrite ? (
                      <button
                        type="button"
                        className="icon-btn-ghost"
                        title="Delete"
                        aria-label="Delete territory"
                        onClick={(e) => {
                          e.stopPropagation();
                          void onDelete(t.id);
                        }}
                      >
                        <Trash2 size={16} />
                      </button>
                    ) : (
                      <span title="Clearance 4+ required" style={{ opacity: 0.45 }}>
                        <Lock size={16} />
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          );
        })}
        {rows.length === 0 ? <p className="text-muted small">No territories yet. Use Draw new to add one.</p> : null}
      </div>
      <SoftToast
        open={toast != null}
        tone={toast?.tone ?? "warning"}
        message={toast?.message ?? ""}
        onClose={() => setToast(null)}
      />
    </div>
  );
}
