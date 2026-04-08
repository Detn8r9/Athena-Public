import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { AuthProvider } from "./auth/AuthContext";
import { ErrorBoundary } from "./ErrorBoundary";
import { MaintenanceScreen } from "./MaintenanceScreen";

type ResilienceStatus = {
  maintenanceMode: boolean;
};

function Root() {
  const [maintenanceMode, setMaintenanceMode] = React.useState(false);
  const [checked, setChecked] = React.useState(false);

  React.useEffect(() => {
    let cancelled = false;
    void invoke<ResilienceStatus>("get_resilience_status")
      .then((s) => {
        if (!cancelled) setMaintenanceMode(Boolean(s?.maintenanceMode));
      })
      .catch(() => {
        if (!cancelled) setMaintenanceMode(false);
      })
      .finally(() => {
        if (!cancelled) setChecked(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!checked) {
    return (
      <div className="glass-panel app-view-placeholder" style={{ padding: "1rem" }}>
        <h2 className="app-view-placeholder-title">ATHENA</h2>
        <p className="text-muted">Starting systems…</p>
      </div>
    );
  }

  if (maintenanceMode) {
    return (
      <div style={{ padding: "1rem" }}>
        <MaintenanceScreen />
      </div>
    );
  }

  return <App />;
}

const rootEl = document.getElementById("root");
if (!rootEl) {
  throw new Error("Missing #root");
}

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <ErrorBoundary>
      <AuthProvider>
        <Root />
      </AuthProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
