/**
 * Local index for map acquisition (offline-first): per-type size multipliers and
 * a mock grid to intersect “tiles” with a coordinate + radius.
 */

import { boundingBoxFromCenterAndRadiusMiles, type GeographicBoundingBox, MAX_EXTRACT_RADIUS_MILES } from "./geographicBoundingBox";

const EARTH_MEAN_RADIUS_MILES = 3958.7613;

export type MapTypeId = "satellite" | "topographic" | "terrain" | "populationDensity" | "streetVector";

export type TauriAcquireCommand = "start_satellite_region_download" | "extract_pmtiles_region";

export interface MapType {
  id: MapTypeId;
  name: string;
  description: string;
  /** Mock MB per square mile for UI estimates. */
  sizePerSquareMileMb: number;
  /** When true, show a Download Area card in the map archive flow. */
  acquirable: boolean;
  tauriCommand?: TauriAcquireCommand;
}

export const MAP_CATALOG: ReadonlyArray<MapType> = [
  {
    id: "satellite",
    name: "Satellite",
    description: "High-resolution MBTiles from MapTiler (saved API key).",
    sizePerSquareMileMb: 50,
    acquirable: true,
    tauriCommand: "start_satellite_region_download",
  },
  {
    id: "streetVector",
    name: "Street vector",
    description: "Protomaps PMTiles cut from the public extract — streets and labels for the selected area.",
    sizePerSquareMileMb: 2,
    acquirable: true,
    tauriCommand: "extract_pmtiles_region",
  },
  {
    id: "topographic",
    name: "Topographic",
    description: "Contours & hillshade (not wired in ATHENA yet).",
    sizePerSquareMileMb: 8,
    acquirable: false,
  },
  {
    id: "terrain",
    name: "Terrain (DEM)",
    description: "Elevation / DEM (not wired in ATHENA yet).",
    sizePerSquareMileMb: 12,
    acquirable: false,
  },
  {
    id: "populationDensity",
    name: "Population density",
    description: "Raster density grid (not wired in ATHENA yet).",
    sizePerSquareMileMb: 0.5,
    acquirable: false,
  },
];

const MOCK_GRID_DEG = 0.1;

function haversineMiles(aLat: number, aLon: number, bLat: number, bLon: number): number {
  const dLat = ((bLat - aLat) * Math.PI) / 180;
  const dLon = ((bLon - aLon) * Math.PI) / 180;
  const p1 = (aLat * Math.PI) / 180;
  const p2 = (bLat * Math.PI) / 180;
  const s =
    Math.sin(dLat / 2) * Math.sin(dLat / 2) +
    Math.sin(dLon / 2) * Math.sin(dLon / 2) * Math.cos(p1) * Math.cos(p2);
  const c = 2 * Math.atan2(Math.sqrt(s), Math.sqrt(1 - s));
  return EARTH_MEAN_RADIUS_MILES * c;
}

/**
 * Great-circle distance from a point to the axis-aligned [minLon,minLat]–[maxLon,maxLat] (degrees).
 * Used to test whether a mock grid cell intersects a radius (disk) on the sphere.
 */
function distancePointToBoxMiles(lat: number, lon: number, box: [number, number, number, number]): number {
  const [minLon, minLat, maxLon, maxLat] = box;
  const clon = Math.max(minLon, Math.min(maxLon, lon));
  const clat = Math.max(minLat, Math.min(maxLat, lat));
  return haversineMiles(lat, lon, clat, clon);
}

export type MockMapTile = {
  row: number;
  col: number;
  bbox: [number, number, number, number];
};

/**
 * 0.1° mock grid. Returns cells whose bbox intersects the great-circle distance disk (radius in miles) around the center.
 */
export function tilesIntersectingCircle(lat: number, lon: number, radiusMiles: number): MockMapTile[] {
  if (!Number.isFinite(lat) || !Number.isFinite(lon) || !Number.isFinite(radiusMiles) || radiusMiles <= 0) {
    return [];
  }
  const r = Math.min(radiusMiles, MAX_EXTRACT_RADIUS_MILES);
  const ext = boundingBoxFromCenterAndRadiusMiles(lat, lon, r);
  const minRow = Math.floor(ext.min_lat / MOCK_GRID_DEG) - 1;
  const maxRow = Math.floor(ext.max_lat / MOCK_GRID_DEG) + 1;
  const minCol = Math.floor(ext.min_lon / MOCK_GRID_DEG) - 1;
  const maxCol = Math.floor(ext.max_lon / MOCK_GRID_DEG) + 1;
  const out: MockMapTile[] = [];
  for (let row = minRow; row <= maxRow; row += 1) {
    for (let col = minCol; col <= maxCol; col += 1) {
      const minLat = row * MOCK_GRID_DEG;
      const maxLati = (row + 1) * MOCK_GRID_DEG;
      const minLon = col * MOCK_GRID_DEG;
      const maxLonv = (col + 1) * MOCK_GRID_DEG;
      const box: [number, number, number, number] = [minLon, minLat, maxLonv, maxLati];
      if (distancePointToBoxMiles(lat, lon, box) <= r) {
        out.push({ row, col, bbox: box });
      }
    }
  }
  return out;
}

export function estimatedDownloadMb(map: MapType, radiusMiles: number): number {
  if (!map.acquirable) return 0;
  const r = Math.max(0, Math.min(radiusMiles, MAX_EXTRACT_RADIUS_MILES));
  const areaSqMi = Math.PI * r * r;
  return areaSqMi * map.sizePerSquareMileMb;
}

/**
 * Bbox in min_lon, min_lat, max_lon, max_lat for the extract pipeline (Tauri), clamped the same as {@link boundingBoxFromCenterAndRadiusMiles}.
 */
export function bboxFromCenterAndRadiusMiles(
  centerLatitudeDegrees: number,
  centerLongitudeDegrees: number,
  radiusMiles: number,
): GeographicBoundingBox {
  return boundingBoxFromCenterAndRadiusMiles(
    centerLatitudeDegrees,
    centerLongitudeDegrees,
    Math.min(radiusMiles, MAX_EXTRACT_RADIUS_MILES),
  );
}

export { MAX_EXTRACT_RADIUS_MILES };
