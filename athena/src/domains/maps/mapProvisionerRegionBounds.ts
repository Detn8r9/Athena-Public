import type { Feature, FeatureCollection, Polygon } from "geojson";

/**
 * Approximate [west, south, east, north] for Kiwix / catalog region labels used in `maps.rs`.
 * Used only for provisioning preview overlays (not navigation).
 */
export const REGION_BBOX_DEG: Record<string, [number, number, number, number]> = {
  "North America": [-168, 7, -52, 72],
  Europe: [-31, 35, 41, 72],
  "Australia & Oceania": [110, -48, 180, -10],
  Asia: [26, -12, 145, 55],
  India: [68, 6, 97, 37],
  Russia: [27, 41, 180, 75],
  Ukraine: [22, 44, 41, 53],
  Africa: [-25, -35, 55, 38],
  "South America": [-82, -56, -34, 13],
  "Central America": [-95, 7, -77, 23],
};

export function bboxPolygon(west: number, south: number, east: number, north: number): Polygon {
  return {
    type: "Polygon",
    coordinates: [
      [
        [west, south],
        [east, south],
        [east, north],
        [west, north],
        [west, south],
      ],
    ],
  };
}

export function regionFeatureCollection(regions: string[]): FeatureCollection {
  const seen = new Set<string>();
  const features: Feature[] = [];
  for (const raw of regions) {
    const key = raw.trim();
    if (!key || seen.has(key)) continue;
    const box = REGION_BBOX_DEG[key];
    if (!box) continue;
    seen.add(key);
    const [w, s, e, n] = box;
    features.push({
      type: "Feature",
      properties: { name: key },
      geometry: bboxPolygon(w, s, e, n),
    });
  }
  return { type: "FeatureCollection", features };
}
