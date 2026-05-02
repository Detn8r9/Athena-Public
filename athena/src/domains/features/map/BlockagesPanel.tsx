import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriListen } from "../../../shared/hooks/useTauriListen";

type RouteBlockageRow = {
  id: string;
  geometryJson: string;
  reason: string;
  severity: string;
  modesAffected: string;
  status: string;
  notes: string;
  proposedByPersonnelId: string;
};

/**
 * List / propose / review route blockages (rank-gated in Rust).
 */
export function BlockagesPanel({ onClose, onBlockagesChanged }: { onClose: () => void; onBlockagesChanged: () => void }) {
  const [rows, setRows] = useState<RouteBlockageRow[]>([]);
  const [reason, setReason] = useState("");
  const [severity, setSeverity] = useState<"impassable" | "slow">("slow");
  const [notes, setNotes] = useState("");
  const [err, setErr] = useState("");

  const load = useCallback(async () => {
    try {
      const list = await invoke<RouteBlockageRow[]>("list_route_blockages", { include_rejected: false });
      setRows(Array.isArray(list) ? list : []);
    } catch {
      setRows([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useTauriListen("peer-blockage-rx", {
    handler: () => {
      void load();
    },
  });

  return (
    <div
      className="map-blockages-panel glass-panel"
      style={{
        position: "absolute",
        right: 12,
        bottom: 12,
        zIndex: 4,
        width: 360,
        maxWidth: "94vw",
        maxHeight: "60vh",
        overflow: "auto",
        padding: 12,
        pointerEvents: "auto",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <strong>Route blockages</strong>
        <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
          <X size={16} />
        </button>
      </div>
      <p style={{ fontSize: 12, opacity: 0.85, margin: "6px 0" }}>
        Propose (rank 2+): a point at a fixed test location. Approve (rank 4+): use buttons on pending rows.
      </p>
      {err ? <p className="quartermaster-hub-err" style={{ fontSize: 12, margin: 0 }} role="alert">{err}</p> : null}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          setErr("");
          const g = {
            type: "Point" as const,
            coordinates: [-98.35, 39.5] as [number, number],
          };
          try {
            await invoke("propose_route_blockage", {
              geometry_json: JSON.stringify(g),
              reason: reason.trim() || "Ad hoc test",
              severity,
              modes_affected: "walk,drive,bike,offroad",
              expires_at: null,
              notes: notes.trim(),
            });
            setReason("");
            setNotes("");
            onBlockagesChanged();
            void load();
          } catch (e2) {
            setErr(String(e2));
          }
        }}
        style={{ display: "grid", gap: 6, marginBottom: 12, fontSize: 12 }}
      >
        <input placeholder="Reason" value={reason} onChange={(e) => setReason(e.target.value)} />
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <span>Severity</span>
          <label>
            <input type="radio" name="sev" checked={severity === "impassable"} onChange={() => setSeverity("impassable")} />
            impassable
          </label>
          <label>
            <input type="radio" name="sev" checked={severity === "slow"} onChange={() => setSeverity("slow")} />
            slow
          </label>
        </div>
        <textarea
          placeholder="Optional notes (rank 2 proposal)"
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          rows={2}
        />
        <button type="submit">Propose test point (Kansas area)</button>
      </form>
      <ul style={{ listStyle: "none", margin: 0, padding: 0, fontSize: 12 }}>
        {rows.map((b) => (
          <li
            key={b.id}
            className="map-panel-list-row"
            style={{ padding: "8px 0", display: "grid", gap: 4 }}
          >
            <div>
              <strong>{b.status}</strong> — {b.severity} — {b.reason}
            </div>
            <div style={{ opacity: 0.8 }}>{b.modesAffected}</div>
            {b.notes ? <div style={{ opacity: 0.7 }}>{b.notes}</div> : null}
            {b.status === "pending" ? (
              <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      await invoke("review_route_blockage", {
                        id: b.id,
                        approved: true,
                        review_notes: null,
                      });
                      onBlockagesChanged();
                      void load();
                    } catch (e2) {
                      window.alert(String(e2));
                    }
                  }}
                >
                  Approve
                </button>
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      await invoke("review_route_blockage", {
                        id: b.id,
                        approved: false,
                        review_notes: "Rejected from map UI",
                      });
                      onBlockagesChanged();
                      void load();
                    } catch (e2) {
                      window.alert(String(e2));
                    }
                  }}
                >
                  Reject
                </button>
              </div>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}
