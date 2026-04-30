import { useEffect, useRef, useState } from "react";
import maplibregl, { type Map as MlMap } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { FeatureCollection } from "geojson";
import { buildOfflineBlankStyle } from "./OfflineMapViewport";
import { buildCirclePolygonRingMiles } from "./geometry";
import type { MockMapTile } from "../../lib/MapCatalogIndex";
import { MAX_EXTRACT_RADIUS_MILES } from "../../geographicBoundingBox";

const SRC_TILES = "map-archive-tiles";
const SRC_RADIUS = "map-archive-radius";
const SRC_CENTER = "map-archive-center";

const LAYER_TILES_FILL = "map-archive-tiles-fill";
const LAYER_TILES_LINE = "map-archive-tiles-line";
const LAYER_RADIUS_FILL = "map-archive-radius-fill";
const LAYER_RADIUS_LINE = "map-archive-radius-line";
const LAYER_CENTER = "map-archive-center-pt";

const GRID_LAYER_ID = "offline-preview-grid-lines";
const SRC_OSM = "map-archive-osm";
const LAYER_OSM = "map-archive-osm-layer";

function tilesToFeatureCollection(tiles: MockMapTile[]): FeatureCollection {
  return {
    type: "FeatureCollection",
    features: tiles.map((t) => {
      const [minLon, minLat, maxLon, maxLat] = t.bbox;
      return {
        type: "Feature" as const,
        id: `tile-${t.row}-${t.col}`,
        properties: {},
        geometry: {
          type: "Polygon" as const,
          coordinates: [
            [
              [minLon, minLat],
              [maxLon, minLat],
              [maxLon, maxLat],
              [minLon, maxLat],
              [minLon, minLat],
            ],
          ],
        },
      };
    }),
  };
}

export type MapArchivePreviewMapProps = {
  center: { lat: number; lon: number } | null;
  radiusMiles: number;
  tiles: MockMapTile[];
};

/**
 * Edge-to-edge preview: mock grid cells + soft radius disk (MapLibre).
 */
export function MapArchivePreviewMap({ center, radiusMiles, tiles }: MapArchivePreviewMapProps) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MlMap | null>(null);
  const [mapReady, setMapReady] = useState(false);

  const r = Math.max(0.1, Math.min(radiusMiles, MAX_EXTRACT_RADIUS_MILES));

  const radiusFc: FeatureCollection = (() => {
    if (!center) {
      return { type: "FeatureCollection", features: [] };
    }
    const ring = buildCirclePolygonRingMiles(center.lat, center.lon, r, 64);
    return {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: {},
          geometry: { type: "Polygon", coordinates: [ring] },
        },
      ],
    };
  })();

  const tileFc = tilesToFeatureCollection(tiles);

  const centerFc: FeatureCollection = (() => {
    if (!center) {
      return { type: "FeatureCollection", features: [] };
    }
    return {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: {},
          geometry: { type: "Point", coordinates: [center.lon, center.lat] },
        },
      ],
    };
  })();

  useEffect(() => {
    if (!rootRef.current || mapRef.current) {
      return;
    }
    const style = buildOfflineBlankStyle() as maplibregl.StyleSpecification;
    if (!style.sources) {
      style.sources = {};
    }
    if (!style.layers) {
      style.layers = [];
    }

    // Visible basemap behind overlays. If offline, tiles simply won't load.
    style.sources[SRC_OSM] = {
      type: "raster",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      tileSize: 256,
      attribution: "OpenStreetMap",
    };

    style.sources[SRC_TILES] = { type: "geojson", data: { type: "FeatureCollection", features: [] } };
    style.sources[SRC_RADIUS] = { type: "geojson", data: { type: "FeatureCollection", features: [] } };
    style.sources[SRC_CENTER] = { type: "geojson", data: { type: "FeatureCollection", features: [] } };

    const bgIdx = style.layers.findIndex((l) => l.id === "offline-background");
    const osmInsertAt = (bgIdx >= 0 ? bgIdx + 1 : 0) as number;
    style.layers.splice(osmInsertAt, 0, { id: LAYER_OSM, type: "raster", source: SRC_OSM });

    const afterGrid = style.layers.findIndex((l) => l.id === GRID_LAYER_ID);
    const ins = (afterGrid >= 0 ? afterGrid + 1 : style.layers.length) as number;
    const extra: maplibregl.LayerSpecification[] = [
      // Make the “download boxes” clearly visible over the basemap.
      { id: LAYER_TILES_FILL, type: "fill", source: SRC_TILES, paint: { "fill-color": "#2563eb", "fill-opacity": 0.16 } },
      {
        id: LAYER_TILES_LINE,
        type: "line",
        source: SRC_TILES,
        paint: { "line-color": "#1e40af", "line-width": 1.2, "line-opacity": 0.55, "line-dasharray": [1.2, 1.2] },
      },
      { id: LAYER_RADIUS_FILL, type: "fill", source: SRC_RADIUS, paint: { "fill-color": "#3b82f6", "fill-opacity": 0.18 } },
      { id: LAYER_RADIUS_LINE, type: "line", source: SRC_RADIUS, paint: { "line-color": "#3b82f6", "line-width": 1.5, "line-opacity": 0.45 } },
      {
        id: LAYER_CENTER,
        type: "circle",
        source: SRC_CENTER,
        paint: { "circle-radius": 5, "circle-color": "#e2e8f0", "circle-stroke-width": 1.2, "circle-stroke-color": "#1e293b" },
      },
    ];
    style.layers.splice(ins, 0, ...extra);

    const m = new maplibregl.Map({
      container: rootRef.current,
      style,
      center: [0, 20],
      zoom: 1.2,
      attributionControl: false,
    });
    m.addControl(new maplibregl.NavigationControl({ showCompass: true }), "top-right");
    mapRef.current = m;
    m.once("load", () => setMapReady(true));
    const el = rootRef.current;
    const ro = new ResizeObserver(() => m.resize());
    ro.observe(el);
    return () => {
      ro.disconnect();
      m.remove();
      mapRef.current = null;
      setMapReady(false);
    };
  }, []);

  useEffect(() => {
    const m = mapRef.current;
    if (!m || !mapReady || !m.isStyleLoaded()) {
      return;
    }
    try {
      (m.getSource(SRC_TILES) as maplibregl.GeoJSONSource).setData(tileFc);
      (m.getSource(SRC_RADIUS) as maplibregl.GeoJSONSource).setData(radiusFc);
      (m.getSource(SRC_CENTER) as maplibregl.GeoJSONSource).setData(centerFc);
    } catch {
      return;
    }
    if (!center) {
      return;
    }
    const ring = buildCirclePolygonRingMiles(center.lat, center.lon, r, 64);
    const lons = ring.map((c) => c[0]!);
    const lats = ring.map((c) => c[1]!);
    const w = (Math.max(...lons) - Math.min(...lons)) * 0.12;
    const h = (Math.max(...lats) - Math.min(...lats)) * 0.12;
    m.fitBounds(
      [
        [Math.min(...lons) - w, Math.min(...lats) - h],
        [Math.max(...lons) + w, Math.max(...lats) + h],
      ] as [[number, number], [number, number]],
      { padding: 48, duration: 520, maxZoom: 14, essential: true },
    );
  }, [mapReady, center, r, tileFc, radiusFc, centerFc, tiles]);

  return (
    <div
      ref={rootRef}
      className="map-archive-preview-map"
      role="img"
      aria-label="Preview of download radius and index tiles"
    />
  );
}
