import { useEffect, useState } from "react";
import { api, ModuleRow } from "../lib/api";

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
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = async () => {
    setRows(await api.modules());
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

  return (
    <div className="module-list">
      <p className="stub-copy">
        Core stays light. Pull heavy assets over the internet or from a LAN peer that already has
        them cached.
      </p>
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
          </p>
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
