/**
 * Simple projected circle → lon/lat polygon (visualization), matching the tactical
 * coverage style in {@link MapViewMaplibre} (64-step ring, km/111.32).
 */

const KM_PER_DEGREE_LAT = 111.32;

/**
 * @param centerLat WGS84 latitude
 * @param centerLon WGS84 longitude
 * @param radiusMiles statute miles
 * @returns closed ring of [lon, lat] suitable for a GeoJSON Polygon
 */
export function buildCirclePolygonRingMiles(
  centerLat: number,
  centerLon: number,
  radiusMiles: number,
  steps = 64,
): [number, number][] {
  const km = radiusMiles * 1.60934;
  const ring: [number, number][] = [];
  for (let i = 0; i <= steps; i += 1) {
    const t = (i / steps) * 2 * Math.PI;
    const dx = km / KM_PER_DEGREE_LAT;
    const latPart = (km / KM_PER_DEGREE_LAT) * Math.sin(t);
    const lonPart = (dx * Math.cos(t)) / Math.cos((centerLat * Math.PI) / 180);
    ring.push([centerLon + lonPart, centerLat + latPart]);
  }
  return ring;
}
