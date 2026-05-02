import { useEffect } from "react";
import { APP_VIEW_LABELS, APP_VIEWS } from "../../app/appView";

interface ShortcutsModalProps {
  open: boolean;
  onClose: () => void;
  /** e.g. "⌘" on macOS, "Ctrl" elsewhere */
  modLabel: string;
}

export function ShortcutsModal({ open, onClose, modLabel }: ShortcutsModalProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="modal-backdrop shortcuts-modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="shortcuts-modal-title"
      onClick={onClose}
    >
      <div className="shortcuts-modal-panel glass-panel" onClick={(e) => e.stopPropagation()}>
        <div className="shortcuts-modal-header">
          <h2 id="shortcuts-modal-title">Keyboard shortcuts</h2>
          <button type="button" className="secondary" onClick={onClose} aria-label="Close shortcuts">
            Close
          </button>
        </div>
        <p className="text-muted small" style={{ marginTop: 0 }}>
          Press <kbd>?</kbd> or <kbd>{modLabel}+/</kbd> to open this panel. Press <kbd>Esc</kbd> to close.
        </p>
        <div className="shortcuts-modal-scroll">
          <table className="shortcuts-table">
            <thead>
              <tr>
                <th scope="col">Action</th>
                <th scope="col">Shortcut</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Show this help</td>
                <td>
                  <kbd>?</kbd> or <kbd>{modLabel}+/</kbd>
                </td>
              </tr>
              <tr>
                <td>New chat</td>
                <td>
                  Sidebar: <strong>New chat</strong>
                </td>
              </tr>
              <tr>
                <td>Send message</td>
                <td>
                  <kbd>Enter</kbd> (chat input)
                </td>
              </tr>
              <tr>
                <td>Open settings</td>
                <td>
                  Header: <strong>Settings</strong>
                </td>
              </tr>
              <tr>
                <td>Open help</td>
                <td>
                  Header: <strong>Help</strong> · Chat: <code>/athena</code>
                </td>
              </tr>
              {APP_VIEWS.map((view) => (
                <tr key={view}>
                  <td>Go to {APP_VIEW_LABELS[view]}</td>
                  <td>Header navigation or dashboard</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
