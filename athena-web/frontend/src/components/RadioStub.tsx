import { useEffect, useState } from "react";
import { api, MeshRadioStatus } from "../lib/api";

export function RadioStub() {
  const [status, setStatus] = useState<MeshRadioStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .radioStatus()
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <div>
      <p className="stub-copy">
        Radio / mesh — typed client wired to <code>/api/radio/status</code> (Tauri parity stub).
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
            Mesh: {status.connected ? "connected" : "disconnected"} · {status.provider}
          </p>
          <p className="stub-copy" style={{ marginTop: "0.35rem" }}>
            Node {status.node_id ?? "—"} · channel {status.channel ?? "—"} · peers heard{" "}
            {status.peers_heard}
          </p>
          <p className="stub-copy" style={{ marginTop: "0.35rem" }}>
            {status.message}
          </p>
        </div>
      ) : (
        <p className="stub-copy" style={{ marginTop: "0.75rem" }}>
          Loading radio status…
        </p>
      )}
      <div className="row-actions" style={{ marginTop: "0.75rem" }}>
        <button type="button" className="btn" disabled>
          Scan channels
        </button>
        <button type="button" className="btn" disabled>
          Open logbook
        </button>
      </div>
    </div>
  );
}
