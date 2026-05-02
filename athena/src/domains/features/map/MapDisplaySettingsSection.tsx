import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type MapDisplaySettings = {
  coordFormat?: string | null;
  unitSystem?: string | null;
  defaultTravelMode?: string | null;
  lastKnownGpsLat?: number | null;
  lastKnownGpsLng?: number | null;
};

/**
 * Settings → Maps: same config as `get_map_display_settings` / `set_map_display_settings` (mirrors map gear popover).
 */
export function MapDisplaySettingsSection() {
  const [s, setS] = useState<MapDisplaySettings | null>(null);
  const [err, setErr] = useState("");

  const load = useCallback(async () => {
    try {
      const v = await invoke<MapDisplaySettings>("get_map_display_settings");
      setS(v);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const save = useCallback(
    async (next: MapDisplaySettings) => {
      setErr("");
      try {
        await invoke("set_map_display_settings", { value: next });
        setS(next);
      } catch (e) {
        setErr(String(e));
      }
    },
    [],
  );

  if (!s) {
    return <p className="text-muted small">{err || "Loading map display settings…"}</p>;
  }

  return (
    <section className="library-section">
      <h3>Map display defaults</h3>
      <p className="text-muted small">Used by the in-map coordinate readout and routing defaults. Also available from the map’s gear popover.</p>
      {err ? <p className="error small">{err}</p> : null}
      <div className="settings-form-grid" style={{ display: "grid", gap: "0.75rem", maxWidth: 420 }}>
        <label>
          Coordinate format
          <select
            className="settings-input"
            value={s.coordFormat ?? "decimal"}
            onChange={(e) => {
              const coordFormat = e.target.value;
              void save({ ...s, coordFormat });
            }}
          >
            <option value="decimal">Decimal</option>
            <option value="dms">DMS</option>
            <option value="mgrs">MGRS</option>
          </select>
        </label>
        <label>
          Unit system
          <select
            className="settings-input"
            value={s.unitSystem ?? "imperial"}
            onChange={(e) => {
              const unitSystem = e.target.value;
              void save({ ...s, unitSystem });
            }}
          >
            <option value="imperial">Imperial (mi / ft)</option>
            <option value="metric">Metric (km / m)</option>
          </select>
        </label>
        <label>
          Default travel mode
          <select
            className="settings-input"
            value={s.defaultTravelMode ?? "walk"}
            onChange={(e) => {
              const defaultTravelMode = e.target.value;
              void save({ ...s, defaultTravelMode });
            }}
          >
            <option value="walk">Walk</option>
            <option value="drive">Drive</option>
            <option value="bike">Bike</option>
            <option value="offroad">Offroad</option>
          </select>
        </label>
        <div className="settings-inline-actions">
          <button
            type="button"
            onClick={() => {
              if (typeof navigator === "undefined" || !navigator.geolocation) {
                setErr("Geolocation not available.");
                return;
              }
              navigator.geolocation.getCurrentPosition(
                (pos) => {
                  void save({
                    ...s,
                    lastKnownGpsLat: pos.coords.latitude,
                    lastKnownGpsLng: pos.coords.longitude,
                  });
                },
                (e) => setErr(e.message || "Geolocation failed"),
                { enableHighAccuracy: true, timeout: 12000 },
              );
            }}
          >
            Save current GPS to profile
          </button>
        </div>
        {s.lastKnownGpsLat != null && s.lastKnownGpsLng != null ? (
          <p className="text-muted small">
            Last saved fix: {s.lastKnownGpsLat.toFixed(5)}, {s.lastKnownGpsLng.toFixed(5)}
          </p>
        ) : (
          <p className="text-muted small">No last-known GPS stored yet.</p>
        )}
      </div>
    </section>
  );
}
