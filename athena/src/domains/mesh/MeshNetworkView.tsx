import { useCallback, useEffect, useState } from "react";
import type { Event } from "@tauri-apps/api/event";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { useTauriListen } from "../../shared/hooks/useTauriListen";

const MESH_P2P_REPLICATION_PROGRESS_EVENT = "athena:mesh-p2p-replication-progress";
const MESH_P2P_SYNC_HISTORY_EVENT = "athena:mesh-p2p-sync-history";

const SYNC_HISTORY_CAP = 80;

export type MeshDiscoveredPeer = {
  instanceKey: string;
  nodeId: number;
  ipv4: string;
  restPort: number;
  manifestBaseUrl: string;
  lastSeenUnixMs: number;
};

export type P2pMissingEntry = {
  path: string;
  fileType: string;
  remoteSizeBytes: number;
  localSizeBytes: number | null;
  remoteModifiedAt?: number | null;
  category: string;
  reason: string;
};

export type P2pSyncHistoryEntry = {
  status: string;
  path: string;
  peerBaseUrl: string;
  detail: string;
  unixMs: number;
};

export type P2pManifestDiffResult = {
  missing: P2pMissingEntry[];
  remoteNodeId: number;
};

export type MeshP2pReplicationProgressPayload = {
  phase: string;
  filesDone: number;
  filesTotal: number;
  currentPath: string;
  bytesWrittenThisFile: number;
  totalBytesThisFile: number;
};

/** Tauri delivers camelCase from serde on the Rust struct; tolerate snake_case if present. */
type P2pSyncHistoryEntryLoose = P2pSyncHistoryEntry & {
  peer_base_url?: string;
  unix_ms?: number;
};

type MeshP2pReplicationPayloadLoose = MeshP2pReplicationProgressPayload & {
  files_done?: number;
  files_total?: number;
  current_path?: string;
  bytes_written_this_file?: number;
  total_bytes_this_file?: number;
};

function formatBytes(n: number): string {
  if (n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function MeshNetworkView() {
  const [peers, setPeers] = useState<MeshDiscoveredPeer[]>([]);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [syncLog, setSyncLog] = useState<string>("");
  const [lastDiff, setLastDiff] = useState<P2pManifestDiffResult | null>(null);
  const [replicationProgress, setReplicationProgress] = useState<MeshP2pReplicationProgressPayload | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [syncHistory, setSyncHistory] = useState<P2pSyncHistoryEntry[]>([]);

  const refreshPeers = useCallback(async () => {
    try {
      const list = await invoke<MeshDiscoveredPeer[]>("mesh_p2p_list_peers");
      setPeers(list);
    } catch (e) {
      setSyncLog(e != null ? String(e) : "Could not list peers.");
    }
  }, []);

  useEffect(() => {
    void refreshPeers();
  }, [refreshPeers]);

  const onDiscovered = useCallback((ev: Event<MeshDiscoveredPeer>) => {
    const p = ev.payload;
    if (!p?.instanceKey) return;
    setPeers((prev) => {
      const next = prev.filter((x) => x.instanceKey !== p.instanceKey);
      next.push(p);
      next.sort((a, b) => a.ipv4.localeCompare(b.ipv4) || a.restPort - b.restPort || a.nodeId - b.nodeId);
      return next;
    });
  }, []);

  useTauriListen<MeshDiscoveredPeer>("peer_discovered", { handler: onDiscovered });

  const onLost = useCallback((ev: Event<{ instanceKey: string }>) => {
    const k = ev.payload?.instanceKey;
    if (!k) return;
    setPeers((prev) => prev.filter((x) => x.instanceKey !== k));
    setSelectedKey((cur) => (cur === k ? null : cur));
  }, []);

  useTauriListen<{ instanceKey: string }>("peer_lost", { handler: onLost });

  const onReplicationProgress = useCallback((ev: Event<MeshP2pReplicationPayloadLoose>) => {
    const p = ev.payload;
    if (!p) return;
    setReplicationProgress({
      phase: String(p.phase ?? ""),
      filesDone: Number(p.filesDone ?? p.files_done ?? 0),
      filesTotal: Number(p.filesTotal ?? p.files_total ?? 0),
      currentPath: String(p.currentPath ?? p.current_path ?? ""),
      bytesWrittenThisFile: Number(p.bytesWrittenThisFile ?? p.bytes_written_this_file ?? 0),
      totalBytesThisFile: Number(p.totalBytesThisFile ?? p.total_bytes_this_file ?? 0),
    });
  }, []);

  useTauriListen<MeshP2pReplicationPayloadLoose>(MESH_P2P_REPLICATION_PROGRESS_EVENT, {
    handler: onReplicationProgress,
  });

  const onSyncHistory = useCallback((ev: Event<P2pSyncHistoryEntryLoose>) => {
    const p = ev.payload;
    if (!p?.path) return;
    const row: P2pSyncHistoryEntry = {
      status: String(p.status ?? ""),
      path: String(p.path ?? ""),
      peerBaseUrl: String(p.peerBaseUrl ?? p.peer_base_url ?? ""),
      detail: String(p.detail ?? ""),
      unixMs: Number(p.unixMs ?? p.unix_ms ?? 0),
    };
    setSyncHistory((prev) => {
      const next = [row, ...prev];
      return next.length > SYNC_HISTORY_CAP ? next.slice(0, SYNC_HISTORY_CAP) : next;
    });
  }, []);

  useTauriListen<P2pSyncHistoryEntryLoose>(MESH_P2P_SYNC_HISTORY_EVENT, { handler: onSyncHistory });

  function selectedPeer(): MeshDiscoveredPeer | undefined {
    return peers.find((x) => x.instanceKey === selectedKey);
  }

  async function syncRawManifest(): Promise<void> {
    const p = selectedPeer();
    if (!p) {
      setSyncLog("Select a peer first.");
      return;
    }
    const base = p.manifestBaseUrl.replace(/\/$/, "");
    const url = `${base}/mesh/manifest`;
    setBusy("raw-manifest");
    try {
      const data = await invoke<unknown>("mesh_p2p_sync_with_peer", { manifestUrl: url });
      setSyncLog(JSON.stringify(data, null, 2).slice(0, 12_000));
    } catch (e) {
      setSyncLog(e != null ? String(e) : "Sync failed.");
    } finally {
      setBusy(null);
    }
  }

  async function diffManifest(): Promise<void> {
    const p = selectedPeer();
    if (!p) {
      setSyncLog("Select a peer first.");
      return;
    }
    const base = p.manifestBaseUrl.replace(/\/$/, "");
    const url = `${base}/mesh/manifest`;
    setBusy("diff");
    setReplicationProgress(null);
    try {
      const diff = await invoke<P2pManifestDiffResult>("mesh_p2p_manifest_diff", { manifestUrl: url });
      setLastDiff(diff);
      const lines = diff.missing.slice(0, 80).map((m) => `${m.category}\t${m.reason}\t${m.path}`);
      const more = diff.missing.length > 80 ? `\n… ${diff.missing.length - 80} more` : "";
      setSyncLog(
        `Remote node ${diff.remoteNodeId}: ${diff.missing.length} missing or incomplete entries (showing up to 80).\n${lines.join("\n")}${more}`,
      );
    } catch (e) {
      setLastDiff(null);
      setSyncLog(e != null ? String(e) : "Diff failed.");
    } finally {
      setBusy(null);
    }
  }

  async function replicateFromPeer(): Promise<void> {
    const p = selectedPeer();
    if (!p) {
      setSyncLog("Select a peer first.");
      return;
    }
    const base = p.manifestBaseUrl.replace(/\/$/, "");
    setBusy("replicate");
    setReplicationProgress(null);
    try {
      await invoke("mesh_p2p_replicate_from_peer", { peerBaseUrl: base });
      setSyncLog(
        "Replication finished. Use “Diff manifest” to refresh gaps; vector-only gaps still need ingest on this node.",
      );
    } catch (e) {
      setSyncLog(e != null ? String(e) : "Replication failed.");
    } finally {
      setBusy(null);
    }
  }

  const prog = replicationProgress;
  const fileBytePct =
    prog && prog.totalBytesThisFile > 0
      ? Math.min(100, (100 * prog.bytesWrittenThisFile) / prog.totalBytesThisFile)
      : prog && prog.phase === "downloading"
        ? null
        : 0;
  const filesPct =
    prog && prog.filesTotal > 0 ? Math.min(100, (100 * prog.filesDone) / prog.filesTotal) : 0;

  return (
    <section className="glass-panel mesh-p2p-dashboard" style={{ padding: "0.85rem", display: "grid", gap: "0.75rem" }}>
      <div>
        <h2 className="mesh-network-hub-section-title" style={{ margin: 0 }}>
          LAN mesh (P2P)
        </h2>
        <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
          Other Athena desktops on this network advertise a REST port via mDNS. Diff the peer manifest against your
          local index, then replicate missing library files over HTTP (streamed; no full-file RAM on either side).
        </p>
      </div>

      <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap", alignItems: "center" }}>
        <button type="button" className="secondary" onClick={() => void refreshPeers()}>
          Refresh list
        </button>
        <button type="button" className="secondary" onClick={() => void syncRawManifest()} disabled={!selectedKey || busy != null}>
          Raw manifest JSON
        </button>
        <button type="button" className="secondary" onClick={() => void diffManifest()} disabled={!selectedKey || busy != null}>
          Diff manifest
        </button>
        <button type="button" className="glass-action-btn" onClick={() => void replicateFromPeer()} disabled={!selectedKey || busy != null}>
          Replicate missing files
        </button>
      </div>

      {prog ? (
        <div style={{ display: "grid", gap: "0.35rem" }} aria-live="polite">
          <div className="text-muted small" style={{ display: "flex", justifyContent: "space-between", gap: "0.5rem" }}>
            <span>
              {prog.phase} — file {prog.filesDone}/{prog.filesTotal}
            </span>
            {prog.currentPath ? (
              <span style={{ fontFamily: "ui-monospace, monospace", overflow: "hidden", textOverflow: "ellipsis" }}>
                {prog.currentPath}
              </span>
            ) : null}
          </div>
          <div
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={fileBytePct ?? filesPct}
            style={{
              height: "8px",
              borderRadius: "6px",
              background: "rgba(255,255,255,0.08)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                height: "100%",
                width: `${fileBytePct ?? filesPct}%`,
                background: "linear-gradient(90deg, #5ad8a6, #5b8def)",
                transition: "width 0.12s ease-out",
              }}
            />
          </div>
          <div className="text-muted small">
            {prog.totalBytesThisFile > 0
              ? `${formatBytes(prog.bytesWrittenThisFile)} / ${formatBytes(prog.totalBytesThisFile)}`
              : prog.filesTotal > 0
                ? `${prog.filesDone} / ${prog.filesTotal} files`
                : null}
          </div>
        </div>
      ) : null}

      {syncHistory.length > 0 ? (
        <div style={{ display: "grid", gap: "0.35rem" }} aria-label="Sync history">
          <h3 className="text-muted small" style={{ margin: 0, fontSize: "0.85rem", fontWeight: 600 }}>
            Sync history
          </h3>
          <div
            className="glass-panel text-muted small"
            style={{ maxHeight: "10rem", overflow: "auto", padding: "0.5rem", fontSize: "0.82rem" }}
          >
            <ul style={{ margin: 0, paddingLeft: "1.1rem" }}>
              {syncHistory.map((h, i) => (
                <li key={`${h.unixMs}-${i}-${h.path}`} style={{ marginBottom: "0.25rem" }}>
                  <span style={{ fontFamily: "ui-monospace, monospace", opacity: 0.85 }}>[{h.status}]</span>{" "}
                  {h.path}
                  {h.peerBaseUrl ? (
                    <span className="text-muted" style={{ opacity: 0.75 }}>
                      {" "}
                      ← {h.peerBaseUrl}
                    </span>
                  ) : null}
                  {h.detail ? <div style={{ marginTop: "0.15rem", whiteSpace: "pre-wrap" }}>{h.detail}</div> : null}
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}

      {lastDiff && lastDiff.missing.some((m) => m.category === "vector") ? (
        <p className="text-muted small" style={{ margin: 0 }}>
          Some gaps are vector-only (no file pull). Re-ingest or run the embedding worker for those paths after files
          match.
        </p>
      ) : null}

      <div style={{ overflow: "auto", maxHeight: "14rem", borderRadius: "10px" }} aria-label="Discovered mesh peers">
        {peers.length === 0 ? (
          <p className="text-muted small" style={{ margin: 0 }}>
            No peers discovered yet. Ensure another Athena node is running on the same LAN.
          </p>
        ) : (
          <table className="mesh-p2p-peer-table" style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.9rem" }}>
            <thead>
              <tr className="text-muted small">
                <th style={{ textAlign: "left", padding: "0.35rem" }}>Select</th>
                <th style={{ textAlign: "left", padding: "0.35rem" }}>Node</th>
                <th style={{ textAlign: "left", padding: "0.35rem" }}>Address</th>
                <th style={{ textAlign: "left", padding: "0.35rem" }}>REST</th>
              </tr>
            </thead>
            <tbody>
              {peers.map((p) => (
                <tr key={p.instanceKey}>
                  <td style={{ padding: "0.35rem" }}>
                    <input
                      type="radio"
                      name="mesh-peer"
                      checked={selectedKey === p.instanceKey}
                      onChange={() => setSelectedKey(p.instanceKey)}
                      aria-label={`Select peer ${p.nodeId}`}
                    />
                  </td>
                  <td style={{ padding: "0.35rem" }}>{p.nodeId}</td>
                  <td style={{ padding: "0.35rem", fontFamily: "ui-monospace, monospace" }}>{p.ipv4}</td>
                  <td style={{ padding: "0.35rem" }}>{p.restPort}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {syncLog ? (
        <pre
          className="glass-panel text-muted small"
          style={{ margin: 0, maxHeight: "12rem", overflow: "auto", padding: "0.6rem", whiteSpace: "pre-wrap" }}
          aria-live="polite"
        >
          {syncLog}
        </pre>
      ) : null}
    </section>
  );
}
