import { useEffect, useState } from "react";
import { api, OfflineMapStatus } from "../lib/api";

export function MapStub() {
  const [status, setStatus] = useState<OfflineMapStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .mapStatus()
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <div>
      <p className="stub-copy">
        Map widget — typed client wired to <code>/api/map/status</code> (Tauri parity stub).
      </p>
      {error ? <p className="stub-copy">{error}</p> : null}
      {status ? (
        <div
          style={{
            marginTop: "0.75rem",
            borderRadius: 12,
            border: "1px solid var(--line)",
            padding: "0.75rem",
            background: "rgba(0,0,0,0.18)",
          }}
        >
          <p style={{ margin: 0, fontSize: "0.9rem" }}>
            Offline maps: {status.ready ? "ready" : "not ready"} · provider {status.provider}
          </p>
          <p className="stub-copy" style={{ marginTop: "0.35rem" }}>
            Basemap: {status.basemap_present ? "yes" : "no"} · tiles {status.tile_count} · regions{" "}
            {status.regions.length || "—"}
          </p>
          <p className="stub-copy" style={{ marginTop: "0.35rem" }}>
            {status.message}
          </p>
        </div>
      ) : (
        <p className="stub-copy" style={{ marginTop: "0.75rem" }}>
          Loading map status…
        </p>
      )}
    </div>
  );
}
