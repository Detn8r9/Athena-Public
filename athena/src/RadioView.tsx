import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, Clock, Radio, Signal } from "lucide-react";

export interface RadioTelemetryPayload {
  rssi: number;
  snr: number;
  timestamp: string;
}

export interface MeshMessageAckPayload {
  packet_id: string;
  delivered: boolean;
}

export interface MeshMessageRow {
  packet_id: string;
  message: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface RadioChannelRow {
  id: string;
  name: string;
  frequency_mhz: number;
  encryption_key: string;
  created_at: string;
}

interface SendRadioMessageResult {
  packet_id: string;
}

function formatTs(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleTimeString();
}

export function RadioView() {
  const [ports, setPorts] = useState<string[]>([]);
  const [selectedPort, setSelectedPort] = useState("");
  const [lines, setLines] = useState<string[]>([]);
  const [connecting, setConnecting] = useState(false);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [txMessage, setTxMessage] = useState("");
  const [sending, setSending] = useState(false);
  const terminalRef = useRef<HTMLDivElement>(null);

  const [telemetry, setTelemetry] = useState<RadioTelemetryPayload | null>(null);
  const [meshMessages, setMeshMessages] = useState<MeshMessageRow[]>([]);
  const [channels, setChannels] = useState<RadioChannelRow[]>([]);
  const [selectedChannelId, setSelectedChannelId] = useState<string>("");
  const [applyingChannel, setApplyingChannel] = useState(false);
  const [newChannelName, setNewChannelName] = useState("");
  const [newChannelMhz, setNewChannelMhz] = useState("");
  const [newChannelKey, setNewChannelKey] = useState("");
  const [savingChannel, setSavingChannel] = useState(false);

  const loadMeshMessages = useCallback(async () => {
    try {
      const list = await invoke<MeshMessageRow[]>("get_mesh_messages", { limit: 80 });
      setMeshMessages(Array.isArray(list) ? list : []);
    } catch {
      setMeshMessages([]);
    }
  }, []);

  const loadChannels = useCallback(async () => {
    try {
      const list = await invoke<RadioChannelRow[]>("list_radio_channels");
      const next = Array.isArray(list) ? list : [];
      setChannels(next);
      setSelectedChannelId((prev) => {
        if (prev && next.some((c) => c.id === prev)) return prev;
        return next[0]?.id ?? "";
      });
    } catch {
      setChannels([]);
    }
  }, []);

  const loadTelemetryHistory = useCallback(async () => {
    try {
      const samples = await invoke<Array<{ rssi: number; snr: number; timestamp: string }>>("get_radio_telemetry_samples", {
        limit: 1,
      });
      const last = Array.isArray(samples) && samples.length > 0 ? samples[0] : null;
      if (last) {
        setTelemetry({ rssi: last.rssi, snr: last.snr, timestamp: last.timestamp });
      }
    } catch {
      /* ignore */
    }
  }, []);

  async function loadPorts() {
    setError(null);
    try {
      const list = await invoke<string[]>("get_available_ports");
      const next = Array.isArray(list) ? list : [];
      setPorts(next);
      setSelectedPort((prev) => (prev && next.includes(prev) ? prev : next[0] ?? ""));
    } catch (e) {
      setError(e != null ? String(e) : "Could not load serial ports.");
      setPorts([]);
      setSelectedPort("");
    }
  }

  useEffect(() => {
    void loadPorts();
    void loadChannels();
  }, [loadChannels]);

  useEffect(() => {
    const unlisten = listen<string>("radio-rx", (event) => {
      const chunk = event.payload ?? "";
      const incoming = String(chunk).replace(/\r/g, "");
      const split = incoming.split("\n").filter((line) => line.length > 0);
      if (split.length === 0) return;
      setLines((prev) => {
        const merged = [...prev, ...split];
        if (merged.length > 2000) return merged.slice(merged.length - 2000);
        return merged;
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<RadioTelemetryPayload>("radio-telemetry", (event) => {
      const p = event.payload;
      if (!p || typeof p.rssi !== "number" || typeof p.snr !== "number") return;
      setTelemetry({
        rssi: p.rssi,
        snr: p.snr,
        timestamp: typeof p.timestamp === "string" ? p.timestamp : new Date().toISOString(),
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<MeshMessageAckPayload>("mesh-message-ack", () => {
      void loadMeshMessages();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [loadMeshMessages]);

  useEffect(() => {
    const el = terminalRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [lines]);

  useEffect(() => {
    if (!connected) return;
    void loadMeshMessages();
    void loadTelemetryHistory();
    const id = window.setInterval(() => {
      void loadMeshMessages();
    }, 4000);
    return () => clearInterval(id);
  }, [connected, loadMeshMessages, loadTelemetryHistory]);

  const meshByPacketId = useMemo(() => {
    const m = new Map<string, MeshMessageRow>();
    for (const row of meshMessages) {
      m.set(row.packet_id, row);
    }
    return m;
  }, [meshMessages]);

  const txLinesWithStatus = useMemo(() => {
    const out: Array<{ line: string; packetId: string | null; status: "pending" | "delivered" | "unknown" }> = [];
    for (const line of lines) {
      const m = /^TX:\s*packet=([a-f0-9-]{36})\s+/i.exec(line);
      if (m) {
        const pid = m[1];
        const row = meshByPacketId.get(pid);
        const status = row?.status === "delivered" ? "delivered" : row?.status === "pending" ? "pending" : "unknown";
        out.push({ line, packetId: pid, status });
        continue;
      }
      if (line.startsWith("TX:") || line.startsWith("packet=")) {
        const pm = /packet=([a-f0-9-]{36})/i.exec(line);
        if (pm) {
          const pid = pm[1];
          const row = meshByPacketId.get(pid);
          const status = row?.status === "delivered" ? "delivered" : row?.status === "pending" ? "pending" : "unknown";
          out.push({ line, packetId: pid, status });
          continue;
        }
      }
      out.push({ line, packetId: null, status: "unknown" });
    }
    return out;
  }, [lines, meshByPacketId]);

  async function handleConnect() {
    if (!selectedPort || connecting) return;
    setConnecting(true);
    setError(null);
    try {
      const ok = await invoke<boolean>("connect_to_radio", { portName: selectedPort });
      if (!ok) {
        setConnected(false);
        setError("Connection failed.");
      } else {
        setConnected(true);
        setLines((prev) => [...prev, `Connected to ${selectedPort}`]);
        void loadMeshMessages();
        void loadTelemetryHistory();
        void loadChannels();
      }
    } catch (e) {
      setConnected(false);
      setError(e != null ? String(e) : "Connection failed.");
    } finally {
      setConnecting(false);
    }
  }

  async function handleSend() {
    const text = txMessage.trim();
    if (!text || sending) return;
    setSending(true);
    setError(null);
    try {
      const result = await invoke<SendRadioMessageResult>("send_radio_message", { message: text });
      const pid = result?.packet_id ?? "";
      setLines((prev) => [...prev, `TX: packet=${pid} ${text}`]);
      setTxMessage("");
      void loadMeshMessages();
    } catch (e) {
      setError(e != null ? String(e) : "Send failed.");
    } finally {
      setSending(false);
    }
  }

  async function handleApplyChannel() {
    if (!selectedChannelId || applyingChannel || !connected) return;
    setApplyingChannel(true);
    setError(null);
    try {
      await invoke("apply_channel_profile", { channelId: selectedChannelId });
      setLines((prev) => [...prev, `Applied channel profile: ${selectedChannelId}`]);
    } catch (e) {
      setError(e != null ? String(e) : "Could not apply channel profile.");
    } finally {
      setApplyingChannel(false);
    }
  }

  async function handleSaveNewChannel() {
    const name = newChannelName.trim();
    const mhz = Number.parseFloat(newChannelMhz);
    const key = newChannelKey.trim();
    if (!name || !Number.isFinite(mhz) || !key || savingChannel) {
      setError("Channel name, frequency (MHz), and encryption key are required.");
      return;
    }
    setSavingChannel(true);
    setError(null);
    try {
      const id = typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `ch-${Date.now()}`;
      const createdAt = new Date().toISOString();
      await invoke("upsert_radio_channel", {
        channel: {
          id,
          name,
          frequency_mhz: mhz,
          encryption_key: key,
          created_at: createdAt,
        },
      });
      setNewChannelName("");
      setNewChannelMhz("");
      setNewChannelKey("");
      await loadChannels();
      setSelectedChannelId(id);
      setLines((prev) => [...prev, `Saved channel profile: ${name}`]);
    } catch (e) {
      setError(e != null ? String(e) : "Could not save channel.");
    } finally {
      setSavingChannel(false);
    }
  }

  async function handleDeleteChannel() {
    if (!selectedChannelId || !channels.some((c) => c.id === selectedChannelId)) return;
    if (!window.confirm("Delete this channel profile from the database?")) return;
    try {
      await invoke("delete_radio_channel", { id: selectedChannelId });
      await loadChannels();
      setLines((prev) => [...prev, `Deleted channel profile.`]);
    } catch (e) {
      setError(e != null ? String(e) : "Could not delete channel.");
    }
  }

  return (
    <div className="radio-view-root glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.9rem" }}>
      <h2 className="app-view-placeholder-title" style={{ marginBottom: "0.2rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
        <Radio size={22} strokeWidth={2} aria-hidden />
        Radio Terminal
      </h2>

      <div className="radio-view-panels" style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: "0.65rem" }}>
        <div className="glass-panel radio-signal-panel">
          <div className="radio-signal-panel-header">
            <Signal size={16} strokeWidth={2} aria-hidden />
            <span>Signal health</span>
          </div>
          {telemetry ? (
            <dl className="radio-signal-metrics">
              <div>
                <dt>RSSI</dt>
                <dd>{telemetry.rssi} dBm</dd>
              </div>
              <div>
                <dt>SNR</dt>
                <dd>{telemetry.snr.toFixed(1)} dB</dd>
              </div>
              <div className="radio-signal-ts">{formatTs(telemetry.timestamp)}</div>
            </dl>
          ) : (
            <p className="radio-signal-muted">No telemetry yet. Expect serial lines: <code>ATH:SIG rssi=… snr=…</code></p>
          )}
        </div>

        <div className="glass-panel radio-channel-panel">
          <div className="radio-channel-panel-header">Encrypted channel profile</div>
          <div className="radio-channel-row">
            <label className="radio-channel-label" htmlFor="radio-channel-select">
              Profile
            </label>
            <select
              id="radio-channel-select"
              className="theme-select radio-channel-select"
              value={selectedChannelId}
              onChange={(e) => setSelectedChannelId(e.target.value)}
              disabled={channels.length === 0}
            >
              {channels.length === 0 ? (
                <option value="">No profiles — add one below</option>
              ) : (
                channels.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name} ({c.frequency_mhz} MHz)
                  </option>
                ))
              )}
            </select>
          </div>
          <div className="radio-channel-actions">
            <button type="button" className="secondary" onClick={() => void handleApplyChannel()} disabled={!connected || !selectedChannelId || applyingChannel}>
              {applyingChannel ? "Applying…" : "Apply to radio"}
            </button>
            <button type="button" className="secondary" onClick={() => void handleDeleteChannel()} disabled={!selectedChannelId || channels.length === 0}>
              Delete profile
            </button>
          </div>
          <div className="radio-channel-add">
            <div className="radio-channel-add-title">New profile</div>
            <input
              type="text"
              placeholder="Name"
              value={newChannelName}
              onChange={(e) => setNewChannelName(e.target.value)}
              className="radio-channel-input"
              aria-label="Channel name"
            />
            <input
              type="text"
              inputMode="decimal"
              placeholder="Frequency MHz"
              value={newChannelMhz}
              onChange={(e) => setNewChannelMhz(e.target.value)}
              className="radio-channel-input"
              aria-label="Frequency MHz"
            />
            <input
              type="password"
              placeholder="Encryption key"
              value={newChannelKey}
              onChange={(e) => setNewChannelKey(e.target.value)}
              className="radio-channel-input"
              autoComplete="off"
              aria-label="Encryption key"
            />
            <button
              type="button"
              className="glass-action-btn"
              onClick={() => void handleSaveNewChannel()}
              disabled={savingChannel}
            >
              {savingChannel ? "Saving…" : "Save profile"}
            </button>
          </div>
        </div>
      </div>

      <div style={{ display: "flex", gap: "0.6rem", alignItems: "center", flexWrap: "wrap" }}>
        <select
          className="theme-select"
          value={selectedPort}
          onChange={(e) => setSelectedPort(e.target.value)}
          disabled={ports.length === 0 || connecting}
          aria-label="Available serial ports"
          style={{ minWidth: "18rem" }}
        >
          {ports.length === 0 ? (
            <option value="">No serial ports found</option>
          ) : (
            ports.map((port) => (
              <option key={port} value={port}>
                {port}
              </option>
            ))
          )}
        </select>
        <button type="button" onClick={() => void loadPorts()} className="secondary" disabled={connecting}>
          Refresh
        </button>
        <button
          type="button"
          className="glass-action-btn"
          onClick={() => void handleConnect()}
          disabled={!selectedPort || connecting}
        >
          {connecting ? "Connecting..." : connected ? "Connected" : "Connect"}
        </button>
      </div>
      {error ? (
        <p className="error" role="alert" style={{ margin: 0 }}>
          {error}
        </p>
      ) : null}

      <div className="glass-panel radio-mesh-status-panel">
        <div className="radio-mesh-status-title">Outbound packets</div>
        <ul className="radio-mesh-status-list">
          {meshMessages.length === 0 ? (
            <li className="radio-mesh-status-muted">No tracked packets yet.</li>
          ) : (
            meshMessages.slice(0, 24).map((row) => (
              <li key={row.packet_id} className="radio-mesh-status-item">
                <span className="radio-mesh-status-icon" title={row.status}>
                  {row.status === "delivered" ? (
                    <CheckCircle2 size={16} className="radio-icon-delivered" aria-hidden />
                  ) : (
                    <Clock size={16} className="radio-icon-pending" aria-hidden />
                  )}
                </span>
                <span className="radio-mesh-status-text">
                  <span className="radio-mesh-status-msg">{row.message}</span>
                  <span className="radio-mesh-status-meta">
                    {row.status} · {formatTs(row.updated_at)}
                  </span>
                </span>
              </li>
            ))
          )}
        </ul>
      </div>

      <div
        className="glass-panel radio-terminal-scroll"
        ref={terminalRef}
        style={{
          minHeight: "16rem",
          maxHeight: "40vh",
          overflowY: "auto",
          padding: "0.8rem",
          textAlign: "left",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace)",
          fontSize: "0.88rem",
          lineHeight: 1.42,
          color: "var(--text)",
        }}
        aria-label="Radio terminal output"
      >
        {lines.length === 0 ? (
          <div className="radio-terminal-muted">(waiting for radio data)</div>
        ) : (
          txLinesWithStatus.map((row, i) => (
            <div key={`${i}-${row.line.slice(0, 48)}`} className="radio-terminal-line">
              {row.packetId ? (
                <span className="radio-terminal-status-icon" title={row.status}>
                  {row.status === "delivered" ? (
                    <CheckCircle2 size={14} className="radio-icon-delivered" aria-hidden />
                  ) : row.status === "pending" ? (
                    <Clock size={14} className="radio-icon-pending" aria-hidden />
                  ) : null}
                </span>
              ) : null}
              <span>{row.line}</span>
            </div>
          ))
        )}
      </div>
      <div style={{ display: "flex", gap: "0.6rem", alignItems: "center", flexWrap: "wrap" }}>
        <input
          type="text"
          value={txMessage}
          onChange={(e) => setTxMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void handleSend();
            }
          }}
          placeholder="Type a radio message…"
          disabled={!connected || sending}
          style={{
            flex: "1 1 20rem",
            minHeight: "38px",
            borderRadius: "10px",
            border: "1px solid var(--border)",
            background: "var(--surface-solid)",
            color: "var(--text)",
            padding: "0 0.75rem",
            fontFamily: "var(--font)",
          }}
          aria-label="Radio transmit message"
        />
        <button type="button" onClick={() => void handleSend()} disabled={!connected || sending || txMessage.trim().length === 0}>
          {sending ? "Sending..." : "Send"}
        </button>
      </div>
    </div>
  );
}
