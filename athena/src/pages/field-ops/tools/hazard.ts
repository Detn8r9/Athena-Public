/**
 * Downwind hazard zone (S21): which way smoke, gas or fallout is heading and when it arrives.
 *
 * This is geometry, not a dispersion model: a wedge pointing downwind from the source, widening
 * with distance. It tells you which side to be on and roughly how long you have. It says nothing
 * about concentration or how far the hazard stays dangerous.
 */

import { bearingDegrees, destination, distanceMeters, normalizeBearing, type LatLon } from "./geo";

export type Wind = {
  /** Direction the wind is blowing FROM, degrees true (the way a weather vane reports it). */
  fromDegrees: number;
  /** Metres per second. */
  speedMs: number;
};

/** Wedge half-angle. Light winds wander, so the wedge is wider. */
export function plumeHalfAngle(speedMs: number): number {
  if (speedMs < 2) return 45;
  if (speedMs < 5) return 30;
  return 20;
}

/** Direction the hazard travels: the opposite of where the wind comes from. */
export function downwindBearing(wind: Wind): number {
  return normalizeBearing(wind.fromDegrees + 180);
}

export type PointRisk = {
  /** True when the point lies inside the downwind wedge. */
  inPath: boolean;
  distanceMeters: number;
  /** Degrees between the downwind line and the direction to the point. */
  offAxisDegrees: number;
  /** Minutes until the leading edge reaches the point. Null when out of the path or no wind. */
  minutesToArrival: number | null;
};

export function assessPoint(source: LatLon, wind: Wind, point: LatLon): PointRisk {
  const dist = distanceMeters(source, point);
  const toPoint = bearingDegrees(source, point);
  const diff = Math.abs(((toPoint - downwindBearing(wind) + 540) % 360) - 180);
  // Right at the source everyone is in the path.
  const inPath = dist < 50 || diff <= plumeHalfAngle(wind.speedMs);
  return {
    inPath,
    distanceMeters: dist,
    offAxisDegrees: diff,
    minutesToArrival: inPath && wind.speedMs > 0 ? dist / wind.speedMs / 60 : null,
  };
}

export type Escape = {
  /** The two crosswind bearings: the quickest way out of the path. */
  crosswind: [number, number];
  /** Straight into the wind: away from where the hazard is going. */
  upwind: number;
};

/** Which way to move. Crosswind gets you out of the wedge fastest; never move downwind. */
export function escapeBearings(wind: Wind): Escape {
  const down = downwindBearing(wind);
  return {
    crosswind: [normalizeBearing(down + 90), normalizeBearing(down - 90)],
    upwind: normalizeBearing(wind.fromDegrees),
  };
}

/** Outline of the wedge the leading edge has covered after `minutes`, for drawing on a map. */
export function plumePolygon(source: LatLon, wind: Wind, minutes: number, arcPoints = 8): LatLon[] {
  const reach = Math.max(0, wind.speedMs) * Math.max(0, minutes) * 60;
  const half = plumeHalfAngle(wind.speedMs);
  const down = downwindBearing(wind);
  const outline: LatLon[] = [source];
  for (let i = 0; i <= arcPoints; i++) {
    outline.push(destination(source, down - half + (2 * half * i) / arcPoints, reach));
  }
  outline.push(source);
  return outline;
}

/** Kilometres per hour, knots or miles per hour to metres per second. */
export function toMetersPerSecond(value: number, unit: "ms" | "kmh" | "knots" | "mph"): number {
  switch (unit) {
    case "kmh":
      return value / 3.6;
    case "knots":
      return value * 0.514444;
    case "mph":
      return value * 0.44704;
    default:
      return value;
  }
}
