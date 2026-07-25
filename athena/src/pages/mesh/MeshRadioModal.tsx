import React, { useCallback, useRef, useState } from "react";
import { athenaInvoke as invoke, canUseHostCapability } from "../../shared/ipc/athenaIpc";

export interface MeshRadioMessage {
  id: string;
  senderId: number;
  text: string;
  at: number;
  channel: number;
}

/** Default channel for authenticated ops chatter. */
const OPS_CHANNEL = 1;

interface MeshRadioModalProps {
  open: boolean;
  onClose: () => void;
  messages: MeshRadioMessage[];
}

export function MeshRadioModal({ open, onClose, messages }: MeshRadioModalProps) {
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [wantAck, setWantAck] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);
  const meshUsbOk = canUseHostCapability("meshUsb");


  const scrollLog = useCallback(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  React.useEffect(() => {
    if (open) scrollLog();
  }, [open, messages.length, scrollLog]);

  React.useEffect(() => {
    if (!open) setHelpOpen(false);
  }, [open]);

  async function invokeSend(text: string) {
    setError(null);
    setSending(true);
    try {
      await invoke("send_mesh_message", {
        text,
        wantAck,
        channel: OPS_CHANNEL,
      });
    } catch (e) {
      setError(e != null ? String(e) : "Send failed");
    } finally {
      setSending(false);
    }
  }

  async function handleSend() {
    const text = input.trim();
    if (!text || sending) return;
    await invokeSend(text);
    setInput("");
  }

  async function handleSendTestPing() {
    if (sending) return;
    const stamp = new Date().toLocaleString(undefined, {
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    });
    await invokeSend(`Athena test ${stamp}`);
  }

  if (!open) return null;

  return (
    <div className="modal-backdrop mesh-radio-backdrop" onClick={onClose}>
      <div className="library-modal mesh-radio-modal glass-panel" onClick={(e) => e.stopPropagation()}>
        <div className="library-header mesh-radio-header">
          <h2>Meshtastic</h2>
          <div className="mesh-radio-header-actions">
            <button
              type="button"
              className={`mesh-radio-help-toggle${helpOpen ? " is-open" : ""}`}
              onClick={() => setHelpOpen((v) => !v)}
              aria-expanded={helpOpen}
              aria-controls="mesh-radio-help-panel"
              title="About mesh & testing"
            >
              ?
            </button>
            <button type="button" className="close-btn" onClick={onClose} aria-label="Close Meshtastic">
              ×
            </button>
          </div>
        </div>

        {helpOpen && (
          <div id="mesh-radio-help-panel" className="mesh-radio-help-panel" role="region" aria-label="Meshtastic help">
            <p className="library-desc mesh-radio-desc">
              USB LoRa radio: send and receive <strong>General Operations</strong> broadcast text on <strong>channel 1</strong>.
              Plug in your device; Athena listens in the background. Encrypted packets are ignored.
            </p>
            <p className="library-desc mesh-radio-desc">
              With <strong>Settings → Features → Auto-reply to mesh messages</strong> on, other nodes&apos; broadcast
              messages become AI prompts; Athena replies by <strong>direct message</strong> to each sender and saves a{" "}
              <strong>private chat</strong> per node (<code>Athena (private) · …</code> in the sidebar)—separate from
              this operations broadcast log.
            </p>
            <section className="library-section mesh-radio-test-tips" aria-label="Testing with your phone">
              <h3>Testing with the phone app</h3>
              <p className="text-muted small">
                The radio usually allows <strong>one</strong> data link at a time: <strong>USB (Athena)</strong> or{" "}
                <strong>BLE (phone)</strong>. If the phone disconnects when Athena opens, that&apos;s normal—try the order
                below.
              </p>
              <ol className="mesh-radio-test-steps">
                <li>
                  Plug USB into your Mac and open Athena; wait until the terminal shows{" "}
                  <code>[mesh] serial connected and configured</code>.
                </li>
                <li>
                  Open the Meshtastic app on your phone. If BLE connects while Athena is running, great. If not, fully quit
                  Athena and confirm the phone can connect—then you know the limit is USB vs BLE sharing.
                </li>
                <li>
                  Use <strong>Send test ping</strong> (or type your own text) on the <strong>same channel</strong> as the
                  operations channel configured in the phone app (channel 1). Your message is broadcast from this node over LoRa.
                </li>
                <li>
                  On the phone, open the <strong>same channel / Messages</strong> view. You should see this node&apos;s
                  outgoing text when BLE is linked to the same radio. If USB blocks BLE, quit Athena and use the phone alone
                  to verify the mesh; use Athena alone to verify sends from the desktop.
                </li>
              </ol>
            </section>
          </div>
        )}

        <div className="mesh-radio-body">
          <section className="library-section mesh-radio-send-section" aria-label="Send mesh message">
            {!meshUsbOk ? (
              <p className="text-muted small" style={{ marginTop: 0 }}>
                Mesh USB send is available on the desktop host only. Status and received logs may still appear when the
                host is connected.
              </p>
            ) : null}
            {error != null && (
              <p className="mesh-radio-error" role="alert">
                {error}
              </p>
            )}
            <div className="mesh-radio-options">
              <label className="mesh-radio-option">
                <input
                  type="checkbox"
                  checked={wantAck}
                  onChange={(e) => setWantAck(e.target.checked)}
                  disabled={!meshUsbOk}
                />
                Request ACK
              </label>
              <span className="mesh-radio-primary-badge" title="Broadcasts use channel 1 (General Operations)">
                General Ops (ch 1)
              </span>
            </div>
            <textarea
              className="mesh-radio-compose"
              rows={2}
              placeholder="Broadcast on General Ops (ch 1)…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  if (meshUsbOk) void handleSend();
                }
              }}
              disabled={sending || !meshUsbOk}
              aria-label="Meshtastic message text"
            />
            <div className="row mesh-radio-send-row mesh-radio-send-actions">
              <button
                type="button"
                onClick={() => void handleSend()}
                disabled={sending || !input.trim() || !meshUsbOk}
              >
                {sending ? "Sending…" : "Send on mesh"}
              </button>
              <button
                type="button"
                className="secondary"
                onClick={() => void handleSendTestPing()}
                disabled={sending || !meshUsbOk}
              >
                Send test ping
              </button>
            </div>
          </section>

          <section className="library-section mesh-radio-log-section" aria-label="Received mesh messages">
            <h3>Received · General Ops broadcast (ch 1)</h3>
            <div className="mesh-radio-log" role="log" aria-live="polite">
              {messages.length === 0 ? (
                <p className="text-muted mesh-radio-log-empty">
                  No General Ops broadcasts yet. Other nodes&apos; public messages on channel 1 appear here.
                </p>
              ) : (
                <ul className="mesh-radio-message-list">
                  {messages.map((m) => (
                    <li key={m.id} className="mesh-radio-message">
                      <span className="mesh-radio-message-meta">
                        {new Date(m.at).toLocaleTimeString(undefined, {
                          hour: "numeric",
                          minute: "2-digit",
                          second: "2-digit",
                        })}{" "}
                        · node {m.senderId} · ch {m.channel}
                      </span>
                      <div className="mesh-radio-message-text">{m.text}</div>
                    </li>
                  ))}
                </ul>
              )}
              <div ref={logEndRef} />
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
