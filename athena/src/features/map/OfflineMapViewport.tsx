import { useEffect, useRef, useState } from "react";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import { PMTiles, TileType } from "pmtiles";
import "maplibre-gl/dist/maplibre-gl.css";
import { buildMbTilesRasterTilesTemplate, buildPmtilesMapLibreUrl } from "./offlineMapArchiveUrls";

const SOURCE_ID_RASTER_BASE = "offline-map-manager-mbtiles-raster-base";
const LAYER_ID_RASTER_BASE = "offline-map-manager-mbtiles-raster-base-layer";

const SOURCE_ID_PM_OVERLAY = "offline-map-manager-pmtiles-overlay";
const LAYER_ID_PM_RASTER_OVERLAY = "offline-map-manager-pmtiles-raster-overlay-layer";

const VECTOR_LAYER_ID_PREFIX = "offline-archive-vector-layer-";

const LAYER_ID_BACKGROUND = "offline-background";
const LAYER_ID_GRID = "offline-preview-grid-lines";

const OFFLINE_PREVIEW_GRID_GEOJSON: GeoJSON.FeatureCollection = {
  type: "FeatureCollection",
  features: (() => {
    const meridianSpacingDegrees = 45;
    const parallelSpacingDegrees = 30;
    const latitudeClamp = 85.05112878;
    const features: GeoJSON.Feature[] = [];

    for (let longitudeDegrees = -180; longitudeDegrees <= 180; longitudeDegrees += meridianSpacingDegrees) {
      features.push({
        type: "Feature",
        properties: {},
        geometry: {
          type: "LineString",
          coordinates: [
            [longitudeDegrees, -latitudeClamp],
            [longitudeDegrees, latitudeClamp],
          ],
        },
      });
    }

    for (let latitudeDegrees = -60; latitudeDegrees <= 60; latitudeDegrees += parallelSpacingDegrees) {
      features.push({
        type: "Feature",
        properties: {},
        geometry: {
          type: "LineString",
          coordinates: [
            [-180, latitudeDegrees],
            [180, latitudeDegrees],
          ],
        },
      });
    }

    return features;
  })(),
};

export function buildOfflineBlankStyle(): maplibregl.StyleSpecification {
  return {
    version: 8,
    name: "offline-map-manager-blank",
    sources: {
      "offline-preview-grid": {
        type: "geojson",
        data: OFFLINE_PREVIEW_GRID_GEOJSON,
      },
    },
    layers: [
      {
        id: LAYER_ID_BACKGROUND,
        type: "background",
        paint: {
          "background-color": "#1c212b",
        },
      },
      {
        id: LAYER_ID_GRID,
        type: "line",
        source: "offline-preview-grid",
        paint: {
          "line-color": "#3a4556",
          "line-width": 1,
          "line-opacity": 0.45,
        },
      },
    ],
  };
}

function pickColorForSourceLayer(sourceLayerIdentifier: string): string {
  let hash = 0;
  for (let index = 0; index < sourceLayerIdentifier.length; index += 1) {
    hash = (hash * 31 + sourceLayerIdentifier.charCodeAt(index)) >>> 0;
  }
  const hue = hash % 360;
  return `hsl(${hue} 46% 54%)`;
}

interface VectorLayerMetadataEntry {
  id: string;
}

function parseVectorLayerMetadata(metadata: unknown): VectorLayerMetadataEntry[] {
  if (metadata === null || metadata === undefined) {
    return [];
  }
  const record = metadata as { vector_layers?: unknown };
  const raw = record.vector_layers;
  if (typeof raw === "string") {
    try {
      const parsed: unknown = JSON.parse(raw);
      return Array.isArray(parsed) ? (parsed as VectorLayerMetadataEntry[]) : [];
    } catch {
      return [];
    }
  }
  if (Array.isArray(raw)) {
    return raw as VectorLayerMetadataEntry[];
  }
  return [];
}

function removeDynamicArchiveLayersAndSources(map: MapLibreMap): void {
  const style = map.getStyle();
  const layers = style.layers;
  if (layers === undefined) {
    return;
  }
  for (let index = layers.length - 1; index >= 0; index -= 1) {
    const layerId = layers[index].id;
    if (
      layerId === LAYER_ID_RASTER_BASE ||
      layerId === LAYER_ID_PM_RASTER_OVERLAY ||
      layerId.startsWith(VECTOR_LAYER_ID_PREFIX)
    ) {
      if (map.getLayer(layerId) !== undefined) {
        map.removeLayer(layerId);
      }
    }
  }
  if (map.getSource(SOURCE_ID_RASTER_BASE) !== undefined) {
    map.removeSource(SOURCE_ID_RASTER_BASE);
  }
  if (map.getSource(SOURCE_ID_PM_OVERLAY) !== undefined) {
    map.removeSource(SOURCE_ID_PM_OVERLAY);
  }
}

function setPreviewGridVisibility(map: MapLibreMap, archivesActive: boolean): void {
  if (map.getLayer(LAYER_ID_GRID) === undefined) {
    return;
  }
  map.setLayoutProperty(LAYER_ID_GRID, "visibility", archivesActive ? "none" : "visible");
}

function fitMapToHeaderBounds(
  map: MapLibreMap,
  header: { minLon: number; minLat: number; maxLon: number; maxLat: number },
): void {
  if (header.minLon >= header.maxLon || header.minLat >= header.maxLat) {
    return;
  }
  map.fitBounds(
    [
      [header.minLon, header.minLat],
      [header.maxLon, header.maxLat],
    ],
    { duration: 0, padding: 20, maxZoom: 15 },
  );
}

async function synchronizeOfflineMapArchives(
  map: MapLibreMap,
  baseRasterArchiveFileName: string | null,
  overlayArchiveFileName: string | null,
  isCancelled: () => boolean,
): Promise<void> {
  if (!map.isStyleLoaded()) {
    return;
  }

  removeDynamicArchiveLayersAndSources(map);

  const archivesActive = baseRasterArchiveFileName !== null || overlayArchiveFileName !== null;
  setPreviewGridVisibility(map, archivesActive);

  let boundsHeader: { minLon: number; minLat: number; maxLon: number; maxLat: number } | null = null;

  if (baseRasterArchiveFileName !== null) {
    if (isCancelled()) {
      return;
    }
    map.addSource(SOURCE_ID_RASTER_BASE, {
      type: "raster",
      tiles: [buildMbTilesRasterTilesTemplate(baseRasterArchiveFileName)],
      tileSize: 256,
      scheme: "xyz",
      minzoom: 0,
      maxzoom: 24,
    });
    map.addLayer({
      id: LAYER_ID_RASTER_BASE,
      type: "raster",
      source: SOURCE_ID_RASTER_BASE,
      paint: {
        "raster-opacity": 1,
        "raster-resampling": "linear",
      },
    });
  }

  if (overlayArchiveFileName !== null) {
    if (isCancelled()) {
      return;
    }
    const pmtilesUrl = buildPmtilesMapLibreUrl(overlayArchiveFileName);
    const coreUrl = pmtilesUrl.replace(/^pmtiles:\/\//, "");
    const pmtilesArchive = new PMTiles(coreUrl);
    const header = await pmtilesArchive.getHeader();
    if (isCancelled()) {
      return;
    }
    boundsHeader = {
      minLon: header.minLon,
      minLat: header.minLat,
      maxLon: header.maxLon,
      maxLat: header.maxLat,
    };

    const isVectorArchive =
      header.tileType === TileType.Mvt || header.tileType === TileType.Mlt;

    if (isVectorArchive) {
      map.addSource(SOURCE_ID_PM_OVERLAY, {
        type: "vector",
        url: pmtilesUrl,
      });

      const metadata = await pmtilesArchive.getMetadata();
      if (isCancelled()) {
        return;
      }
      let vectorLayers = parseVectorLayerMetadata(metadata);
      if (vectorLayers.length === 0) {
        vectorLayers = [
          { id: "water" },
          { id: "transportation" },
          { id: "building" },
          { id: "landuse" },
          { id: "landcover" },
        ];
      }

      for (const entry of vectorLayers) {
        const sourceLayerIdentifier = entry.id;
        if (typeof sourceLayerIdentifier !== "string" || sourceLayerIdentifier.length === 0) {
          continue;
        }
        const color = pickColorForSourceLayer(sourceLayerIdentifier);
        const fillLayerId = `${VECTOR_LAYER_ID_PREFIX}${sourceLayerIdentifier}-fill`;
        map.addLayer({
          id: fillLayerId,
          type: "fill",
          source: SOURCE_ID_PM_OVERLAY,
          "source-layer": sourceLayerIdentifier,
          filter: ["in", "$type", "Polygon", "MultiPolygon"],
          paint: {
            "fill-color": color,
            "fill-opacity": 0.38,
          },
        });
        map.addLayer({
          id: `${VECTOR_LAYER_ID_PREFIX}${sourceLayerIdentifier}-line`,
          type: "line",
          source: SOURCE_ID_PM_OVERLAY,
          "source-layer": sourceLayerIdentifier,
          filter: ["in", "$type", "LineString", "MultiLineString"],
          paint: {
            "line-color": color,
            "line-width": 1.25,
            "line-opacity": 0.9,
          },
        });
      }
    } else {
      map.addSource(SOURCE_ID_PM_OVERLAY, {
        type: "raster",
        url: pmtilesUrl,
        tileSize: 256,
      });
      map.addLayer({
        id: LAYER_ID_PM_RASTER_OVERLAY,
        type: "raster",
        source: SOURCE_ID_PM_OVERLAY,
        paint: {
          "raster-opacity": 1,
          "raster-resampling": "linear",
        },
      });
    }
  }

  if (boundsHeader !== null) {
    if (isCancelled()) {
      return;
    }
    fitMapToHeaderBounds(map, boundsHeader);
  }
}

/**
 * React hook that attaches offline raster and PMTiles archive sources to an existing MapLibre map.
 * Raster MBTiles render below PMTiles overlays. Vector PMTiles layers render above raster bases.
 */
function useOfflineMapArchiveLayers(
  map: MapLibreMap | null,
  baseRasterArchiveFileName: string | null,
  overlayArchiveFileName: string | null,
): void {
  useEffect(() => {
    if (map === null) {
      return;
    }
    let cancelled = false;
    const isCancelled = (): boolean => cancelled;

    const run = (): void => {
      void synchronizeOfflineMapArchives(map, baseRasterArchiveFileName, overlayArchiveFileName, isCancelled).catch(
        (error: unknown) => {
          console.error("Offline map archive synchronization failed:", error);
        },
      );
    };

    if (map.isStyleLoaded()) {
      run();
    } else {
      map.once("load", run);
    }

    return () => {
      cancelled = true;
      map.off("load", run);
    };
  }, [map, baseRasterArchiveFileName, overlayArchiveFileName]);
}

export interface OfflineMapViewportProps {
  activeSelectionDescription?: string;
  /** File name in `maps/`, for example `noaa-northeast-coastal.mbtiles`, served via `athena-mbtiles` tiles. */
  baseRasterArchiveFileName: string | null;
  /** File name in `maps/`, for example `global-osm-vector.pmtiles`, served via `pmtiles://athena-map://…`. */
  overlayArchiveFileName: string | null;
}

export function OfflineMapViewport({
  activeSelectionDescription,
  baseRasterArchiveFileName,
  overlayArchiveFileName,
}: OfflineMapViewportProps) {
  const rootElementReference = useRef<HTMLDivElement | null>(null);
  const [mapInstance, setMapInstance] = useState<MapLibreMap | null>(null);

  useOfflineMapArchiveLayers(mapInstance, baseRasterArchiveFileName, overlayArchiveFileName);

  useEffect(() => {
    const rootElement = rootElementReference.current;
    if (rootElement === null) {
      return;
    }

    const map = new maplibregl.Map({
      attributionControl: false,
      boxZoom: true,
      container: rootElement,
      dragRotate: false,
      maxPitch: 0,
      pitchWithRotate: false,
      style: buildOfflineBlankStyle(),
      center: [0, 20],
      zoom: 1.2,
    });

    map.addControl(
      new maplibregl.NavigationControl({
        showCompass: false,
        visualizePitch: false,
      }),
      "top-right",
    );

    const handleLoad = (): void => {
      setMapInstance(map);
    };
    map.on("load", handleLoad);

    const resizeObserver = new ResizeObserver(() => {
      map.resize();
    });
    resizeObserver.observe(rootElement);

    return () => {
      resizeObserver.disconnect();
      map.off("load", handleLoad);
      setMapInstance(null);
      map.remove();
    };
  }, []);

  return (
    <div
      className="offline-map-viewport-root"
      ref={rootElementReference}
      role="application"
      aria-label={
        activeSelectionDescription !== undefined && activeSelectionDescription.length > 0
          ? `Offline map preview. ${activeSelectionDescription}`
          : "Offline map preview"
      }
    />
  );
}
