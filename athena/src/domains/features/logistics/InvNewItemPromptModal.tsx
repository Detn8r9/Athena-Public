import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";

export type NewItemLine = {
  lineId: string | null;
  proposalId: string | null;
  itemName: string | null;
  directorate: string | null;
  delta: number | null;
};

type Props = {
  open: boolean;
  lines: NewItemLine[];
  busy: boolean;
  canApply: boolean;
  onClose: () => void;
  onAdd: (proposalId: string) => void;
  onSkip: () => void;
};

export function InvNewItemPromptModal({ open, lines, busy, canApply, onClose, onAdd, onSkip }: Props) {
  if (!open || lines.length === 0) return null;
  const pid = lines[0]?.proposalId;
  return (
    <div className="modal-backdrop" onClick={() => !busy && onClose()}>
      <div className="qm-modal glass-panel" onClick={(e) => e.stopPropagation()}>
        <h3>New inventory items</h3>
        <p className="text-muted small">
          The log suggests items not yet in your directorate inventory. Add them to stock, or skip to keep only the log
          entry.
        </p>
        <ul style={{ margin: "0.6rem 0", paddingLeft: "1.1rem" }}>
          {lines.map((l, i) => (
            <li key={l.lineId ?? i}>
              <strong>{l.itemName ?? "Item"}</strong>
              {l.directorate ? ` · ${l.directorate}` : null}
              {l.delta != null ? ` · qty ${l.delta > 0 ? "+" : ""}${l.delta}` : null}
            </li>
          ))}
        </ul>
        <div className="qm-modal-actions">
          <button type="button" className="secondary" onClick={onSkip} disabled={busy}>
            Skip all
          </button>
          <button
            type="button"
            className="glass-action-btn"
            disabled={busy || !canApply || !pid}
            title={!canApply ? "Rank 3+ required to add inventory" : "Apply proposal"}
            onClick={() => {
              if (pid) onAdd(pid);
            }}
          >
            {busy ? "…" : "Add all"}
          </button>
        </div>
      </div>
    </div>
  );
}

export async function applyInventoryProposal(proposalId: string): Promise<void> {
  await invoke("apply_inventory_proposal", { proposal_id: proposalId });
}
