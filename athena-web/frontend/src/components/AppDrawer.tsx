import { useEffect, useState } from "react";
import { api, CatalogApp, InstalledApp } from "../lib/api";

type Props = {
  onClose: () => void;
  maxPinned: number;
};

export function AppDrawer({ onClose, maxPinned }: Props) {
  const [catalog, setCatalog] = useState<CatalogApp[]>([]);
  const [installed, setInstalled] = useState<InstalledApp[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = async () => {
    const [c, i] = await Promise.all([api.appsCatalog(), api.appsInstalled().catch(() => [])]);
    setCatalog(c);
    setInstalled(i);
  };

  useEffect(() => {
    void refresh().catch((e) => setError(String(e)));
  }, []);

  const pinnedCount = installed.filter((a) => a.pinned).length;

  const install = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      await api.installApp(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const togglePin = async (app: InstalledApp) => {
    if (!app.pinned && pinnedCount >= maxPinned) {
      setError(`Home grid allows ${maxPinned} pinned apps. Unpin one first.`);
      return;
    }
    setBusy(app.id);
    try {
      await api.pinApp(app.id, !app.pinned);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <div className="drawer-backdrop" onClick={onClose} />
      <aside className="drawer" aria-label="More Apps">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h2>More Apps</h2>
          <button type="button" className="btn ghost" onClick={onClose}>
            Close
          </button>
        </div>
        <p className="stub-copy">
          Installed Docker apps stay here unless pinned to the home grid (max {maxPinned}). Ports
          are assigned automatically when they collide.
        </p>
        {error ? <p className="stub-copy">{error}</p> : null}

        <h3 style={{ margin: "0.5rem 0 0", fontSize: "0.85rem", color: "var(--muted)" }}>
          Installed
        </h3>
        <div className="app-list">
          {installed.length === 0 ? <p className="stub-copy">No apps installed yet.</p> : null}
          {installed.map((app) => (
            <div key={app.id} className="app-card">
              <h4>
                {app.name} {app.pinned ? "· pinned" : ""}
              </h4>
              <p>
                :{app.host_port} → {app.container_port} · {app.status}
              </p>
              <div className="row-actions" style={{ marginTop: "0.4rem" }}>
                <button type="button" className="btn" onClick={() => void api.startApp(app.id).then(refresh)}>
                  Start
                </button>
                <button type="button" className="btn" onClick={() => void api.stopApp(app.id).then(refresh)}>
                  Stop
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={busy === app.id}
                  onClick={() => void togglePin(app)}
                >
                  {app.pinned ? "Unpin" : "Pin"}
                </button>
                <a className="btn" href={`http://${window.location.hostname}:${app.host_port}`} target="_blank" rel="noreferrer">
                  Open
                </a>
              </div>
            </div>
          ))}
        </div>

        <h3 style={{ margin: "0.75rem 0 0", fontSize: "0.85rem", color: "var(--muted)" }}>
          App store
        </h3>
        <div className="app-list">
          {catalog.map((app) => {
            const already = installed.some((i) => i.id === app.id);
            return (
              <div key={app.id} className="app-card">
                <h4>{app.name}</h4>
                <p>{app.description}</p>
                <div className="row-actions" style={{ marginTop: "0.4rem" }}>
                  <button
                    type="button"
                    className="btn primary"
                    disabled={already || busy === app.id}
                    onClick={() => void install(app.id)}
                  >
                    {already ? "Installed" : busy === app.id ? "Installing…" : "Install"}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </aside>
    </>
  );
}
