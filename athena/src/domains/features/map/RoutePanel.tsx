import { useCallback, useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";
import type { GeoJSONSource, Map } from "maplibre-gl";
import type { Feature, FeatureCollection, LineString } from "geojson";
import { SRC_ROUTE } from "./maplibreLayerIds";

type ValhallaRouteResult = {
  geojson: unknown;
  mode: string;
  source: string;
  distanceMeters?: number | null;
  durationSeconds?: number | null;
};

type Mode = "walk" | "drive" | "bike" | "offroad";

/**
 * A→B routing via `map_offline_route_geojson` (local Valhalla or straight-line fallback).
 */
export function RoutePanel({ map, onClose }: { map: Map | null; onClose: () => void }) {
  const [mode, setMode] = useState<Mode>("walk");
  const [lat1, setLat1] = useState("");
  const [lon1, setLon1] = useState("");
  const [lat2, setLat2] = useState("");
  const [lon2, setLon2] = useState("");
  const [status, setStatus] = useState("");
  const [valhallaOk, setValhallaOk] = useState<boolean | null>(null);
  const [pickTarget, setPickTarget] = useState<null | "a" | "b">(null);
  const pickTargetRef = useRef<null | "a" | "b">(null);
  pickTargetRef.current = pickTarget;

  const showRoute = useCallback(
    (fc: FeatureCollection) => {
      if (!map || !map.getSource(SRC_ROUTE)) return;
      const src = map.getSource(SRC_ROUTE) as GeoJSONSource;
      src.setData(fc);
    },
    [map],
  );

  useEffect(() => {
    void invoke<{ reachable: boolean; message: string }>("valhalla_get_status")
      .then((s) => {
        setValhallaOk(s.reachable);
        if (!s.reachable) setStatus(`Valhalla: ${s.message} (using straight-line fallback if needed).`);
      })
      .catch(() => setValhallaOk(false));
  }, []);

  const pickA = useCallback(() => {
    if (!map) return;
    const c = map.getCenter();
    setLat1(c.lat.toFixed(6));
    setLon1(c.lng.toFixed(6));
  }, [map]);

  const pickB = useCallback(() => {
    if (!map) return;
    const c = map.getCenter();
    setLat2(c.lat.toFixed(6));
    setLon2(c.lng.toFixed(6));
  }, [map]);

  useEffect(() => {
    if (!map) return;
    const onClick = (e: { lngLat: { lat: number; lng: number } }) => {
      const t = pickTargetRef.current;
      if (t == null) return;
      if (t === "a") {
        setLat1(e.lngLat.lat.toFixed(6));
        setLon1(e.lngLat.lng.toFixed(6));
      } else {
        setLat2(e.lngLat.lat.toFixed(6));
        setLon2(e.lngLat.lng.toFixed(6));
      }
      pickTargetRef.current = null;
      setPickTarget(null);
      setStatus("");
    };
    map.on("click", onClick);
    return () => {
      map.off("click", onClick);
    };
  }, [map]);

  return (
    <div
      className="map-route-panel glass-panel"
      style={{
        position: "absolute",
        left: 12,
        bottom: 12,
        zIndex: 4,
        width: 320,
        maxWidth: "90vw",
        padding: 12,
        pointerEvents: "auto",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <strong>Route</strong>
        <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
          <X size={16} />
        </button>
      </div>
      {valhallaOk !== null ? (
        <p
          className="route-panel-engine-status"
          data-ok={valhallaOk ? "1" : "0"}
          style={{ fontSize: 11, margin: "4px 0 8px" }}
        >
          {valhallaOk ? "Valhalla: reachable" : "Valhalla: offline (straight-line + avoid polygons best-effort in engine)"}
        </p>
      ) : null}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6, fontSize: 12 }}>
        <label>
          A lat
          <input className="map-inp" value={lat1} onChange={(e) => setLat1(e.target.value)} />
        </label>
        <label>
          A lon
          <input className="map-inp" value={lon1} onChange={(e) => setLon1(e.target.value)} />
        </label>
        <label>
          B lat
          <input className="map-inp" value={lat2} onChange={(e) => setLat2(e.target.value)} />
        </label>
        <label>
          B lon
          <input className="map-inp" value={lon2} onChange={(e) => setLon2(e.target.value)} />
        </label>
      </div>
      <div style={{ display: "flex", gap: 6, marginTop: 6, flexWrap: "wrap" }}>
        <button type="button" onClick={pickA}>
          A = map center
        </button>
        <button type="button" onClick={pickB}>
          B = map center
        </button>
        <button
          type="button"
          onClick={() => {
            setPickTarget("a");
            setStatus("Click the map to set A.");
          }}
          aria-pressed={pickTarget === "a"}
        >
          Pick A on map
        </button>
        <button
          type="button"
          onClick={() => {
            setPickTarget("b");
            setStatus("Click the map to set B.");
          }}
          aria-pressed={pickTarget === "b"}
        >
          Pick B on map
        </button>
        <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12 }}>
          Mode
          <select value={mode} onChange={(e) => setMode(e.target.value as Mode)} className="map-inp">
            <option value="walk">Walk</option>
            <option value="drive">Drive</option>
            <option value="bike">Bike</option>
            <option value="offroad">Offroad</option>
          </select>
        </label>
      </div>
      <button
        type="button"
        style={{ marginTop: 10, width: "100%" }}
        onClick={async () => {
          const a1 = parseFloat(lat1);
          const o1 = parseFloat(lon1);
          const a2 = parseFloat(lat2);
          const o2 = parseFloat(lon2);
          if (![a1, o1, a2, o2].every((n) => Number.isFinite(n))) {
            setStatus("Enter four numeric coordinates.");
            return;
          }
          setStatus("Routing…");
          try {
            const r = await invoke<ValhallaRouteResult>("map_offline_route_geojson", {
              lat1: a1,
              lon1: o1,
              lat2: a2,
              lon2: o2,
              mode,
            });
            setStatus(
              `Source: ${r.source} · dist ${r.distanceMeters != null ? `${Math.round(r.distanceMeters)} m` : "n/a"}`,
            );
            const gj = r.geojson;
            if (gj && typeof gj === "object") {
              const t = (gj as { type?: string }).type;
              if (t === "LineString" || t === "Feature" || t === "FeatureCollection") {
                if (t === "LineString") {
                  const feat: Feature<LineString> = {
                    type: "Feature",
                    properties: {},
                    geometry: gj as LineString,
                  };
                  showRoute({ type: "FeatureCollection", features: [feat] });
                } else if (t === "FeatureCollection") {
                  showRoute(gj as FeatureCollection);
                } else {
                  showRoute({ type: "FeatureCollection", features: [gj as Feature] });
                }
                if (map) {
                  try {
                    map.fitBounds(
                      [
                        [Math.min(o1, o2) - 0.05, Math.min(a1, a2) - 0.05],
                        [Math.max(o1, o2) + 0.05, Math.max(a1, a2) + 0.05],
                      ],
                      { padding: 40, duration: 500 },
                    );
                  } catch {
                    /* empty */
                  }
                }
                return;
              }
            }
            setStatus("Unexpected GeoJSON from router.");
          } catch (e) {
            setStatus(String(e));
          }
        }}
      >
        Compute route
      </button>
      {status ? <p style={{ fontSize: 11, marginTop: 8, opacity: 0.9 }}>{status}</p> : null}
    </div>
  );
}
