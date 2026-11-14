import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke, athenaListen as listen } from "../../shared/ipc/athenaIpc";
import "./MeshSecurityPanel.css";
import { Copy, KeyRound, Lock, RefreshCw, ShieldCheck, Unlink } from "lucide-react";

type ChannelStatus = {
  radioConnected: boolean;
  configuredIndex: number | null;
  onRadio: boolean;
  joinUrl: string | null;
  channelUtilization: number | null;
};

type ConnectionSettings = { radioAddress: string | null; linkDays: number };

type LinkView = {
  nodeId: number;
  username: string | null;
  keyFingerprint: string;
  verifiedAt: number;
  expiresAt: number;
};

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function day(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString();
}

/**
 * Remote access over Meshtastic: the private ATHENA channel, how the radio is connected, and
 * which radios are linked. Members reach the station by encrypted direct message; station
 * records travel only on the private channel.
 */
export function MeshSecurityPanel() {
  const [status, setStatus] = useState<ChannelStatus | null>(null);
  const [conn, setConn] = useState<ConnectionSettings | null>(null);
  const [address, setAddress] = useState("");
  const [days, setDays] = useState(30);
  const [links, setLinks] = useState<LinkView[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await invoke<ChannelStatus>("mesh_athena_channel_status"));
    } catch {
      setStatus(null);
    }
    try {
      const c = await invoke<ConnectionSettings>("get_mesh_connection_settings");
      setConn(c);
      setAddress(c.radioAddress ?? "");
      setDays(c.linkDays);
    } catch {
      setConn(null);
    }
    try {
      setLinks(await invoke<LinkView[]>("mesh_list_links"));
    } catch {
      // Below clearance 4 the list is not shown.
      setLinks(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const u = listen("athena:mesh-connection", () => void refresh());
    return () => {
      void u.then((fn) => fn());
    };
  }, [refresh]);

  async function run(action: () => Promise<unknown>, done: string) {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await action();
      setNote(done);
      await refresh();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  const util = status?.channelUtilization;
  const ready = status?.onRadio === true;

  return (
    <section className="mesh-network-hub-status glass-panel mesh-security-panel" aria-labelledby="mesh-security-title">
      <h2 id="mesh-security-title" className="mesh-network-hub-section-title">
        <ShieldCheck size={18} strokeWidth={2} aria-hidden /> Remote access
      </h2>
      <p className="text-muted small">
        Members reach this station from anywhere on the mesh with an <strong>encrypted direct message</strong>{" "}
        (Meshtastic 2.5 or newer). They link their radio once with <code>REGISTER Name PIN</code>, then send{" "}
        <code>?help</code>, questions for the AI, or <code>//op</code> for the operator. Broadcasts and unencrypted
        messages are never acted on, five wrong PINs lock linking for 30 minutes, and each radio is pinned to its key.
      </p>

      {error ? (
        <div className="mesh-profile-manager-error" role="alert">
          {error}
        </div>
      ) : null}
      {note ? (
        <p className="mesh-network-hub-status-ok" role="status">
          {note}
        </p>
      ) : null}

      <h3 className="mesh-security-subtitle">
        <Lock size={16} strokeWidth={2} aria-hidden /> ATHENA channel
      </h3>
      {status === null ? (
        <p className="text-muted small">Sign in to see the channel status.</p>
      ) : ready ? (
        <>
          <p className="mesh-network-hub-status-ok">
            <span className="mesh-network-hub-dot mesh-network-hub-dot-ok" aria-hidden />
            Private channel active (slot {status.configuredIndex}). Logbook, calendar and map reports travel only here.
          </p>
          {status.joinUrl ? (
            <div className="mesh-security-join">
              <p className="text-muted small">
                Members join by opening this link on their phone with the Meshtastic app. It contains the channel key:
                share it only in person or over a channel you trust.
              </p>
              <div className="mesh-security-join-row">
                <input className="mesh-security-join-url" readOnly value={status.joinUrl} aria-label="Channel join link" />
                <button
                  type="button"
                  className="glass-action-btn"
                  onClick={() => void navigator.clipboard?.writeText(status.joinUrl ?? "").then(() => setNote("Join link copied."))}
                >
                  <Copy size={16} strokeWidth={2} aria-hidden /> Copy
                </button>
              </div>
            </div>
          ) : null}
        </>
      ) : (
        <>
          <p className="mesh-network-hub-status-strong">
            {status.radioConnected
              ? "No private channel yet. Station records are not sent until one is set up."
              : "Connect the radio to set up the private channel."}
          </p>
          <button
            type="button"
            className="glass-action-btn"
            disabled={busy || !status.radioConnected}
            onClick={() => void run(() => invoke("mesh_setup_athena_channel"), "Private ATHENA channel created on the radio.")}
          >
            <KeyRound size={16} strokeWidth={2} aria-hidden /> Set up private channel
          </button>
          <p className="text-muted small">
            Adds a channel named ATHENA with a new random key in a free slot. The public channel stays as it is.
          </p>
        </>
      )}
      {typeof util === "number" ? (
        <p className="text-muted small">
          Channel use {util.toFixed(0)}%{util >= 25 ? " — busy; ATHENA is spacing out what it sends." : "."}
        </p>
      ) : null}

      {conn ? (
        <>
          <h3 className="mesh-security-subtitle">Radio connection</h3>
          <form
            className="mesh-security-form"
            onSubmit={(e) => {
              e.preventDefault();
              void run(
                () => invoke("set_mesh_connection_settings", { radioAddress: address.trim() || null, linkDays: days }),
                "Saved. A new radio address applies when the radio link next reconnects (restart ATHENA).",
              );
            }}
          >
            <label className="mesh-profile-field">
              <span>Radio on the network (leave empty for USB)</span>
              <input
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                placeholder="192.168.1.40 (Meshtastic Wi‑Fi, port 4403)"
                inputMode="url"
                autoComplete="off"
              />
            </label>
            <label className="mesh-profile-field">
              <span>Days a linked radio stays linked</span>
              <input
                type="number"
                min={1}
                max={365}
                value={days}
                onChange={(e) => setDays(Math.max(1, Math.min(365, Number(e.target.value) || 30)))}
              />
            </label>
            <button type="submit" className="glass-action-btn" disabled={busy}>
              Save
            </button>
          </form>
        </>
      ) : null}

      {links ? (
        <>
          <h3 className="mesh-security-subtitle">Linked radios</h3>
          {links.length === 0 ? (
            <p className="text-muted small">No radios linked yet.</p>
          ) : (
            <ul className="mesh-profile-list">
              {links.map((l) => {
                const expired = l.expiresAt * 1000 < Date.now();
                return (
                  <li key={l.nodeId} className="mesh-profile-list-item">
                    <div className="mesh-profile-list-main">
                      <span className="mesh-profile-list-name">{l.username ?? `Node ${l.nodeId}`}</span>
                      <span className="mesh-profile-list-meta text-muted">
                        Node {l.nodeId} · key {l.keyFingerprint} · {expired ? "expired" : "until"} {day(l.expiresAt)}
                      </span>
                    </div>
                    <div className="mesh-security-link-actions">
                      <button
                        type="button"
                        className="glass-action-btn"
                        disabled={busy}
                        onClick={() => void run(() => invoke("mesh_renew_radio", { nodeId: l.nodeId }), "Link renewed.")}
                      >
                        <RefreshCw size={16} strokeWidth={2} aria-hidden /> Renew
                      </button>
                      <button
                        type="button"
                        className="glass-action-btn"
                        disabled={busy}
                        onClick={() => {
                          if (window.confirm(`Unlink ${l.username ?? `node ${l.nodeId}`}? They will need their PIN to link again.`)) {
                            void run(() => invoke("mesh_unlink_radio", { nodeId: l.nodeId }), "Radio unlinked.");
                          }
                        }}
                      >
                        <Unlink size={16} strokeWidth={2} aria-hidden /> Unlink
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </>
      ) : null}
    </section>
  );
}
