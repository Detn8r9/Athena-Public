import { invoke } from "@tauri-apps/api/core";
import { StationAiOperatorConsole } from "./StationAiOperatorConsole";
import { Antenna, Cpu, Mic, RefreshCw, Waves } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";

export type HardwareStatus = {
  voiceDistance: boolean;
  meshData: boolean;
  broadcastRx: boolean;
  lanRemoteAi: boolean;
};

export type CommunicationsHubProps = {
  onOpenMesh: () => void;
  onOpenVoiceComms: () => void;
  meshEmergencyStrip?: string | null;
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

export function CommunicationsHub({ onOpenMesh, onOpenVoiceComms, meshEmergencyStrip }: CommunicationsHubProps) {
  const [status, setStatus] = useState<HardwareStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  const s = status;

  return (
    <div className="home-view-scroll communications-hub-root">
      <div className="communications-hub-hero glass-panel">
        <div className="communications-hub-hero-top">
          <div>
            <h2 className="communications-hub-title">Communications Hub</h2>
            <p className="communications-hub-lead text-muted">
              Capability-driven dashboard for voice, mesh data, broadcast listen, and remote AI.
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
            <span className="text-muted communications-hub-soon">Listen controls will appear here when hardware is ready.</span>
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
    </div>
  );
}
