import { useEffect, useState } from "react";
import { api, ModuleRow, PeersResponse } from "../lib/api";

function formatBytes(n: number): string {
  if (!n) return "—";
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(0)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function ModuleDownloader() {
  const [rows, setRows] = useState<ModuleRow[]>([]);
  const [peers, setPeers] = useState<PeersResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);

  const refresh = async () => {
    const [mods, p] = await Promise.all([api.modules(), api.peers()]);
    setRows(mods);
    setPeers(p);
  };

  useEffect(() => {
    void refresh().catch((e) => setError(String(e)));
  }, []);

  const install = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await api.installModule(id, true);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const rescan = async () => {
    setScanning(true);
    setError(null);
    try {
      const p = await api.scanPeers();
      setPeers(p);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  return (
    <div className="module-list">
      <p className="stub-copy">
        Core stays light. Docker voice modules auto-start on install. Prefer LAN peers (docker save →
        load) before hitting the internet.
      </p>

      <div
        style={{
          border: "1px solid var(--line)",
          borderRadius: 12,
          padding: "0.65rem 0.75rem",
          background: "rgba(0,0,0,0.18)",
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", gap: "0.5rem", alignItems: "center" }}>
          <strong style={{ fontSize: "0.9rem" }}>
            LAN peers ({peers?.peers?.length ?? 0})
          </strong>
          <button type="button" className="btn" disabled={scanning} onClick={() => void rescan()}>
            {scanning ? "Scanning…" : "Rescan LAN"}
          </button>
        </div>
        <p className="stub-copy" style={{ marginTop: "0.35rem" }}>
          {peers?.scanned_at
            ? `Last scan ${new Date(peers.scanned_at * 1000).toLocaleTimeString()}`
            : "Boot scan pending or empty — use Rescan LAN."}
        </p>
        {(peers?.peers ?? []).slice(0, 6).map((p) => (
          <p key={p.base_url} className="stub-copy" style={{ marginTop: "0.2rem" }}>
            {p.base_url}
          </p>
        ))}
      </div>

      {error ? <p className="stub-copy">{error}</p> : null}
      {rows.map((m) => (
        <div key={m.id} className="module-card">
          <h4>
            {m.name} {m.required ? "· required" : ""}
          </h4>
          <p>{m.description}</p>
          <p style={{ marginTop: "0.35rem" }}>
            {m.category} · {formatBytes(m.size_bytes)} · {m.status}
            {m.source ? ` via ${m.source}` : ""}
            {m.gpu_effective ? " · GPU" : m.provider === "docker" ? " · CPU" : ""}
          </p>
          {m.resolved_image ? (
            <p className="stub-copy" style={{ marginTop: "0.2rem" }}>
              {m.resolved_image}
              {m.host_port ? ` · :${m.host_port}` : ""}
            </p>
          ) : null}
          {m.status === "downloading" ? (
            <div className="progress">
              <span style={{ width: `${Math.round(m.progress * 100)}%` }} />
            </div>
          ) : null}
          {m.error ? <p className="stub-copy">{m.error}</p> : null}
          <div className="row-actions" style={{ marginTop: "0.45rem" }}>
            <button
              type="button"
              className="btn"
              disabled={busyId === m.id || m.status === "installed" || m.status === "downloading"}
              onClick={() => void install(m.id)}
            >
              {m.status === "installed" ? "Installed" : busyId === m.id ? "Working…" : "Download"}
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
