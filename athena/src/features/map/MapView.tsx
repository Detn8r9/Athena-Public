import { createElement, Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  MapContainer,
  TileLayer,
  Marker,
  Popup,
  Rectangle,
  Circle,
  Polyline,
  GeoJSON,
  useMap,
  useMapEvents,
} from "react-leaflet";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import "leaflet-draw/dist/leaflet.draw.css";
import "leaflet-draw";
import type { Geometry } from "geojson";
import { buildMbTilesRasterTilesTemplate } from "./offlineMapArchiveUrls";
import { TacticalSidebar, type TacticalLayerVisibilityState } from "./TacticalSidebar";
import {
  startSatelliteRegionDownload,
  type SatelliteRegionDownloadCompleteEventPayload,
  type SatelliteRegionDownloadErrorEventPayload,
  type SatelliteRegionDownloadProgressEventPayload,
} from "./offlineMapManagerCatalog";
import { MapSettingsPopover } from "./MapSettingsPopover";
import { PoiDetailsModal } from "./PoiDetailsModal";
import { getPoiLucideIcon } from "./tacticalLucideIconRegistry";
import type { MapBasemap } from "./mapViewTypes";

export type { MapBasemap };

export interface Poi {
  id: string;
  name: string;
  lat: number;
  lon: number;
  notes: string;
}

export type MapMarkerCategory = "hazard" | "water" | "cache" | "med";

export interface MapMarker {
  id: string;
  lat: number;
  lng: number;
  title: string;
  description: string;
  created_at: string;
  category: string;
}

export interface CoordinateConversionResult {
  decimal_degrees: string;
  decimal_degrees_lat: string;
  decimal_degrees_lng: string;
  utm: string;
  mgrs: string;
}

export interface MapDrawingRecord {
  id: string;
  feature_type: string;
  geometry_json: string;
  style_json: string;
  created_at: string;
}

const MAP_MARKER_CATEGORIES: Array<{ id: MapMarkerCategory; label: string }> = [
  { id: "hazard", label: "Hazard" },
  { id: "water", label: "Water" },
  { id: "cache", label: "Cache" },
  { id: "med", label: "Med" },
];

/** CSS filters applied to the default Leaflet marker image for category distinction. */
const MAP_MARKER_CATEGORY_FILTERS: Record<MapMarkerCategory, string> = {
  hazard: "hue-rotate(0deg) saturate(1.35) brightness(1.05)",
  water: "hue-rotate(175deg) saturate(1.45) brightness(1.02)",
  cache: "hue-rotate(42deg) saturate(1.2) brightness(1.08)",
  med: "hue-rotate(285deg) saturate(1.25) brightness(1.05)",
};

function normalizeMapMarkerCategory(raw: string | undefined): MapMarkerCategory {
  const t = (raw ?? "hazard").trim().toLowerCase();
  if (t === "hazard" || t === "water" || t === "cache" || t === "med") {
    return t;
  }
  return "hazard";
}

const mapMarkerCategoryIconCache = new Map<string, L.DivIcon>();

function getMapMarkerCategoryIcon(category: string): L.DivIcon {
  const cat = normalizeMapMarkerCategory(category);
  const cached = mapMarkerCategoryIconCache.get(cat);
  if (cached) {
    return cached;
  }
  const filter = MAP_MARKER_CATEGORY_FILTERS[cat];
  const html = `<div class="map-marker-category-pin" style="width:25px;height:41px;filter:${filter}"><img src="https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon.png" alt="" width="25" height="41" style="display:block" /></div>`;
  const icon = L.divIcon({
    className: "map-marker-category-wrapper",
    html,
    iconSize: [25, 41],
    iconAnchor: [12, 41],
    popupAnchor: [1, -34],
  });
  mapMarkerCategoryIconCache.set(cat, icon);
  return icon;
}

interface PeerNode {
  node_id: string;
  latitude: number;
  longitude: number;
  last_seen_at: string;
}

interface PeerPositionEventPayload {
  node_id: string;
  latitude: number;
  longitude: number;
  timestamp: string;
}

// Fix default marker icons in Leaflet with bundlers
const defaultIcon = L.icon({
  iconUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon.png",
  iconRetinaUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon-2x.png",
  shadowUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-shadow.png",
  iconSize: [25, 41],
  iconAnchor: [12, 41],
  popupAnchor: [1, -34],
  shadowSize: [41, 41],
});

const TACTICAL_MESH_COVERAGE_RADIUS_METERS = 2500;

export type TacticalMapPointOfInterest = {
  id: string;
  name: string;
  lat: number;
  lon: number;
  notes: string;
  updated_at: number;
};

export type TacticalMapMeshNodeLocation = {
  node_id: string;
  label: string;
  lat: number;
  lon: number;
  last_seen_at: number;
};

type TacticalResourceKind = "power" | "water" | "food" | "equipment";

/** Preset Lucide names from the tactical POI modal; these map to resource sub-layers, not Custom Pins. */
function lucidePresetIconToResourceKind(lucideIconName: string): TacticalResourceKind | undefined {
  switch (lucideIconName) {
    case "Zap":
      return "power";
    case "Droplets":
      return "water";
    case "Apple":
      return "food";
    case "Home":
      return "equipment";
    default:
      return undefined;
  }
}

function parseTacticalResourceKindLegacy(notes: string): TacticalResourceKind {
  const match = notes.trim().match(/^@(power|water|food|equipment)\b/i);
  if (match) {
    const token = match[1].toLowerCase();
    if (token === "power" || token === "water" || token === "food" || token === "equipment") {
      return token;
    }
  }
  return "equipment";
}

/** Lucide export name from notes `@icon:MapPin` (matches `add_tactical_poi` storage). */
function parseTacticalLucideIconName(notes: string): string | null {
  const m = notes.trim().match(/@icon:([A-Za-z0-9]+)/);
  return m ? m[1] : null;
}

/** Meters from `@range_m:123` in tactical POI notes (network coverage). */
function parseTacticalNetworkCoverageRadiusMeters(notes: string): number | null {
  const m = notes.match(/@range_m:(\d+)/i);
  if (!m) return null;
  const n = Number.parseInt(m[1], 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

function stripTacticalNotesBody(notes: string): string {
  return notes
    .replace(/@network\b/gi, "")
    .replace(/@icon:[A-Za-z0-9]+/gi, "")
    .replace(/@range_m:\d+\b/gi, "")
    .replace(/@(power|water|food|equipment)\b/gi, "")
    .replace(/\s+/g, " ")
    .trim();
}

type TacticalPointClassification =
  | { bucket: "network_pin" }
  | { bucket: "custom_pin" }
  | { bucket: "resource"; kind: TacticalResourceKind };

function classifyTacticalPoint(notes: string): TacticalPointClassification {
  if (/@network\b/i.test(notes)) {
    return { bucket: "network_pin" };
  }
  const lucide = parseTacticalLucideIconName(notes);
  if (lucide) {
    const presetKind = lucidePresetIconToResourceKind(lucide);
    if (presetKind !== undefined) {
      return { bucket: "resource", kind: presetKind };
    }
    return { bucket: "custom_pin" };
  }
  return { bucket: "resource", kind: parseTacticalResourceKindLegacy(notes) };
}

/** Resource kind for styling fallbacks; custom `@icon:` pins use equipment-colored fallback when Lucide is missing. */
function parseTacticalResourceKind(notes: string): TacticalResourceKind {
  const c = classifyTacticalPoint(notes);
  if (c.bucket === "custom_pin" || c.bucket === "network_pin") {
    return "equipment";
  }
  return c.kind;
}

function tacticalResourceColor(kind: TacticalResourceKind): string {
  switch (kind) {
    case "power":
      return "#fbbf24";
    case "water":
      return "#38bdf8";
    case "food":
      return "#4ade80";
    case "equipment":
    default:
      return "#c4b5fd";
  }
}

function buildTacticalResourceIcon(kind: TacticalResourceKind): L.DivIcon {
  const color = tacticalResourceColor(kind);
  return L.divIcon({
    className: "tactical-resource-marker",
    html: `<div style="width:18px;height:18px;border-radius:4px;background:${color};border:2px solid #0f172a;box-shadow:0 1px 3px rgba(0,0,0,0.35);"></div>`,
    iconSize: [18, 18],
    iconAnchor: [9, 9],
    popupAnchor: [0, -9],
  });
}

const tacticalLucideMarkerCache = new Map<string, L.DivIcon>();

function getTacticalLucideMarkerIcon(lucideIconName: string): L.DivIcon | null {
  const Icon = getPoiLucideIcon(lucideIconName);
  if (!Icon) {
    return null;
  }
  const cached = tacticalLucideMarkerCache.get(lucideIconName);
  if (cached) {
    return cached;
  }
  const svg = renderToStaticMarkup(
    createElement(Icon, {
      size: 15,
      color: "#0f172a",
      strokeWidth: 2.25,
      absoluteStrokeWidth: true,
    }),
  );
  const html = `<div class="tactical-lucide-marker-chip" style="display:flex;align-items:center;justify-content:center;width:22px;height:22px;border-radius:6px;background:#c4b5fd;border:2px solid #0f172a;box-shadow:0 1px 3px rgba(0,0,0,0.35);">${svg}</div>`;
  const icon = L.divIcon({
    className: "tactical-lucide-marker",
    html,
    iconSize: [22, 22],
    iconAnchor: [11, 11],
    popupAnchor: [0, -11],
  });
  tacticalLucideMarkerCache.set(lucideIconName, icon);
  return icon;
}

const meshNodeMarkerIcon = L.divIcon({
  className: "tactical-mesh-node-marker",
  html:
    '<div style="width:16px;height:16px;border-radius:50%;background:#38bdf8;border:2px solid #0f172a;box-shadow:0 1px 3px rgba(0,0,0,0.35);"></div>',
  iconSize: [16, 16],
  iconAnchor: [8, 8],
  popupAnchor: [0, -8],
});

const livePeerMarkerIcon = L.divIcon({
  className: "tactical-live-peer-marker",
  html:
    '<div style="width:16px;height:16px;border-radius:50%;background:#f472b6;border:2px solid #0f172a;box-shadow:0 1px 3px rgba(0,0,0,0.35);"></div>',
  iconSize: [16, 16],
  iconAnchor: [8, 8],
  popupAnchor: [0, -8],
});

function OfflineRasterLayer({
  rootPath,
  convertFileSrcFn,
  attribution,
}: {
  rootPath: string;
  convertFileSrcFn: (path: string) => string;
  attribution: string;
}) {
  const map = useMap();
  useEffect(() => {
    const layer = L.tileLayer("", {
      attribution,
      maxZoom: 19,
      minZoom: 0,
      updateWhenIdle: false,
      updateWhenZooming: true,
      keepBuffer: 8,
    });
    layer.getTileUrl = (coords) => {
      const path = `${rootPath}/${coords.z}/${coords.x}/${coords.y}.png`;
      return convertFileSrcFn(path);
    };
    layer.addTo(map);
    return () => {
      map.removeLayer(layer);
    };
  }, [map, rootPath, convertFileSrcFn, attribution]);
  return null;
}

function normalizeLatLngBox(
  a: { lat: number; lng: number },
  b: { lat: number; lng: number },
): [[number, number], [number, number]] {
  const minLat = Math.min(a.lat, b.lat);
  const maxLat = Math.max(a.lat, b.lat);
  const minLon = Math.min(a.lng, b.lng);
  const maxLon = Math.max(a.lng, b.lng);
  return [
    [minLat, minLon],
    [maxLat, maxLon],
  ];
}

function boundsFromLeafletBox(box: [[number, number], [number, number]]) {
  const [[minLat, minLon], [maxLat, maxLon]] = box;
  return { minLon, minLat, maxLon, maxLat };
}

function SatelliteDownloadBoxTool({
  enabled,
  onPreviewChange,
  onSelectionChange,
}: {
  enabled: boolean;
  onPreviewChange: (box: [[number, number], [number, number]] | null) => void;
  onSelectionChange: (box: [[number, number], [number, number]]) => void;
}) {
  const startCornerRef = useRef<{ lat: number; lng: number } | null>(null);
  useMapEvents({
    click(event) {
      if (!enabled) return;
      const clickPoint = { lat: event.latlng.lat, lng: event.latlng.lng };
      if (startCornerRef.current === null) {
        startCornerRef.current = clickPoint;
        onPreviewChange(normalizeLatLngBox(clickPoint, clickPoint));
        return;
      }
      const box = normalizeLatLngBox(startCornerRef.current, clickPoint);
      startCornerRef.current = null;
      onPreviewChange(null);
      onSelectionChange(box);
    },
    mousemove(event) {
      if (!enabled || startCornerRef.current === null) return;
      onPreviewChange(normalizeLatLngBox(startCornerRef.current, event.latlng));
    },
    contextmenu() {
      if (!enabled) return;
      startCornerRef.current = null;
      onPreviewChange(null);
    },
  });

  useEffect(() => {
    if (enabled) return;
    startCornerRef.current = null;
    onPreviewChange(null);
  }, [enabled, onPreviewChange]);

  return null;
}

/** Applies parent-controlled center/zoom without remounting the map (avoids flash on zoom changes). */
function MapExternalViewSync({ center, zoom }: { center: [number, number]; zoom: number }) {
  const map = useMap();
  useEffect(() => {
    map.setView(center, zoom, { animate: true });
  }, [map, center[0], center[1], zoom]);
  return null;
}

interface MapViewProps {
  pois: Poi[];
  /** When omitted and `embedded` is true, no back control is shown. */
  onClose?: () => void;
  /** When true, map fills the app stage instead of a fixed fullscreen overlay. */
  embedded?: boolean;
  onAddPoi: (name: string, lat: number, lon: number, notes: string) => Promise<void>;
  onRemovePoi: (id: string) => Promise<void>;
  mapChatInput: string;
  setMapChatInput: (v: string) => void;
  onMapChatSend: () => void;
  mapChatSending: boolean;
  mapChatMessage: string;
  layerFilters: { pois: boolean; roads: boolean; rivers: boolean };
  setLayerFilters: (f: (prev: { pois: boolean; roads: boolean; rivers: boolean }) => { pois: boolean; roads: boolean; rivers: boolean }) => void;
  /** Tauri `convertFileSrc` for local tile PNG paths */
  convertFileSrcFn: (path: string) => string;
  basemap: MapBasemap;
  onBasemapChange: (b: MapBasemap) => void;
  offlineOsmRoot: string | null;
  offlineTopoRoot: string | null;
  offlineImportedRoot: string | null;
  /** Enabled archive-driven MBTiles rasters chosen in map archive manager. File names include `.mbtiles`. */
  archiveRasterFileNames: string[];
  /** Native zoom ranges discovered from MBTiles archives (`tiles` table), keyed by file name. */
  archiveRasterNativeZoomRangesByFile: Record<string, [number, number]>;
  /** Geographic coverage boxes from MBTiles metadata (`bounds`), keyed by file name. */
  archiveRasterBoundsByFile: Record<string, [number, number, number, number]>;
  /** User saved location string, used as default label for map-based satellite downloads. */
  savedUserLocationAddress: string;
  /** Called when an in-map satellite download completes (`map_id` from backend). */
  onSatelliteRegionDownloaded?: (mapId: string) => void;
  mapCenter: [number, number];
  mapZoom: number;
  /** Opens the map archive manager (PMTiles / MBTiles catalog and downloads). */
  onOpenOfflineMapManager?: () => void;
  /** SQLite-backed tactical points of interest (import / sync). Notes may start with @power, @water, @food, or @equipment. */
  tacticalPointsOfInterest: TacticalMapPointOfInterest[];
  /** SQLite-backed mesh node positions for tactical network layers. */
  tacticalMeshNodeLocations: TacticalMapMeshNodeLocation[];
  /** Refresh tactical points and nodes from the database (for example after import). */
  onReloadTacticalMapLayers: () => void;
  /** Persist a tactical POI from the right-click pin modal (`add_tactical_poi`). */
  onAddTacticalPoiPin: (payload: {
    name: string;
    description: string;
    lucideIconName: string;
    pinLayer: "resource" | "network";
    lat: number;
    lng: number;
    coverageRadiusMeters?: number | null;
  }) => Promise<void>;
  tacticalLayerVisibility: TacticalLayerVisibilityState;
  onTacticalLayerVisibilityChange: (next: TacticalLayerVisibilityState) => void;
}

/** Double-click places a persisted map marker (SQLite); right-click opens the tactical POI modal. */
function MapDoubleClickMapMarkerPlacement({
  satelliteDrawModeEnabled,
  onDoubleClickLatLng,
}: {
  satelliteDrawModeEnabled: boolean;
  onDoubleClickLatLng: (lat: number, lng: number) => void;
}) {
  useMapEvents({
    dblclick(e) {
      if (satelliteDrawModeEnabled) {
        return;
      }
      onDoubleClickLatLng(e.latlng.lat, e.latlng.lng);
    },
  });
  return null;
}

/** Step 1: capture map right-click position for tactical POI placement (modal wiring in later steps). */
function MapRightClickPinPlacementEvents({
  satelliteDrawModeEnabled,
  onContextMenuLatLng,
}: {
  satelliteDrawModeEnabled: boolean;
  onContextMenuLatLng: (lat: number, lng: number) => void;
}) {
  useMapEvents({
    contextmenu(e) {
      const native = e.originalEvent;
      if (native) {
        native.preventDefault();
      }
      if (satelliteDrawModeEnabled) {
        return;
      }
      onContextMenuLatLng(e.latlng.lat, e.latlng.lng);
    },
  });
  return null;
}

function MapCursorProbe({
  onCursorLatLng,
}: {
  onCursorLatLng: (value: { lat: number; lng: number } | null) => void;
}) {
  useMapEvents({
    mousemove(e) {
      onCursorLatLng({ lat: e.latlng.lat, lng: e.latlng.lng });
    },
    mouseout() {
      onCursorLatLng(null);
    },
  });
  return null;
}

function MapLeafletDrawToolbar({ onDrawingSaved }: { onDrawingSaved: () => void }) {
  const map = useMap();
  useEffect(() => {
    const drawnItems = new L.FeatureGroup();
    map.addLayer(drawnItems);
    const drawControl = new L.Control.Draw({
      position: "topleft",
      draw: {
        polyline: {
          shapeOptions: {
            color: "#38bdf8",
            weight: 3,
            opacity: 0.92,
          },
        },
        polygon: {
          showArea: true,
          metric: true,
          shapeOptions: {
            color: "#38bdf8",
            weight: 2,
            opacity: 0.92,
            fillColor: "#38bdf8",
            fillOpacity: 0.14,
          },
        },
        marker: false,
        circle: false,
        rectangle: false,
        circlemarker: false,
      },
      edit: {
        featureGroup: drawnItems,
        remove: true,
      },
    });
    map.addControl(drawControl);

    const onCreated = (event: L.LeafletEvent) => {
      const e = event as L.DrawEvents.Created;
      const layer = e.layer;
      const layerType = e.layerType;
      if (layerType !== "polyline" && layerType !== "polygon") {
        map.removeLayer(layer);
        return;
      }
      const gj = layer.toGeoJSON();
      if (gj.type !== "Feature" || gj.geometry == null) {
        map.removeLayer(layer);
        return;
      }
      const opts = (layer as L.Path).options ?? {};
      const stylePayload = {
        color: opts.color ?? "#38bdf8",
        weight: opts.weight ?? (layerType === "polyline" ? 3 : 2),
        opacity: opts.opacity ?? 0.92,
        fillOpacity: opts.fillOpacity ?? (layerType === "polygon" ? 0.14 : undefined),
      };
      void invoke<MapDrawingRecord>("save_drawing", {
        featureType: layerType === "polyline" ? "polyline" : "polygon",
        geometryJson: JSON.stringify(gj.geometry),
        styleJson: JSON.stringify(stylePayload),
        id: null,
      })
        .then(() => {
          map.removeLayer(layer);
          onDrawingSaved();
        })
        .catch(() => {
          map.removeLayer(layer);
        });
    };

    map.on(L.Draw.Event.CREATED, onCreated);
    return () => {
      map.off(L.Draw.Event.CREATED, onCreated);
      map.removeControl(drawControl);
      map.removeLayer(drawnItems);
    };
  }, [map, onDrawingSaved]);
  return null;
}

function PersistedDrawingGeoJsonLayerReadOnly({ drawing }: { drawing: MapDrawingRecord }) {
  let geometry: Geometry;
  try {
    geometry = JSON.parse(drawing.geometry_json) as Geometry;
  } catch {
    return null;
  }
  let pathStyle: L.PathOptions;
  try {
    pathStyle = JSON.parse(drawing.style_json) as L.PathOptions;
  } catch {
    pathStyle = { color: "#38bdf8", weight: 3 };
  }
  const feature = {
    type: "Feature" as const,
    geometry,
    properties: { id: drawing.id },
  };
  return <GeoJSON key={drawing.id} data={feature} style={() => pathStyle} interactive={false} />;
}

function MapMarkerPlacementModal({
  open,
  latLng,
  onClose,
  onSave,
}: {
  open: boolean;
  latLng: { lat: number; lng: number } | null;
  onClose: () => void;
  onSave: (payload: { title: string; description: string; category: MapMarkerCategory }) => void | Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [category, setCategory] = useState<MapMarkerCategory>("hazard");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setTitle("");
      setDescription("");
      setCategory("hazard");
      setSaving(false);
      setError(null);
    }
  }, [open]);

  useEffect(() => {
    if (open && latLng) {
      setTitle("");
      setDescription("");
      setCategory("hazard");
      setError(null);
    }
  }, [open, latLng?.lat, latLng?.lng]);

  if (!open || latLng === null) {
    return null;
  }

  return (
    <div className="modal-backdrop map-marker-modal-backdrop" onClick={onClose}>
      <div
        className="library-modal glass-panel map-marker-placement-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="map-marker-modal-title"
      >
        <div className="library-header">
          <h2 id="map-marker-modal-title">Add map marker</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="map-marker-modal-body">
          <label className="map-marker-modal-label">
            Title
            <input
              type="text"
              className="map-marker-modal-input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. Rally point"
              autoFocus
            />
          </label>
          <label className="map-marker-modal-label">
            Category
            <select
              className="map-marker-modal-input"
              value={category}
              onChange={(e) => setCategory(normalizeMapMarkerCategory(e.target.value))}
            >
              {MAP_MARKER_CATEGORIES.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
          <label className="map-marker-modal-label">
            Description (optional)
            <textarea
              className="map-marker-modal-textarea"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              placeholder="Notes…"
            />
          </label>
          {error ? (
            <p className="error" role="alert">
              {error}
            </p>
          ) : null}
          <div className="map-marker-modal-actions">
            <button type="button" className="secondary" onClick={onClose} disabled={saving}>
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                const t = title.trim();
                if (!t) {
                  setError("Title is required.");
                  return;
                }
                setError(null);
                setSaving(true);
                void Promise.resolve(
                  onSave({
                    title: t,
                    description: description.trim(),
                    category,
                  }),
                )
                  .then(() => {
                    onClose();
                  })
                  .catch((err: unknown) => {
                    setError(err != null ? String(err) : "Could not save marker.");
                  })
                  .finally(() => {
                    setSaving(false);
                  });
              }}
              disabled={saving || !title.trim()}
            >
              {saving ? "Saving…" : "Save marker"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function MapView({
  pois,
  onClose,
  embedded = false,
  onAddPoi: _onAddPoi,
  onRemovePoi,
  mapChatInput,
  setMapChatInput,
  onMapChatSend,
  mapChatSending,
  mapChatMessage,
  layerFilters,
  setLayerFilters,
  convertFileSrcFn,
  basemap,
  onBasemapChange,
  offlineOsmRoot,
  offlineTopoRoot,
  offlineImportedRoot,
  archiveRasterFileNames,
  archiveRasterNativeZoomRangesByFile,
  archiveRasterBoundsByFile,
  savedUserLocationAddress,
  onSatelliteRegionDownloaded,
  mapCenter,
  mapZoom,
  onOpenOfflineMapManager,
  tacticalPointsOfInterest,
  tacticalMeshNodeLocations,
  onReloadTacticalMapLayers,
  onAddTacticalPoiPin,
  tacticalLayerVisibility,
  onTacticalLayerVisibilityChange,
}: MapViewProps) {
  const [mapMarkers, setMapMarkers] = useState<MapMarker[]>([]);
  const [mapDrawings, setMapDrawings] = useState<MapDrawingRecord[]>([]);
  const [activePeers, setActivePeers] = useState<PeerNode[]>([]);
  const [cursorLatLng, setCursorLatLng] = useState<{ lat: number; lng: number } | null>(null);
  const [coordHud, setCoordHud] = useState<CoordinateConversionResult | null>(null);
  const [hudShowDd, setHudShowDd] = useState(true);
  const [pendingMapMarkerPlacement, setPendingMapMarkerPlacement] = useState<{ lat: number; lng: number } | null>(null);

  const loadMapMarkers = useCallback(async () => {
    try {
      const list = await invoke<MapMarker[]>("get_map_markers");
      setMapMarkers(Array.isArray(list) ? list : []);
    } catch (_) {
      setMapMarkers([]);
    }
  }, []);

  const loadMapDrawings = useCallback(async () => {
    try {
      const list = await invoke<MapDrawingRecord[]>("get_drawings");
      setMapDrawings(Array.isArray(list) ? list : []);
    } catch (_) {
      setMapDrawings([]);
    }
  }, []);

  useEffect(() => {
    void loadMapMarkers();
    void loadMapDrawings();
  }, [loadMapMarkers, loadMapDrawings]);

  useEffect(() => {
    if (!cursorLatLng) {
      setCoordHud(null);
      return;
    }
    const timer = window.setTimeout(() => {
      void invoke<CoordinateConversionResult>("convert_coordinates", {
        lat: cursorLatLng.lat,
        lng: cursorLatLng.lng,
      })
        .then(setCoordHud)
        .catch(() => setCoordHud(null));
    }, 110);
    return () => clearTimeout(timer);
  }, [cursorLatLng]);

  useEffect(() => {
    const unlisten = listen<MapMarker>("peer-marker-rx", (event) => {
      const marker = event.payload;
      if (!marker || typeof marker.id !== "string") return;
      const normalized: MapMarker = {
        ...marker,
        category:
          typeof marker.category === "string" && marker.category.trim().length > 0 ? marker.category : "hazard",
      };
      setMapMarkers((prev) => [normalized, ...prev.filter((m) => m.id !== normalized.id)]);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<PeerPositionEventPayload>("peer-position-rx", (event) => {
      const payload = event.payload;
      if (
        !payload ||
        typeof payload.node_id !== "string" ||
        typeof payload.latitude !== "number" ||
        typeof payload.longitude !== "number"
      ) {
        return;
      }
      const nextPeer: PeerNode = {
        node_id: payload.node_id,
        latitude: payload.latitude,
        longitude: payload.longitude,
        last_seen_at: typeof payload.timestamp === "string" ? payload.timestamp : new Date().toISOString(),
      };
      setActivePeers((prev) => {
        const existingIndex = prev.findIndex((peer) => peer.node_id === nextPeer.node_id);
        if (existingIndex === -1) {
          return [nextPeer, ...prev];
        }
        const updated = [...prev];
        updated[existingIndex] = nextPeer;
        return updated;
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const meshNodeLinkPositions = useMemo(() => {
    const sorted = [...tacticalMeshNodeLocations].sort((a, b) => a.node_id.localeCompare(b.node_id));
    return sorted.map((node) => [node.lat, node.lon] as [number, number]);
  }, [tacticalMeshNodeLocations]);

  const tacticalResourceIcons = useMemo(
    () => ({
      power: buildTacticalResourceIcon("power"),
      water: buildTacticalResourceIcon("water"),
      food: buildTacticalResourceIcon("food"),
      equipment: buildTacticalResourceIcon("equipment"),
    }),
    [],
  );

  const handleRequestMapMarkerPlacement = useCallback((lat: number, lng: number) => {
    setPendingMapMarkerPlacement({ lat, lng });
  }, []);

  const handleSaveMapMarkerFromModal = useCallback(
    async (payload: { title: string; description: string; category: MapMarkerCategory }) => {
      if (!pendingMapMarkerPlacement) return;
      const marker = await invoke<MapMarker>("add_map_marker", {
        lat: pendingMapMarkerPlacement.lat,
        lng: pendingMapMarkerPlacement.lng,
        title: payload.title,
        description: payload.description,
        category: payload.category,
      });
      setMapMarkers((prev) => [marker, ...prev.filter((m) => m.id !== marker.id)]);
      setPendingMapMarkerPlacement(null);
    },
    [pendingMapMarkerPlacement],
  );

  const handleDrawingSaved = useCallback(() => {
    void loadMapDrawings();
  }, [loadMapDrawings]);

  const handleDeleteDrawing = useCallback(
    (id: string) => {
      void invoke("delete_drawing", { id })
        .then(() => {
          setMapDrawings((prev) => prev.filter((d) => d.id !== id));
        })
        .catch(() => {});
    },
    [],
  );

  const handleDeleteMapMarker = useCallback((id: string) => {
    if (!id) return;
    void invoke("delete_map_marker", { id })
      .then(() => {
        setMapMarkers((prev) => prev.filter((m) => m.id !== id));
      })
      .catch(() => {});
  }, []);

  const handleBroadcastMapMarker = useCallback((id: string) => {
    if (!id) return;
    void invoke("broadcast_map_marker", { id }).catch(() => {});
  }, []);

  const handleDeleteTacticalPoi = useCallback(
    (id: string) => {
      if (!id) return;
      void invoke("delete_tactical_poi", { id })
        .then(() => {
          onReloadTacticalMapLayers();
        })
        .catch(() => {});
    },
    [onReloadTacticalMapLayers],
  );

  const handleBroadcastTacticalPoi = useCallback((id: string) => {
    if (!id) return;
    void invoke("broadcast_tactical_poi", { id }).catch(() => {});
  }, []);

  const canOfflineOsm = Boolean(offlineOsmRoot);
  const canOfflineTopo = Boolean(offlineTopoRoot);
  const canOfflineImported = Boolean(offlineImportedRoot);
  const canOfflineArchiveRaster = archiveRasterFileNames.length > 0;

  const showOnlineOsmBase = basemap === "online_osm" || (basemap === "offline_archive_raster" && !canOfflineTopo);
  const showOfflineOsmBase = basemap === "offline_osm";
  const showOfflineTopoBase = basemap === "offline_topo" || (basemap === "offline_archive_raster" && canOfflineTopo);
  const showOfflineImportedBase = basemap === "offline_imported";
  const showSatelliteOverlay = basemap === "offline_archive_raster" && canOfflineArchiveRaster;
  const downloadedCoverageBoundsList = archiveRasterFileNames
    .map((fileName) => {
      const bounds = archiveRasterBoundsByFile[fileName];
      if (bounds === undefined) return null;
      return {
        fileName,
        bounds: [
          [bounds[1], bounds[0]],
          [bounds[3], bounds[2]],
        ] as [[number, number], [number, number]],
      };
    })
    .filter((row): row is { fileName: string; bounds: [[number, number], [number, number]] } => row !== null);

  const [satelliteDrawModeEnabled, setSatelliteDrawModeEnabled] = useState(false);
  const [satelliteCoverageOnMap, setSatelliteCoverageOnMap] = useState(false);
  const [satelliteSelectionBounds, setSatelliteSelectionBounds] = useState<[[number, number], [number, number]] | null>(null);
  const [satellitePreviewBounds, setSatellitePreviewBounds] = useState<[[number, number], [number, number]] | null>(null);
  const [satelliteDownloadBusy, setSatelliteDownloadBusy] = useState(false);
  const [satelliteDownloadStatus, setSatelliteDownloadStatus] = useState<string>("");
  const [satelliteDownloadJobId, setSatelliteDownloadJobId] = useState<string | null>(null);
  const activeSatelliteJobIdRef = useRef<string | null>(null);

  /** Last map right-click position for pin placement (consumed by POI modal in Step 2+). */
  const [rightClickPinLatLng, setRightClickPinLatLng] = useState<{ lat: number; lng: number } | null>(null);

  const handleMapContextMenuLatLng = useCallback((lat: number, lng: number) => {
    setRightClickPinLatLng({ lat, lng });
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    void (async () => {
      const unlistenProgress = await listen<SatelliteRegionDownloadProgressEventPayload>(
        "satellite_region_download_progress",
        (event) => {
          if (event.payload.job_id !== activeSatelliteJobIdRef.current) return;
          const total = Math.max(1, event.payload.total_tiles);
          const pct = Math.min(100, (event.payload.downloaded_tiles / total) * 100);
          setSatelliteDownloadStatus(
            `${event.payload.downloaded_tiles.toLocaleString()} / ${event.payload.total_tiles.toLocaleString()} tiles (${pct.toFixed(1)}%)`,
          );
        },
      );
      if (disposed) {
        unlistenProgress();
        return;
      }
      unlisten.push(unlistenProgress);

      const unlistenComplete = await listen<SatelliteRegionDownloadCompleteEventPayload>(
        "satellite_region_download_complete",
        (event) => {
          if (event.payload.job_id !== activeSatelliteJobIdRef.current) return;
          activeSatelliteJobIdRef.current = null;
          setSatelliteDownloadBusy(false);
          setSatelliteDownloadJobId(null);
          setSatelliteDownloadStatus("Satellite download complete.");
          onSatelliteRegionDownloaded?.(event.payload.map_id);
        },
      );
      if (disposed) {
        unlistenComplete();
        return;
      }
      unlisten.push(unlistenComplete);

      const unlistenError = await listen<SatelliteRegionDownloadErrorEventPayload>(
        "satellite_region_download_error",
        (event) => {
          if (event.payload.job_id !== activeSatelliteJobIdRef.current) return;
          activeSatelliteJobIdRef.current = null;
          setSatelliteDownloadBusy(false);
          setSatelliteDownloadJobId(null);
          setSatelliteDownloadStatus(`Satellite download failed: ${event.payload.message}`);
        },
      );
      if (disposed) {
        unlistenError();
        return;
      }
      unlisten.push(unlistenError);
    })();

    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, [onSatelliteRegionDownloaded]);

  const handleStartSatelliteDownloadFromSelection = useCallback(() => {
    if (satelliteSelectionBounds === null || satelliteDownloadBusy) return;
    const box = boundsFromLeafletBox(satelliteSelectionBounds);
    setSatelliteDownloadBusy(true);
    setSatelliteDownloadStatus("");
    setSatelliteDownloadJobId(null);
    activeSatelliteJobIdRef.current = null;
    void startSatelliteRegionDownload({
      minLon: box.minLon,
      minLat: box.minLat,
      maxLon: box.maxLon,
      maxLat: box.maxLat,
      minZoom: 10,
      maxZoom: 15,
      locationLabel: savedUserLocationAddress.trim() || "map selection",
    })
      .then((jobId) => {
        activeSatelliteJobIdRef.current = jobId;
        setSatelliteDownloadJobId(jobId);
        setSatelliteDownloadStatus("Satellite download started.");
      })
      .catch((error: unknown) => {
        activeSatelliteJobIdRef.current = null;
        setSatelliteDownloadBusy(false);
        setSatelliteDownloadJobId(null);
        setSatelliteDownloadStatus(error != null ? String(error) : "Satellite download could not be started.");
      });
  }, [satelliteSelectionBounds, satelliteDownloadBusy, savedUserLocationAddress]);

  const handleCancelSatelliteBoxSelection = useCallback(() => {
    setSatelliteDrawModeEnabled(false);
    setSatellitePreviewBounds(null);
    setSatelliteSelectionBounds(null);
    setSatelliteDownloadStatus("");
    setSatelliteCoverageOnMap(false);
  }, []);

  useEffect(() => {
    if (satelliteCoverageOnMap) return;
    setSatelliteDrawModeEnabled(false);
    setSatellitePreviewBounds(null);
  }, [satelliteCoverageOnMap]);

  const handleSatelliteDrawModeToggle = useCallback(() => {
    setSatelliteDrawModeEnabled((previous) => {
      const next = !previous;
      if (next) {
        setSatelliteCoverageOnMap(true);
      }
      return next;
    });
    setSatellitePreviewBounds(null);
  }, []);

  return (
    <div className={`map-view-wrap${embedded ? " map-view-wrap--embedded" : ""}`}>
      <div
        className="map-view-container"
        data-map-contextmenu-lat={rightClickPinLatLng != null ? String(rightClickPinLatLng.lat) : undefined}
        data-map-contextmenu-lng={rightClickPinLatLng != null ? String(rightClickPinLatLng.lng) : undefined}
      >
        <MapContainer
          center={mapCenter}
          zoom={mapZoom}
          zoomAnimation={true}
          fadeAnimation={true}
          zoomSnap={0.25}
          zoomDelta={0.5}
          wheelPxPerZoomLevel={96}
          className="map-view-map"
          style={{ height: "100%", width: "100%" }}
        >
          <MapExternalViewSync center={mapCenter} zoom={mapZoom} />
          {showOnlineOsmBase && (
            <TileLayer
              attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
              url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
              updateWhenIdle={false}
              updateWhenZooming={true}
              keepBuffer={8}
            />
          )}
          {showOfflineOsmBase && offlineOsmRoot && (
            <OfflineRasterLayer
              rootPath={offlineOsmRoot}
              convertFileSrcFn={convertFileSrcFn}
              attribution='Offline tiles &copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
            />
          )}
          {showOfflineTopoBase && offlineTopoRoot && (
            <OfflineRasterLayer
              rootPath={offlineTopoRoot}
              convertFileSrcFn={convertFileSrcFn}
              attribution='Map data: &copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors, SRTM | OpenTopoMap (offline cache)'
            />
          )}
          {showOfflineImportedBase && offlineImportedRoot && (
            <OfflineRasterLayer
              rootPath={offlineImportedRoot}
              convertFileSrcFn={convertFileSrcFn}
              attribution="Offline raster from imported archive — respect the source license."
            />
          )}
          {showSatelliteOverlay &&
            archiveRasterFileNames.map((archiveRasterFileName) => {
              const nativeZoomRange = archiveRasterNativeZoomRangesByFile[archiveRasterFileName];
              const bounds = archiveRasterBoundsByFile[archiveRasterFileName];
              const satelliteNativeMinZoom = nativeZoomRange?.[0] ?? 10;
              const satelliteNativeMaxZoom = nativeZoomRange?.[1] ?? 15;
              const layerBounds =
                bounds !== undefined
                  ? ([
                      [bounds[1], bounds[0]],
                      [bounds[3], bounds[2]],
                    ] as [[number, number], [number, number]])
                  : undefined;
              return (
                <TileLayer
                  key={`satellite-${archiveRasterFileName}`}
                  attribution="Offline satellite MBTiles over terrain base."
                  url={buildMbTilesRasterTilesTemplate(archiveRasterFileName)}
                  bounds={layerBounds}
                  minZoom={0}
                  maxZoom={22}
                  minNativeZoom={satelliteNativeMinZoom}
                  maxNativeZoom={satelliteNativeMaxZoom}
                  updateWhenIdle={false}
                  updateWhenZooming={true}
                  keepBuffer={8}
                />
              );
            })}
          {satelliteCoverageOnMap &&
            downloadedCoverageBoundsList.map((row) => (
              <Rectangle
                key={`coverage-${row.fileName}`}
                bounds={row.bounds}
                pathOptions={{
                  color: "#ff4747",
                  weight: 2,
                  opacity: 0.95,
                  fillColor: "#ff4747",
                  fillOpacity: 0.08,
                }}
              />
            ))}
          {satelliteCoverageOnMap && satelliteSelectionBounds !== null && (
            <Rectangle
              bounds={satelliteSelectionBounds}
              pathOptions={{
                color: "#ff6b6b",
                weight: 2,
                opacity: 0.95,
                fillColor: "#ff6b6b",
                fillOpacity: 0.08,
                dashArray: "8 6",
              }}
            />
          )}
          {satelliteCoverageOnMap && satellitePreviewBounds !== null && (
            <Rectangle
              bounds={satellitePreviewBounds}
              pathOptions={{
                color: "#ff9a9a",
                weight: 2,
                opacity: 0.9,
                fillColor: "#ff9a9a",
                fillOpacity: 0.05,
                dashArray: "6 6",
              }}
            />
          )}
          <SatelliteDownloadBoxTool
            enabled={satelliteDrawModeEnabled}
            onPreviewChange={setSatellitePreviewBounds}
            onSelectionChange={setSatelliteSelectionBounds}
          />
          <MapDoubleClickMapMarkerPlacement
            satelliteDrawModeEnabled={satelliteDrawModeEnabled}
            onDoubleClickLatLng={(lat, lng) => handleRequestMapMarkerPlacement(lat, lng)}
          />
          <MapRightClickPinPlacementEvents
            satelliteDrawModeEnabled={satelliteDrawModeEnabled}
            onContextMenuLatLng={handleMapContextMenuLatLng}
          />
          <MapCursorProbe onCursorLatLng={setCursorLatLng} />
          <MapLeafletDrawToolbar onDrawingSaved={handleDrawingSaved} />
          {tacticalLayerVisibility.activeFamily === "network" &&
            tacticalLayerVisibility.network.coverageCircles &&
            tacticalMeshNodeLocations.map((node) => (
              <Circle
                key={`tactical-coverage-${node.node_id}`}
                center={[node.lat, node.lon]}
                radius={TACTICAL_MESH_COVERAGE_RADIUS_METERS}
                pathOptions={{
                  color: "#38bdf8",
                  weight: 1,
                  opacity: 0.5,
                  fillColor: "#38bdf8",
                  fillOpacity: 0.07,
                }}
              />
            ))}
          {tacticalLayerVisibility.activeFamily === "network" &&
            tacticalLayerVisibility.network.links &&
            meshNodeLinkPositions.length >= 2 && (
            <Polyline
              positions={meshNodeLinkPositions}
              pathOptions={{
                color: "#38bdf8",
                weight: 2,
                opacity: 0.88,
                dashArray: "7 5",
              }}
            />
          )}
          {tacticalLayerVisibility.activeFamily === "network" &&
            tacticalLayerVisibility.network.nodes &&
            tacticalMeshNodeLocations.map((node) => (
              <Marker key={`tactical-mesh-node-${node.node_id}`} position={[node.lat, node.lon]} icon={meshNodeMarkerIcon}>
                <Popup>
                  <strong>{node.label.trim().length > 0 ? node.label : "Mesh node"}</strong>
                  <p>Node ID: {node.node_id}</p>
                </Popup>
              </Marker>
            ))}
          {tacticalPointsOfInterest
            .filter((point) => {
              const classification = classifyTacticalPoint(point.notes);
              if (tacticalLayerVisibility.activeFamily === "network") {
                return classification.bucket === "network_pin";
              }
              if (classification.bucket === "network_pin") {
                return false;
              }
              if (classification.bucket === "custom_pin") {
                return tacticalLayerVisibility.customPins;
              }
              return tacticalLayerVisibility.resources[classification.kind];
            })
            .map((point) => {
              const classification = classifyTacticalPoint(point.notes);
              const kind = parseTacticalResourceKind(point.notes);
              const notesBody = stripTacticalNotesBody(point.notes);
              const lucideName = parseTacticalLucideIconName(point.notes);
              const lucideIcon = lucideName != null ? getTacticalLucideMarkerIcon(lucideName) : null;
              const markerIcon = lucideIcon ?? tacticalResourceIcons[kind];
              const networkCoverageM =
                classification.bucket === "network_pin" ? parseTacticalNetworkCoverageRadiusMeters(point.notes) : null;
              const showNetCoverage =
                networkCoverageM != null &&
                networkCoverageM > 0 &&
                tacticalLayerVisibility.activeFamily === "network" &&
                tacticalLayerVisibility.network.coverageCircles;
              return (
                <Fragment key={`tactical-resource-${point.id}`}>
                  {showNetCoverage ? (
                    <Circle
                      center={[point.lat, point.lon]}
                      radius={networkCoverageM}
                      pathOptions={{
                        color: "#38bdf8",
                        weight: 1,
                        opacity: 0.5,
                        fillColor: "#38bdf8",
                        fillOpacity: 0.07,
                      }}
                    />
                  ) : null}
                  <Marker position={[point.lat, point.lon]} icon={markerIcon}>
                  <Popup>
                    <strong>{point.name}</strong>
                    {notesBody.length > 0 ? <p>{notesBody}</p> : null}
                    <div className="map-popup-actions">
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => handleBroadcastTacticalPoi(point.id)}
                      >
                        Broadcast to Mesh
                      </button>{" "}
                      <button
                        type="button"
                        className="map-popup-remove"
                        onClick={() => handleDeleteTacticalPoi(point.id)}
                      >
                        Delete
                      </button>
                    </div>
                  </Popup>
                </Marker>
                </Fragment>
              );
            })}
          {layerFilters.pois &&
            pois.map((poi) => (
              <Marker key={poi.id} position={[poi.lat, poi.lon]} icon={defaultIcon}>
                <Popup>
                  <strong>{poi.name}</strong>
                  {poi.notes && <p>{poi.notes}</p>}
                  <button type="button" className="map-popup-remove" onClick={() => onRemovePoi(poi.id)}>
                    Remove
                  </button>
                </Popup>
              </Marker>
            ))}
          {mapDrawings.map((drawing) => (
            <PersistedDrawingGeoJsonLayerReadOnly key={`persisted-drawing-${drawing.id}`} drawing={drawing} />
          ))}
          {mapMarkers.map((marker) => (
            <Marker
              key={`map-marker-${marker.id}`}
              position={[marker.lat, marker.lng]}
              icon={getMapMarkerCategoryIcon(marker.category)}
            >
              <Popup>
                <strong>{marker.title}</strong>
                <p className="map-marker-popup-category">Category: {normalizeMapMarkerCategory(marker.category)}</p>
                {marker.description.trim().length > 0 ? <p>{marker.description}</p> : null}
                <div className="map-popup-actions">
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => handleBroadcastMapMarker(marker.id)}
                  >
                    Broadcast to Mesh
                  </button>{" "}
                  <button
                    type="button"
                    className="map-popup-remove"
                    onClick={() => handleDeleteMapMarker(marker.id)}
                  >
                    Delete
                  </button>
                </div>
              </Popup>
            </Marker>
          ))}
          {activePeers.map((peer) => (
            <Marker
              key={`active-peer-${peer.node_id}`}
              position={[peer.latitude, peer.longitude]}
              icon={livePeerMarkerIcon}
            >
              <Popup>
                <strong>Peer {peer.node_id}</strong>
                <p>
                  Last seen:{" "}
                  {Number.isNaN(new Date(peer.last_seen_at).getTime())
                    ? peer.last_seen_at
                    : new Date(peer.last_seen_at).toLocaleString()}
                </p>
              </Popup>
            </Marker>
          ))}
        </MapContainer>
        <div className="map-view-floating-stack">
          <div className="glass-panel map-coordinate-hud">
            <div className="map-coordinate-hud-header">
              <span className="map-coordinate-hud-title">Position</span>
              <button
                type="button"
                className="map-coordinate-hud-toggle"
                onClick={() => setHudShowDd((v) => !v)}
                title={hudShowDd ? "Show MGRS" : "Show decimal / UTM"}
              >
                {hudShowDd ? "DD / UTM" : "MGRS"}
              </button>
            </div>
            {!cursorLatLng ? (
              <p className="map-coordinate-hud-muted">Move the cursor over the map…</p>
            ) : !coordHud ? (
              <p className="map-coordinate-hud-muted">Resolving…</p>
            ) : hudShowDd ? (
              <div className="map-coordinate-hud-body">
                <div className="map-coordinate-hud-line">{coordHud.decimal_degrees}</div>
                <div className="map-coordinate-hud-sub">{coordHud.utm}</div>
              </div>
            ) : (
              <div className="map-coordinate-hud-body map-coordinate-hud-mgrs">{coordHud.mgrs}</div>
            )}
          </div>
          {mapDrawings.length > 0 ? (
            <div className="glass-panel map-drawings-list">
              <div className="map-drawings-list-title">Saved drawings</div>
              <ul className="map-drawings-list-ul">
                {mapDrawings.map((d) => (
                  <li key={d.id} className="map-drawings-list-item">
                    <span className="map-drawings-list-label">
                      {d.feature_type} · {new Date(d.created_at).toLocaleString()}
                    </span>
                    <button type="button" className="map-drawings-list-delete" onClick={() => handleDeleteDrawing(d.id)}>
                      Delete
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
        <MapMarkerPlacementModal
          open={pendingMapMarkerPlacement !== null}
          latLng={pendingMapMarkerPlacement}
          onClose={() => setPendingMapMarkerPlacement(null)}
          onSave={handleSaveMapMarkerFromModal}
        />
        <PoiDetailsModal
          open={rightClickPinLatLng !== null}
          latLng={rightClickPinLatLng}
          onClose={() => setRightClickPinLatLng(null)}
          onSave={onAddTacticalPoiPin}
        />
        <TacticalSidebar
          initialVisibilityState={tacticalLayerVisibility}
          onVisibilityStateChange={onTacticalLayerVisibilityChange}
          onTacticalDataImported={onReloadTacticalMapLayers}
        />
        <MapSettingsPopover
          basemap={basemap}
          onBasemapChange={onBasemapChange}
          canOfflineOsm={canOfflineOsm}
          canOfflineTopo={canOfflineTopo}
          canOfflineImported={canOfflineImported}
          canOfflineArchiveRaster={canOfflineArchiveRaster}
          layerFilters={layerFilters}
          setLayerFilters={setLayerFilters}
          satelliteCoverageOnMap={satelliteCoverageOnMap}
          onSatelliteCoverageOnMapChange={setSatelliteCoverageOnMap}
          satelliteDrawModeEnabled={satelliteDrawModeEnabled}
          onSatelliteDrawModeToggle={handleSatelliteDrawModeToggle}
          satelliteSelectionBounds={satelliteSelectionBounds}
          satelliteDownloadBusy={satelliteDownloadBusy}
          satelliteDownloadStatus={satelliteDownloadStatus}
          satelliteDownloadJobId={satelliteDownloadJobId}
          onStartSatelliteDownloadFromSelection={handleStartSatelliteDownloadFromSelection}
          onCancelSatelliteBoxSelection={handleCancelSatelliteBoxSelection}
        />
      </div>

      {!embedded ? (
        <header className="map-view-header">
          {onClose !== undefined ? (
            <button type="button" className="map-view-back" onClick={onClose} aria-label="Back to chat">
              ← Back
            </button>
          ) : null}
          <h2 className="map-view-title">Map</h2>
          {onOpenOfflineMapManager !== undefined ? (
            <div className="map-view-header-actions">
              <button
                type="button"
                className="map-view-archives-btn"
                onClick={onOpenOfflineMapManager}
                title="Catalog, downloads, and regional Protomaps extracts"
                aria-label="Open map archive manager"
              >
                Map archives…
              </button>
            </div>
          ) : null}
        </header>
      ) : null}
      {embedded && onOpenOfflineMapManager !== undefined ? (
        <div className="map-view-floating-archives">
          <button
            type="button"
            className="map-view-archives-btn"
            onClick={onOpenOfflineMapManager}
            title="Catalog, downloads, and regional Protomaps extracts"
            aria-label="Open map archive manager"
          >
            Map archives…
          </button>
        </div>
      ) : null}

      <div
        className={[
          "map-view-chat-zone",
          embedded ? "map-view-chat-zone--embedded" : "",
          mapChatMessage ? "map-view-chat-zone-with-response" : "",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        {mapChatMessage && (
          <div className="map-view-response-box">
            <p className="map-view-response-text">{mapChatMessage}</p>
          </div>
        )}
        <form
          className="map-view-chat-bar"
          onSubmit={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onMapChatSend();
          }}
        >
          <input
            type="text"
            className="map-chat-input"
            placeholder="Ask about the map, e.g. “mark my location”…"
            value={mapChatInput}
            onChange={(e) => setMapChatInput(e.target.value)}
            onKeyDown={(e) => {
              // Leaflet enables keyboard panning on the map; stop Enter/Escape from bubbling so submit works here.
              if (e.key === "Enter" && !e.shiftKey) {
                e.stopPropagation();
                if ((e.nativeEvent as KeyboardEvent).isComposing) return;
              }
            }}
            disabled={mapChatSending}
            aria-label="Map chat input"
          />
          <button
            type="submit"
            className="map-chat-send"
            disabled={mapChatSending || !mapChatInput.trim()}
            aria-label="Send"
          >
            →
          </button>
        </form>
      </div>
    </div>
  );
}
