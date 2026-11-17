/**
 * MapLibre-based main map (replaces Leaflet / react-leaflet).
 * Offline-first: basemap from online OSM, local PNG tree, or MBTiles via Tauri protocols.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { athenaListen as listen } from "../../../shared/ipc/athenaIpc";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";
import maplibregl, { type Map as MlMap, type StyleSpecification } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { FeatureCollection, Geometry } from "geojson";
import { buildAthenaMapRasterTilesTemplate, isWindowsWebviewHost } from "./offlineMapArchiveUrls";
import { registerPmtilesMaplibreProtocolOnce } from "../../../shared/lib/maplibrePmtilesProtocolRegistration";
import { mergeTacticalLayerVisibility, type TacticalLayerVisibilityState } from "./TacticalSidebar";
import { MapOverlaysPanel } from "./MapOverlaysPanel";
import {
  blockagesAllowed,
  drawingsAllowed,
  libraryPoisAllowed,
  markerCategoryAllowed,
  meshAllowed,
  peersAllowed,
  tacticalCoverageAllowed,
  tacticalPointAllowed,
} from "./mapOverlayPolicy";
import {
  startSatelliteRegionDownload,
  type SatelliteRegionDownloadCompleteEventPayload,
  type SatelliteRegionDownloadErrorEventPayload,
  type SatelliteRegionDownloadProgressEventPayload,
} from "./offlineMapManagerCatalog";
import { MapSettingsPopover } from "./MapSettingsPopover";
import { PoiDetailsModal } from "./PoiDetailsModal";
import { MapMarkerPlacementModal } from "./MapMarkerPlacementModal";
import { normalizeMapMarkerCategory, type MapMarkerCategory } from "./mapMarkerTypes";
import { isProbablySatelliteArchiveFileName } from "./satelliteMapArchive";
import { LAYER_ROUTE, SRC_ROUTE } from "./maplibreLayerIds";
import {
  classifyTacticalPoint,
  parseTacticalResourceKind,
  parseTacticalNetworkCoverageRadiusMeters,
  tacticalResourceColor,
  type TacticalResourceKind,
} from "./tacticalMapClassify";
import type { CoordinateConversionResult, MapBasemap } from "./mapViewTypes";
import { MapLeftRail } from "./MapLeftRail";
import { TerritoriesPanel } from "./TerritoriesPanel";
import { TerritorySaveModal } from "./TerritorySaveModal";
import { MapLayersPanel } from "./MapLayersPanel";
import { SavedPlacesPanel } from "./SavedPlacesPanel";
import { MapCoordinateReadout } from "./MapCoordinateReadout";
import { MapSmartBar, mapAssistantSendOnInvoke } from "./MapSmartBar";
import { RoutePanel } from "./RoutePanel";
import { DirectionsPanel } from "./DirectionsPanel";
import { BlockagesPanel } from "./BlockagesPanel";
import { ApprovalInbox } from "./ApprovalInbox";
import { ChevronDown, ChevronUp, Crosshair, Globe } from "lucide-react";
import type { GeoJSONSource } from "maplibre-gl";
import type { Feature, LineString } from "geojson";
import { SoftToast } from "../../../ui/components/SoftToast";
import { formatMapToolChipText, markerProposalOf, summarizeMapToolResults, type MapToolResult } from "./mapToolSummary";
import { addStreetMaps, placeFromFeatures, streetMapAssetUrls, streetMapFiles, type PlaceAtPoint } from "./streetMapLayers";
import { PlaceCard } from "./PlaceCard";
import { MAP_PANEL_EVENT, takeMapPanelRequest } from "../../../shared/navigation/goTo";

export type { CoordinateConversionResult, MapBasemap } from "./mapViewTypes";
export type { MapMarkerCategory } from "./mapMarkerTypes";

export interface Poi {
  id: string;
  name: string;
  lat: number;
  lon: number;
  notes: string;
}
export interface MapMarker {
  id: string;
  lat: number;
  lng: number;
  title: string;
  description: string;
  created_at: string;
  category: string;
}
export interface MapDrawingRecord {
  id: string;
  feature_type: string;
  geometry_json: string;
  style_json: string;
  created_at: string;
}
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

type RouteBlockageMapRow = {
  id: string;
  geometryJson: string;
  severity?: string;
};

const SRC_TERRITORIES = "athena-ml-territories";
const LAYER_TERR_FILL = "athena-ml-territories-fill";
const LAYER_TERR_LINE = "athena-ml-territories-line";
const SRC_TERRITORY_DRAFT = "athena-ml-territory-draft";
const LAYER_TERR_DRAFT_LINE = "athena-ml-territory-draft-line";
const LAYER_TERR_DRAFT_FILL = "athena-ml-territory-draft-fill";

const SRC_MARKERS = "athena-ml-markers";
const LAYER_MARKERS = "athena-ml-markers-circles";
const SRC_USER_LOC = "athena-ml-user-location";
const LAYER_USER_LOC_HALO = "athena-ml-user-location-halo";
const LAYER_USER_LOC_CORE = "athena-ml-user-location-core";
const SRC_TACTICAL = "athena-ml-tactical";
const LAYER_TACTICAL = "athena-ml-tactical-circles";
const SRC_MESH = "athena-ml-mesh";
const LAYER_MESH = "athena-ml-mesh-circles";
const SRC_POIS = "athena-ml-pois";
const LAYER_POIS = "athena-ml-pois-circles";
const SRC_PEERS = "athena-ml-peers";
const LAYER_PEERS = "athena-ml-peers-circles";
const SRC_DRAWINGS = "athena-ml-drawings";
const LAYER_DRAW_LINES = "athena-ml-draw-lines";
const LAYER_DRAW_FILL = "athena-ml-draw-fills";
const SRC_BLOCKAGES = "athena-ml-blockages";
const LAYER_BLOCK_FILL = "athena-ml-blockages-fill";
const LAYER_BLOCK_LINE = "athena-ml-blockages-line";
const LAYER_BLOCK_PT = "athena-ml-blockages-pt";
const SRC_SAT_PREVIEW = "athena-ml-sat-preview";
const LAYER_SAT_PREVIEW = "athena-ml-sat-preview-line";

let gOfflinePngRoot: string | null = null;
let gConvertFile: ((path: string) => string) | null = null;

function emptyFc(): FeatureCollection {
  return { type: "FeatureCollection", features: [] };
}

function buildRasterStyle(
  basemap: MapBasemap,
  options: {
    canOfflineOsm: boolean;
    canOfflineTopo: boolean;
    canOfflineImported: boolean;
    canOfflineArchiveRaster: boolean;
    showSatellite: boolean;
    offlineOsmRoot: string | null;
    offlineTopoRoot: string | null;
    offlineImportedRoot: string | null;
    archiveRasterFileNames: string[];
  },
): StyleSpecification {
  const sources: NonNullable<StyleSpecification["sources"]> = {};
  const layers: maplibregl.LayerSpecification[] = [];
  if (basemap === "online_osm" || (!options.canOfflineTopo && basemap === "offline_archive_raster")) {
    sources.osm = {
      type: "raster",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      tileSize: 256,
      attribution: "OpenStreetMap",
    };
    layers.push({ id: "osm", type: "raster", source: "osm" });
  } else if (basemap === "offline_osm" && options.canOfflineOsm && options.offlineOsmRoot) {
    gOfflinePngRoot = options.offlineOsmRoot;
    sources.png = { type: "raster", tiles: ["athena://offline-tile/{z}/{x}/{y}"], tileSize: 256 };
    layers.push({ id: "png", type: "raster", source: "png" });
  } else if (basemap === "offline_topo" && options.canOfflineTopo && options.offlineTopoRoot) {
    gOfflinePngRoot = options.offlineTopoRoot;
    sources.png = { type: "raster", tiles: ["athena://offline-tile/{z}/{x}/{y}"], tileSize: 256 };
    layers.push({ id: "png", type: "raster", source: "png" });
  } else if (basemap === "offline_imported" && options.canOfflineImported && options.offlineImportedRoot) {
    gOfflinePngRoot = options.offlineImportedRoot;
    sources.png = { type: "raster", tiles: ["athena://offline-tile/{z}/{x}/{y}"], tileSize: 256 };
    layers.push({ id: "png", type: "raster", source: "png" });
  } else {
    sources.osm = {
      type: "raster",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      tileSize: 256,
      attribution: "OpenStreetMap",
    };
    layers.push({ id: "osm", type: "raster", source: "osm" });
  }
  if (basemap === "offline_archive_raster" && options.canOfflineArchiveRaster && options.showSatellite) {
    for (const name of options.archiveRasterFileNames) {
      if (!isProbablySatelliteArchiveFileName(name)) continue;
      const id = `sat-${encodeURIComponent(name)}`.replace(/%/g, "_");
      sources[id] = {
        type: "raster",
        tiles: [buildAthenaMapRasterTilesTemplate(name)],
        tileSize: 256,
        scheme: "xyz",
        minzoom: 0,
        maxzoom: 24,
      };
      layers.push({
        id: `${id}-layer`,
        type: "raster",
        source: id,
        paint: { "raster-opacity": 0.95 },
      });
    }
  }
  sources[SRC_TERRITORIES] = { type: "geojson", data: emptyFc() };
  layers.push(
    {
      id: LAYER_TERR_FILL,
      type: "fill",
      source: SRC_TERRITORIES,
      paint: { "fill-color": ["get", "color"], "fill-opacity": 0.2 },
    },
    {
      id: LAYER_TERR_LINE,
      type: "line",
      source: SRC_TERRITORIES,
      paint: { "line-color": ["get", "color"], "line-width": 3, "line-opacity": 0.95 },
    },
  );
  sources[SRC_TERRITORY_DRAFT] = { type: "geojson", data: emptyFc() };
  layers.push(
    {
      id: LAYER_TERR_DRAFT_FILL,
      type: "fill",
      source: SRC_TERRITORY_DRAFT,
      paint: { "fill-color": "#f59e0b", "fill-opacity": 0.12 },
    },
    {
      id: LAYER_TERR_DRAFT_LINE,
      type: "line",
      source: SRC_TERRITORY_DRAFT,
      paint: { "line-color": "#f59e0b", "line-width": 2, "line-dasharray": [2, 2] },
    },
  );
  sources[SRC_MARKERS] = { type: "geojson", data: emptyFc() };
  layers.push({
    id: LAYER_MARKERS,
    type: "circle",
    source: SRC_MARKERS,
    paint: {
      "circle-radius": 7,
      "circle-color": ["get", "color"],
      "circle-stroke-color": "#0f172a",
      "circle-stroke-width": 1,
    },
  });
  sources[SRC_USER_LOC] = { type: "geojson", data: emptyFc() };
  // User location "puck": obvious, not confused with other pins.
  layers.push(
    {
      id: LAYER_USER_LOC_HALO,
      type: "circle",
      source: SRC_USER_LOC,
      paint: {
        "circle-radius": 18,
        "circle-color": "#38bdf8",
        "circle-opacity": 0.22,
        "circle-blur": 0.85,
      },
    },
    {
      id: LAYER_USER_LOC_CORE,
      type: "circle",
      source: SRC_USER_LOC,
      paint: {
        "circle-radius": 7.5,
        "circle-color": "#0ea5e9",
        "circle-opacity": 0.98,
        "circle-stroke-color": "rgba(255,255,255,0.95)",
        "circle-stroke-width": 2.5,
      },
    },
  );
  sources[SRC_TACTICAL] = { type: "geojson", data: emptyFc() };
  layers.push({
    id: LAYER_TACTICAL,
    type: "circle",
    source: SRC_TACTICAL,
    paint: { "circle-radius": 6, "circle-color": ["get", "color"], "circle-stroke-color": "#0f172a", "circle-stroke-width": 1 },
  });
  sources[SRC_MESH] = { type: "geojson", data: emptyFc() };
  layers.push({
    id: LAYER_MESH,
    type: "circle",
    source: SRC_MESH,
    paint: { "circle-radius": 5, "circle-color": "#38bdf8", "circle-stroke-color": "#0f172a", "circle-stroke-width": 1 },
  });
  sources[SRC_POIS] = { type: "geojson", data: emptyFc() };
  layers.push({
    id: LAYER_POIS,
    type: "circle",
    source: SRC_POIS,
    paint: { "circle-radius": 5, "circle-color": "#22c55e", "circle-stroke-color": "#0f172a", "circle-stroke-width": 1 },
  });
  sources[SRC_PEERS] = { type: "geojson", data: emptyFc() };
  layers.push({
    id: LAYER_PEERS,
    type: "circle",
    source: SRC_PEERS,
    paint: { "circle-radius": 5, "circle-color": "#f472b6", "circle-stroke-color": "#0f172a", "circle-stroke-width": 1 },
  });
  sources[SRC_DRAWINGS] = { type: "geojson", data: emptyFc() };
  layers.push(
    { id: LAYER_DRAW_FILL, type: "fill", source: SRC_DRAWINGS, filter: ["==", ["get", "fkind"], "polygon"] as unknown as maplibregl.FilterSpecification, paint: { "fill-color": "#38bdf8", "fill-opacity": 0.2 } },
    { id: LAYER_DRAW_LINES, type: "line", source: SRC_DRAWINGS, filter: ["in", ["get", "fkind"], ["literal", ["polyline", "polygon"]]] as unknown as maplibregl.FilterSpecification, paint: { "line-color": "#38bdf8", "line-width": 2 } },
  );
  sources[SRC_BLOCKAGES] = { type: "geojson", data: emptyFc() };
  layers.push(
    {
      id: LAYER_BLOCK_FILL,
      type: "fill",
      source: SRC_BLOCKAGES,
      filter: ["==", ["geometry-type"], "Polygon"] as unknown as maplibregl.FilterSpecification,
      paint: { "fill-color": "#fb923c", "fill-opacity": 0.32 },
    },
    {
      id: LAYER_BLOCK_LINE,
      type: "line",
      source: SRC_BLOCKAGES,
      filter: ["==", ["geometry-type"], "LineString"] as unknown as maplibregl.FilterSpecification,
      paint: { "line-color": "#fb923c", "line-width": 3, "line-opacity": 0.9 },
    },
    {
      id: LAYER_BLOCK_PT,
      type: "circle",
      source: SRC_BLOCKAGES,
      filter: ["==", ["geometry-type"], "Point"] as unknown as maplibregl.FilterSpecification,
      paint: {
        "circle-radius": 7,
        "circle-color": "#f97316",
        "circle-stroke-color": "#0f172a",
        "circle-stroke-width": 1,
      },
    },
  );
  sources[SRC_SAT_PREVIEW] = { type: "geojson", data: emptyFc() };
  layers.push({ id: LAYER_SAT_PREVIEW, type: "line", source: SRC_SAT_PREVIEW, paint: { "line-color": "#ff6b6b", "line-dasharray": [2, 2], "line-width": 2 } });
  sources[SRC_ROUTE] = { type: "geojson", data: emptyFc() };
  layers.push({
    id: LAYER_ROUTE,
    type: "line",
    source: SRC_ROUTE,
    layout: { "line-cap": "round", "line-join": "round" },
    paint: { "line-color": "#a855f7", "line-width": 4, "line-opacity": 0.92 },
  });

  const assets = streetMapAssetUrls();
  return { version: 8, name: "athena-ml", sources, layers, glyphs: assets.glyphs, sprite: assets.sprite };
}

interface MapViewProps {
  pois: Poi[];
  onClose?: () => void;
  embedded?: boolean;
  onAddPoi: (name: string, lat: number, lon: number, notes: string) => Promise<void>;
  onRemovePoi: (id: string) => Promise<void>;
  mapChatInput: string;
  setMapChatInput: (v: string) => void;
  onMapChatSend: () => void;
  mapChatSending: boolean;
  mapChatMessage: string;
  layerFilters: { pois: boolean; roads: boolean; rivers: boolean };
  setLayerFilters: Dispatch<SetStateAction<{ pois: boolean; roads: boolean; rivers: boolean }>>;
  convertFileSrcFn: (path: string) => string;
  basemap: MapBasemap;
  onBasemapChange: (b: MapBasemap) => void;
  offlineOsmRoot: string | null;
  offlineTopoRoot: string | null;
  offlineImportedRoot: string | null;
  archiveRasterFileNames: string[];
  archiveRasterNativeZoomRangesByFile: Record<string, [number, number]>;
  archiveRasterBoundsByFile: Record<string, [number, number, number, number]>;
  savedUserLocationAddress: string;
  onSatelliteRegionDownloaded?: (mapId: string) => void;
  mapCenter: [number, number];
  mapZoom: number;
  tacticalPointsOfInterest: TacticalMapPointOfInterest[];
  tacticalMeshNodeLocations: TacticalMapMeshNodeLocation[];
  onReloadTacticalMapLayers: () => void;
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
  /** For AI approval inbox + badge (rank 4+). */
  authRankLevel?: number;
  /** Enabled MBTiles rasters for the main map (file names with `.mbtiles`). */
  enabledMainMapRasterArchiveFileNames?: string[];
  onMainMapArchiveRasterLayersChange?: (archiveFileNames: string[]) => void;
  /** Opens centralized Data Acquisition (Maps tab) instead of legacy in-map WAN downloads. */
  onOpenDataAcquisition?: () => void;
  /** Resolved GGUF basename / Ollama tag for map-scoped inference (wired from App; map chat send still lives in App). */
  resolvedInferenceModel?: string | null;
}

export function MapView({
  pois,
  onClose,
  embedded = false,
  onAddPoi: _onAddPoi,
  onRemovePoi: _onRemovePoi,
  layerFilters,
  setLayerFilters,
  convertFileSrcFn,
  basemap,
  onBasemapChange,
  offlineOsmRoot,
  offlineTopoRoot,
  offlineImportedRoot,
  archiveRasterFileNames,
  archiveRasterNativeZoomRangesByFile: _z,
  archiveRasterBoundsByFile: _b,
  savedUserLocationAddress,
  onSatelliteRegionDownloaded,
  mapCenter,
  mapZoom,
  tacticalPointsOfInterest,
  tacticalMeshNodeLocations,
  onReloadTacticalMapLayers,
  onAddTacticalPoiPin,
  tacticalLayerVisibility,
  onTacticalLayerVisibilityChange,
  authRankLevel = 0,
  enabledMainMapRasterArchiveFileNames,
  onMainMapArchiveRasterLayersChange,
  onOpenDataAcquisition,
  resolvedInferenceModel: _resolvedInferenceModel,
}: MapViewProps) {
  const tacticalVis = useMemo(
    () => mergeTacticalLayerVisibility(tacticalLayerVisibility),
    [tacticalLayerVisibility],
  );
  const hostRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MlMap | null>(null);
  const [mlMap, setMlMap] = useState<MlMap | null>(null);
  const [mapMarkers, setMapMarkers] = useState<MapMarker[]>([]);
  const [mapDrawings, setMapDrawings] = useState<MapDrawingRecord[]>([]);
  const [activePeers, setActivePeers] = useState<
    { node_id: string; latitude: number; longitude: number; last_seen_at: string }[]
  >([]);
  const [cursorLatLng, setCursorLatLng] = useState<{ lat: number; lng: number } | null>(null);
  const [coordHud, setCoordHud] = useState<CoordinateConversionResult | null>(null);
  const [hudShowDd, setHudShowDd] = useState(true);
  const [mapAssistantReply, setMapAssistantReply] = useState<string>("");
  const [isProcessing, setIsProcessing] = useState(false);
  const [mapAssistantToolResults, setMapAssistantToolResults] = useState<MapToolResult[]>([]);
  const [mapAssistantCollapsed, setMapAssistantCollapsed] = useState(false);
  const [pendingMapMarkerPlacement, setPendingMapMarkerPlacement] = useState<{ lat: number; lng: number } | null>(null);
  const [rightClickPinLatLng, setRightClickPinLatLng] = useState<{ lat: number; lng: number } | null>(null);
  const [mapContextMenu, setMapContextMenu] = useState<{
    x: number;
    y: number;
    lat: number;
    lng: number;
  } | null>(null);
  const [routeHereLatLng, setRouteHereLatLng] = useState<{ lat: number; lng: number } | null>(null);
  const [routeHereSeq, setRouteHereSeq] = useState(0);
  /** Building or place the user tapped on the offline street map. */
  const [tappedPlace, setTappedPlace] = useState<PlaceAtPoint | null>(null);
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);
  const [blockages, setBlockages] = useState<RouteBlockageMapRow[]>([]);
  const [savedPlaces, setSavedPlaces] = useState<unknown[]>([]);
  const [showRoute, setShowRoute] = useState(false);
  const [showDirections, setShowDirections] = useState(false);
  const [showBlockages, setShowBlockages] = useState(false);
  const [overlaysOpen, setOverlaysOpen] = useState(false);
  const [layersOpen, setLayersOpen] = useState(false);
  // Links from Settings / "find anything" can open a map panel directly.
  const [mapSettingsOpenSeq, setMapSettingsOpenSeq] = useState(0);
  const [mapFilesOpenSeq, setMapFilesOpenSeq] = useState(0);
  const [routingSetupOpenSeq, setRoutingSetupOpenSeq] = useState(0);
  useEffect(() => {
    const apply = () => {
      const panel = takeMapPanelRequest();
      if (panel === "layers") setLayersOpen(true);
      else if (panel === "map_files") {
        setLayersOpen(true);
        setMapFilesOpenSeq((n) => n + 1);
      }
      else if (panel === "map_settings") setMapSettingsOpenSeq((n) => n + 1);
      else if (panel === "directions") setShowDirections(true);
      else if (panel === "routing_setup") {
        setShowDirections(true);
        setRoutingSetupOpenSeq((n) => n + 1);
      }
    };
    apply();
    window.addEventListener(MAP_PANEL_EVENT, apply);
    return () => window.removeEventListener(MAP_PANEL_EVENT, apply);
  }, []);
  const [savedOpen, setSavedOpen] = useState(false);
  const [territoriesOpen, setTerritoriesOpen] = useState(false);
  const [territoriesVisible, setTerritoriesVisible] = useState(true);
  const [territoryRows, setTerritoryRows] = useState<
    {
      id: string;
      name: string;
      status: string;
      color: string;
      geometryJson: string;
    }[]
  >([]);
  const [isDrawingTerritory, setIsDrawingTerritory] = useState(false);
  const [territoryDrawPoints, setTerritoryDrawPoints] = useState<[number, number][]>([]);
  const [territorySaveOpen, setTerritorySaveOpen] = useState(false);
  const isDrawingTerritoryRef = useRef(false);
  const territoryDrawPointsRef = useRef<[number, number][]>([]);
  const satPreviewStart = useRef<maplibregl.LngLat | null>(null);
  const [satelliteDrawModeEnabled, setSatelliteDrawModeEnabled] = useState(false);
  const [satelliteCoverageOnMap, setSatelliteCoverageOnMap] = useState(false);
  const [satelliteSelectionBounds, setSatelliteSelectionBounds] = useState<[[number, number], [number, number]] | null>(
    null,
  );
  const [, setSatellitePreviewBounds] = useState<[[number, number], [number, number]] | null>(null);
  const [satelliteDownloadBusy, setSatelliteDownloadBusy] = useState(false);
  const [satelliteDownloadStatus, setSatelliteDownloadStatus] = useState("");
  const [satelliteDownloadJobId, setSatelliteDownloadJobId] = useState<string | null>(null);
  const activeSatelliteJobIdRef = useRef<string | null>(null);
  const [aiPendingCount, setAiPendingCount] = useState(0);
  const [approvalInboxOpen, setApprovalInboxOpen] = useState(false);
  const chatZoneRef = useRef<HTMLDivElement | null>(null);

  const canOfflineOsm = Boolean(offlineOsmRoot);
  const canOfflineTopo = Boolean(offlineTopoRoot);
  const canOfflineImported = Boolean(offlineImportedRoot);
  const canOfflineArchiveRaster = archiveRasterFileNames.length > 0;
  const hasSatelliteHeuristic = useMemo(
    () => archiveRasterFileNames.some((f) => isProbablySatelliteArchiveFileName(f)),
    [archiveRasterFileNames],
  );

  const showOnlineOsmBase = basemap === "online_osm" || (basemap === "offline_archive_raster" && !canOfflineTopo);

  const setUserLocationMarker = useCallback(
    (lat: number, lng: number) => {
      if (!mlMap) return;
      const src = mlMap.getSource(SRC_USER_LOC) as GeoJSONSource | undefined;
      if (!src) return;
      src.setData({
        type: "FeatureCollection",
        features: [
          {
            type: "Feature",
            properties: {},
            geometry: { type: "Point", coordinates: [lng, lat] },
          },
        ],
      } as any);
    },
    [mlMap],
  );

  const flyToSavedOrNativeAndMark = useCallback(async () => {
    if (!mlMap) return;
    // 1) Prefer explicit ATHENA Settings → Location (address), then geocode.
    try {
      const addr = await invoke<string | null>("get_user_location");
      const q = String(addr ?? "").trim();
      if (q.length > 0) {
        const geo = await invoke<[number, number]>("geocode_place_query", { query: q });
        const lat = geo?.[0];
        const lon = geo?.[1];
        if (typeof lat === "number" && typeof lon === "number") {
          const z = Math.max(mlMap.getZoom(), 14);
          mlMap.flyTo({ center: [lon, lat], zoom: z, essential: true });
          setUserLocationMarker(lat, lon);
          return;
        }
      }
    } catch {
      // Ignore and fall back.
    }

    // 2) Last-known GPS from map settings.
    try {
      const s = await invoke<{ lastKnownGpsLat?: number | null; lastKnownGpsLng?: number | null }>("get_map_display_settings");
      const lat = s?.lastKnownGpsLat ?? null;
      const lon = s?.lastKnownGpsLng ?? null;
      if (typeof lat === "number" && typeof lon === "number") {
        const z = Math.max(mlMap.getZoom(), 14);
        mlMap.flyTo({ center: [lon, lat], zoom: z, essential: true });
        setUserLocationMarker(lat, lon);
        return;
      }
    } catch {
      // Ignore and fall back.
    }

    // 3) Native geolocation.
    if (typeof navigator === "undefined" || !navigator.geolocation) {
      setToast({
        tone: "warning",
        message: "No location set in ATHENA. Please set your location in Settings, or enable macOS Location Services.",
      });
      return;
    }

    navigator.geolocation.getCurrentPosition(
      (pos) => {
        const lng = pos.coords.longitude;
        const lat = pos.coords.latitude;
        const z = Math.max(mlMap.getZoom(), 14);
        mlMap.flyTo({ center: [lng, lat], zoom: z, essential: true });
        setUserLocationMarker(lat, lng);
      },
      () => {
        setToast({
          tone: "warning",
          message: "No location set in ATHENA. Please set your location in Settings, or enable macOS Location Services.",
        });
      },
      { enableHighAccuracy: true, timeout: 9000, maximumAge: 60_000 },
    );
  }, [mlMap, setUserLocationMarker]);

  useEffect(() => {
    isDrawingTerritoryRef.current = isDrawingTerritory;
  }, [isDrawingTerritory]);
  useEffect(() => {
    territoryDrawPointsRef.current = territoryDrawPoints;
  }, [territoryDrawPoints]);

  const fitWorld = useCallback(() => {
    if (!mlMap) return;
    try {
      mlMap.fitBounds(
        [
          [-180, -85],
          [180, 85],
        ],
        { padding: 16, duration: 600 },
      );
    } catch {
      mlMap.fitBounds([
        [-180, -85],
        [180, 85],
      ]);
    }
  }, [mlMap]);

  const territoryGeometryJsonForSave = useMemo(() => {
    if (territoryDrawPoints.length < 3) return "";
    const first = territoryDrawPoints[0]!;
    const ring: [number, number][] = [...territoryDrawPoints, first];
    return JSON.stringify({ type: "Polygon", coordinates: [ring] });
  }, [territoryDrawPoints]);

  const flyToTerritoryById = useCallback(
    (id: string) => {
      if (!mlMap) return;
      const t = territoryRows.find((x) => x.id === id);
      if (!t) return;
      try {
        const g = JSON.parse(t.geometryJson) as { coordinates?: number[][][] };
        const ring = g.coordinates?.[0];
        if (!ring || ring.length < 2) return;
        let sx = 0;
        let sy = 0;
        for (const p of ring) {
          sx += p[0] ?? 0;
          sy += p[1] ?? 0;
        }
        const n = ring.length;
        mlMap.flyTo({
          center: [sx / n, sy / n],
          zoom: Math.max(mlMap.getZoom(), 12),
          essential: true,
        });
      } catch {
        /* empty */
      }
    },
    [mlMap, territoryRows],
  );

  // Dynamically reserve space above the bottom assistant bar so docked panels never clip behind it.
  useLayoutEffect(() => {
    const el = chatZoneRef.current;
    if (!el || typeof window === "undefined") return;
    const root = document.documentElement;
    const prev = root.style.getPropertyValue("--bottom-safe-area");

    const compute = () => {
      const r = el.getBoundingClientRect();
      const occupied = Math.max(0, window.innerHeight - r.top);
      // Add a small cushion so the panel never kisses the bar.
      const px = Math.min(window.innerHeight * 0.6, occupied + 12);
      root.style.setProperty("--bottom-safe-area", `${Math.ceil(px)}px`);
    };

    compute();
    const onResize = () => compute();
    window.addEventListener("resize", onResize);
    let ro: ResizeObserver | null = null;
    if ("ResizeObserver" in window) {
      ro = new ResizeObserver(() => compute());
      ro.observe(el);
    }
    return () => {
      window.removeEventListener("resize", onResize);
      ro?.disconnect();
      if (prev) root.style.setProperty("--bottom-safe-area", prev);
      else root.style.removeProperty("--bottom-safe-area");
    };
  }, [embedded, mapAssistantReply, mapAssistantCollapsed, mapAssistantToolResults.length]);

  const loadTerritories = useCallback(async () => {
    try {
      const list = await invoke<
        { id: string; name: string; status: string; color: string; geometryJson: string }[]
      >("get_territories");
      setTerritoryRows(Array.isArray(list) ? list : []);
    } catch {
      setTerritoryRows([]);
    }
  }, []);

  const applyMapAssistantToolEffects = useCallback(
    (tools: MapToolResult[]) => {
      if (!mlMap || tools.length === 0) return;
      for (const t of tools) {
        if (!t || !t.ok) continue;
        if (t.tool === "route") {
          const data: any = t.data;
          const gj = data?.geojson;
          if (!gj || typeof gj !== "object") continue;
          const typ = (gj as any).type as string | undefined;
          let fc: FeatureCollection | null = null;
          if (typ === "LineString") {
            const feat: Feature<LineString> = { type: "Feature", properties: {}, geometry: gj as LineString };
            fc = { type: "FeatureCollection", features: [feat] };
          } else if (typ === "FeatureCollection") {
            fc = gj as FeatureCollection;
          } else if (typ === "Feature") {
            fc = { type: "FeatureCollection", features: [gj as Feature] };
          }
          if (!fc) continue;
          try {
            if (mlMap.getSource(SRC_ROUTE)) {
              const src = mlMap.getSource(SRC_ROUTE) as GeoJSONSource;
              src.setData(fc);
            }
          } catch {
            // ignore tool effect failures; UI already has tool results for debugging
          }
        }
        if (t.tool === "directions") {
          const data: any = t.data;
          const r = data?.route;
          const gj = r?.geojson;
          if (!gj || typeof gj !== "object") continue;
          const typ = (gj as any).type as string | undefined;
          let fc: FeatureCollection | null = null;
          if (typ === "LineString") {
            const feat: Feature<LineString> = { type: "Feature", properties: {}, geometry: gj as LineString };
            fc = { type: "FeatureCollection", features: [feat] };
          } else if (typ === "FeatureCollection") {
            fc = gj as FeatureCollection;
          } else if (typ === "Feature") {
            fc = { type: "FeatureCollection", features: [gj as Feature] };
          }
          if (!fc) continue;
          try {
            if (mlMap.getSource(SRC_ROUTE)) {
              const src = mlMap.getSource(SRC_ROUTE) as GeoJSONSource;
              src.setData(fc);
            }
          } catch {
            /* empty */
          }
        }
        if (t.tool === "update_territory_status") {
          void loadTerritories();
        }
      }
    },
    [mlMap, loadTerritories],
  );

  const loadMapMarkers = useCallback(async () => {
    try {
      const list = await invoke<MapMarker[]>("get_map_markers");
      setMapMarkers(Array.isArray(list) ? list : []);
    } catch {
      setMapMarkers([]);
    }
  }, []);
  const loadMapDrawings = useCallback(async () => {
    try {
      const list = await invoke<MapDrawingRecord[]>("get_drawings");
      setMapDrawings(Array.isArray(list) ? list : []);
    } catch {
      setMapDrawings([]);
    }
  }, []);
  const loadBlockages = useCallback(async () => {
    try {
      const list = await invoke<RouteBlockageMapRow[]>("list_route_blockages", { include_rejected: false });
      setBlockages(Array.isArray(list) ? list : []);
    } catch {
      setBlockages([]);
    }
  }, []);
  const loadSavedPlaces = useCallback(async () => {
    try {
      const list = await invoke<unknown[]>("list_saved_places");
      setSavedPlaces(Array.isArray(list) ? list : []);
    } catch {
      setSavedPlaces([]);
    }
  }, []);

  useEffect(() => {
    void registerPmtilesMaplibreProtocolOnce();
  }, []);
  useEffect(() => {
    if (authRankLevel < 4) {
      setAiPendingCount(0);
      return;
    }
    const loadN = () => {
      void invoke<number>("ai_count_pending")
        .then((n) => setAiPendingCount(typeof n === "number" ? n : 0))
        .catch(() => setAiPendingCount(0));
    };
    loadN();
    const id = window.setInterval(loadN, 45_000);
    return () => window.clearInterval(id);
  }, [authRankLevel]);
  useEffect(() => {
    gConvertFile = convertFileSrcFn;
  }, [convertFileSrcFn]);

  useEffect(() => {
    void loadMapMarkers();
    void loadMapDrawings();
    void loadBlockages();
    void loadSavedPlaces();
  }, [loadMapDrawings, loadMapMarkers, loadBlockages, loadSavedPlaces]);

  useEffect(() => {
    if (!hostRef.current || mapRef.current) return;
    gOfflinePngRoot =
      showOnlineOsmBase
        ? null
        : basemap === "offline_osm"
          ? offlineOsmRoot
          : basemap === "offline_topo"
            ? offlineTopoRoot
            : basemap === "offline_imported"
              ? offlineImportedRoot
              : null;
    const style = buildRasterStyle(basemap, {
      canOfflineOsm,
      canOfflineTopo,
      canOfflineImported,
      canOfflineArchiveRaster,
      showSatellite: hasSatelliteHeuristic,
      offlineOsmRoot,
      offlineTopoRoot,
      offlineImportedRoot,
      archiveRasterFileNames,
    });
    const map = new maplibregl.Map({
      container: hostRef.current,
      style,
      center: [mapCenter[1], mapCenter[0]],
      zoom: mapZoom,
      minZoom: 0,
      maxZoom: 22,
      pitch: 0,
      bearing: 0,
      dragRotate: false,
      touchPitch: false,
      transformRequest: (url) => {
        if (url.startsWith("athena://offline-tile/") && gConvertFile && gOfflinePngRoot) {
          const m = url.match(/athena:\/\/offline-tile\/(\d+)\/(\d+)\/(\d+)/);
          if (m) {
            const p = `${gOfflinePngRoot.replace(/\/$/, "")}/${m[1]}/${m[2]}/${m[3]}.png`;
            return { url: gConvertFile(p) };
          }
        }
        if (isWindowsWebviewHost() && url.startsWith("http://athena-mbtiles.localhost/")) {
          return { url };
        }
        if (url.startsWith("athena-mbtiles://")) {
          return { url };
        }
        if (isWindowsWebviewHost() && url.startsWith("http://athena-map.localhost/")) {
          return { url };
        }
        if (url.startsWith("athena-map://")) {
          return { url };
        }
        return { url };
      },
    });
    map.on("moveend", () => {
      const z = map.getZoom();
      void z;
    });
    map.on("click", (e) => {
      if (isDrawingTerritoryRef.current) {
        setTerritoryDrawPoints((p) => [...p, [e.lngLat.lng, e.lngLat.lat]]);
      }
    });
    map.on("dblclick", (e) => {
      e.preventDefault();
      if (isDrawingTerritoryRef.current) {
        if (territoryDrawPointsRef.current.length >= 3) {
          setTerritorySaveOpen(true);
        }
        return;
      }
      if (satelliteDrawModeEnabled) return;
      setPendingMapMarkerPlacement({ lat: e.lngLat.lat, lng: e.lngLat.lng });
    });
    map.on("contextmenu", (e) => {
      e.preventDefault();
      if (isDrawingTerritoryRef.current) return;
      if (satelliteDrawModeEnabled) return;
      const oe = (e as unknown as { originalEvent?: MouseEvent }).originalEvent;
      const x = oe?.clientX ?? 0;
      const y = oe?.clientY ?? 0;
      setMapContextMenu({ x, y, lat: e.lngLat.lat, lng: e.lngLat.lng });
    });
    map.on("click", () => {
      setMapContextMenu(null);
    });
    map.on("mousemove", (e) => {
      setCursorLatLng({ lat: e.lngLat.lat, lng: e.lngLat.lng });
    });
    map.on("click", (e) => {
      if (isDrawingTerritoryRef.current) return;
      if (!satelliteDrawModeEnabled) return;
      if (satPreviewStart.current === null) {
        satPreviewStart.current = e.lngLat;
        setSatellitePreviewBounds(null);
        return;
      }
      const a = satPreviewStart.current;
      const b = e.lngLat;
      setSatelliteSelectionBounds([
        [a.lat, a.lng],
        [b.lat, b.lng],
      ]);
      satPreviewStart.current = null;
    });
    mapRef.current = map;
    setMlMap(map);
    return () => {
      map.remove();
      mapRef.current = null;
      setMlMap(null);
    };
  }, [
    // One-shot init (guarded by `mapRef.current`). Keep deps minimal to avoid redundant effect executions.
    basemap,
    showOnlineOsmBase,
    hasSatelliteHeuristic,
    offlineOsmRoot,
    offlineTopoRoot,
    offlineImportedRoot,
    archiveRasterFileNames,
  ]);

  // Offline street maps (downloaded Protomaps extracts): roads, names, buildings, addresses.
  // Over aerial imagery only roads and labels are drawn so the imagery shows through.
  useEffect(() => {
    const m = mlMap;
    if (!m) return;
    let cancelled = false;
    let streetLayerIds: string[] = [];
    const overImagery = basemap === "offline_archive_raster" && hasSatelliteHeuristic;
    const onClick = (e: maplibregl.MapMouseEvent) => {
      if (isDrawingTerritoryRef.current || satelliteDrawModeEnabled || streetLayerIds.length === 0) return;
      const live = streetLayerIds.filter((id) => m.getLayer(id));
      const pad = 4;
      const hits = m.queryRenderedFeatures(
        [
          [e.point.x - pad, e.point.y - pad],
          [e.point.x + pad, e.point.y + pad],
        ],
        { layers: live },
      );
      const place = placeFromFeatures(hits, e.lngLat.lat, e.lngLat.lng);
      setTappedPlace(place);
    };
    const add = () => {
      void invoke<string[]>("list_downloaded_offline_map_ids")
        .then((ids) => {
          if (cancelled || !Array.isArray(ids)) return;
          const files = streetMapFiles(ids);
          if (files.length === 0) return;
          streetLayerIds = addStreetMaps(m, files, overImagery, LAYER_TERR_FILL);
        })
        .catch(() => undefined);
    };
    if (m.isStyleLoaded()) add();
    else m.once("load", add);
    m.on("click", onClick);
    return () => {
      cancelled = true;
      m.off("click", onClick);
    };
  }, [mlMap, basemap, hasSatelliteHeuristic, satelliteDrawModeEnabled]);

  useEffect(() => {
    const m = mapRef.current;
    const host = hostRef.current;
    if (!m || !host) return;
    let pressId: number | null = null;
    let startX = 0;
    let startY = 0;
    const clear = () => {
      if (pressId != null) {
        window.clearTimeout(pressId);
        pressId = null;
      }
    };
    const onTouchStart = (ev: TouchEvent) => {
      if (!ev.touches || ev.touches.length !== 1) return;
      const t = ev.touches[0]!;
      startX = t.clientX;
      startY = t.clientY;
      clear();
      pressId = window.setTimeout(() => {
        pressId = null;
        const rect = host.getBoundingClientRect();
        const px = startX - rect.left;
        const py = startY - rect.top;
        const lngLat = m.unproject([px, py]);
        setMapContextMenu({ x: startX, y: startY, lat: lngLat.lat, lng: lngLat.lng });
      }, 520);
    };
    const onTouchMove = (ev: TouchEvent) => {
      if (!ev.touches || ev.touches.length !== 1) return;
      const t = ev.touches[0]!;
      const dx = t.clientX - startX;
      const dy = t.clientY - startY;
      if (Math.hypot(dx, dy) > 10) clear();
    };
    const onTouchEnd = () => clear();
    host.addEventListener("touchstart", onTouchStart, { passive: true });
    host.addEventListener("touchmove", onTouchMove, { passive: true });
    host.addEventListener("touchend", onTouchEnd, { passive: true });
    host.addEventListener("touchcancel", onTouchEnd, { passive: true });
    return () => {
      clear();
      host.removeEventListener("touchstart", onTouchStart);
      host.removeEventListener("touchmove", onTouchMove);
      host.removeEventListener("touchend", onTouchEnd);
      host.removeEventListener("touchcancel", onTouchEnd);
    };
  }, [mlMap]);

  useEffect(() => {
    const m = mapRef.current;
    if (!m) return;
    m.flyTo({ center: [mapCenter[1], mapCenter[0]], zoom: mapZoom, essential: true });
  }, [mapCenter, mapZoom]);

  const markerColor = (cat: string): string => {
    const c = normalizeMapMarkerCategory(cat);
    if (c === "water") return "#38bdf8";
    if (c === "cache") return "#f59e0b";
    if (c === "med") return "#e879f9";
    return "#ef4444";
  };

  const markerFc: FeatureCollection = useMemo(() => {
    return {
      type: "FeatureCollection",
      features: mapMarkers
        .filter((m) => markerCategoryAllowed(tacticalVis, m.category))
        .map((m) => ({
          type: "Feature",
          id: m.id,
          properties: { title: m.title, id: m.id, color: markerColor(m.category) },
          geometry: { type: "Point", coordinates: [m.lng, m.lat] },
        })),
    };
  }, [mapMarkers, tacticalVis]);

  const territoryFc: FeatureCollection = useMemo(() => {
    const feats: GeoJSON.Feature[] = [];
    for (const t of territoryRows) {
      try {
        const geom = JSON.parse(t.geometryJson) as GeoJSON.Polygon;
        if (geom.type !== "Polygon") continue;
        feats.push({
          type: "Feature",
          id: t.id,
          properties: { id: t.id, name: t.name, status: t.status, color: t.color },
          geometry: geom,
        });
      } catch {
        /* skip */
      }
    }
    return { type: "FeatureCollection", features: feats };
  }, [territoryRows]);

  const territoryDraftFc: FeatureCollection = useMemo(() => {
    if (territoryDrawPoints.length < 2) {
      return emptyFc();
    }
    if (territoryDrawPoints.length < 3) {
      return {
        type: "FeatureCollection",
        features: [
          {
            type: "Feature",
            properties: {} as Record<string, unknown>,
            geometry: { type: "LineString", coordinates: territoryDrawPoints },
          },
        ],
      };
    }
    const first = territoryDrawPoints[0]!;
    const ring: [number, number][] = [...territoryDrawPoints, first];
    return {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: {} as Record<string, unknown>,
          geometry: { type: "Polygon", coordinates: [ring] },
        },
      ],
    };
  }, [territoryDrawPoints]);

  const tacticalFc: FeatureCollection = useMemo(() => {
    const feats: GeoJSON.Feature[] = [];
    const v = tacticalVis;
    for (const point of tacticalPointsOfInterest) {
      const notes = point.notes;
      const c = classifyTacticalPoint(notes);
      if (c.bucket === "network_pin") {
        const r = parseTacticalNetworkCoverageRadiusMeters(notes);
        if (tacticalCoverageAllowed(v, notes) && r) {
          const km = r / 1000;
          const steps = 64;
          const ring: [number, number][] = [];
          for (let i = 0; i <= steps; i += 1) {
            const t = (i / steps) * 2 * Math.PI;
            const dx = km / 111.32;
            ring.push([
              point.lon + (dx * Math.cos(t)) / Math.cos((point.lat * Math.PI) / 180),
              point.lat + (km / 111.32) * Math.sin(t),
            ]);
          }
          feats.push({
            type: "Feature",
            properties: { k: "cov" },
            geometry: { type: "Polygon", coordinates: [ring] },
          });
        }
      }
      if (!tacticalPointAllowed(v, notes)) {
        continue;
      }
      const kind: TacticalResourceKind = c.bucket === "resource" ? c.kind : parseTacticalResourceKind(notes);
      feats.push({
        type: "Feature",
        properties: { name: point.name, id: point.id, color: tacticalResourceColor(kind) },
        geometry: { type: "Point", coordinates: [point.lon, point.lat] },
      });
    }
    return { type: "FeatureCollection", features: feats };
  }, [tacticalVis, tacticalPointsOfInterest]);

  const meshFc: FeatureCollection = useMemo(
    () => ({
      type: "FeatureCollection",
      features: meshAllowed(tacticalVis)
        ? tacticalMeshNodeLocations.map((node) => ({
            type: "Feature",
            properties: { id: node.node_id, label: node.label },
            geometry: { type: "Point", coordinates: [node.lon, node.lat] } as const,
          }))
        : [],
    }),
    [tacticalVis, tacticalMeshNodeLocations],
  );

  const poiFc: FeatureCollection = useMemo(() => {
    if (!libraryPoisAllowed(tacticalVis, layerFilters.pois)) {
      return { type: "FeatureCollection", features: [] };
    }
    return {
      type: "FeatureCollection",
      features: pois.map((p) => ({
        type: "Feature",
        properties: { name: p.name, id: p.id },
        geometry: { type: "Point", coordinates: [p.lon, p.lat] },
      })),
    };
  }, [layerFilters.pois, pois, tacticalVis]);

  const peerFc: FeatureCollection = useMemo(
    () => ({
      type: "FeatureCollection",
      features: peersAllowed(tacticalVis)
        ? activePeers.map((peer) => ({
            type: "Feature",
            properties: { id: peer.node_id },
            geometry: { type: "Point", coordinates: [peer.longitude, peer.latitude] },
          }))
        : [],
    }),
    [activePeers, tacticalVis],
  );

  const drawingFc: FeatureCollection = useMemo(() => {
    if (!drawingsAllowed(tacticalVis)) {
      return { type: "FeatureCollection", features: [] };
    }
    const feats: GeoJSON.Feature[] = [];
    for (const d of mapDrawings) {
      let g: Geometry;
      try {
        g = JSON.parse(d.geometry_json) as Geometry;
      } catch {
        continue;
      }
      const fkind = d.feature_type;
      feats.push({ type: "Feature", properties: { id: d.id, fkind }, geometry: g });
    }
    return { type: "FeatureCollection", features: feats };
  }, [mapDrawings, tacticalVis]);

  const blockageFc: FeatureCollection = useMemo(() => {
    if (!blockagesAllowed(tacticalVis)) {
      return { type: "FeatureCollection", features: [] };
    }
    const feats: GeoJSON.Feature[] = [];
    for (const row of blockages) {
      let g: Geometry;
      try {
        g = JSON.parse(row.geometryJson) as Geometry;
      } catch {
        continue;
      }
      feats.push({
        type: "Feature",
        properties: { id: row.id, severity: row.severity ?? "" },
        geometry: g,
      });
    }
    return { type: "FeatureCollection", features: feats };
  }, [blockages, tacticalVis]);

  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_MARKERS)) return;
    (m.getSource(SRC_MARKERS) as maplibregl.GeoJSONSource).setData(markerFc);
  }, [markerFc]);
  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_TACTICAL)) return;
    (m.getSource(SRC_TACTICAL) as maplibregl.GeoJSONSource).setData(tacticalFc);
  }, [tacticalFc]);
  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_MESH)) return;
    (m.getSource(SRC_MESH) as maplibregl.GeoJSONSource).setData(meshFc);
  }, [meshFc]);
  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_POIS)) return;
    (m.getSource(SRC_POIS) as maplibregl.GeoJSONSource).setData(poiFc);
  }, [poiFc]);
  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_PEERS)) return;
    (m.getSource(SRC_PEERS) as maplibregl.GeoJSONSource).setData(peerFc);
  }, [peerFc]);
  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_DRAWINGS)) return;
    (m.getSource(SRC_DRAWINGS) as maplibregl.GeoJSONSource).setData(drawingFc);
  }, [drawingFc]);
  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_BLOCKAGES)) return;
    (m.getSource(SRC_BLOCKAGES) as maplibregl.GeoJSONSource).setData(blockageFc);
  }, [blockageFc]);

  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_TERRITORIES)) return;
    (m.getSource(SRC_TERRITORIES) as maplibregl.GeoJSONSource).setData(territoryFc);
  }, [territoryFc]);

  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getSource(SRC_TERRITORY_DRAFT)) return;
    (m.getSource(SRC_TERRITORY_DRAFT) as maplibregl.GeoJSONSource).setData(territoryDraftFc);
  }, [territoryDraftFc]);

  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getLayer(LAYER_TERR_FILL)) return;
    const v = territoriesVisible ? "visible" : "none";
    try {
      m.setLayoutProperty(LAYER_TERR_FILL, "visibility", v);
      m.setLayoutProperty(LAYER_TERR_LINE, "visibility", v);
    } catch {
      /* empty */
    }
  }, [territoriesVisible]);

  useEffect(() => {
    const m = mapRef.current;
    if (!m || !m.getLayer(LAYER_TERR_DRAFT_LINE)) return;
    const v = isDrawingTerritory ? "visible" : "none";
    try {
      m.setLayoutProperty(LAYER_TERR_DRAFT_FILL, "visibility", v);
      m.setLayoutProperty(LAYER_TERR_DRAFT_LINE, "visibility", v);
    } catch {
      /* empty */
    }
  }, [isDrawingTerritory]);

  useEffect(() => {
    void loadTerritories();
  }, [loadTerritories]);

  useEffect(() => {
    let cancelled = false;
    const u: (() => void)[] = [];
    void (async () => {
      u.push(
        await listen("peer-marker-rx", (e) => {
          const payload = e.payload as MapMarker;
          setMapMarkers((prev) => {
            const o = prev.filter((x) => x.id !== payload.id);
            return [...o, payload];
          });
        }),
      );
      if (cancelled) return;
      u.push(
        await listen("peer-saved-place-rx", () => {
          void loadSavedPlaces();
        }),
      );
      u.push(
        await listen("peer-blockage-rx", () => {
          void loadBlockages();
        }),
      );
      u.push(
        await listen("athena:territories-updated", () => {
          void loadTerritories();
        }),
      );
      u.push(
        await listen<{ lat: number; lng: number; zoom?: number }>("athena:map-focus", (e) => {
          const p = e.payload;
          if (!p || typeof p.lat !== "number" || typeof p.lng !== "number") {
            return;
          }
          const m = mapRef.current;
          if (!m) return;
          m.flyTo({
            center: [p.lng, p.lat],
            zoom: typeof p.zoom === "number" ? p.zoom : m.getZoom(),
            essential: true,
          });
        }),
      );
      u.push(
        await listen<{
          node_id: string;
          latitude: number;
          longitude: number;
          timestamp?: string;
        }>("peer-position-rx", (e) => {
          const p = e.payload;
          if (
            !p ||
            typeof p.node_id !== "string" ||
            typeof p.latitude !== "number" ||
            typeof p.longitude !== "number"
          ) {
            return;
          }
          const next = {
            node_id: p.node_id,
            latitude: p.latitude,
            longitude: p.longitude,
            last_seen_at: typeof p.timestamp === "string" ? p.timestamp : new Date().toISOString(),
          };
          setActivePeers((prev) => {
            const i = prev.findIndex((x) => x.node_id === next.node_id);
            if (i === -1) return [next, ...prev];
            const u2 = [...prev];
            u2[i] = next;
            return u2;
          });
        }),
      );
    })();
    return () => {
      cancelled = true;
      u.forEach((x) => x());
    };
  }, [loadBlockages, loadSavedPlaces, loadTerritories]);

  useEffect(() => {
    if (!cursorLatLng) {
      setCoordHud(null);
      return;
    }
    void invoke<CoordinateConversionResult>("convert_coordinates", {
      lat: cursorLatLng.lat,
      lng: cursorLatLng.lng,
    })
      .then((c) => setCoordHud(c))
      .catch(() => setCoordHud(null));
  }, [cursorLatLng]);

  const handleSaveMapMarkerFromModal = useCallback(
    async (payload: { title: string; description: string; category: MapMarkerCategory }) => {
      if (pendingMapMarkerPlacement === null) return;
      const m = await invoke<MapMarker>("add_map_marker", {
        lat: pendingMapMarkerPlacement.lat,
        lng: pendingMapMarkerPlacement.lng,
        title: payload.title,
        description: payload.description,
        category: payload.category,
      });
      setMapMarkers((p) => [...p.filter((x) => x.id !== m.id), m]);
    },
    [pendingMapMarkerPlacement],
  );

  const handleStartSatelliteDownloadFromSelection = useCallback(() => {
    if (satelliteSelectionBounds === null || satelliteDownloadBusy) return;
    const a = satelliteSelectionBounds[0];
    const b = satelliteSelectionBounds[1];
    const minLon = Math.min(a[1], b[1]);
    const maxLon = Math.max(a[1], b[1]);
    const minLat = Math.min(a[0], b[0]);
    const maxLat = Math.max(a[0], b[0]);
    setSatelliteDownloadBusy(true);
    void startSatelliteRegionDownload({
      minLon,
      minLat,
      maxLon,
      maxLat,
      minZoom: 10,
      maxZoom: 15,
      locationLabel: savedUserLocationAddress.trim() || "map selection",
    })
      .then((jobId) => {
        activeSatelliteJobIdRef.current = jobId;
        setSatelliteDownloadJobId(jobId);
        setSatelliteDownloadStatus("Satellite download started.");
      })
      .catch((err) => {
        setSatelliteDownloadBusy(false);
        setSatelliteDownloadStatus(String(err));
      });
  }, [satelliteSelectionBounds, satelliteDownloadBusy, savedUserLocationAddress]);

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    let lastUiUpdateAt = 0;
    void (async () => {
      unlisten.push(
        await listen<SatelliteRegionDownloadProgressEventPayload>("satellite_region_download_progress", (event) => {
          if (event.payload.job_id !== activeSatelliteJobIdRef.current) return;
          const now = Date.now();
          if (now - lastUiUpdateAt < 200) return;
          lastUiUpdateAt = now;
          const total = Math.max(1, event.payload.total_tiles);
          const pct = Math.min(100, (event.payload.downloaded_tiles / total) * 100);
          setSatelliteDownloadStatus(
            `${event.payload.downloaded_tiles.toLocaleString()} / ${event.payload.total_tiles.toLocaleString()} tiles (${pct.toFixed(1)}%)`,
          );
        }),
      );
      if (disposed) {
        for (const stop of unlisten) stop();
        return;
      }
      unlisten.push(
        await listen<SatelliteRegionDownloadCompleteEventPayload>("satellite_region_download_complete", (event) => {
          if (event.payload.job_id !== activeSatelliteJobIdRef.current) return;
          activeSatelliteJobIdRef.current = null;
          setSatelliteDownloadBusy(false);
          setSatelliteDownloadJobId(null);
          setSatelliteDownloadStatus("Satellite download complete.");
          onSatelliteRegionDownloaded?.(event.payload.map_id);
        }),
      );
      if (disposed) {
        for (const stop of unlisten) stop();
        return;
      }
      unlisten.push(
        await listen<SatelliteRegionDownloadErrorEventPayload>("satellite_region_download_error", (event) => {
          if (event.payload.job_id !== activeSatelliteJobIdRef.current) return;
          activeSatelliteJobIdRef.current = null;
          setSatelliteDownloadBusy(false);
          setSatelliteDownloadJobId(null);
          setSatelliteDownloadStatus(`Satellite download failed: ${event.payload.message}`);
        }),
      );
    })();
    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, [onSatelliteRegionDownloaded]);

  return (
    <div className={`map-view-wrap${embedded ? " map-view-wrap--embedded" : ""}`}>
      <div
        className="map-view-container"
        data-map-contextmenu-lat={rightClickPinLatLng != null ? String(rightClickPinLatLng.lat) : undefined}
        data-map-contextmenu-lng={rightClickPinLatLng != null ? String(rightClickPinLatLng.lng) : undefined}
        data-saved-places-count={savedPlaces.length}
        data-blockages-count={blockages.length}
        style={{ position: "relative", minHeight: embedded ? "100%" : "100vh" }}
      >
        <div
          className="map-view-map"
          ref={hostRef}
          style={{ position: "absolute", inset: 0, zIndex: 0 }}
        />
        {mapContextMenu ? (
          <div
            className="map-context-menu glass-panel"
            style={{
              position: "fixed",
              left: Math.min(window.innerWidth - 240, Math.max(12, mapContextMenu.x)),
              top: Math.min(window.innerHeight - 220, Math.max(12, mapContextMenu.y)),
              zIndex: 4000,
              padding: 10,
              borderRadius: 16,
              minWidth: 210,
              background: "color-mix(in srgb, var(--glass-bg-soft) 88%, rgba(0, 0, 0, 0.12))",
              border: "1px solid var(--glass-border-subtle)",
              boxShadow: "0 26px 80px -32px rgba(0,0,0,0.7)",
              backdropFilter: "blur(18px) saturate(175%)",
              WebkitBackdropFilter: "blur(18px) saturate(175%)",
            }}
            role="menu"
            aria-label="Map actions"
            onClick={(e) => e.stopPropagation()}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
            }}
          >
            <button
              type="button"
              className="map-context-menu__item"
              role="menuitem"
              onClick={() => {
                setPendingMapMarkerPlacement({ lat: mapContextMenu.lat, lng: mapContextMenu.lng });
                setMapContextMenu(null);
              }}
            >
              Add tactical marker
            </button>
            <button
              type="button"
              className="map-context-menu__item"
              role="menuitem"
              onClick={() => {
                setRightClickPinLatLng({ lat: mapContextMenu.lat, lng: mapContextMenu.lng });
                setMapContextMenu(null);
              }}
            >
              Bookmark location
            </button>
            <button
              type="button"
              className="map-context-menu__item"
              role="menuitem"
              onClick={() => {
                setShowDirections(true);
                setRouteHereLatLng({ lat: mapContextMenu.lat, lng: mapContextMenu.lng });
                setRouteHereSeq((s) => s + 1);
                setMapContextMenu(null);
              }}
            >
              Route here
            </button>
            <button
              type="button"
              className="map-context-menu__item"
              role="menuitem"
              onClick={() => {
                const text = `${mapContextMenu.lat.toFixed(6)}, ${mapContextMenu.lng.toFixed(6)}`;
                void (async () => {
                  try {
                    if (navigator?.clipboard?.writeText) {
                      await navigator.clipboard.writeText(text);
                    } else {
                      const ta = document.createElement("textarea");
                      ta.value = text;
                      ta.style.position = "fixed";
                      ta.style.left = "-9999px";
                      document.body.appendChild(ta);
                      ta.focus();
                      ta.select();
                      document.execCommand("copy");
                      document.body.removeChild(ta);
                    }
                    setToast({ tone: "success", message: "Coordinates copied." });
                  } catch {
                    setToast({ tone: "warning", message: "Could not copy coordinates." });
                  }
                })();
                setMapContextMenu(null);
              }}
            >
              Copy coordinates
            </button>
          </div>
        ) : null}
        <div
          className="map-view-locate-controls glass-panel"
          style={{
            position: "absolute",
            left: "0.35rem",
            top: "0.35rem",
            zIndex: 1003,
            pointerEvents: "auto",
            display: "flex",
            gap: 8,
            padding: 6,
            borderRadius: 16,
          }}
          role="group"
          aria-label="Map position controls"
        >
          <button type="button" className="map-locate-btn" onClick={fitWorld} aria-label="World overview" title="World overview">
            <Globe size={18} />
            <span>World</span>
          </button>
          <button
            type="button"
            className="map-locate-btn"
            onClick={() => void flyToSavedOrNativeAndMark()}
            aria-label="My location"
            title="My location"
          >
            <Crosshair size={18} />
            <span>My location</span>
          </button>
        </div>
        <div
          className="map-view-left-stack"
          style={{
            position: "absolute",
            left: "0.65rem",
            top: "calc(0.35rem + 56px)",
            bottom: "var(--bottom-bar-offset, var(--bottom-safe-area))",
            zIndex: 1001,
            pointerEvents: "auto",
            overflow: "visible",
          }}
        >
          <MapLeftRail
            layersOpen={layersOpen}
            onLayersOpenChange={(v) => {
              setLayersOpen(v);
              if (v) {
                setOverlaysOpen(false);
                setSavedOpen(false);
                setTerritoriesOpen(false);
                setShowDirections(false);
                setShowRoute(false);
              }
            }}
            overlaysOpen={overlaysOpen}
            onOverlaysOpenChange={(v) => {
              setOverlaysOpen(v);
              if (v) {
                setLayersOpen(false);
                setSavedOpen(false);
                setTerritoriesOpen(false);
                setShowDirections(false);
                setShowRoute(false);
              }
            }}
            savedOpen={savedOpen}
            onSavedOpenChange={(v) => {
              setSavedOpen(v);
              if (v) {
                setLayersOpen(false);
                setOverlaysOpen(false);
                setTerritoriesOpen(false);
                setShowDirections(false);
                setShowRoute(false);
              }
            }}
            territoriesOpen={territoriesOpen}
            onTerritoriesOpenChange={(v) => {
              setTerritoriesOpen(v);
              if (v) {
                setLayersOpen(false);
                setOverlaysOpen(false);
                setSavedOpen(false);
                setShowDirections(false);
                setShowRoute(false);
                setIsDrawingTerritory(false);
                setTerritoryDrawPoints([]);
              }
            }}
            directionsOpen={showDirections}
            onToggleRoute={() => {
              setShowDirections((v) => {
                const next = !v;
                if (next) {
                  setLayersOpen(false);
                  setOverlaysOpen(false);
                  setSavedOpen(false);
                  setTerritoriesOpen(false);
                  setShowRoute(false);
                }
                return next;
              });
            }}
            hasSatelliteRaster={hasSatelliteHeuristic}
            showApprovalInbox={authRankLevel >= 4}
            aiPendingCount={aiPendingCount}
            onOpenApprovalInbox={() => setApprovalInboxOpen(true)}
          />
          <MapLayersPanel
            open={layersOpen}
            onClose={() => setLayersOpen(false)}
            openFilesSeq={mapFilesOpenSeq}
            enabledMainMapRasterArchiveFileNames={enabledMainMapRasterArchiveFileNames}
            onMainMapArchiveRasterLayersChange={onMainMapArchiveRasterLayersChange}
            onOpenDataAcquisition={onOpenDataAcquisition}
          />
          <SavedPlacesPanel
            open={savedOpen}
            onClose={() => setSavedOpen(false)}
            map={mlMap}
            onRouteToLatLng={(latLng) => {
              setShowDirections(true);
              setRouteHereLatLng(latLng);
              setRouteHereSeq((s) => s + 1);
            }}
          />
          <MapOverlaysPanel
            panelOpen={overlaysOpen}
            onClose={() => setOverlaysOpen(false)}
            tacticalLayerVisibility={tacticalVis}
            onTacticalLayerVisibilityChange={onTacticalLayerVisibilityChange}
            layerFilters={layerFilters}
            setLayerFilters={setLayerFilters}
            onOpenBlockagesManager={() => {
              setShowBlockages(true);
              setOverlaysOpen(false);
            }}
            onTacticalDataImported={onReloadTacticalMapLayers}
          />
          <TerritoriesPanel
            open={territoriesOpen}
            onClose={() => setTerritoriesOpen(false)}
            rows={territoryRows}
            onRefresh={() => void loadTerritories()}
            territoriesVisible={territoriesVisible}
            onTerritoriesVisibleChange={setTerritoriesVisible}
            onStartDraw={() => {
              setIsDrawingTerritory(true);
              setTerritoryDrawPoints([]);
              setTerritorySaveOpen(false);
            }}
            onFlyTo={flyToTerritoryById}
          />
          <TerritorySaveModal
            open={territorySaveOpen}
            onClose={() => setTerritorySaveOpen(false)}
            geometryJson={territoryGeometryJsonForSave}
            onSaved={() => {
              void loadTerritories();
              setIsDrawingTerritory(false);
              setTerritoryDrawPoints([]);
            }}
          />
        </div>
        {isDrawingTerritory ? (
          <div
            className="map-territory-draw-hud glass-panel"
            style={{
              position: "fixed",
              left: "50%",
              bottom: "calc(var(--bottom-bar-offset, var(--bottom-safe-area, 80px)) + 12px)",
              transform: "translateX(-50%)",
              zIndex: 1700,
              display: "flex",
              flexWrap: "wrap",
              alignItems: "center",
              gap: 10,
              padding: "10px 14px",
              borderRadius: 16,
              pointerEvents: "auto",
              maxWidth: "min(560px, calc(100vw - 24px))",
            }}
            role="toolbar"
            aria-label="Draw territory"
          >
            <span className="small" style={{ opacity: 0.9 }}>
              Click to add corners · double-click to finish, or use Finish
            </span>
            <button
              type="button"
              className="secondary small-btn"
              onClick={() => setTerritoryDrawPoints((p) => p.slice(0, -1))}
              disabled={territoryDrawPoints.length === 0}
            >
              Undo
            </button>
            <button
              type="button"
              className="secondary small-btn"
              onClick={() => {
                setIsDrawingTerritory(false);
                setTerritoryDrawPoints([]);
                setTerritorySaveOpen(false);
              }}
            >
              Cancel
            </button>
            <button
              type="button"
              className="primary small-btn"
              onClick={() => {
                if (territoryDrawPoints.length >= 3) setTerritorySaveOpen(true);
              }}
              disabled={territoryDrawPoints.length < 3}
            >
              Finish
            </button>
          </div>
        ) : null}
        <div className="map-view-floating-stack" style={{ zIndex: 2, pointerEvents: "none" }}>
          <div style={{ pointerEvents: "auto", display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 8 }}>
            <MapCoordinateReadout
              map={mlMap}
              cursorLatLng={cursorLatLng}
              hudShowDd={hudShowDd}
              onToggleHud={setHudShowDd}
              coordHud={coordHud}
            />
          </div>
        </div>

        <div
          className={`map-view-chat-zone${mapAssistantReply ? " map-view-chat-zone-with-response" : ""}${
            embedded ? " map-view-chat-zone--embedded" : ""
          }`}
          ref={chatZoneRef}
        >
          {mapAssistantReply || isProcessing ? (
            <div className={`map-view-response-box glass-panel${mapAssistantCollapsed ? " is-collapsed" : ""}`}>
              <div className="map-view-response-head">
                <button
                  type="button"
                  className="secondary small-btn map-view-response-toggle"
                  onClick={() => setMapAssistantCollapsed((v) => !v)}
                  aria-label={mapAssistantCollapsed ? "Show response" : "Hide response"}
                  title={mapAssistantCollapsed ? "Show response" : "Hide response"}
                >
                  {mapAssistantCollapsed ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                </button>
                <div className="map-view-response-meta text-muted small">
                  {isProcessing ? "ATHENA is processing…" : mapAssistantToolResults.length > 0 ? `${mapAssistantToolResults.length} tool(s)` : "AI response"}
                </div>
              </div>
              {!mapAssistantCollapsed ? (
                <>
                  {isProcessing ? (
                    <div className="map-assistant-processing" aria-label="ATHENA is processing">
                      <span className="map-assistant-dot" />
                      <span className="map-assistant-dot" />
                      <span className="map-assistant-dot" />
                    </div>
                  ) : null}
                  {mapAssistantToolResults.length > 0 ? (
                    <div className="map-smart-chips">
                      {mapAssistantToolResults.map((t, i) => (
                        <span
                          key={`${t.tool}-${i}`}
                          className={"map-smart-chip" + (t.ok ? " map-smart-chip--ok" : " map-smart-chip--err")}
                          title={t.error ?? ""}
                        >
                          {formatMapToolChipText(t)}
                        </span>
                      ))}
                    </div>
                  ) : null}
                  {!isProcessing ? <p className="map-view-response-text">{mapAssistantReply}</p> : null}
                  {!isProcessing
                    ? mapAssistantToolResults.map((t, i) => {
                        const proposal = markerProposalOf(t);
                        if (!proposal) return null;
                        return (
                          <div key={`proposal-${i}`} style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
                            <button
                              type="button"
                              className="glass-action-btn"
                              onClick={() => {
                                const c = mlMap?.getCenter();
                                const lat = proposal.lat ?? c?.lat;
                                const lng = proposal.lng ?? c?.lng;
                                if (lat == null || lng == null) return;
                                void invoke<MapMarker>("add_map_marker", {
                                  lat,
                                  lng,
                                  title: proposal.title,
                                  description: proposal.description,
                                  category: proposal.category,
                                })
                                  .then((m) => {
                                    setMapMarkers((p) => [...p.filter((x) => x.id !== m.id), m]);
                                    setMapAssistantToolResults([]);
                                    setMapAssistantReply(`Added the marker "${m.title}".`);
                                  })
                                  .catch((e) => setMapAssistantReply(`Could not add the marker: ${String(e)}`));
                              }}
                            >
                              Add marker “{proposal.title}”
                            </button>
                            <button
                              type="button"
                              className="secondary"
                              onClick={() => {
                                setMapAssistantToolResults([]);
                                setMapAssistantReply("Marker not added.");
                              }}
                            >
                              Cancel
                            </button>
                          </div>
                        );
                      })
                    : null}
                </>
              ) : null}
            </div>
          ) : null}

          <div className="map-view-chat-bar">
            <MapSmartBar
              onSend={async (msg) => {
                setIsProcessing(true);
                try {
                  const m = mlMap;
                  const response = await mapAssistantSendOnInvoke(
                    msg,
                    m
                      ? { center: [m.getCenter().lng, m.getCenter().lat], zoom: m.getZoom() }
                      : null,
                  );
                  return response;
                } catch (e) {
                  console.error("Map assistant failed:", e);
                  return { reply: String(e), toolResults: [] };
                } finally {
                  setIsProcessing(false);
                }
              }}
              onResult={(r) => {
                try {
                  const reply = String((r as any)?.reply ?? "").trim();
                  const tools = Array.isArray((r as any)?.toolResults) ? ((r as any).toolResults as MapToolResult[]) : [];

                  applyMapAssistantToolEffects(tools);

                  // If we only executed tools and have nothing to say, keep the map clear.
                  if (!reply && tools.length > 0) {
                    const dTool = tools.find((t) => t?.tool === "directions" && t.ok) as
                      | { data?: { ambiguous?: boolean; route?: { source?: string } } }
                      | undefined;
                    if (dTool?.data?.ambiguous) {
                      setMapAssistantReply("Several places matched your text. Please narrow the from/to names, pick candidates in Directions, or use saved markers.");
                      setMapAssistantToolResults(tools);
                      setMapAssistantCollapsed(false);
                      return;
                    }
                    if (dTool?.data?.route?.source === "straight_line_fallback") {
                      setMapAssistantReply(
                        "Valhalla routing is offline, so this is a straight-line path (not turn-by-turn roads). Open Directions → routing setup (gear): build a routing region, start the Valhalla sidecar (Docker) on port 8002.",
                      );
                      setMapAssistantToolResults(tools);
                      setMapAssistantCollapsed(false);
                      return;
                    }
                    const routeTool = tools.find((t) => t?.tool === "route" && t.ok);
                    const source = (routeTool as { data?: { source?: string } } | undefined)?.data?.source;
                    if (source && String(source) === "straight_line_fallback") {
                      setMapAssistantReply(
                        "Routing engine is offline right now, so ATHENA drew a straight-line fallback. Enable a routing region / start Valhalla for real roads & paths.",
                      );
                      setMapAssistantToolResults(tools);
                      setMapAssistantCollapsed(false);
                      return;
                    }
                    setMapAssistantReply(summarizeMapToolResults(tools));
                    setMapAssistantToolResults(tools);
                    setMapAssistantCollapsed(false);
                    return;
                  }
                  if (!reply) {
                    setMapAssistantReply(
                      "The map assistant had nothing to do for that. It can find saved markers and places, give directions and routes, mark territory status, and add a marker (say: add a marker here called North well).",
                    );
                    setMapAssistantToolResults([]);
                    setMapAssistantCollapsed(false);
                    return;
                  }

                  setMapAssistantReply(reply);
                  setMapAssistantToolResults(tools);
                  setMapAssistantCollapsed(false);
                } catch (err) {
                  // Never throw from a UI callback; render an explicit error instead.
                  setMapAssistantReply(`System Error: ${String(err)}`);
                  setMapAssistantToolResults([]);
                  setMapAssistantCollapsed(false);
                }
              }}
            />
          </div>
        </div>
        {showDirections ? (
          <DirectionsPanel
            map={mlMap}
            onClose={() => setShowDirections(false)}
            onOpenAdvancedRoute={() => {
              setShowDirections(false);
              setShowRoute(true);
            }}
            savedUserLocationAddress={savedUserLocationAddress}
            routeToLatLng={routeHereLatLng}
            routeRequestSeq={routeHereSeq}
            onOpenDataAcquisition={onOpenDataAcquisition}
            openRoutingSetupSeq={routingSetupOpenSeq}
          />
        ) : null}
        {tappedPlace ? (
          <PlaceCard
            place={tappedPlace}
            onClose={() => setTappedPlace(null)}
            onDirections={(p) => {
              setTappedPlace(null);
              setShowDirections(true);
              setRouteHereLatLng(p);
              setRouteHereSeq((s) => s + 1);
            }}
            onAddMarker={(p) => {
              setTappedPlace(null);
              setPendingMapMarkerPlacement(p);
            }}
          />
        ) : null}
        {showRoute ? <RoutePanel map={mlMap} onClose={() => setShowRoute(false)} /> : null}
        {showBlockages ? (
          <BlockagesPanel
            onClose={() => setShowBlockages(false)}
            onBlockagesChanged={() => {
              void loadBlockages();
            }}
          />
        ) : null}
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
        <SoftToast
          open={toast != null}
          tone={toast?.tone ?? "success"}
          message={toast?.message ?? ""}
          onClose={() => setToast(null)}
        />
        <ApprovalInbox
          open={approvalInboxOpen}
          onClose={() => setApprovalInboxOpen(false)}
          onChanged={() => {
            void invoke<number>("ai_count_pending")
              .then((n) => setAiPendingCount(typeof n === "number" ? n : 0))
              .catch(() => {});
          }}
        />
        <MapSettingsPopover
          openRequestSeq={mapSettingsOpenSeq}
          basemap={basemap}
          onBasemapChange={onBasemapChange}
          canOfflineOsm={canOfflineOsm}
          canOfflineTopo={canOfflineTopo}
          canOfflineImported={canOfflineImported}
          canOfflineArchiveRaster={canOfflineArchiveRaster}
          showSatelliteUi={hasSatelliteHeuristic}
          layerFilters={layerFilters}
          setLayerFilters={setLayerFilters}
          satelliteCoverageOnMap={satelliteCoverageOnMap}
          onSatelliteCoverageOnMapChange={setSatelliteCoverageOnMap}
          satelliteDrawModeEnabled={satelliteDrawModeEnabled}
          onSatelliteDrawModeToggle={() => setSatelliteDrawModeEnabled((p) => !p)}
          satelliteSelectionBounds={satelliteSelectionBounds}
          satelliteDownloadBusy={satelliteDownloadBusy}
          satelliteDownloadStatus={satelliteDownloadStatus}
          satelliteDownloadJobId={satelliteDownloadJobId}
          onStartSatelliteDownloadFromSelection={handleStartSatelliteDownloadFromSelection}
          onCancelSatelliteBoxSelection={() => {
            setSatelliteDrawModeEnabled(false);
            setSatelliteSelectionBounds(null);
            setSatellitePreviewBounds(null);
            satPreviewStart.current = null;
            setSatelliteDownloadStatus("");
          }}
          onOpenDataAcquisition={onOpenDataAcquisition}
        />
      </div>
      {!embedded && (
        <header className="map-view-header">
          {onClose !== undefined ? (
            <button type="button" className="map-view-back" onClick={onClose} aria-label="Back to chat">
              ← Back
            </button>
          ) : null}
          <h2 className="map-view-title">Map</h2>
        </header>
      )}
    </div>
  );
}
