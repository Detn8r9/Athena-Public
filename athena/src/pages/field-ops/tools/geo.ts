/**
 * Navigation without GPS (S22): distance, bearing, dead reckoning and sun direction.
 * Pure maths on a spherical Earth, accurate to well under 1% over field distances.
 */

export type LatLon = { lat: number; lon: number };

const EARTH_RADIUS_M = 6_371_000;
const rad = (deg: number) => (deg * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;

/** Wrap an angle into 0 ≤ a < 360. */
export function normalizeBearing(a: number): number {
  return ((a % 360) + 360) % 360;
}

/** Great-circle distance in metres. */
export function distanceMeters(a: LatLon, b: LatLon): number {
  const dLat = rad(b.lat - a.lat);
  const dLon = rad(b.lon - a.lon);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(rad(a.lat)) * Math.cos(rad(b.lat)) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH_RADIUS_M * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** Initial true bearing from `a` to `b`, degrees clockwise from north. */
export function bearingDegrees(a: LatLon, b: LatLon): number {
  const dLon = rad(b.lon - a.lon);
  const y = Math.sin(dLon) * Math.cos(rad(b.lat));
  const x = Math.cos(rad(a.lat)) * Math.sin(rad(b.lat)) - Math.sin(rad(a.lat)) * Math.cos(rad(b.lat)) * Math.cos(dLon);
  return normalizeBearing(deg(Math.atan2(y, x)));
}

/** Where you end up after travelling `meters` from `start` on a true bearing. */
export function destination(start: LatLon, bearing: number, meters: number): LatLon {
  const d = meters / EARTH_RADIUS_M;
  const b = rad(bearing);
  const lat1 = rad(start.lat);
  const lat2 = Math.asin(Math.sin(lat1) * Math.cos(d) + Math.cos(lat1) * Math.sin(d) * Math.cos(b));
  const lon2 = rad(start.lon) + Math.atan2(Math.sin(b) * Math.sin(d) * Math.cos(lat1), Math.cos(d) - Math.sin(lat1) * Math.sin(lat2));
  return { lat: deg(lat2), lon: ((deg(lon2) + 540) % 360) - 180 };
}

/**
 * Convert a compass (magnetic) bearing to a true bearing. `declination` is degrees east of true
 * north (west is negative), as printed on a topographic map.
 */
export function magneticToTrue(magnetic: number, declination: number): number {
  return normalizeBearing(magnetic + declination);
}

export function trueToMagnetic(trueBearing: number, declination: number): number {
  return normalizeBearing(trueBearing - declination);
}

export type Leg = {
  /** Compass bearing walked, in degrees. */
  bearing: number;
  /** Distance in metres. */
  meters: number;
};

export type DeadReckoning = {
  position: LatLon;
  /** Sum of all legs. */
  travelledMeters: number;
  /** Straight-line distance back to the start. */
  metersFromStart: number;
  /** True bearing to walk to return to the start. */
  bearingToStart: number;
};

/** Follow a series of legs from a known start. Bearings are magnetic unless declination is 0. */
export function deadReckon(start: LatLon, legs: Leg[], declination = 0): DeadReckoning {
  let position = start;
  let travelled = 0;
  for (const leg of legs) {
    if (!(leg.meters > 0) || !Number.isFinite(leg.bearing)) continue;
    position = destination(position, magneticToTrue(leg.bearing, declination), leg.meters);
    travelled += leg.meters;
  }
  return {
    position,
    travelledMeters: travelled,
    metersFromStart: distanceMeters(position, start),
    bearingToStart: travelled > 0 ? bearingDegrees(position, start) : 0,
  };
}

/** Distance from a pace count. A "pace" here is one double step (same foot to same foot). */
export function metersFromPaces(paces: number, metersPerPace: number): number {
  return Math.max(0, paces) * Math.max(0, metersPerPace);
}

const COMPASS_POINTS = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];

/** 16-point compass name for a bearing. */
export function compassPoint(bearing: number): string {
  return COMPASS_POINTS[Math.round(normalizeBearing(bearing) / 22.5) % 16]!;
}

export type SunPosition = {
  /** True bearing of the sun, degrees clockwise from north. */
  azimuth: number;
  /** Degrees above the horizon; negative when the sun is down. */
  elevation: number;
};

/**
 * Where the sun is in the sky, from the low-precision Astronomical Almanac formulae (good to a
 * fraction of a degree, which is far better than a hand compass). Needs the date, the time and a
 * rough position; with those, the sun is a compass that needs no magnet.
 */
export function sunPosition(at: Date, where: LatLon): SunPosition {
  const n = at.getTime() / 86_400_000 + 2_440_587.5 - 2_451_545.0;
  const meanLon = normalizeBearing(280.46 + 0.9856474 * n);
  const meanAnomaly = rad(normalizeBearing(357.528 + 0.9856003 * n));
  const eclipticLon = rad(meanLon + 1.915 * Math.sin(meanAnomaly) + 0.02 * Math.sin(2 * meanAnomaly));
  const obliquity = rad(23.439 - 0.0000004 * n);
  const rightAscension = Math.atan2(Math.cos(obliquity) * Math.sin(eclipticLon), Math.cos(eclipticLon));
  const declination = Math.asin(Math.sin(obliquity) * Math.sin(eclipticLon));
  const gmstHours = (((18.697374558 + 24.06570982441908 * n) % 24) + 24) % 24;
  const hourAngle = rad(gmstHours * 15 + where.lon) - rightAscension;
  const lat = rad(where.lat);
  const elevation = Math.asin(Math.sin(lat) * Math.sin(declination) + Math.cos(lat) * Math.cos(declination) * Math.cos(hourAngle));
  const azimuth = Math.atan2(-Math.sin(hourAngle), Math.tan(declination) * Math.cos(lat) - Math.sin(lat) * Math.cos(hourAngle));
  return { azimuth: normalizeBearing(deg(azimuth)), elevation: deg(elevation) };
}

/** Parse "51.5, -0.12" or "51.5 -0.12". Returns null when it is not a valid position. */
export function parseLatLon(text: string): LatLon | null {
  const parts = text
    .trim()
    .split(/[\s,;]+/)
    .filter(Boolean)
    .map(Number);
  if (parts.length !== 2 || parts.some((n) => !Number.isFinite(n))) return null;
  const [lat, lon] = parts as [number, number];
  if (Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
  return { lat, lon };
}

export function formatLatLon(p: LatLon): string {
  return `${p.lat.toFixed(5)}, ${p.lon.toFixed(5)}`;
}

export function formatDistance(meters: number): string {
  if (meters < 1000) return `${Math.round(meters)} m`;
  return `${(meters / 1000).toFixed(meters < 10_000 ? 2 : 1)} km`;
}
