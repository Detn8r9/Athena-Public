import React, { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";

export type MeshProfileRow = {
  id: string;
  nodeId: number;
  username: string;
  counterSign: string;
  createdAt: number;
};

type Props = {
  onClose: () => void;
  onSaved: () => void;
};

export function MeshProfileManager({ onClose, onSaved }: Props) {
  const [nodeId, setNodeId] = useState("");
  const [username, setUsername] = useState("");
  const [counterSign, setCounterSign] = useState("");
  const [profiles, setProfiles] = useState<MeshProfileRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const list = await invoke<MeshProfileRow[]>("list_mesh_profiles");
      setProfiles(Array.isArray(list) ? list : []);
    } catch (e) {
      setError(e != null ? String(e) : "Failed to load profiles");
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleAdd(ev: React.FormEvent) {
    ev.preventDefault();
    setError(null);
    const n = Number.parseInt(nodeId.trim(), 10);
    if (!Number.isFinite(n) || n < 0) {
      setError("Enter a valid node ID (non-negative integer).");
      return;
    }
    setBusy(true);
    try {
      await invoke("add_mesh_profile", {
        nodeId: n,
        username: username.trim(),
        counterSign: counterSign.trim(),
      });
      setNodeId("");
      setUsername("");
      setCounterSign("");
      await load();
      onSaved();
    } catch (e) {
      setError(e != null ? String(e) : "Save failed");
    } finally {
      setBusy(false);
    }
  }

  async function handleRemove(id: string) {
    setError(null);
    setBusy(true);
    try {
      await invoke("remove_mesh_profile", { id });
      await load();
      onSaved();
    } catch (e) {
      setError(e != null ? String(e) : "Remove failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mesh-profile-manager-overlay" role="dialog" aria-modal="true" aria-labelledby="mesh-profile-manager-title">
      <div className="mesh-profile-manager-panel glass-panel">
        <div className="mesh-profile-manager-head">
          <h2 id="mesh-profile-manager-title">Mesh node profiles</h2>
          <button type="button" className="mesh-profile-manager-close" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <p className="mesh-profile-manager-hint text-muted">
          <strong>Trusted radios:</strong> add each handheld by node ID, display name, and a private counter-sign. Prefer
          this over remote signup when you know who should join. Remote signup (Settings → Mesh) lets strangers register
          with one shared phrase — use only with people you trust.
        </p>
        {error ? <div className="mesh-profile-manager-error" role="alert">{error}</div> : null}
        <form className="mesh-profile-manager-form" onSubmit={handleAdd}>
          <label className="mesh-profile-field">
            <span>Node ID</span>
            <input
              type="number"
              min={0}
              value={nodeId}
              onChange={(e) => setNodeId(e.target.value)}
              placeholder="e.g. 1234567890"
              disabled={busy}
            />
          </label>
          <label className="mesh-profile-field">
            <span>Username</span>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="Callsign or label"
              disabled={busy}
            />
          </label>
          <label className="mesh-profile-field">
            <span>Counter-sign code</span>
            <input
              type="text"
              value={counterSign}
              onChange={(e) => setCounterSign(e.target.value)}
              placeholder="Shared secret phrase"
              disabled={busy}
              autoComplete="off"
            />
          </label>
          <button type="submit" className="mesh-profile-add-btn" disabled={busy}>
            {busy ? "Saving…" : "Save profile"}
          </button>
        </form>
        <ul className="mesh-profile-list">
          {profiles.length === 0 ? (
            <li className="mesh-profile-list-empty text-muted">
              No trusted radios yet. Add a node above, or turn on remote signup in Settings → Mesh if you want phrase-based
              registration.
            </li>
          ) : (
            profiles.map((p) => (
              <li key={p.id} className="mesh-profile-list-item">
                <div className="mesh-profile-list-main">
                  <span className="mesh-profile-list-name">{p.username}</span>
                  <span className="mesh-profile-list-meta text-muted">Node {p.nodeId}</span>
                </div>
                <button
                  type="button"
                  className="mesh-profile-list-remove"
                  onClick={() => void handleRemove(p.id)}
                  disabled={busy}
                  aria-label={`Remove profile ${p.username}`}
                >
                  Remove
                </button>
              </li>
            ))
          )}
        </ul>
      </div>
    </div>
  );
}
