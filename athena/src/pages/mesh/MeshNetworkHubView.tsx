import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke, athenaListen as listen, canUseHostCapability } from "../../shared/ipc/athenaIpc";
import { Antenna, MessageSquare, Settings } from "lucide-react";
import { MeshChatView } from "./MeshChatView";
import { MeshNetworkView } from "../../domains/mesh/MeshNetworkView";
import { CommsHubIcon } from "../../shared/CommsHubIcon";

export type MeshRadioIdentity = {
  connected: boolean;
  nodeId: number;
  displayName: string;
};

type Props = {
  meshRadioConnected: boolean | null;
  onOpenQuickSend: () => void;
  onOpenMeshSettings: () => void;
  onGoToOfflineChat: () => void;
  onGoToComms: () => void;
};

export function MeshNetworkHubView({
  meshRadioConnected,
  onOpenQuickSend,
  onOpenMeshSettings,
  onGoToOfflineChat,
  onGoToComms,
}: Props) {
  const [identity, setIdentity] = useState<MeshRadioIdentity | null>(null);

  const refreshIdentity = useCallback(async () => {
    try {
      const id = await invoke<MeshRadioIdentity>("get_mesh_radio_identity");
      setIdentity(id);
    } catch {
      setIdentity(null);
    }
  }, []);

  useEffect(() => {
    void refreshIdentity();
    const u = listen("athena:mesh-connection", () => void refreshIdentity());
    return () => {
      void u.then((fn) => fn());
    };
  }, [refreshIdentity]);

  const connected = meshRadioConnected === true;
  const stationLabel =
    identity?.displayName && identity.displayName.trim().length > 0
      ? identity.displayName
      : identity?.nodeId
        ? `Node ${identity.nodeId}`
        : "This station";

  return (
    <div className="mesh-network-hub">
      <header className="mesh-network-hub-hero glass-panel">
        <div className="mesh-network-hub-hero-icon" aria-hidden>
          <Antenna size={28} strokeWidth={2} />
        </div>
        <div className="mesh-network-hub-hero-text">
          <h1 className="mesh-network-hub-title">Meshtastic &amp; mesh network</h1>
          <p className="mesh-network-hub-lead text-muted">
            Talk to people over long-range LoRa radio when a compatible USB device is plugged into this computer. No
            account or internet required. Your offline AI chats stay on the &quot;Offline chat&quot; tab; mesh
            conversations are listed here by radio.
          </p>
        </div>
      </header>

      <section className="mesh-network-hub-status glass-panel" aria-live="polite">
        <h2 className="mesh-network-hub-section-title">This computer&apos;s radio</h2>
        {meshRadioConnected === null ? (
          <p className="text-muted">Checking USB connection…</p>
        ) : !connected ? (
          <div className="mesh-network-hub-status-body">
            <p className="mesh-network-hub-status-strong">No Meshtastic radio detected yet.</p>
            <p className="text-muted small">
              Plug in your device with a USB cable, wait a few seconds, and the status here should turn green. You do not
              need to pick a serial port in most cases—Athena finds the radio automatically.
            </p>
          </div>
        ) : (
          <div className="mesh-network-hub-status-body">
            <p className="mesh-network-hub-status-ok">
              <span className="mesh-network-hub-dot mesh-network-hub-dot-ok" aria-hidden />
              Connected
            </p>
            <p className="mesh-network-hub-station-name">
              <strong>{stationLabel}</strong>
              {identity?.nodeId ? (
                <span className="text-muted small"> · Node number {identity.nodeId}</span>
              ) : null}
            </p>
            <p className="text-muted small">
              Other radios see traffic from this node number. Names come from your handheld&apos;s settings when the mesh
              shares them.
            </p>
          </div>
        )}
        <div className="mesh-network-hub-actions">
          <button
            type="button"
            className="glass-action-btn"
            onClick={onOpenQuickSend}
            disabled={!connected || !canUseHostCapability("meshUsb")}
            title={
              !canUseHostCapability("meshUsb")
                ? "Mesh USB send requires the Athena desktop host."
                : undefined
            }
          >
            Quick test message
          </button>
          <button type="button" className="glass-action-btn" onClick={onOpenMeshSettings}>
            <Settings size={16} strokeWidth={2} aria-hidden /> Mesh &amp; signup settings
          </button>
          <button type="button" className="glass-action-btn" onClick={onGoToOfflineChat}>
            <MessageSquare size={16} strokeWidth={2} aria-hidden /> Offline chat (AI)
          </button>
          <button type="button" className="glass-action-btn" onClick={onGoToComms}>
            <CommsHubIcon size={16} strokeWidth={2} /> Communications Hub
          </button>
        </div>
      </section>

      <MeshNetworkView />

      <section className="mesh-network-hub-chat glass-panel">
        <h2 className="mesh-network-hub-section-title">Conversations over the mesh</h2>
        <p className="text-muted small mesh-network-hub-chat-intro">
          Choose a contact on the left, read the thread, or send from this computer. Use the gear for{" "}
          <strong>Trusted radios</strong> (manual node ID + counter-sign). Remote signup (Settings → Mesh) is optional and
          should only use a phrase shared with people you trust.
        </p>
        <div className="mesh-network-hub-chat-frame">
          <MeshChatView />
        </div>
      </section>
    </div>
  );
}
