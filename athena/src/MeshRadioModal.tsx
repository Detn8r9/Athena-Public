import React, { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface MeshRadioMessage {
  id: string;
  senderId: number;
  text: string;
  at: number;
}

/** Primary Meshtastic channel (broadcast visible to all nodes on the network). */
const PRIMARY_CHANNEL = 0;

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
        channel: PRIMARY_CHANNEL,
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
              USB LoRa radio: send and receive <strong>Primary channel</strong> broadcast text (visible to all nodes on
              that channel). Plug in your device; Athena listens in the background. Encrypted packets are ignored.
            </p>
            <p className="library-desc mesh-radio-desc">
              With <strong>Settings → Features → Auto-reply to mesh messages</strong> on, other nodes&apos; broadcast
              messages become AI prompts; Athena replies by <strong>direct message</strong> to each sender and saves a{" "}
              <strong>private chat</strong> per node (<code>Athena (private) · …</code> in the sidebar)—separate from
              this public mesh log.
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
                  Use <strong>Send test ping</strong> (or type your own text) on the <strong>same Primary channel</strong> as
                  in the phone app (channel 0). Your message is broadcast from this node over LoRa.
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
            {error != null && (
              <p className="mesh-radio-error" role="alert">
                {error}
              </p>
            )}
            <div className="mesh-radio-options">
              <label className="mesh-radio-option">
                <input type="checkbox" checked={wantAck} onChange={(e) => setWantAck(e.target.checked)} />
                Request ACK
              </label>
              <span className="mesh-radio-primary-badge" title="Broadcasts use the Primary channel (channel 0)">
                Primary channel
              </span>
            </div>
            <textarea
              className="mesh-radio-compose"
              rows={2}
              placeholder="Broadcast on Primary channel…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void handleSend();
                }
              }}
              disabled={sending}
              aria-label="Meshtastic message text"
            />
            <div className="row mesh-radio-send-row mesh-radio-send-actions">
              <button type="button" onClick={() => void handleSend()} disabled={sending || !input.trim()}>
                {sending ? "Sending…" : "Send on mesh"}
              </button>
              <button type="button" className="secondary" onClick={() => void handleSendTestPing()} disabled={sending}>
                Send test ping
              </button>
            </div>
          </section>

          <section className="library-section mesh-radio-log-section" aria-label="Received mesh messages">
            <h3>Received · Primary broadcast</h3>
            <div className="mesh-radio-log" role="log" aria-live="polite">
              {messages.length === 0 ? (
                <p className="text-muted mesh-radio-log-empty">
                  No Primary-channel broadcasts yet. Other nodes&apos; public messages on channel 0 appear here.
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
                        · node {m.senderId}
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
