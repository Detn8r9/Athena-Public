/**
 * Axis-aligned geographic bounding boxes in WGS84 degrees (longitude, latitude).
 * Suitable for tools that expect min_lon, min_lat, max_lon, max_lat (e.g. Protomaps `pmtiles extract --bbox=...`).
 */

/** Mean Earth radius in statute miles (WGS84 approximation). */
const EARTH_MEAN_RADIUS_MILES = 3958.7613;

/** Maximum great-circle radius for Protomaps region extract (matches backend guard). */
export const MAX_EXTRACT_RADIUS_MILES = 100;

/** Minimum half-span in degrees so min_lon &lt; max_lon and min_lat &lt; max_lat after floating-point math. */
const MIN_HALF_SPAN_DEGREES = 1e-7;

/** Longitude must stay within this inclusive range. */
const LON_MIN = -180;
const LON_MAX = 180;

/** Latitude must stay within this inclusive range. */
const LAT_MIN = -90;
const LAT_MAX = 90;

function degreesToRadians(degrees: number): number {
  return (degrees * Math.PI) / 180;
}

function radiansToDegrees(radians: number): number {
  return (radians * 180) / Math.PI;
}

export interface GeographicBoundingBox {
  min_lon: number;
  min_lat: number;
  max_lon: number;
  max_lat: number;
}

function isFiniteNumber(value: number): boolean {
  return typeof value === "number" && Number.isFinite(value);
}

/**
 * Computes the geographic bounding box of a circle on the Earth’s surface:
 * center (`centerLatitudeDegrees`, `centerLongitudeDegrees`) and `radiusMiles` (great-circle / “as the crow flies”).
 *
 * Longitude span uses the cosine of latitude so one mile east–west matches one mile north–south at the equator.
 * Results are clamped to WGS84 limits; at the poles, longitude span is capped so the box stays valid.
 */
export function boundingBoxFromCenterAndRadiusMiles(
  centerLatitudeDegrees: number,
  centerLongitudeDegrees: number,
  radiusMiles: number,
): GeographicBoundingBox {
  if (!isFiniteNumber(centerLatitudeDegrees) || !isFiniteNumber(centerLongitudeDegrees) || !isFiniteNumber(radiusMiles)) {
    throw new Error("Center latitude, center longitude, and radius in miles must be finite numbers.");
  }
  if (radiusMiles <= 0) {
    throw new Error("Radius in miles must be greater than zero.");
  }
  if (radiusMiles > MAX_EXTRACT_RADIUS_MILES) {
    throw new Error(
      `Radius must be at most ${MAX_EXTRACT_RADIUS_MILES} miles for regional map extract (got ${radiusMiles}).`,
    );
  }
  if (centerLatitudeDegrees < LAT_MIN || centerLatitudeDegrees > LAT_MAX) {
    throw new Error(`Center latitude must be between ${LAT_MIN} and ${LAT_MAX} degrees.`);
  }
  if (centerLongitudeDegrees < LON_MIN || centerLongitudeDegrees > LON_MAX) {
    throw new Error(`Center longitude must be between ${LON_MIN} and ${LON_MAX} degrees.`);
  }

  const centerLatRad = degreesToRadians(centerLatitudeDegrees);
  const angularDistanceRadians = radiusMiles / EARTH_MEAN_RADIUS_MILES;

  const deltaLatitudeDegrees = radiansToDegrees(angularDistanceRadians);

  const cosLatitude = Math.cos(centerLatRad);
  const cosLatitudeMagnitude = Math.max(Math.abs(cosLatitude), 1e-10);
  let deltaLongitudeDegrees = radiansToDegrees(angularDistanceRadians / cosLatitudeMagnitude);
  if (deltaLongitudeDegrees > 180) {
    deltaLongitudeDegrees = 180;
  }

  let min_lat = centerLatitudeDegrees - deltaLatitudeDegrees;
  let max_lat = centerLatitudeDegrees + deltaLatitudeDegrees;
  let min_lon = centerLongitudeDegrees - deltaLongitudeDegrees;
  let max_lon = centerLongitudeDegrees + deltaLongitudeDegrees;

  if (min_lat < LAT_MIN) {
    min_lat = LAT_MIN;
  }
  if (max_lat > LAT_MAX) {
    max_lat = LAT_MAX;
  }
  if (min_lat > max_lat) {
    const mid = (min_lat + max_lat) / 2;
    min_lat = mid - MIN_HALF_SPAN_DEGREES;
    max_lat = mid + MIN_HALF_SPAN_DEGREES;
    min_lat = Math.max(LAT_MIN, min_lat);
    max_lat = Math.min(LAT_MAX, max_lat);
  }
  if (min_lat >= max_lat) {
    min_lat = Math.max(LAT_MIN, centerLatitudeDegrees - MIN_HALF_SPAN_DEGREES);
    max_lat = Math.min(LAT_MAX, centerLatitudeDegrees + MIN_HALF_SPAN_DEGREES);
  }

  if (min_lon < LON_MIN) {
    min_lon = LON_MIN;
  }
  if (max_lon > LON_MAX) {
    max_lon = LON_MAX;
  }
  if (min_lon >= max_lon) {
    min_lon = Math.max(LON_MIN, centerLongitudeDegrees - MIN_HALF_SPAN_DEGREES);
    max_lon = Math.min(LON_MAX, centerLongitudeDegrees + MIN_HALF_SPAN_DEGREES);
  }
  if (min_lon >= max_lon) {
    max_lon = Math.min(LON_MAX, min_lon + 2 * MIN_HALF_SPAN_DEGREES);
  }

  return {
    min_lon,
    min_lat,
    max_lon,
    max_lat,
  };
}
