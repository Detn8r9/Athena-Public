import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { type UnlistenFn } from "@tauri-apps/api/event";
import { StationAiOperatorConsole } from "../station/StationAiOperatorConsole";
import { Antenna, Cpu, Mic, RefreshCw, Waves } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { RequireClearance } from "../../ui/components/RequireClearance";

export type HardwareStatus = {
  voiceDistance: boolean;
  meshData: boolean;
  broadcastRx: boolean;
  lanRemoteAi: boolean;
};

import "../field-ops/fieldOps.css";

// Radio planning, intercepts and other-station tools used to live in Field Ops; Comms is now their home.
const RadioPlanTab = lazy(() => import("../field-ops/RadioPlanTab").then((m) => ({ default: m.RadioPlanTab })));
const RadioDataTab = lazy(() => import("../field-ops/RadioDataTab").then((m) => ({ default: m.RadioDataTab })));
const StationsTab = lazy(() => import("../field-ops/StationsTab").then((m) => ({ default: m.StationsTab })));

export type CommsSection = "overview" | "radio_plan" | "radio_data" | "stations";

export const COMMS_SECTIONS: ReadonlyArray<{ id: CommsSection; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "radio_plan", label: "Radio plan" },
  { id: "radio_data", label: "Radio data & intercepts" },
  { id: "stations", label: "Other stations" },
];

export type CommunicationsHubProps = {
  onOpenMesh: () => void;
  onOpenVoiceComms: () => void;
  meshEmergencyStrip?: string | null;
  /** Which section is open; owned by the app so other screens can deep-link here. */
  section?: CommsSection;
  onSectionChange?: (section: CommsSection) => void;
};

function CapabilityCard(props: {
  title: string;
  description: string;
  connected: boolean;
  icon: ReactNode;
  actions?: ReactNode;
}) {
  const { title, description, connected, icon, actions } = props;
  return (
    <div
      className={`communications-hub-card${connected ? "" : " communications-hub-card--disconnected"}`}
      aria-disabled={!connected}
    >
      <div className="communications-hub-card-head">
        <span className="communications-hub-card-icon" aria-hidden>
          {icon}
        </span>
        <h3 className="communications-hub-card-title">{title}</h3>
      </div>
      <p className="communications-hub-card-desc">{description}</p>
      {!connected ? (
        <p className="communications-hub-card-disconnected" role="status">
          Hardware disconnected
        </p>
      ) : null}
      {connected && actions ? <div className="communications-hub-card-actions">{actions}</div> : null}
    </div>
  );
}

export function CommunicationsHub({
  onOpenMesh,
  onOpenVoiceComms,
  meshEmergencyStrip,
  section = "overview",
  onSectionChange,
}: CommunicationsHubProps) {
  const [status, setStatus] = useState<HardwareStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [distress, setDistress] = useState<Array<{ id: string; from: number; text: string; at: number }>>([]);
  const [distressDraft, setDistressDraft] = useState("");
  const [distressSending, setDistressSending] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const s = await invoke<HardwareStatus>("check_hardware_status");
      setStatus(s);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let un: UnlistenFn | undefined;
    void listen<{ sender_id: number; text: string; channel?: number }>("athena:mesh-text", (ev) => {
      const p = ev.payload;
      if (!p || typeof p.sender_id !== "number" || typeof p.text !== "string") return;
      const ch = typeof p.channel === "number" ? p.channel : 0;
      if (ch !== 0) return;
      const t = p.text.trim();
      if (!t) return;
      const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      setDistress((prev) => [...prev, { id, from: p.sender_id, text: t, at: Date.now() }].slice(-60));
    }).then((fn) => {
      un = fn;
    });
    return () => {
      void un?.();
    };
  }, []);

  const distressSummary = useMemo(() => {
    if (distress.length === 0) return null;
    const last = distress[distress.length - 1]!;
    return `Distress (ch 0): last from node ${last.from} · ${last.text.length > 80 ? `${last.text.slice(0, 77)}…` : last.text}`;
  }, [distress]);

  const sendDistressReply = useCallback(async () => {
    const text = distressDraft.trim();
    if (!text || distressSending) return;
    setDistressSending(true);
    setDistressDraft("");
    try {
      await invoke("send_mesh_message", {
        text,
        channel: 0,
        destinationNodeId: null,
        wantAck: false,
      });
      const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      setDistress((prev) => [...prev, { id, from: 0, text: `REPLY: ${text}`, at: Date.now() }].slice(-60));
    } catch (e) {
      const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      setDistress((prev) => [...prev, { id, from: 0, text: `Send failed: ${String(e)}`, at: Date.now() }].slice(-60));
    } finally {
      setDistressSending(false);
    }
  }, [distressDraft, distressSending]);

  const s = status;

  return (
    <div className="home-view-scroll communications-hub-root">
      <div className="communications-hub-hero glass-panel">
        <div className="communications-hub-hero-top">
          <div>
            <h2 className="communications-hub-title">Comms</h2>
            <p className="communications-hub-lead text-muted">
              Talk to people and other stations: answer distress calls, use a radio, send mesh messages, and manage AI requests from other devices. Each card lights up when its hardware is connected.
            </p>
          </div>
          <button
            type="button"
            className="secondary communications-hub-refresh"
            onClick={() => void refresh()}
            disabled={loading}
            aria-busy={loading}
          >
            <RefreshCw size={16} strokeWidth={2} className={loading ? "communications-hub-spin" : ""} aria-hidden />
            Refresh hardware
          </button>
        </div>
        {error ? (
          <p className="communications-hub-error" role="alert">
            {error}
          </p>
        ) : null}
        {meshEmergencyStrip ? (
          <p className="communications-hub-mesh-emergency" role="alert" aria-live="polite">
            {meshEmergencyStrip}
          </p>
        ) : null}
        {distressSummary ? (
          <p className="communications-hub-mesh-emergency" role="alert" aria-live="polite">
            {distressSummary}
          </p>
        ) : null}
      </div>

      <div role="tablist" aria-label="Comms sections" style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap", marginBottom: 14, flexShrink: 0 }}>
        {COMMS_SECTIONS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={t.id === section}
            className={t.id === section ? "glass-action-btn" : "secondary"}
            onClick={() => onSectionChange?.(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>

      {section !== "overview" ? (
        <div className="fo-hub" style={{ padding: 0, maxWidth: "none", flexShrink: 0 }}>
          <Suspense fallback={<p className="text-muted">Loading…</p>}>
            {section === "radio_plan" ? <RadioPlanTab /> : section === "radio_data" ? <RadioDataTab /> : <StationsTab />}
          </Suspense>
        </div>
      ) : (
      <>
      <div className="glass-panel" style={{ padding: "0.9rem 1rem", borderRadius: 14, marginBottom: 14 }}>
        <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 12 }}>
          <h3 style={{ margin: 0 }}>Distress Monitor · Channel 0 (Open)</h3>
          <span className="text-muted small">Always listening</span>
        </div>
        <p className="text-muted small" style={{ marginTop: 6 }}>
          This feed listens on Channel 0 for open distress calls. Replies from here are forced onto Channel 0 so Level 0 guests receive them.
        </p>
        <div style={{ display: "grid", gap: 8, maxHeight: 180, overflow: "auto", paddingRight: 4 }}>
          {distress.length === 0 ? (
            <div className="text-muted small">No Channel 0 distress traffic yet.</div>
          ) : (
            distress.map((m) => (
              <div key={m.id} className="profile-activity-line">
                <div className="text-muted small">
                  {new Date(m.at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" })} · {m.from === 0 ? "This station" : `node ${m.from}`}
                </div>
                <div style={{ whiteSpace: "pre-wrap" }}>{m.text}</div>
              </div>
            ))
          )}
        </div>
        <div style={{ display: "flex", gap: 10, marginTop: 10 }}>
          <input
            value={distressDraft}
            onChange={(e) => setDistressDraft(e.target.value)}
            placeholder="Reply on Channel 0 (open distress)…"
            className="comms-hub-distress-input"
          />
          <button type="button" className="secondary" disabled={distressSending || !distressDraft.trim()} onClick={() => void sendDistressReply()}>
            {distressSending ? "Sending…" : "Send"}
          </button>
        </div>
      </div>

      <div className="communications-hub-grid">
        <CapabilityCard
          title="Tactical Voice"
          description="PTT and voice-over-distance paths when a compatible radio or bridge is present."
          connected={s?.voiceDistance === true}
          icon={<Mic size={22} strokeWidth={2} />}
          actions={
            <button type="button" className="secondary" onClick={onOpenVoiceComms}>
              Open voice comms
            </button>
          }
        />
        <CapabilityCard
          title="Mesh Data"
          description="Meshtastic USB link and mesh messaging through ATHENA."
          connected={s?.meshData === true}
          icon={<Antenna size={22} strokeWidth={2} />}
          actions={
            <button type="button" className="secondary" onClick={onOpenMesh}>
              Open mesh network
            </button>
          }
        />
        <CapabilityCard
          title="Broadcast Listen"
          description="Passive AM, FM, or streamed public audio when SDR or network listen is available."
          connected={s?.broadcastRx === true}
          icon={<Waves size={22} strokeWidth={2} />}
          actions={
            <RequireClearance required_level={2}>
              {s?.broadcastRx ? (
                <span className="text-muted">Receiver detected. Tuning controls are not available in this version yet.</span>
              ) : (
                <span className="text-muted">Not available yet. A future version will use a USB software-defined radio (SDR) to listen to AM/FM broadcasts.</span>
              )}
            </RequireClearance>
          }
        />
        <CapabilityCard
          title="Station AI"
          description="Remote mesh and LAN clients can queue requests for the local model; operator oversight lives here."
          connected={s?.lanRemoteAi === true}
          icon={<Cpu size={22} strokeWidth={2} />}
          actions={<StationAiOperatorConsole />}
        />
      </div>
      </>
      )}
    </div>
  );
}
