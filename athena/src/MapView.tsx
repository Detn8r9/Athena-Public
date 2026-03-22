import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { MapContainer, TileLayer, Marker, Popup, Rectangle, Circle, Polyline, useMap, useMapEvents } from "react-leaflet";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import { buildMbTilesRasterTilesTemplate } from "./offlineMapArchiveUrls";
import {
  TacticalSidebar,
  defaultTacticalLayerVisibilityState,
  type TacticalLayerVisibilityState,
} from "./TacticalSidebar";
import {
  startSatelliteRegionDownload,
  type SatelliteRegionDownloadCompleteEventPayload,
  type SatelliteRegionDownloadErrorEventPayload,
  type SatelliteRegionDownloadProgressEventPayload,
} from "./offlineMapManagerCatalog";
import { MapSettingsPopover } from "./MapSettingsPopover";
import type { MapBasemap } from "./mapViewTypes";

export type { MapBasemap };

export interface Poi {
  id: string;
  name: string;
  lat: number;
  lon: number;
  notes: string;
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

function parseTacticalResourceKind(notes: string): TacticalResourceKind {
  const match = notes.trim().match(/^@(power|water|food|equipment)\b/i);
  if (match) {
    const token = match[1].toLowerCase();
    if (token === "power" || token === "water" || token === "food" || token === "equipment") {
      return token;
    }
  }
  return "equipment";
}

function stripResourceDirectiveFromNotes(notes: string): string {
  return notes.replace(/^\s*@(power|water|food|equipment)\s*/i, "").trim();
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

const meshNodeMarkerIcon = L.divIcon({
  className: "tactical-mesh-node-marker",
  html:
    '<div style="width:16px;height:16px;border-radius:50%;background:#38bdf8;border:2px solid #0f172a;box-shadow:0 1px 3px rgba(0,0,0,0.35);"></div>',
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

interface MapViewProps {
  pois: Poi[];
  onClose: () => void;
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
}

function AddMarkerOnClick({ onAdd }: { onAdd: (lat: number, lon: number) => void }) {
  const [pending, setPending] = useState<{ lat: number; lon: number } | null>(null);
  useMapEvents({
    click(e) {
      setPending({ lat: e.latlng.lat, lon: e.latlng.lng });
    },
  });
  useEffect(() => {
    if (!pending) return;
    onAdd(pending.lat, pending.lon);
    setPending(null);
  }, [pending, onAdd]);
  return null;
}

export function MapView({
  pois,
  onClose,
  onAddPoi,
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
}: MapViewProps) {
  const [tacticalLayerVisibility, setTacticalLayerVisibility] = useState<TacticalLayerVisibilityState>(
    defaultTacticalLayerVisibilityState,
  );

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

  const handleAddAt = useCallback(
    (lat: number, lon: number) => {
      const name = window.prompt("Name this location (e.g. Homebase):")?.trim();
      const notes = (window.prompt("Notes (optional):") ?? "").trim();
      if (name) void onAddPoi(name, lat, lon, notes);
    },
    [onAddPoi],
  );

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
    <div className="map-view-wrap">
      <div className="map-view-container">
        <MapContainer
          key={`${mapCenter[0]}-${mapCenter[1]}-${mapZoom}`}
          center={mapCenter}
          zoom={mapZoom}
          zoomAnimation={true}
          fadeAnimation={true}
          className="map-view-map"
          style={{ height: "100%", width: "100%" }}
        >
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
          {tacticalLayerVisibility.network.coverageCircles &&
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
          {tacticalLayerVisibility.network.links && meshNodeLinkPositions.length >= 2 && (
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
          {tacticalLayerVisibility.network.nodes &&
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
              const kind = parseTacticalResourceKind(point.notes);
              return tacticalLayerVisibility.resources[kind];
            })
            .map((point) => {
              const kind = parseTacticalResourceKind(point.notes);
              const notesBody = stripResourceDirectiveFromNotes(point.notes);
              return (
                <Marker
                  key={`tactical-resource-${point.id}`}
                  position={[point.lat, point.lon]}
                  icon={tacticalResourceIcons[kind]}
                >
                  <Popup>
                    <strong>{point.name}</strong>
                    {notesBody.length > 0 ? <p>{notesBody}</p> : null}
                  </Popup>
                </Marker>
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
          <AddMarkerOnClick onAdd={(lat, lon) => handleAddAt(lat, lon)} />
        </MapContainer>
        <TacticalSidebar
          onVisibilityStateChange={setTacticalLayerVisibility}
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

      <header className="map-view-header">
        <button type="button" className="map-view-back" onClick={onClose} aria-label="Back to chat">
          ← Back
        </button>
        <h2 className="map-view-title">Map</h2>
        {onOpenOfflineMapManager !== undefined && (
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
        )}
      </header>

      <div className={`map-view-chat-zone ${mapChatMessage ? "map-view-chat-zone-with-response" : ""}`}>
        {mapChatMessage && (
          <div className="map-view-response-box">
            <p className="map-view-response-text">{mapChatMessage}</p>
          </div>
        )}
        <div className="map-view-chat-bar">
          <input
            type="text"
            className="map-chat-input"
            placeholder="Ask about the map, e.g. “mark my location”…"
            value={mapChatInput}
            onChange={(e) => setMapChatInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), onMapChatSend())}
            disabled={mapChatSending}
            aria-label="Map chat input"
          />
          <button type="button" className="map-chat-send" onClick={onMapChatSend} disabled={mapChatSending || !mapChatInput.trim()} aria-label="Send">
            →
          </button>
        </div>
      </div>
    </div>
  );
}
