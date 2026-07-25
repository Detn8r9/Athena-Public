import { useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";

type DiagnosticFailure = {
  key: string;
  message: string;
};

type SidecarBinaryStatus = {
  name: string;
  resolvedPath: string | null;
  executable: boolean;
  present: boolean;
  error: string | null;
};

type ResilienceStatus = {
  maintenanceMode: boolean;
  failures: DiagnosticFailure[];
  sidecars: SidecarBinaryStatus[];
  lastHeartbeatUnixMs: number | null;
  heartbeatComponents: Record<string, string>;
};

export function MaintenanceScreen() {
  const [status, setStatus] = useState<ResilienceStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<ResilienceStatus>("get_resilience_status")
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch((e) => {
        if (!cancelled) setErr(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="glass-panel app-view-placeholder" style={{ padding: "1rem", maxWidth: 900, margin: "0 auto" }}>
      <h2 className="app-view-placeholder-title">Maintenance Mode</h2>
      <p className="text-muted" style={{ marginTop: 0 }}>
        ATHENA detected missing or unhealthy system dependencies at startup. Fix the items below, then restart the station.
      </p>

      {err ? (
        <p className="error" role="alert">
          {err}
        </p>
      ) : null}

      {!status ? (
        <p className="text-muted">Loading diagnostics…</p>
      ) : (
        <div style={{ display: "grid", gap: "0.75rem" }}>
          <div className="glass-panel" style={{ padding: "0.75rem" }}>
            <div style={{ fontWeight: 700, marginBottom: "0.35rem" }}>Startup failures</div>
            {status.failures.length === 0 ? (
              <div className="text-muted small">No failures reported.</div>
            ) : (
              <ul style={{ margin: 0, paddingLeft: "1.25rem" }}>
                {status.failures.map((f) => (
                  <li key={f.key} style={{ marginBottom: "0.35rem" }}>
                    <code>{f.key}</code>: {f.message}
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="glass-panel" style={{ padding: "0.75rem" }}>
            <div style={{ fontWeight: 700, marginBottom: "0.35rem" }}>Sidecar status</div>
            <div style={{ display: "grid", gap: "0.5rem" }}>
              {status.sidecars.map((s) => (
                <div key={s.name} className="glass-panel" style={{ padding: "0.65rem" }}>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "baseline" }}>
                    <div style={{ fontWeight: 700 }}>{s.name}</div>
                    <div className="text-muted small">
                      {s.present ? (s.executable ? "OK" : "Not executable") : "Missing"}
                    </div>
                  </div>
                  {s.resolvedPath ? (
                    <div className="text-muted small">
                      Path: <code>{s.resolvedPath}</code>
                    </div>
                  ) : null}
                  {s.error ? (
                    <div className="text-muted small" role="alert">
                      {s.error}
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
          </div>

          <div className="glass-panel" style={{ padding: "0.75rem" }}>
            <div style={{ fontWeight: 700, marginBottom: "0.35rem" }}>Heartbeat</div>
            <div className="text-muted small">
              Last heartbeat: {status.lastHeartbeatUnixMs ? new Date(status.lastHeartbeatUnixMs).toLocaleString() : "N/A"}
            </div>
            <pre style={{ margin: "0.5rem 0 0", whiteSpace: "pre-wrap" }}>
              {JSON.stringify(status.heartbeatComponents ?? {}, null, 2)}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

