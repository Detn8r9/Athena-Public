import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Inbox } from "lucide-react";

type Counts = {
  pendingProposals: number;
  newItemPrompts: number;
  suppressedDuplicates: number;
};

export function InvQueueButton() {
  const [counts, setCounts] = useState<Counts | null>(null);
  const [open, setOpen] = useState(false);

  const load = useCallback(() => {
    void invoke<Counts>("inv_queue_count")
      .then((c) => setCounts(c))
      .catch(() => setCounts(null));
  }, []);

  useEffect(() => {
    load();
    const id = window.setInterval(load, 30_000);
    return () => window.clearInterval(id);
  }, [load]);

  const total = counts
    ? counts.pendingProposals + counts.newItemPrompts + counts.suppressedDuplicates
    : 0;

  return (
    <>
      <button
        type="button"
        className="llh-tab"
        onClick={() => {
          setOpen(true);
          load();
        }}
        title="Inventory queue"
        aria-label="Open inventory queue"
      >
        <Inbox size={18} aria-hidden />
        Inv. Queue
        {total > 0 ? (
          <span
            aria-label={`${total} queue items`}
            style={{
              marginLeft: 6,
              padding: "0.1rem 0.45rem",
              borderRadius: 999,
              fontSize: "0.75rem",
              color: "var(--text)",
              background: "color-mix(in srgb, var(--primary) 20%, transparent)",
              border: "1px solid color-mix(in srgb, var(--primary) 35%, var(--border))",
            }}
          >
            {total}
          </span>
        ) : null}
      </button>
      {open ? (
        <div className="modal-backdrop" onClick={() => setOpen(false)}>
          <div className="qm-modal glass-panel" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 420 }}>
            <h3>Inventory queue</h3>
            {counts ? (
              <ul className="text-muted small" style={{ lineHeight: 1.6 }}>
                <li>Pending proposals: {counts.pendingProposals}</li>
                <li>New item prompts (add_item): {counts.newItemPrompts}</li>
                <li>Suppressed duplicates (24h): {counts.suppressedDuplicates}</li>
              </ul>
            ) : (
              <p className="text-muted small">Could not load counts.</p>
            )}
            <p className="text-muted small">Use the Notes tab to review and apply or reject proposals.</p>
            <div className="qm-modal-actions">
              <button type="button" className="glass-action-btn" onClick={() => setOpen(false)}>
                Close
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
