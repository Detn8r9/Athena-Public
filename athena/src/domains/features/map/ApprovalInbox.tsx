import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

type AiStagedChangeRow = {
  id: string;
  entityType: string;
  action: string;
  targetEntityId?: string | null;
  payloadJson: string;
  diffSummary: string;
  source: string;
  status: string;
  createdAt: string;
};

/**
 * Rank 4+ review queue for `ai_staged_changes`.
 */
export function ApprovalInbox({ open, onClose, onChanged }: { open: boolean; onClose: () => void; onChanged?: () => void }) {
  const [rows, setRows] = useState<AiStagedChangeRow[]>([]);
  const [err, setErr] = useState("");

  const load = useCallback(async () => {
    setErr("");
    try {
      const list = await invoke<AiStagedChangeRow[]>("ai_list_staged", { filters: { status: "pending" } });
      setRows(Array.isArray(list) ? list : []);
    } catch (e) {
      setErr(String(e));
      setRows([]);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void load();
  }, [open, load]);

  if (!open) return null;

  return (
    <div className="approval-inbox-backdrop">
      <div className="glass-panel approval-inbox-modal" style={{ maxWidth: 640, maxHeight: "80vh", overflow: "auto", width: "100%", padding: 16 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3 style={{ margin: 0 }}>Approval inbox</h3>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </div>
        <p className="text-muted small" style={{ marginTop: 8 }}>
          Staged map / inventory / logbook changes. Rank 4+ only.
        </p>
        {err ? <p className="error small">{err}</p> : null}
        <ul style={{ listStyle: "none", margin: "12px 0 0", padding: 0 }}>
          {rows.map((r) => (
            <li
              key={r.id}
              style={{
                borderTop: "1px solid var(--border)",
                padding: "10px 0",
                fontSize: 13,
              }}
            >
              <div>
                <strong>{r.entityType}</strong> · {r.action} · <span className="text-muted">{r.source}</span>
              </div>
              {r.diffSummary ? <p style={{ margin: "4px 0" }}>{r.diffSummary}</p> : null}
              <pre
                style={{
                  fontSize: 11,
                  maxHeight: 100,
                  overflow: "auto",
                  background: "var(--surface-solid, #0f172a)",
                  padding: 8,
                  borderRadius: 6,
                }}
              >
                {r.payloadJson.slice(0, 2000)}
                {r.payloadJson.length > 2000 ? "…" : ""}
              </pre>
              <div style={{ display: "flex", gap: 8, marginTop: 6 }}>
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      await invoke("ai_review_change", { id: r.id, approved: true, review_notes: null });
                      onChanged?.();
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
                  className="secondary"
                  onClick={async () => {
                    try {
                      await invoke("ai_review_change", {
                        id: r.id,
                        approved: false,
                        review_notes: "Rejected from approval inbox",
                      });
                      onChanged?.();
                      void load();
                    } catch (e2) {
                      window.alert(String(e2));
                    }
                  }}
                >
                  Reject
                </button>
              </div>
            </li>
          ))}
        </ul>
        {rows.length === 0 && !err ? <p className="text-muted small">No pending items.</p> : null}
      </div>
    </div>
  );
}
