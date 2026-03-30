import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type ActionKind = "name" | "region" | "key" | null;
type SavedKind = "name" | "region" | "key" | null;

const REGIONS = ["US915", "EU868", "ANZ915", "AS923", "IN865", "RU864", "KR920"];

export function RadioSettingsView() {
  const [nodeName, setNodeName] = useState("");
  const [region, setRegion] = useState("US915");
  const [primaryKey, setPrimaryKey] = useState("");
  const [loading, setLoading] = useState<ActionKind>(null);
  const [saved, setSaved] = useState<SavedKind>(null);
  const [error, setError] = useState<string | null>(null);

  const isBusy = useMemo(() => loading !== null, [loading]);

  const showSaved = (kind: SavedKind) => {
    setSaved(kind);
    window.setTimeout(() => {
      setSaved((prev) => (prev === kind ? null : prev));
    }, 1800);
  };

  const handleUpdate = async (kind: Exclude<ActionKind, null>, configType: string, value: string) => {
    const normalized = value.trim();
    if (normalized.length === 0) {
      setError("Value cannot be empty.");
      return;
    }
    setError(null);
    setLoading(kind);
    try {
      await invoke<boolean>("set_radio_config", { configType, value: normalized });
      showSaved(kind);
      if (kind === "key") {
        setPrimaryKey("");
      }
    } catch (e) {
      setError(e != null ? String(e) : "Could not update radio config.");
    } finally {
      setLoading(null);
    }
  };

  return (
    <div className="radio-settings-view-scroll">
    <section className="glass-panel" style={{ padding: "0.9rem", marginTop: "0.8rem" }}>
      <h4 style={{ margin: "0 0 0.7rem", color: "var(--primary)" }}>Device Configuration</h4>
      <div style={{ display: "grid", gap: "0.75rem" }}>
        <div className="glass-panel" style={{ padding: "0.7rem", display: "grid", gap: "0.5rem" }}>
          <label htmlFor="radio-node-name-input" style={{ fontWeight: 600 }}>
            Node Identity
          </label>
          <input
            id="radio-node-name-input"
            type="text"
            value={nodeName}
            onChange={(e) => setNodeName(e.target.value)}
            placeholder="Node Name"
            className="map-chat-input"
            disabled={isBusy}
          />
          <button
            type="button"
            className="secondary"
            disabled={isBusy}
            onClick={() => void handleUpdate("name", "node_name", nodeName)}
          >
            {loading === "name" ? "Updating..." : "Update Name"}
          </button>
          {saved === "name" ? <span className="text-muted">Saved</span> : null}
        </div>

        <div className="glass-panel" style={{ padding: "0.7rem", display: "grid", gap: "0.5rem" }}>
          <label htmlFor="radio-region-select" style={{ fontWeight: 600 }}>
            Radio Region
          </label>
          <select
            id="radio-region-select"
            className="theme-select"
            value={region}
            onChange={(e) => setRegion(e.target.value)}
            disabled={isBusy}
          >
            {REGIONS.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="secondary"
            disabled={isBusy}
            onClick={() => void handleUpdate("region", "region", region)}
          >
            {loading === "region" ? "Updating..." : "Update Region"}
          </button>
          {saved === "region" ? <span className="text-muted">Saved</span> : null}
        </div>

        <div className="glass-panel" style={{ padding: "0.7rem", display: "grid", gap: "0.5rem" }}>
          <label htmlFor="radio-primary-key-input" style={{ fontWeight: 600 }}>
            Channel Encryption
          </label>
          <input
            id="radio-primary-key-input"
            type="password"
            value={primaryKey}
            onChange={(e) => setPrimaryKey(e.target.value)}
            placeholder="Primary Channel Key"
            className="map-chat-input"
            disabled={isBusy}
          />
          <button
            type="button"
            className="secondary"
            disabled={isBusy}
            onClick={() => void handleUpdate("key", "primary_channel_key", primaryKey)}
          >
            {loading === "key" ? "Updating..." : "Update Key"}
          </button>
          {saved === "key" ? <span className="text-muted">Saved</span> : null}
        </div>
      </div>
      {error ? (
        <p role="alert" className="text-muted" style={{ margin: "0.75rem 0 0", color: "var(--danger, #ef4444)" }}>
          {error}
        </p>
      ) : null}
    </section>
    </div>
  );
}
