import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { X, Navigation, Footprints, Bike, Truck, MapPin, Settings } from "lucide-react";
import { AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import type { Map as MlMap, GeoJSONSource } from "maplibre-gl";
import type { Feature, FeatureCollection, LineString } from "geojson";
import { SRC_ROUTE } from "./maplibreLayerIds";
import { RoutingSetupModal } from "./RoutingSetupModal";
import { SoftToast } from "../../../ui/components/SoftToast";

// RoutingSetupModal moved to its own file and redesigned to match Map Archive Manager.

type GeocodeMatch = {
  id: string;
  regionId: string;
  featureClass: string;
  label: string;
  lat: number;
  lon: number;
  importance: number;
};

type EndpointResolution = {
  query: string;
  selected: GeocodeMatch | null;
  candidates: GeocodeMatch[];
  ambiguous: boolean;
};

type ValhallaRouteResult = {
  geojson: unknown;
  mode: string;
  source: string;
  distanceMeters?: number | null;
  durationSeconds?: number | null;
  maneuvers?: unknown;
};

type DirectionsResult = {
  ambiguous: boolean;
  from: EndpointResolution | null;
  to: EndpointResolution | null;
  route: ValhallaRouteResult | null;
  haversineMeters: number;
  fromLabel: string | null;
  toLabel: string | null;
  error: string | null;
};

type MapUnit = "imperial" | "metric";

function formatDistance(m: number, unit: MapUnit): string {
  if (unit === "imperial") {
    const mi = m / 1609.34;
    return mi >= 10 ? `${mi.toFixed(0)} mi` : `${mi.toFixed(1)} mi`;
  }
  return m >= 1000 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`;
}

function formatDuration(sec: number | null | undefined): string {
  if (sec == null || !Number.isFinite(sec)) return "—";
  const s = Math.round(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m} min`;
  return `${s}s`;
}

function formatArrivalClock(sec: number | null | undefined): string | null {
  if (sec == null || !Number.isFinite(sec)) return null;
  const t = new Date(Date.now() + sec * 1000);
  return t.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function geoToFeatureCollection(geo: unknown): FeatureCollection | null {
  if (!geo || typeof geo !== "object") return null;
  const o = geo as { type?: string };
  if (o.type === "LineString") {
    const feat: Feature<LineString> = { type: "Feature", properties: {}, geometry: geo as LineString };
    return { type: "FeatureCollection", features: [feat] };
  }
  if (o.type === "Feature") {
    return { type: "FeatureCollection", features: [geo as Feature] };
  }
  if (o.type === "FeatureCollection") {
    return geo as FeatureCollection;
  }
  return null;
}

function routeSourceLabel(r: ValhallaRouteResult | null, mode: string): string {
  if (!r) return "—";
  if (r.source === "straight_line_fallback") return "Straight-line (engine offline)";
  if (mode === "offroad") return "Paths (Valhalla pedestrian / tracks)";
  if (mode === "walk" || mode === "bike") return `Valhalla (${mode})`;
  return "Roads (Valhalla)";
}

type ManeuverRow = { instruction?: string; narrative?: string; type?: number };

function maneuversToRows(m: unknown): ManeuverRow[] {
  if (!m) return [];
  if (!Array.isArray(m)) return [];
  return m.filter((x) => x && typeof x === "object") as ManeuverRow[];
}

/**
 * Google-style offline directions: from/to text, mode, geocode autocomplete, Valhalla route on map.
 */
export function DirectionsPanel({
  map,
  onClose,
  onOpenAdvancedRoute,
  savedUserLocationAddress,
  routeToLatLng,
  routeRequestSeq,
}: {
  map: MlMap | null;
  onClose: () => void;
  onOpenAdvancedRoute: () => void;
  savedUserLocationAddress?: string;
  routeToLatLng?: { lat: number; lng: number } | null;
  routeRequestSeq?: number;
}) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [mode, setMode] = useState<"drive" | "walk" | "bike" | "offroad">("drive");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [result, setResult] = useState<DirectionsResult | null>(null);
  const [fromHits, setFromHits] = useState<GeocodeMatch[]>([]);
  const [toHits, setToHits] = useState<GeocodeMatch[]>([]);
  const [unit, setUnit] = useState<MapUnit>("imperial");
  const [activeRegionId, setActiveRegionId] = useState<string | null>(null);
  const [turnsOpen, setTurnsOpen] = useState(true);
  const [locBusy, setLocBusy] = useState(false);
  const [routingOpen, setRoutingOpen] = useState(false);
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);
  const routeBusyRef = useRef(false);
  const lastRouteSeqRef = useRef<number>(-1);

  useEffect(() => {
    void invoke<{ unitSystem?: string }>("get_map_display_settings")
      .then((s) => {
        const u = s?.unitSystem?.toLowerCase() ?? "";
        if (u === "metric" || u === "si") setUnit("metric");
        else setUnit("imperial");
      })
      .catch(() => {});
  }, []);

  const refreshActiveRegion = useCallback(() => {
    void invoke<{ activeRoutingRegionId?: string | null }>("valhalla_get_status")
      .then((st) => {
        setActiveRegionId(st.activeRoutingRegionId ?? null);
      })
      .catch(() => setActiveRegionId(null));
  }, []);

  useEffect(() => {
    void refreshActiveRegion();
  }, [refreshActiveRegion]);

  const runSearch = useCallback(
    async (q: string, which: "from" | "to") => {
      const t = q.trim();
      if (t.length < 2) {
        if (which === "from") setFromHits([]);
        else setToHits([]);
        return;
      }
      try {
        const list = await invoke<GeocodeMatch[]>("geocode_search_cmd", {
          query: t,
          region_id: activeRegionId,
          max_results: 8,
        });
        const arr = Array.isArray(list) ? list : [];
        if (which === "from") setFromHits(arr);
        else setToHits(arr);
      } catch {
        if (which === "from") setFromHits([]);
        else setToHits([]);
      }
    },
    [activeRegionId],
  );

  useEffect(() => {
    const id = window.setTimeout(() => void runSearch(from, "from"), 300);
    return () => window.clearTimeout(id);
  }, [from, runSearch]);

  useEffect(() => {
    const id = window.setTimeout(() => void runSearch(to, "to"), 300);
    return () => window.clearTimeout(id);
  }, [to, runSearch]);

  const showRoute = useCallback(
    (r: ValhallaRouteResult | null) => {
      if (!map || !r?.geojson) return;
      const fc = geoToFeatureCollection(r.geojson);
      if (!fc) return;
      try {
        const src = map.getSource(SRC_ROUTE) as GeoJSONSource;
        src.setData(fc);
        const coords: number[][] = [];
        for (const f of fc.features) {
          const g = f.geometry as LineString | undefined;
          if (g?.type === "LineString" && g.coordinates?.length) {
            for (const c of g.coordinates) coords.push(c);
          }
        }
        if (coords.length >= 2) {
          const lons = coords.map((c) => c[0]!);
          const lats = coords.map((c) => c[1]!);
          map.fitBounds(
            [
              [Math.min(...lons), Math.min(...lats)],
              [Math.max(...lons), Math.max(...lats)],
            ],
            { padding: 48, duration: 600, maxZoom: 16 },
          );
        }
      } catch {
        /* empty */
      }
    },
    [map],
  );

  const runRoute = useCallback(
    async (fromValue?: string, toValue?: string) => {
      const fromQ = (fromValue ?? from).trim();
      const toQ = (toValue ?? to).trim();
      setErr(null);
      setResult(null);
      setBusy(true);
      try {
        const res = await invoke<DirectionsResult>("directions_resolve_cmd", { from: fromQ, to: toQ, mode });
        setResult(res);
        if (res.error) {
          setErr(res.error);
        } else if (res.ambiguous) {
          setErr("Multiple matches — pick a suggestion or narrow the name.");
        } else if (res.route) {
          showRoute(res.route);
        }
      } catch (e) {
        setErr(String(e));
      } finally {
        setBusy(false);
      }
    },
    [from, to, mode, showRoute],
  );

  const submit = useCallback(async () => runRoute(), [runRoute]);

  const pickCandidate = (which: "from" | "to", m: GeocodeMatch) => {
    if (which === "from") {
      setFrom(m.label);
      setFromHits([]);
    } else {
      setTo(m.label);
      setToHits([]);
    }
  };

  const useMyLocation = useCallback(async () => {
    setLocBusy(true);
    setErr(null);
    try {
      const s = await invoke<{
        lastKnownGpsLat?: number | null;
        lastKnownGpsLng?: number | null;
      }>("get_map_display_settings");
      let lat = s?.lastKnownGpsLat ?? null;
      let lon = s?.lastKnownGpsLng ?? null;
      if (lat == null || lon == null) {
        if (typeof navigator === "undefined" || !navigator.geolocation) {
          setToast({
            tone: "warning",
            message: "No location set in ATHENA. Please set your location in Settings, or enable macOS Location Services.",
          });
          return;
        }
        const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
          navigator.geolocation.getCurrentPosition(resolve, reject, {
            enableHighAccuracy: true,
            timeout: 12_000,
            maximumAge: 60_000,
          });
        });
        lat = pos.coords.latitude;
        lon = pos.coords.longitude;
        const cur = await invoke<{
          coordFormat?: string | null;
          unitSystem?: string | null;
          defaultTravelMode?: string | null;
        }>("get_map_display_settings");
        await invoke("set_map_display_settings", {
          value: {
            coordFormat: cur.coordFormat ?? null,
            unitSystem: cur.unitSystem ?? null,
            defaultTravelMode: cur.defaultTravelMode ?? null,
            lastKnownGpsLat: lat,
            lastKnownGpsLng: lon,
          },
        });
      }
      const hits = await invoke<GeocodeMatch[]>("geocode_reverse_cmd", {
        lat,
        lon,
        region_id: activeRegionId,
        max_results: 1,
      });
      const h = Array.isArray(hits) && hits[0] ? hits[0] : null;
      if (h) {
        setFrom(h.label);
      } else {
        setFrom(`${lat.toFixed(5)}, ${lon.toFixed(5)}`);
      }
    } catch (e) {
      setToast({
        tone: "warning",
        message: "No location set in ATHENA. Please set your location in Settings, or enable macOS Location Services.",
      });
    } finally {
      setLocBusy(false);
    }
  }, [activeRegionId]);

  useEffect(() => {
    const seq = typeof routeRequestSeq === "number" ? routeRequestSeq : -1;
    if (!routeToLatLng || seq < 0) return;
    if (seq === lastRouteSeqRef.current) return;
    lastRouteSeqRef.current = seq;
    if (routeBusyRef.current) return;
    routeBusyRef.current = true;

    void (async () => {
      try {
        const lat = routeToLatLng.lat;
        const lng = routeToLatLng.lng;
        const toText = `${lat.toFixed(5)}, ${lng.toFixed(5)}`;
        setTo(toText);

        // Explicitly set origin to ATHENA Settings → Location (address/area) when available.
        let fromText = "";
        try {
          const saved = await invoke<string | null>("get_user_location");
          fromText = String(saved ?? "").trim();
        } catch {
          fromText = "";
        }
        if (fromText.length > 0) {
          setFrom(fromText);
          await runRoute(fromText, toText);
          return;
        }

        // Fallback: last-known GPS / native geolocation (updates From field), then route.
        await useMyLocation();
        const after = from.trim();
        await runRoute(after.length ? after : undefined, toText);
      } finally {
        routeBusyRef.current = false;
      }
    })();
  }, [routeToLatLng, routeRequestSeq, from, runRoute, useMyLocation]);

  const summary = useMemo(() => {
    if (!result?.route) return null;
    const r = result.route;
    const dist = r.distanceMeters ?? result.haversineMeters;
    const dLabel = formatDistance(dist, unit);
    const tLabel = formatDuration(r.durationSeconds);
    const arrive = formatArrivalClock(r.durationSeconds);
    const src = routeSourceLabel(r, mode);
    return { dLabel, tLabel, src, arrive };
  }, [result, unit, mode]);

  const maneuverRows = useMemo(
    () => maneuversToRows(result?.route?.maneuvers),
    [result?.route?.maneuvers],
  );

  return (
    <div
      className="map-directions-panel glass-panel"
      style={{
        position: "fixed",
        left: "var(--map-panel-left, 96px)",
        top: "var(--header-offset, 72px)",
        zIndex: 1205,
        width: 360,
        maxWidth: "92vw",
        height: "calc(100dvh - var(--header-offset, 72px) - env(safe-area-inset-bottom, 0px))",
        overflow: "hidden",
        padding: 0,
        pointerEvents: "auto",
      }}
      role="dialog"
      aria-label="Directions"
      aria-modal="true"
    >
      <div className="map-directions-panel__head">
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button
            type="button"
            className={`close-btn${routingOpen ? " is-active" : ""}`}
            onClick={() => setRoutingOpen(true)}
            aria-label="Open routing setup"
            title="Routing setup"
          >
            <Settings size={16} />
          </button>
          <strong>Directions</strong>
        </div>
        <button type="button" className="close-btn" onClick={onClose} aria-label="Close directions">
          <X size={16} />
        </button>
      </div>
      <div className="map-directions-panel__scroll">
        <p className="text-muted small" style={{ margin: "0 0 8px" }}>
          Offline place search from a built geocode index (PBF). Use the gear for Valhalla / routing regions. Valhalla is
          optional for road routes.
        </p>
        <div style={{ display: "grid", gap: 8 }}>
        <label className="map-directions-field">
          <span className="text-muted small">From</span>
          <div className="map-directions-row">
            <input
              className="map-inp"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              placeholder="Address or place name"
              autoComplete="off"
            />
            <button
              type="button"
              className="secondary small-btn map-directions-loc"
              onClick={() => void useMyLocation()}
              disabled={locBusy}
              title="Use last known or current GPS, then label from reverse geocode"
            >
              <MapPin size={16} />
            </button>
          </div>
          {fromHits.length > 0 ? (
            <ul className="map-directions-suggest">
              {fromHits.map((h) => (
                <li key={h.id}>
                  <button type="button" onClick={() => pickCandidate("from", h)}>
                    {h.label} <span className="text-muted">({h.featureClass})</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </label>
        <label className="map-directions-field">
          <span className="text-muted small">To</span>
          <input
            className="map-inp"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            placeholder="Address or place name"
            autoComplete="off"
          />
          {toHits.length > 0 ? (
            <ul className="map-directions-suggest">
              {toHits.map((h) => (
                <li key={h.id}>
                  <button type="button" onClick={() => pickCandidate("to", h)}>
                    {h.label} <span className="text-muted">({h.featureClass})</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </label>
        <div className="map-directions-modes" role="group" aria-label="Travel mode">
          {(
            [
              ["drive", "Drive", Navigation],
              ["walk", "Walk", Footprints],
              ["bike", "Bike", Bike],
              ["offroad", "Offroad", Truck],
            ] as const
          ).map(([k, lab, Icon]) => (
            <button
              key={k}
              type="button"
              className={`map-directions-mode${mode === k ? " is-active" : ""}`}
              onClick={() => setMode(k)}
            >
              <Icon size={16} aria-hidden />
              {lab}
            </button>
          ))}
        </div>
        <button
          type="button"
          style={{ marginTop: 4 }}
          onClick={() => void submit()}
          disabled={busy || !from.trim() || !to.trim()}
        >
          {busy ? "…" : "Get directions"}
        </button>
        {err ? (
          <p className="quartermaster-hub-err" role="alert">
            {err}
          </p>
        ) : null}
        {result?.ambiguous && (result.from?.ambiguous || result.to?.ambiguous) ? (
          <div className="map-directions-disambig" role="dialog" aria-label="Disambiguate places" aria-modal="true">
            <p className="text-muted small" style={{ marginTop: 0 }}>
              Several OSM features matched. Choose the correct endpoints:
            </p>
            {result.from?.ambiguous ? (
              <div className="text-muted small">
                <strong>From</strong>
                <ul>
                  {result.from.candidates.map((c) => (
                    <li key={c.id}>
                      <button type="button" onClick={() => setFrom(c.label)}>
                        {c.label}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
            {result.to?.ambiguous ? (
              <div className="text-muted small">
                <strong>To</strong>
                <ul>
                  {result.to.candidates.map((c) => (
                    <li key={c.id}>
                      <button type="button" onClick={() => setTo(c.label)}>
                        {c.label}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
        ) : null}
        {summary && result && !result.error && !result.ambiguous ? (
          <div className="map-directions-summary">
            <div>
              <strong>{summary.dLabel}</strong> · {summary.tLabel} ETA
              {summary.arrive ? (
                <span className="text-muted small"> · arr {summary.arrive}</span>
              ) : null}
            </div>
            <div className="text-muted small">{summary.src}</div>
            {result.fromLabel && result.toLabel ? (
              <div className="text-muted small">
                {result.fromLabel} → {result.toLabel}
              </div>
            ) : null}
          </div>
        ) : null}
        {maneuverRows.length > 0 && result && !result.ambiguous ? (
          <details className="map-directions-turns" open={turnsOpen} onToggle={(e) => setTurnsOpen(e.currentTarget.open)}>
            <summary>Turn-by-turn ({maneuverRows.length})</summary>
            <ol>
              {maneuverRows.map((row, i) => (
                <li key={i}>{row.instruction || row.narrative || "—"}</li>
              ))}
            </ol>
          </details>
        ) : null}
        <AnimatePresence initial={false}>
          <RoutingSetupModal
            open={routingOpen}
            onClose={() => setRoutingOpen(false)}
            savedUserLocationAddress={savedUserLocationAddress ?? ""}
          />
        </AnimatePresence>
          <button type="button" className="secondary small-btn" onClick={onOpenAdvancedRoute}>
            Advanced: lat/lon route
          </button>
        </div>
      </div>
      <SoftToast
        open={toast != null}
        tone={toast?.tone ?? "warning"}
        message={toast?.message ?? ""}
        onClose={() => setToast(null)}
      />
    </div>
  );
}
