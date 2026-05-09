import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import maplibregl, { type Map as MlMap, type StyleSpecification } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { FeatureCollection } from "geojson";
import { bboxPolygon, regionFeatureCollection } from "./mapProvisionerRegionBounds";
import { formatBytesBinary } from "../../shared/format/formatBytesBinary";
import { useDownloadQueueStore } from "../../stores/downloadQueueStore";
import "./MapProvisioner.css";

const SRC_EXISTING = "da-provision-existing";
const SRC_TARGET = "da-provision-target";
const LAYER_EXISTING_FILL = "da-provision-existing-fill";
const LAYER_EXISTING_LINE = "da-provision-existing-line";
const LAYER_TARGET_FILL = "da-provision-target-fill";
const LAYER_TARGET_LINE = "da-provision-target-line";

/**
 * Blended average download density (MiB per square mile) for tactical offline use ~ZL 0–15:
 * pyramidal MBTiles/PMTiles-style footprint, not max-zoom raw tile counts.
 */
export const LAYER_MB_PER_SQ_MI = {
  satellite: 4.5,
  topo: 1.2,
  street: 0.8,
  terrain3d: 2.5,
} as const;

function formatDensityMibPerSqMi(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0";
  const t = n.toFixed(1);
  return t.endsWith(".0") ? t.slice(0, -2) : t;
}

export type ProvisionLayerId = keyof typeof LAYER_MB_PER_SQ_MI;

export interface MapProvisionerDownloadedLike {
  region: string;
  name: string;
}

export interface MapProvisionerProps {
  downloadedMaps: MapProvisionerDownloadedLike[];
  onProvisionEstimateBytesChange: (bytes: number) => void;
}

function emptyFc(): FeatureCollection {
  return { type: "FeatureCollection", features: [] };
}

function buildProvisionerStyle(): StyleSpecification {
  return {
    version: 8,
    name: "athena-provisioner-preview",
    glyphs: "https://demotiles.maplibre.org/font/{fontstack}/{range}.pbf",
    sources: {
      osm: {
        type: "raster",
        tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
        tileSize: 256,
        attribution: "© OpenStreetMap",
      },
      [SRC_EXISTING]: { type: "geojson", data: emptyFc() },
      [SRC_TARGET]: { type: "geojson", data: emptyFc() },
    },
    layers: [
      { id: "da-osm", type: "raster", source: "osm" },
      {
        id: LAYER_EXISTING_FILL,
        type: "fill",
        source: SRC_EXISTING,
        paint: {
          "fill-color": "#22d3ee",
          "fill-opacity": 0.14,
          "fill-opacity-transition": { duration: 400, delay: 0 },
        },
      },
      {
        id: LAYER_EXISTING_LINE,
        type: "line",
        source: SRC_EXISTING,
        paint: {
          "line-color": "#22d3ee",
          "line-width": 1.2,
          "line-opacity": 0.45,
        },
      },
      {
        id: LAYER_TARGET_FILL,
        type: "fill",
        source: SRC_TARGET,
        paint: {
          "fill-color": "#f87171",
          "fill-opacity": 0.22,
          "fill-opacity-transition": { duration: 550, delay: 0 },
        },
      },
      {
        id: LAYER_TARGET_LINE,
        type: "line",
        source: SRC_TARGET,
        paint: {
          "line-color": "#f87171",
          "line-width": 2.4,
          "line-opacity": 0.95,
        },
      },
    ],
  };
}

/** Axis-aligned square bbox (degrees) circumscribing a circle of `radiusMiles` around center. */
function squareBboxFromRadiusMiles(centerLat: number, centerLng: number, radiusMiles: number): [number, number, number, number] {
  const r = Math.max(0.25, radiusMiles);
  const latFactor = 1 / 69.172;
  const cosLat = Math.max(0.2, Math.cos((centerLat * Math.PI) / 180));
  const lonFactor = 1 / (69.172 * cosLat);
  const dLat = r * latFactor;
  const dLon = r * lonFactor;
  return [centerLng - dLon, centerLat - dLat, centerLng + dLon, centerLat + dLat];
}

function parseLatLonInput(raw: string): { lat: number; lng: number } | null {
  const t = raw.trim();
  const m = t.match(
    /^([-+]?\d+(?:\.\d+)?)\s*[,;\s]+\s*([-+]?\d+(?:\.\d+)?)$/,
  );
  if (!m) return null;
  const a = Number(m[1]);
  const b = Number(m[2]);
  if (!Number.isFinite(a) || !Number.isFinite(b)) return null;
  if (Math.abs(a) <= 90 && Math.abs(b) <= 180) {
    return { lat: a, lng: b };
  }
  if (Math.abs(b) <= 90 && Math.abs(a) <= 180) {
    return { lat: b, lng: a };
  }
  return null;
}

async function geocodeNominatim(query: string, signal: AbortSignal): Promise<{ lat: number; lng: number; label: string } | null> {
  const q = query.trim();
  if (!q) return null;
  const url = new URL("https://nominatim.openstreetmap.org/search");
  url.searchParams.set("format", "json");
  url.searchParams.set("limit", "1");
  url.searchParams.set("q", q);
  const res = await fetch(url.toString(), {
    signal,
    headers: {
      Accept: "application/json",
      "User-Agent": "AthenaMapProvisioner/1.0 (offline geographic provisioning preview)",
    },
  });
  if (!res.ok) return null;
  const rows = (await res.json()) as Array<{ lat: string; lon: string; display_name?: string }>;
  const row = rows[0];
  if (!row) return null;
  const lat = Number(row.lat);
  const lng = Number(row.lon);
  if (!Number.isFinite(lat) || !Number.isFinite(lng)) return null;
  return { lat, lng, label: row.display_name ?? q };
}

export function MapProvisioner({
  downloadedMaps,
  onProvisionEstimateBytesChange,
}: MapProvisionerProps) {
  const pinMapProvisionBytes = useDownloadQueueStore((s) => s.pinMapProvisionBytes);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MlMap | null>(null);
  const pulseRef = useRef<number | null>(null);
  const [mapReady, setMapReady] = useState(false);

  const [targetQuery, setTargetQuery] = useState("");
  const [radiusMiles, setRadiusMiles] = useState(35);
  const [center, setCenter] = useState<{ lat: number; lng: number } | null>(null);
  const [geoLabel, setGeoLabel] = useState<string | null>(null);
  const [geoBusy, setGeoBusy] = useState(false);
  const [geoErr, setGeoErr] = useState<string | null>(null);

  const [layers, setLayers] = useState<Record<ProvisionLayerId, boolean>>({
    satellite: true,
    topo: false,
    street: true,
    terrain3d: false,
  });

  const existingFc = useMemo(() => {
    const regions = downloadedMaps.map((m) => m.region);
    return regionFeatureCollection(regions);
  }, [downloadedMaps]);

  const targetFc = useMemo((): FeatureCollection => {
    if (!center) return emptyFc();
    const [w, s, e, n] = squareBboxFromRadiusMiles(center.lat, center.lng, radiusMiles);
    return {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: { kind: "provision-bbox" },
          geometry: bboxPolygon(w, s, e, n),
        },
      ],
    };
  }, [center, radiusMiles]);

  const areaSqMi = useMemo(() => {
    if (!center) return 0;
    const sideMi = 2 * radiusMiles;
    return sideMi * sideMi;
  }, [center, radiusMiles]);

  const estimateBytes = useMemo(() => {
    if (!center || areaSqMi <= 0) return 0;
    let mb = 0;
    (Object.keys(LAYER_MB_PER_SQ_MI) as ProvisionLayerId[]).forEach((id) => {
      if (layers[id]) mb += LAYER_MB_PER_SQ_MI[id] * areaSqMi;
    });
    return Math.round(mb * 1024 * 1024);
  }, [center, areaSqMi, layers]);

  useEffect(() => {
    onProvisionEstimateBytesChange(estimateBytes);
  }, [estimateBytes, onProvisionEstimateBytesChange]);

  useEffect(() => {
    if (!hostRef.current || mapRef.current) return;
    setMapReady(false);
    const map = new maplibregl.Map({
      container: hostRef.current,
      style: buildProvisionerStyle(),
      center: [-98, 39],
      zoom: 3.2,
      minZoom: 1.5,
      maxZoom: 14,
      pitch: 0,
      bearing: 0,
      dragRotate: false,
      touchPitch: false,
      attributionControl: { compact: true },
    });
    mapRef.current = map;

    map.on("load", () => {
      setMapReady(true);
    });

    return () => {
      if (pulseRef.current != null) {
        window.clearInterval(pulseRef.current);
        pulseRef.current = null;
      }
      setMapReady(false);
      map.remove();
      mapRef.current = null;
    };
  }, []);

  useEffect(() => {
    const map = mapRef.current;
    if (!mapReady || !map?.isStyleLoaded()) return;
    const src = map.getSource(SRC_EXISTING) as maplibregl.GeoJSONSource | undefined;
    if (src) src.setData(existingFc);
    try {
      const b = new maplibregl.LngLatBounds();
      let any = false;
      for (const f of existingFc.features) {
        if (f.geometry.type !== "Polygon") continue;
        for (const ring of f.geometry.coordinates) {
          for (const pt of ring) {
            b.extend(pt as [number, number]);
            any = true;
          }
        }
      }
      if (any) map.fitBounds(b, { padding: 36, maxZoom: 5.5, duration: 400 });
    } catch {
      // ignore fitBounds failures
    }
  }, [mapReady, existingFc]);

  useEffect(() => {
    const map = mapRef.current;
    if (!mapReady || !map?.isStyleLoaded()) return;
    const src = map.getSource(SRC_TARGET) as maplibregl.GeoJSONSource | undefined;
    if (src) src.setData(targetFc);
    if (center && targetFc.features.length > 0) {
      const g = targetFc.features[0]!.geometry;
      if (g.type === "Polygon") {
        const ring = g.coordinates[0] ?? [];
        const b = new maplibregl.LngLatBounds();
        for (const pt of ring) b.extend(pt as [number, number]);
        map.fitBounds(b, { padding: 48, maxZoom: 12, duration: 600 });
      }
    }
  }, [mapReady, targetFc, center]);

  useEffect(() => {
    const map = mapRef.current;
    if (!mapReady || !map?.isStyleLoaded() || !center) {
      if (pulseRef.current != null) {
        window.clearInterval(pulseRef.current);
        pulseRef.current = null;
      }
      return;
    }
    if (pulseRef.current != null) window.clearInterval(pulseRef.current);
    let o = 0.18;
    let dir: 1 | -1 = 1;
    pulseRef.current = window.setInterval(() => {
      const m = mapRef.current;
      if (!m?.isStyleLoaded()) return;
      o += 0.045 * dir;
      if (o >= 0.42) dir = -1;
      if (o <= 0.12) dir = 1;
      try {
        m.setPaintProperty(LAYER_TARGET_FILL, "fill-opacity", o);
      } catch {
        // layer may be removed during teardown
      }
    }, 480);
    return () => {
      if (pulseRef.current != null) {
        window.clearInterval(pulseRef.current);
        pulseRef.current = null;
      }
    };
  }, [mapReady, center]);

  const runResolveTarget = useCallback(async () => {
    setGeoErr(null);
    const raw = targetQuery.trim();
    if (!raw) {
      setGeoErr("Enter a place name or coordinates.");
      return;
    }
    const coord = parseLatLonInput(raw);
    if (coord) {
      setCenter(coord);
      setGeoLabel(`${coord.lat.toFixed(4)}, ${coord.lng.toFixed(4)} (manual)`);
      return;
    }
    setGeoBusy(true);
    const ac = new AbortController();
    const tid = window.setTimeout(() => ac.abort(), 12_000);
    try {
      const hit = await geocodeNominatim(raw, ac.signal);
      if (!hit) {
        setGeoErr("No results (try coordinates: lat, lon).");
        return;
      }
      setCenter({ lat: hit.lat, lng: hit.lng });
      setGeoLabel(hit.label);
    } catch {
      setGeoErr("Geocoding failed or offline.");
    } finally {
      window.clearTimeout(tid);
      setGeoBusy(false);
    }
  }, [targetQuery]);

  const toggleLayer = (id: ProvisionLayerId) => {
    setLayers((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const selectedMbPerSqMi = (Object.keys(LAYER_MB_PER_SQ_MI) as ProvisionLayerId[])
    .filter((k) => layers[k])
    .reduce((s, k) => s + LAYER_MB_PER_SQ_MI[k], 0);

  return (
    <section className="map-provisioner" aria-label="Offline geographic provisioning">
      <header className="map-provisioner__head">
        <h2 className="map-provisioner__title">Interactive geographic provisioning</h2>
        <p className="map-provisioner__lead">
          Preview a download footprint and estimated size. Existing catalog regions on disk are shown as shaded coverage. Does not change tactical map routing.
        </p>
      </header>
      <div className="map-provisioner__grid">
        <div className="map-provisioner__map-wrap">
          <div ref={hostRef} className="map-provisioner__map-host" role="application" aria-label="Provisioning map preview" />
        </div>
        <aside className="map-provisioner__aside">
          <label className="map-provisioner__field">
            <span className="map-provisioner__label">Target location</span>
            <input
              className="map-provisioner__input"
              type="text"
              placeholder="City, region — or lat, lon"
              value={targetQuery}
              onChange={(e) => setTargetQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void runResolveTarget();
              }}
            />
            <button type="button" className="map-provisioner__btn" disabled={geoBusy} onClick={() => void runResolveTarget()}>
              {geoBusy ? "Resolving…" : "Set target"}
            </button>
            {geoErr ? <p className="map-provisioner__err">{geoErr}</p> : null}
            {geoLabel && !geoErr ? <p className="map-provisioner__ok">{geoLabel}</p> : null}
          </label>

          <div className="map-provisioner__field">
            <div className="map-provisioner__label-row">
              <span className="map-provisioner__label">Radius (miles)</span>
              <span className="map-provisioner__mono">{radiusMiles} mi</span>
            </div>
            <input
              type="range"
              min={2}
              max={400}
              step={1}
              value={radiusMiles}
              onChange={(e) => setRadiusMiles(Number(e.target.value) || 2)}
              className="map-provisioner__range"
            />
            <p className="map-provisioner__hint">
              Orange box = axis-aligned bounding square (side = 2× radius) used for rough tile-area math.
            </p>
          </div>

          <fieldset className="map-provisioner__layers">
            <legend className="map-provisioner__label">Layers to provision</legend>
            {(Object.keys(LAYER_MB_PER_SQ_MI) as ProvisionLayerId[]).map((id) => (
              <label key={id} className="map-provisioner__check">
                <input type="checkbox" checked={layers[id]} onChange={() => toggleLayer(id)} />
                <span>
                  {id === "satellite" && "Satellite (high res)"}
                  {id === "topo" && "Topographic"}
                  {id === "street" && "Street / routing"}
                  {id === "terrain3d" && "Terrain (3D)"}
                  <span className="map-provisioner__mono"> · ~{formatDensityMibPerSqMi(LAYER_MB_PER_SQ_MI[id])} MiB/mi²</span>
                </span>
              </label>
            ))}
          </fieldset>

          <div className="map-provisioner__stats">
            <div>
              <span className="map-provisioner__stat-k">BBox area (approx.)</span>
              <span className="map-provisioner__stat-v">{center ? `${areaSqMi.toFixed(0)} mi²` : "—"}</span>
            </div>
            <div>
              <span className="map-provisioner__stat-k">Selected layer density</span>
              <span className="map-provisioner__stat-v">
                {selectedMbPerSqMi > 0 ? `~${formatDensityMibPerSqMi(selectedMbPerSqMi)} MiB/mi²` : "—"}
              </span>
            </div>
            <div>
              <span className="map-provisioner__stat-k">Estimated download (preview)</span>
              <span className="map-provisioner__stat-v">
                {estimateBytes > 0 ? formatBytesBinary(estimateBytes) : "—"}
              </span>
            </div>
            <p className="map-provisioner__queue-hint text-muted small">
              The live estimate updates as you move the region. Pin it to freeze this footprint in the hub total (along
              with Packages / Academy / Models) until you clear the planned queue.
            </p>
            <button
              type="button"
              className="map-provisioner__btn"
              style={{ marginTop: 8 }}
              disabled={estimateBytes <= 0}
              onClick={() => pinMapProvisionBytes(estimateBytes)}
            >
              Pin map estimate to download plan
            </button>
          </div>
        </aside>
      </div>
    </section>
  );
}
