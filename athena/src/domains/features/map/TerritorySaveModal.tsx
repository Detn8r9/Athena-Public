import { motion } from "framer-motion";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";

const STATUS_PRESETS: { id: "friendly" | "enemy" | "disputed" | "unexplored"; label: string; color: string }[] = [
  { id: "friendly", label: "Friendly", color: "#3b82f6" },
  { id: "enemy", label: "Enemy", color: "#ef4444" },
  { id: "disputed", label: "Disputed", color: "#f59e0b" },
  { id: "unexplored", label: "Unexplored", color: "#94a3b8" },
];

export function TerritorySaveModal({
  open,
  onClose,
  geometryJson,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  geometryJson: string;
  onSaved: () => void;
}) {
  const [name, setName] = useState("");
  const [status, setStatus] = useState<(typeof STATUS_PRESETS)[number]["id"]>("friendly");
  const [color, setColor] = useState(STATUS_PRESETS[0]!.color);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  useEffect(() => {
    if (!open) return;
    setName("");
    setStatus("friendly");
    setColor(STATUS_PRESETS[0]!.color);
    setErr("");
  }, [open]);

  useEffect(() => {
    const p = STATUS_PRESETS.find((x) => x.id === status);
    if (p) setColor(p.color);
  }, [status]);

  if (!open) return null;

  const modal = (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <motion.div
        className="library-modal glass-panel"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Save territory"
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: 10 }}
        transition={{ type: "spring", stiffness: 360, damping: 32 }}
        style={{ maxWidth: 400, width: "92vw", padding: 20 }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
          <strong>Save territory</strong>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <p className="text-muted small" style={{ margin: "0 0 12px" }}>
          Name this area and set how it is classified. Edges use your color with light fill.
        </p>
        {err ? (
          <p style={{ color: "#fca5a5", fontSize: 13, margin: "0 0 8px" }} role="alert">
            {err}
          </p>
        ) : null}
        <label className="small" style={{ display: "block", marginBottom: 6 }}>
          Name
        </label>
        <input
          className="map-territory-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Forward OP — Sector 7"
          style={{ width: "100%", marginBottom: 12, padding: "10px 12px", borderRadius: 12 }}
        />
        <div style={{ marginBottom: 10 }}>
          <span className="small text-muted" style={{ display: "block", marginBottom: 6 }}>
            Status
          </span>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
            {STATUS_PRESETS.map((p) => (
              <button
                key={p.id}
                type="button"
                className={"secondary small-btn" + (status === p.id ? " is-active" : "")}
                onClick={() => setStatus(p.id)}
              >
                {p.label}
              </button>
            ))}
          </div>
        </div>
        <label className="small" style={{ display: "block", marginBottom: 6 }}>
          Color
        </label>
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
          style={{ width: "100%", height: 40, border: "none", background: "transparent", marginBottom: 16, cursor: "pointer" }}
        />
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="secondary small-btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="primary small-btn"
            disabled={busy}
            onClick={async () => {
              const n = name.trim();
              if (n.length < 1) {
                setErr("Enter a name.");
                return;
              }
              setErr("");
              setBusy(true);
              try {
                await invoke("upsert_territory", {
                  args: {
                    id: null,
                    name: n,
                    status,
                    color,
                    geometryJson,
                  },
                });
                onSaved();
                onClose();
              } catch (e) {
                setErr(String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "Saving…" : "Save territory"}
          </button>
        </div>
      </motion.div>
    </div>
  );

  if (typeof document === "undefined") return null;
  return createPortal(modal, document.body);
}
