import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Antenna, CheckCircle2, Clock, Radio, Signal } from "lucide-react";
import { usePageVisible } from "../../usePageVisible";
import { useLexicon } from "../../shared/lexicon";
import type { MeshRadioIdentity } from "../../MeshNetworkHubView";

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

export interface LanClusterPeerView {
  node_id: number;
  ram_gb: number;
  cpu_cores: number;
  cpu_tier: string;
  ollama_models: string[];
  ollama_ok: boolean;
  updated_at_ms: number;
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

interface SdrDeviceRow {
  index: number;
  label: string;
}

interface SdrStartResult {
  port: number;
  sampleRateHz: number;
  channels: number;
  streamUrl: string;
}

function formatTs(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleTimeString();
}

type RadioViewProps = {
  /** Opens the full Meshtastic / mesh inbox screen in the main app shell. */
  onOpenMeshNetwork?: () => void;
  /** From the main app; avoids a second USB probe in this panel. */
  meshRadioConnected?: boolean | null;
};

export function RadioView({ onOpenMeshNetwork, meshRadioConnected }: RadioViewProps) {
  const lx = useLexicon();
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
  const [clusterPeers, setClusterPeers] = useState<LanClusterPeerView[]>([]);
  const [channels, setChannels] = useState<RadioChannelRow[]>([]);
  const [selectedChannelId, setSelectedChannelId] = useState<string>("");
  const [applyingChannel, setApplyingChannel] = useState(false);
  const [newChannelName, setNewChannelName] = useState("");
  const [newChannelMhz, setNewChannelMhz] = useState("");
  const [newChannelKey, setNewChannelKey] = useState("");
  const [savingChannel, setSavingChannel] = useState(false);

  const [sdrDevices, setSdrDevices] = useState<SdrDeviceRow[]>([]);
  const [sdrDeviceIndex, setSdrDeviceIndex] = useState<number>(0);
  const [sdrFreqMhz, setSdrFreqMhz] = useState("162.55");
  const [sdrMode, setSdrMode] = useState<"fm" | "wbfm" | "am">("fm");
  const [sdrRunning, setSdrRunning] = useState(false);
  const [sdrStarting, setSdrStarting] = useState(false);
  const [sdrLogs, setSdrLogs] = useState<string[]>([]);
  const [sdrStreamMeta, setSdrStreamMeta] = useState<string | null>(null);
  const sdrCanvasRef = useRef<HTMLCanvasElement>(null);
  const sdrAudioContextRef = useRef<AudioContext | null>(null);
  const sdrGainRef = useRef<GainNode | null>(null);
  const sdrAnalyserRef = useRef<AnalyserNode | null>(null);
  const sdrNextPlayRef = useRef(0);
  const sdrAbortRef = useRef<AbortController | null>(null);
  const sdrRafRef = useRef<number | null>(null);
  const pageVisible = usePageVisible();
  const [meshIdentity, setMeshIdentity] = useState<MeshRadioIdentity | null>(null);

  const refreshMeshIdentity = useCallback(async () => {
    try {
      const id = await invoke<MeshRadioIdentity>("get_mesh_radio_identity");
      setMeshIdentity(id);
    } catch {
      setMeshIdentity(null);
    }
  }, []);

  useEffect(() => {
    if (!onOpenMeshNetwork) return;
    void refreshMeshIdentity();
    const u = listen("athena:mesh-connection", () => void refreshMeshIdentity());
    return () => {
      void u.then((fn) => fn());
    };
  }, [onOpenMeshNetwork, refreshMeshIdentity]);

  const loadMeshMessages = useCallback(async () => {
    try {
      const list = await invoke<MeshMessageRow[]>("get_mesh_messages", { limit: 80 });
      setMeshMessages(Array.isArray(list) ? list : []);
    } catch {
      setMeshMessages([]);
    }
  }, []);

  const loadClusterPeers = useCallback(async () => {
    try {
      const list = await invoke<LanClusterPeerView[]>("get_lan_cluster_peers");
      setClusterPeers(Array.isArray(list) ? list : []);
    } catch {
      setClusterPeers([]);
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

  const loadSdrDevices = useCallback(async () => {
    try {
      const list = await invoke<SdrDeviceRow[]>("sdr_list_devices");
      setSdrDevices(Array.isArray(list) ? list : []);
    } catch {
      setSdrDevices([]);
    }
  }, []);

  function mergeSdrU8(a: Uint8Array, b: Uint8Array): Uint8Array {
    const o = new Uint8Array(a.length + b.length);
    o.set(a, 0);
    o.set(b, a.length);
    return o;
  }

  function stopSdrPlayback() {
    if (sdrRafRef.current != null) {
      cancelAnimationFrame(sdrRafRef.current);
      sdrRafRef.current = null;
    }
    sdrAbortRef.current?.abort();
    sdrAbortRef.current = null;
    const ac = sdrAudioContextRef.current;
    if (ac) {
      void ac.close();
      sdrAudioContextRef.current = null;
    }
    sdrGainRef.current = null;
    sdrAnalyserRef.current = null;
    sdrNextPlayRef.current = 0;
  }

  async function runSdrStream(result: SdrStartResult) {
    const AudioContextCtor =
      window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    const ac = new AudioContextCtor({ sampleRate: result.sampleRateHz }) as AudioContext;
    sdrAudioContextRef.current = ac;
    await ac.resume();

    const gain = ac.createGain();
    gain.gain.value = 0.85;
    sdrGainRef.current = gain;

    const analyser = ac.createAnalyser();
    analyser.fftSize = 2048;
    analyser.smoothingTimeConstant = 0.72;
    sdrAnalyserRef.current = analyser;

    gain.connect(analyser);
    analyser.connect(ac.destination);

    let nextPlay = ac.currentTime + 0.12;
    sdrNextPlayRef.current = nextPlay;
    const frameSamples = Math.max(2048, Math.floor(result.sampleRateHz / 10));
    let carry = new Int16Array(0);

    const controller = new AbortController();
    sdrAbortRef.current = controller;

    const freqData = new Uint8Array(analyser.frequencyBinCount);

    const drawLoop = () => {
      sdrRafRef.current = requestAnimationFrame(drawLoop);
      const can = sdrCanvasRef.current;
      const an = sdrAnalyserRef.current;
      if (!can || !an) return;
      const ctx = can.getContext("2d");
      if (!ctx) return;
      an.getByteFrequencyData(freqData);
      const fill = getComputedStyle(can).getPropertyValue("--text").trim() || "currentColor";
      ctx.clearRect(0, 0, can.width, can.height);
      const n = freqData.length;
      const barW = can.width / n;
      for (let i = 0; i < n; i++) {
        const bh = (freqData[i] / 255) * can.height;
        ctx.fillStyle = fill;
        ctx.fillRect(i * barW, can.height - bh, Math.max(1, barW - 0.5), bh);
      }
    };
    drawLoop();

    const scheduleFrames = (i16: Int16Array) => {
      if (i16.length === 0) return;
      const merged = new Int16Array(carry.length + i16.length);
      merged.set(carry, 0);
      merged.set(i16, carry.length);
      carry = merged;
      while (carry.length >= frameSamples) {
        const slice = carry.subarray(0, frameSamples);
        carry = carry.subarray(frameSamples);
        const f32 = new Float32Array(slice.length);
        for (let i = 0; i < slice.length; i++) f32[i] = slice[i] / 32768;
        const buf = ac.createBuffer(1, f32.length, result.sampleRateHz);
        buf.copyToChannel(f32, 0);
        const src = ac.createBufferSource();
        src.buffer = buf;
        src.connect(gain);
        const now = ac.currentTime;
        const lead = 0.08;
        const startAt = Math.max(now + lead, nextPlay);
        src.start(startAt);
        nextPlay = startAt + buf.duration;
      }
      sdrNextPlayRef.current = nextPlay;
    };

    const resp = await fetch(result.streamUrl, { signal: controller.signal });
    if (!resp.ok) {
      throw new Error(`SDR stream HTTP ${resp.status}`);
    }
    const reader = resp.body?.getReader();
    if (!reader) throw new Error("SDR stream: no response body");

    let bytePending = new Uint8Array(0);
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (!value?.length) continue;
        bytePending = mergeSdrU8(bytePending, value);
        const aligned = bytePending.byteLength - (bytePending.byteLength % 2);
        if (aligned === 0) continue;
        const view = bytePending.subarray(0, aligned);
        bytePending = bytePending.subarray(aligned);
        const samples = new Int16Array(view.buffer, view.byteOffset, view.byteLength / 2);
        scheduleFrames(samples);
      }
    } finally {
      reader.releaseLock();
    }
  }

  async function handleSdrStart() {
    stopSdrPlayback();
    setSdrStarting(true);
    setError(null);
    try {
      const mhz = Number.parseFloat(sdrFreqMhz);
      if (!Number.isFinite(mhz) || mhz <= 0) {
        setError(lx.radioErrValidFreqMhz);
        return;
      }
      const hz = Math.round(mhz * 1e6);
      const result = await invoke<SdrStartResult>("sdr_start", {
        args: {
          frequencyHz: hz,
          mode: sdrMode,
          deviceIndex: sdrDeviceIndex,
          sampleRateHz: 48000,
        },
      });
      setSdrStreamMeta(`127.0.0.1:${result.port} · ${result.sampleRateHz} Hz mono`);
      setSdrRunning(true);
      await runSdrStream(result);
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "AbortError")) {
        setError(e != null ? String(e) : lx.radioErrSdrStart);
      }
      await invoke("sdr_stop").catch(() => {});
      stopSdrPlayback();
    } finally {
      setSdrStarting(false);
      setSdrRunning(false);
      setSdrStreamMeta(null);
    }
  }

  async function handleSdrStop() {
    stopSdrPlayback();
    try {
      await invoke("sdr_stop");
    } catch {
      /* ignore */
    }
    setSdrRunning(false);
    setSdrStreamMeta(null);
  }

  async function loadPorts() {
    setError(null);
    try {
      const list = await invoke<string[]>("get_available_ports");
      const next = Array.isArray(list) ? list : [];
      setPorts(next);
      setSelectedPort((prev) => (prev && next.includes(prev) ? prev : next[0] ?? ""));
    } catch (e) {
      setError(e != null ? String(e) : lx.radioErrLoadPorts);
      setPorts([]);
      setSelectedPort("");
    }
  }

  useEffect(() => {
    void loadPorts();
    void loadChannels();
    void loadSdrDevices();
  }, [loadChannels, loadSdrDevices]);

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
    void loadClusterPeers();
    const unlisten = listen("athena:lan-cluster-updated", () => {
      void loadClusterPeers();
    });
    const pollMs = pageVisible ? 12_000 : 60_000;
    const id = window.setInterval(() => void loadClusterPeers(), pollMs);
    return () => {
      window.clearInterval(id);
      void unlisten.then((fn) => fn());
    };
  }, [loadClusterPeers, pageVisible]);

  useEffect(() => {
    const el = terminalRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [lines]);

  useEffect(() => {
    if (!connected) return;
    void loadMeshMessages();
    void loadTelemetryHistory();
    const pollMs = pageVisible ? 5000 : 20_000;
    const id = window.setInterval(() => {
      void loadMeshMessages();
    }, pollMs);
    return () => clearInterval(id);
  }, [connected, loadMeshMessages, loadTelemetryHistory, pageVisible]);

  useEffect(() => {
    const uLog = listen<string>("athena:sdr-log", (e) => {
      const line = e.payload ?? "";
      if (!line) return;
      setSdrLogs((prev) => [...prev.slice(-120), line]);
    });
    const uStop = listen("athena:sdr-stopped", () => {
      stopSdrPlayback();
      setSdrRunning(false);
      setSdrStreamMeta(null);
    });
    return () => {
      void uLog.then((f) => f());
      void uStop.then((f) => f());
    };
  }, []);

  useEffect(() => {
    return () => {
      stopSdrPlayback();
      void invoke("sdr_stop").catch(() => {});
    };
  }, []);

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
        setError(lx.radioErrConnectionFailed);
      } else {
        setConnected(true);
        setLines((prev) => [...prev, `Connected to ${selectedPort}`]);
        void loadMeshMessages();
        void loadTelemetryHistory();
        void loadChannels();
      }
    } catch (e) {
      setConnected(false);
      setError(e != null ? String(e) : lx.radioErrConnectionFailed);
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
      setError(e != null ? String(e) : lx.radioErrSendFailed);
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
      setError(e != null ? String(e) : lx.radioErrApplyChannel);
    } finally {
      setApplyingChannel(false);
    }
  }

  async function handleSaveNewChannel() {
    const name = newChannelName.trim();
    const mhz = Number.parseFloat(newChannelMhz);
    const key = newChannelKey.trim();
    if (!name || !Number.isFinite(mhz) || !key || savingChannel) {
      setError(lx.radioErrChannelFields);
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
      setError(e != null ? String(e) : lx.radioErrSaveChannel);
    } finally {
      setSavingChannel(false);
    }
  }

  async function handleDeleteChannel() {
    if (!selectedChannelId || !channels.some((c) => c.id === selectedChannelId)) return;
    if (!window.confirm(lx.radioConfirmDeleteChannel)) return;
    try {
      await invoke("delete_radio_channel", { id: selectedChannelId });
      await loadChannels();
      setLines((prev) => [...prev, `Deleted channel profile.`]);
    } catch (e) {
      setError(e != null ? String(e) : lx.radioErrDeleteChannel);
    }
  }

  const meshUsbOk = meshRadioConnected === true || meshIdentity?.connected === true;

  return (
    <div className="radio-view-root glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.9rem" }}>
      <h2 className="app-view-placeholder-title" style={{ marginBottom: "0.2rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
        <Radio size={22} strokeWidth={2} aria-hidden />
        {lx.radioTitle}
      </h2>
      <p className="text-muted small" style={{ margin: 0 }}>
        Comms is for serial packet radios, logs, and SDR listening. Meshtastic (long-range mesh handhelds over USB) has
        its own inbox—use the card below so you are not hunting through technical menus.
      </p>

      {onOpenMeshNetwork ? (
        <section className="glass-panel radio-meshtastic-bridge" style={{ padding: "0.9rem", display: "grid", gap: "0.5rem" }}>
          <h3 style={{ margin: 0, display: "flex", alignItems: "center", gap: "0.45rem", fontSize: "1.05rem" }}>
            <Antenna size={20} strokeWidth={2} aria-hidden />
            Meshtastic mesh inbox
          </h3>
          <p className="text-muted small" style={{ margin: 0 }}>
            Read and send mesh text in plain language, manage who is allowed to talk to this station, and see whether
            your USB radio is connected.
          </p>
          {meshUsbOk ? (
            <p className="small" style={{ margin: 0 }}>
              <span className="mesh-network-hub-dot mesh-network-hub-dot-ok" style={{ marginRight: "0.35rem" }} aria-hidden />
              USB mesh radio:{" "}
              <strong>
                {meshIdentity?.displayName?.trim() ||
                  (meshIdentity?.nodeId ? `Node ${meshIdentity.nodeId}` : "Connected")}
              </strong>
            </p>
          ) : (
            <p className="text-muted small" style={{ margin: 0 }}>
              No USB Meshtastic device detected yet. Plug one in, wait a few seconds, then open the mesh screen—you can
              still read help text there while offline.
            </p>
          )}
          <button type="button" className="glass-action-btn" onClick={onOpenMeshNetwork} style={{ justifySelf: "start" }}>
            Open Communications Hub (mesh inbox)
          </button>
        </section>
      ) : null}

      <div className="radio-view-panels" style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: "0.65rem" }}>
        <div className="glass-panel radio-signal-panel">
          <div className="radio-signal-panel-header">
            <Signal size={16} strokeWidth={2} aria-hidden />
            <span>{lx.radioSignalHealth}</span>
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
            <p className="radio-signal-muted">
              {lx.radioNoTelemetry} <code>ATH:SIG rssi=… snr=…</code>
            </p>
          )}
        </div>

        <div className="glass-panel radio-channel-panel">
          <div className="radio-channel-panel-header">{lx.radioEncryptedChannelHeader}</div>
          <div className="radio-channel-row">
            <label className="radio-channel-label" htmlFor="radio-channel-select">
              {lx.radioProfileLabel}
            </label>
            <select
              id="radio-channel-select"
              className="theme-select radio-channel-select"
              value={selectedChannelId}
              onChange={(e) => setSelectedChannelId(e.target.value)}
              disabled={channels.length === 0}
            >
              {channels.length === 0 ? (
                <option value="">{lx.radioNoProfilesOption}</option>
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
              {applyingChannel ? lx.stateApplying : lx.radioApplyToRadio}
            </button>
            <button type="button" className="secondary" onClick={() => void handleDeleteChannel()} disabled={!selectedChannelId || channels.length === 0}>
              {lx.radioDeleteProfile}
            </button>
          </div>
          <div className="radio-channel-add">
            <div className="radio-channel-add-title">{lx.radioNewProfile}</div>
            <input
              type="text"
              placeholder={lx.radioPlaceholderName}
              value={newChannelName}
              onChange={(e) => setNewChannelName(e.target.value)}
              className="radio-channel-input"
              aria-label={lx.radioAriaChannelName}
            />
            <input
              type="text"
              inputMode="decimal"
              placeholder={lx.radioPlaceholderFreqMhz}
              value={newChannelMhz}
              onChange={(e) => setNewChannelMhz(e.target.value)}
              className="radio-channel-input"
              aria-label={lx.radioAriaFreqMhz}
            />
            <input
              type="password"
              placeholder={lx.radioPlaceholderEncryptionKey}
              value={newChannelKey}
              onChange={(e) => setNewChannelKey(e.target.value)}
              className="radio-channel-input"
              autoComplete="off"
              aria-label={lx.radioAriaEncryptionKey}
            />
            <button
              type="button"
              className="glass-action-btn"
              onClick={() => void handleSaveNewChannel()}
              disabled={savingChannel}
            >
              {savingChannel ? lx.stateSaving : lx.radioSaveProfile}
            </button>
          </div>
        </div>
      </div>

      <div className="glass-panel radio-mesh-status-panel">
        <div className="radio-mesh-status-title">{lx.radioSdrTitle}</div>
        <p className="radio-mesh-status-muted" style={{ margin: "0 0 0.5rem" }}>
          {lx.radioSdrBlurb}
        </p>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", marginBottom: "0.6rem" }}>
          <label className="radio-channel-label" htmlFor="sdr-device">
            {lx.radioDongle}
          </label>
          <select
            id="sdr-device"
            className="theme-select"
            value={String(sdrDeviceIndex)}
            onChange={(e) => setSdrDeviceIndex(Number.parseInt(e.target.value, 10) || 0)}
            aria-label={lx.radioAriaRtlDevice}
          >
            {sdrDevices.map((d) => (
              <option key={d.index} value={String(d.index)}>
                {d.label}
              </option>
            ))}
          </select>
          <button type="button" className="secondary" onClick={() => void loadSdrDevices()}>
            {lx.radioRefreshDevices}
          </button>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", marginBottom: "0.6rem" }}>
          <label className="radio-channel-label" htmlFor="sdr-freq">
            {lx.radioMhzLabel}
          </label>
          <input
            id="sdr-freq"
            type="text"
            inputMode="decimal"
            className="radio-channel-input"
            value={sdrFreqMhz}
            onChange={(e) => setSdrFreqMhz(e.target.value)}
            aria-label={lx.radioAriaFreqMegahertz}
            style={{ width: "6.5rem" }}
          />
          <label className="radio-channel-label" htmlFor="sdr-mode">
            {lx.radioModeLabel}
          </label>
          <select
            id="sdr-mode"
            className="theme-select"
            value={sdrMode}
            onChange={(e) => setSdrMode(e.target.value as "fm" | "wbfm" | "am")}
            aria-label={lx.radioAriaDemodMode}
          >
            <option value="fm">{lx.radioModeNfm}</option>
            <option value="wbfm">{lx.radioModeWbfm}</option>
            <option value="am">{lx.radioModeAm}</option>
          </select>
          <button
            type="button"
            className="glass-action-btn"
            onClick={() => void handleSdrStart()}
            disabled={sdrStarting || sdrRunning}
          >
            {sdrStarting ? lx.stateStarting : lx.radioStart}
          </button>
          <button type="button" className="secondary" onClick={() => void handleSdrStop()} disabled={!sdrRunning && !sdrStarting}>
            {lx.radioStop}
          </button>
        </div>
        {sdrStreamMeta ? <p className="radio-mesh-status-meta">{sdrStreamMeta}</p> : null}
        <div style={{ width: "100%", minHeight: "140px", marginTop: "0.5rem" }}>
          <canvas
            ref={sdrCanvasRef}
            width={600}
            height={220}
            style={{ width: "100%", height: "140px", display: "block" }}
            aria-label={lx.radioAriaSdrSpectrum}
          />
        </div>
        <ul className="radio-mesh-status-list" style={{ marginTop: "0.65rem", maxHeight: "7rem", overflow: "auto" }}>
          {sdrLogs.length === 0 ? (
            <li className="radio-mesh-status-muted">{lx.radioSdrLogPlaceholder}</li>
          ) : (
            sdrLogs.slice(-40).map((line, i) => (
              <li key={`${i}-${line.slice(0, 24)}`} className="radio-mesh-status-item">
                <span className="radio-mesh-status-text">
                  <span className="radio-mesh-status-meta">{line}</span>
                </span>
              </li>
            ))
          )}
        </ul>
      </div>

      <div style={{ display: "flex", gap: "0.6rem", alignItems: "center", flexWrap: "wrap" }}>
        <select
          className="theme-select"
          value={selectedPort}
          onChange={(e) => setSelectedPort(e.target.value)}
          disabled={ports.length === 0 || connecting}
          aria-label={lx.radioAriaSerialPorts}
          style={{ minWidth: "18rem" }}
        >
          {ports.length === 0 ? (
            <option value="">{lx.radioNoSerialPorts}</option>
          ) : (
            ports.map((port) => (
              <option key={port} value={port}>
                {port}
              </option>
            ))
          )}
        </select>
        <button type="button" onClick={() => void loadPorts()} className="secondary" disabled={connecting}>
          {lx.radioRefresh}
        </button>
        <button
          type="button"
          className="glass-action-btn"
          onClick={() => void handleConnect()}
          disabled={!selectedPort || connecting}
        >
          {connecting ? lx.actionConnecting : connected ? lx.radioConnected : lx.radioConnect}
        </button>
      </div>
      {error ? (
        <p className="error" role="alert" style={{ margin: 0 }}>
          {error}
        </p>
      ) : null}

      <div className="glass-panel radio-mesh-status-panel">
        <div className="radio-mesh-status-title">{lx.radioOutboundPackets}</div>
        <ul className="radio-mesh-status-list">
          {meshMessages.length === 0 ? (
            <li className="radio-mesh-status-muted">{lx.radioNoTrackedPackets}</li>
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

      <div className="glass-panel radio-mesh-status-panel">
        <div className="radio-mesh-status-title">{lx.radioLanClusterTitle}</div>
        <ul className="radio-mesh-status-list">
          {clusterPeers.length === 0 ? (
            <li className="radio-mesh-status-muted">{lx.radioNoLanPeers}</li>
          ) : (
            clusterPeers.map((p) => (
              <li key={p.node_id} className="radio-mesh-status-item">
                <span className="radio-mesh-status-text">
                  <span className="radio-mesh-status-msg">Node {p.node_id}</span>
                  <span className="radio-mesh-status-meta">
                    {p.ram_gb} GB RAM · {p.cpu_cores} cores · {p.cpu_tier}
                    {p.ollama_ok ? "" : " · Ollama offline"}
                  </span>
                  <span className="radio-mesh-status-meta">
                    Models: {p.ollama_models.length ? p.ollama_models.slice(0, 6).join(", ") : "(none)"}
                    {p.ollama_models.length > 6 ? "…" : ""}
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
        aria-label={lx.radioTerminalOutputAria}
      >
        {lines.length === 0 ? (
          <div className="radio-terminal-muted">{lx.radioTerminalWaiting}</div>
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
          placeholder={lx.radioTxPlaceholder}
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
          aria-label={lx.radioTxAria}
        />
        <button type="button" onClick={() => void handleSend()} disabled={!connected || sending || txMessage.trim().length === 0}>
          {sending ? lx.stateSending : lx.radioSend}
        </button>
      </div>
    </div>
  );
}
