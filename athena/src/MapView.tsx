import { useCallback, useEffect, useState } from "react";
import { MapContainer, TileLayer, Marker, Popup, useMap, useMapEvents } from "react-leaflet";
import L from "leaflet";
import "leaflet/dist/leaflet.css";

export interface Poi {
  id: string;
  name: string;
  lat: number;
  lon: number;
  notes: string;
}

export type MapBasemap = "online_osm" | "offline_osm" | "offline_topo" | "offline_imported";

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
  mapCenter: [number, number];
  mapZoom: number;
  /** Opens the map archive manager (PMTiles / MBTiles catalog and downloads). */
  onOpenOfflineMapManager?: () => void;
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
  mapCenter,
  mapZoom,
  onOpenOfflineMapManager,
}: MapViewProps) {
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

  return (
    <div className="map-view-wrap">
      <div className="map-view-header">
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
      </div>

      <div className="map-view-basemap-bar" role="toolbar" aria-label="Basemap">
        <span className="map-basemap-label">Basemap</span>
        <select
          className="map-basemap-select"
          value={basemap}
          onChange={(e) => onBasemapChange(e.target.value as MapBasemap)}
          aria-label="Map basemap"
        >
          <option value="online_osm">OpenStreetMap (online)</option>
          <option value="offline_osm" disabled={!canOfflineOsm}>
            OpenStreetMap (offline cache){!canOfflineOsm ? " — download in Settings" : ""}
          </option>
          <option value="offline_topo" disabled={!canOfflineTopo}>
            Terrain / hillshade (OpenTopoMap offline){!canOfflineTopo ? " — download in Settings" : ""}
          </option>
          <option value="offline_imported" disabled={!canOfflineImported}>
            Imported raster (MBTiles / PMTiles){!canOfflineImported ? " — import in Settings" : ""}
          </option>
        </select>
      </div>

      <div className="map-view-filters">
        <label className="map-filter-item">
          <input
            type="checkbox"
            checked={layerFilters.pois}
            onChange={(e) => setLayerFilters((f) => ({ ...f, pois: e.target.checked }))}
          />
          Points of interest
        </label>
        <label className="map-filter-item">
          <input type="checkbox" checked={layerFilters.roads} onChange={(e) => setLayerFilters((f) => ({ ...f, roads: e.target.checked }))} />
          Road names (online OSM only)
        </label>
        <label className="map-filter-item">
          <input type="checkbox" checked={layerFilters.rivers} onChange={(e) => setLayerFilters((f) => ({ ...f, rivers: e.target.checked }))} />
          Rivers (online OSM only)
        </label>
      </div>

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

      <div className="map-view-container">
        <MapContainer
          key={`${mapCenter[0]}-${mapCenter[1]}-${mapZoom}`}
          center={mapCenter}
          zoom={mapZoom}
          className="map-view-map"
          style={{ height: "100%", width: "100%" }}
        >
          {basemap === "online_osm" && (
            <TileLayer
              attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
              url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
            />
          )}
          {basemap === "offline_osm" && offlineOsmRoot && (
            <OfflineRasterLayer
              rootPath={offlineOsmRoot}
              convertFileSrcFn={convertFileSrcFn}
              attribution='Offline tiles &copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
            />
          )}
          {basemap === "offline_topo" && offlineTopoRoot && (
            <OfflineRasterLayer
              rootPath={offlineTopoRoot}
              convertFileSrcFn={convertFileSrcFn}
              attribution='Map data: &copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors, SRTM | OpenTopoMap (offline cache)'
            />
          )}
          {basemap === "offline_imported" && offlineImportedRoot && (
            <OfflineRasterLayer
              rootPath={offlineImportedRoot}
              convertFileSrcFn={convertFileSrcFn}
              attribution="Offline raster from imported archive — respect the source license."
            />
          )}
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
      </div>
    </div>
  );
}
