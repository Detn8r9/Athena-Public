import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAuth } from "./AuthContext";

type OobeFirstAccountProps = {
  lexiconThemeId: string;
  onCompleted: () => void;
};

export function OobeFirstAccount({ lexiconThemeId, onCompleted }: OobeFirstAccountProps) {
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [pin, setPin] = useState("");
  const [pinConfirm, setPinConfirm] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [architectKey, setArchitectKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = useCallback(async () => {
    setError(null);
    const u = username.trim();
    const p = pin.trim();
    const pc = pinConfirm.trim();
    if (!u) {
      setError("Enter a username.");
      return;
    }
    if (p.length < 4) {
      setError("PIN must be at least 4 characters.");
      return;
    }
    if (p !== pc) {
      setError("PIN and confirmation do not match.");
      return;
    }
    const keyTrim = architectKey.trim();
    const architectKeyArg = advancedOpen && keyTrim.length > 0 ? keyTrim : null;
    setBusy(true);
    try {
      await invoke("oobe_create_account", {
        username: u,
        pin: p,
        architectKey: architectKeyArg,
      });
      await login(u, p);
      onCompleted();
    } catch (e) {
      setError(e != null ? String(e) : "Setup failed.");
    } finally {
      setBusy(false);
    }
  }, [username, pin, pinConfirm, advancedOpen, architectKey, login, onCompleted]);

  return (
    <div className="app" data-theme={lexiconThemeId} style={{ padding: 18 }}>
      <div className="glass-panel" style={{ padding: 20, maxWidth: 480, margin: "32px auto", display: "grid", gap: "0.75rem" }}>
        <h2 style={{ margin: 0 }}>First Run — Create operator</h2>
        <p className="text-muted" style={{ margin: 0, lineHeight: 1.45 }}>
          Create the first local operator account. By default this account is <strong>Level 4 (Station Admin)</strong>. Optional
          advanced controls can grant <strong>Level 5 (Architect)</strong> when a valid override key is supplied.
        </p>
        <label className="text-muted small" style={{ display: "grid", gap: 4 }}>
          Username (lock-screen login name)
          <input
            className="sidebar-search"
            autoComplete="off"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            disabled={busy}
          />
        </label>
        <label className="text-muted small" style={{ display: "grid", gap: 4 }}>
          PIN
          <input
            className="sidebar-search"
            type="password"
            autoComplete="new-password"
            value={pin}
            onChange={(e) => setPin(e.target.value)}
            disabled={busy}
          />
        </label>
        <label className="text-muted small" style={{ display: "grid", gap: 4 }}>
          Confirm PIN
          <input
            className="sidebar-search"
            type="password"
            autoComplete="new-password"
            value={pinConfirm}
            onChange={(e) => setPinConfirm(e.target.value)}
            disabled={busy}
          />
        </label>
        <button type="button" className="secondary" onClick={() => setAdvancedOpen((v) => !v)} disabled={busy}>
          {advancedOpen ? "Hide advanced options" : "Advanced options"}
        </button>
        {advancedOpen ? (
          <label className="text-muted small" style={{ display: "grid", gap: 4 }}>
            Architect override key
            <input
              className="sidebar-search"
              type="password"
              autoComplete="off"
              value={architectKey}
              onChange={(e) => setArchitectKey(e.target.value)}
              disabled={busy}
              placeholder="Optional — grants Level 5 when correct"
            />
          </label>
        ) : null}
        {error ? (
          <p className="error" role="alert" style={{ margin: 0 }}>
            {error}
          </p>
        ) : null}
        <button type="button" className="primary" onClick={() => void submit()} disabled={busy}>
          {busy ? "Creating…" : "Create account and continue"}
        </button>
      </div>
    </div>
  );
}
