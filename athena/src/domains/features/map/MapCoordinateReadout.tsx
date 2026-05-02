import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Map } from "maplibre-gl";
import type { CoordinateConversionResult } from "./mapViewTypes";

type CoordMode = "decimal" | "dms" | "mgrs" | "utm";

/**
 * Bottom-right readout: formats follow app map display settings when available.
 */
export function MapCoordinateReadout({
  map: _map,
  cursorLatLng,
  coordHud,
  hudShowDd,
  onToggleHud: _onToggleHud,
}: {
  map: Map | null;
  cursorLatLng: { lat: number; lng: number } | null;
  coordHud: CoordinateConversionResult | null;
  hudShowDd: boolean;
  onToggleHud: (v: boolean) => void;
}) {
  const [mode, setMode] = useState<CoordMode | "auto">("auto");
  const [configMode, setConfigMode] = useState<string | null>(null);

  useEffect(() => {
    void invoke<{
      coordFormat?: string | null;
    }>("get_map_display_settings")
      .then((c) => setConfigMode(c.coordFormat ?? null))
      .catch(() => setConfigMode(null));
  }, [cursorLatLng?.lat, cursorLatLng?.lng]);

  if (!cursorLatLng || !coordHud) {
    return null;
  }

  const eff: CoordMode =
    mode === "auto"
      ? (configMode === "dms" || configMode === "mgrs" || configMode === "utm" || configMode === "decimal"
          ? (configMode as CoordMode)
          : "decimal")
      : mode;

  const line1 =
    eff === "dms" ? `${coordHud.decimal_degrees_lat} / ${coordHud.decimal_degrees_lng}` : coordHud.decimal_degrees;
  const line2 = eff === "mgrs" ? coordHud.mgrs : eff === "utm" ? coordHud.utm : coordHud.decimal_degrees;

  return (
    <div
      className="map-coord-readout glass-panel"
      style={{
        marginTop: 8,
        minWidth: 200,
        padding: "8px 10px",
        fontSize: 12,
        lineHeight: 1.35,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 4 }}>
        <span style={{ opacity: 0.8 }}>Cursor</span>
        <select
          value={mode}
          onChange={(e) => setMode(e.target.value as CoordMode | "auto")}
          className="map-coord-mode-select"
          style={{ fontSize: 11 }}
        >
          <option value="auto">Auto (settings)</option>
          <option value="decimal">Decimal</option>
          <option value="dms">DMS (fields)</option>
          <option value="mgrs">MGRS</option>
          <option value="utm">UTM</option>
        </select>
      </div>
      {hudShowDd && eff === "dms" ? (
        <div>
          {coordHud.decimal_degrees_lat} · {coordHud.decimal_degrees_lng}
        </div>
      ) : (
        <div style={{ fontFamily: "var(--athena-mono, ui-monospace, monospace)" }}>{line1}</div>
      )}
      {eff === "mgrs" || eff === "utm" ? <div style={{ marginTop: 2, opacity: 0.9 }}>{line2}</div> : null}
    </div>
  );
}
