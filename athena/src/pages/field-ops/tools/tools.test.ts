import { describe, expect, it } from "vitest";
import {
  bearingDegrees,
  compassPoint,
  deadReckon,
  destination,
  distanceMeters,
  magneticToTrue,
  metersFromPaces,
  parseLatLon,
  sunPosition,
  trueToMagnetic,
} from "./geo";
import { assessPoint, downwindBearing, escapeBearings, plumeHalfAngle, plumePolygon, toMetersPerSecond } from "./hazard";
import { correctedTime, fitClock, uncertaintySeconds } from "./timekeeping";
import { classifyTendency, outlook, pressureTrend, seaLevelPressure } from "./weather";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

describe("distance and bearing", () => {
  it("measures one degree of latitude as about 111.2 km", () => {
    expect(distanceMeters({ lat: 0, lon: 0 }, { lat: 1, lon: 0 })).toBeCloseTo(111_195, -2);
  });

  it("gives cardinal bearings", () => {
    const o = { lat: 45, lon: 10 };
    expect(bearingDegrees(o, { lat: 46, lon: 10 })).toBeCloseTo(0, 5);
    expect(bearingDegrees(o, { lat: 44, lon: 10 })).toBeCloseTo(180, 5);
    expect(bearingDegrees({ lat: 0, lon: 0 }, { lat: 0, lon: 1 })).toBeCloseTo(90, 5);
    expect(bearingDegrees({ lat: 0, lon: 0 }, { lat: 0, lon: -1 })).toBeCloseTo(270, 5);
  });

  it("destination and distance/bearing agree with each other", () => {
    const start = { lat: 51.5, lon: -0.12 };
    const end = destination(start, 73, 4200);
    expect(distanceMeters(start, end)).toBeCloseTo(4200, 0);
    expect(bearingDegrees(start, end)).toBeCloseTo(73, 1);
  });

  it("applies declination east-positive and round-trips", () => {
    expect(magneticToTrue(350, 15)).toBe(5);
    expect(magneticToTrue(10, -15)).toBe(355);
    expect(trueToMagnetic(magneticToTrue(123, -7), -7)).toBe(123);
  });

  it("names compass points", () => {
    expect(compassPoint(0)).toBe("N");
    expect(compassPoint(359)).toBe("N");
    expect(compassPoint(45)).toBe("NE");
    expect(compassPoint(202.5)).toBe("SSW");
  });

  it("parses positions and rejects nonsense", () => {
    expect(parseLatLon("51.5, -0.12")).toEqual({ lat: 51.5, lon: -0.12 });
    expect(parseLatLon(" 51.5   -0.12 ")).toEqual({ lat: 51.5, lon: -0.12 });
    expect(parseLatLon("91, 0")).toBeNull();
    expect(parseLatLon("north of here")).toBeNull();
  });
});

describe("dead reckoning", () => {
  it("a closed square brings you back to the start", () => {
    const start = { lat: 40, lon: -105 };
    const r = deadReckon(start, [
      { bearing: 0, meters: 500 },
      { bearing: 90, meters: 500 },
      { bearing: 180, meters: 500 },
      { bearing: 270, meters: 500 },
    ]);
    expect(r.travelledMeters).toBe(2000);
    expect(r.metersFromStart).toBeLessThan(1);
  });

  it("out-and-back legs give the reverse bearing home", () => {
    const start = { lat: 40, lon: -105 };
    const r = deadReckon(start, [{ bearing: 60, meters: 1500 }]);
    expect(r.metersFromStart).toBeCloseTo(1500, 0);
    expect(r.bearingToStart).toBeCloseTo(240, 0);
  });

  it("applies declination to every leg and skips empty legs", () => {
    const start = { lat: 40, lon: -105 };
    const r = deadReckon(start, [{ bearing: 350, meters: 1000 }, { bearing: 10, meters: 0 }], 10);
    // 350 magnetic + 10 east = due north.
    expect(r.position.lon).toBeCloseTo(-105, 4);
    expect(r.position.lat).toBeGreaterThan(40);
    expect(r.travelledMeters).toBe(1000);
  });

  it("converts paces to distance", () => {
    expect(metersFromPaces(64, 1.5)).toBe(96);
    expect(metersFromPaces(-3, 1.5)).toBe(0);
  });
});

describe("sun position", () => {
  it("is due south and at the expected height at midsummer noon in London", () => {
    const p = sunPosition(new Date(Date.UTC(2027, 5, 21, 12, 0, 0)), { lat: 51.5, lon: -0.13 });
    expect(p.elevation).toBeGreaterThan(61);
    expect(p.elevation).toBeLessThan(62.5);
    expect(Math.abs(p.azimuth - 180)).toBeLessThan(3);
  });

  it("is in the north-east near the horizon at midsummer dawn in London", () => {
    const p = sunPosition(new Date(Date.UTC(2027, 5, 21, 4, 0, 0)), { lat: 51.5, lon: -0.13 });
    expect(p.elevation).toBeGreaterThan(-3);
    expect(p.elevation).toBeLessThan(5);
    expect(p.azimuth).toBeGreaterThan(40);
    expect(p.azimuth).toBeLessThan(60);
  });

  it("is nearly overhead at the equator at the equinox", () => {
    const p = sunPosition(new Date(Date.UTC(2027, 2, 20, 12, 0, 0)), { lat: 0, lon: 0 });
    expect(p.elevation).toBeGreaterThan(86);
  });

  it("is north at noon in the southern hemisphere and below the horizon at midnight", () => {
    const noon = sunPosition(new Date(Date.UTC(2027, 5, 21, 2, 0, 0)), { lat: -33.9, lon: 151.2 });
    expect(Math.min(noon.azimuth, 360 - noon.azimuth)).toBeLessThan(10);
    const night = sunPosition(new Date(Date.UTC(2027, 5, 21, 0, 0, 0)), { lat: 51.5, lon: -0.13 });
    expect(night.elevation).toBeLessThan(0);
  });
});

describe("weather outlook", () => {
  const T0 = 1_800_000_000_000;
  const series = (start: number, per3h: number, hours: number) =>
    Array.from({ length: hours + 1 }, (_, i) => ({ at: T0 + i * HOUR, hpa: start + (per3h / 3) * i }));

  it("measures the trend as hPa per 3 hours", () => {
    const t = pressureTrend(series(1015, -4.5, 6))!;
    expect(t.changePer3h).toBeCloseTo(-4.5, 6);
    expect(t.spanHours).toBe(6);
  });

  it("needs at least an hour of readings", () => {
    expect(pressureTrend([{ at: T0, hpa: 1010 }])).toBeNull();
    expect(pressureTrend([{ at: T0, hpa: 1010 }, { at: T0 + 20 * 60_000, hpa: 1008 }])).toBeNull();
  });

  it("only uses the recent window", () => {
    const old = [{ at: T0 - 40 * HOUR, hpa: 990 }];
    const t = pressureTrend([...old, ...series(1015, 0, 6)])!;
    expect(t.changePer3h).toBeCloseTo(0, 6);
  });

  it("classifies tendency by the marine bands", () => {
    expect(classifyTendency(-6.5)).toBe("falling_very_fast");
    expect(classifyTendency(-4)).toBe("falling_fast");
    expect(classifyTendency(-2)).toBe("falling");
    expect(classifyTendency(-1)).toBe("steady");
    expect(classifyTendency(1)).toBe("steady");
    expect(classifyTendency(2)).toBe("rising");
    expect(classifyTendency(5)).toBe("rising_fast");
  });

  it("warns of a storm when pressure collapses", () => {
    const o = outlook(series(1010, -7, 4))!;
    expect(o.tendency).toBe("falling_very_fast");
    expect(o.warning).toBe(true);
    expect(o.headline).toBe("Storm likely within hours");
  });

  it("calls high steady pressure settled and does not warn", () => {
    const o = outlook(series(1028, 0, 6))!;
    expect(o.headline).toBe("Settled");
    expect(o.warning).toBe(false);
  });

  it("distinguishes a fall from high pressure from a fall at low pressure", () => {
    expect(outlook(series(1030, -2, 6))!.headline).toBe("Fair for now, change coming");
    const low = outlook(series(1006, -2, 6))!;
    expect(low.headline).toBe("Rain or wind likely");
    expect(low.warning).toBe(true);
  });

  it("corrects a reading taken at altitude up to sea level", () => {
    expect(seaLevelPressure(1013.25, 0)).toBeCloseTo(1013.25, 6);
    const corrected = seaLevelPressure(900, 1000);
    expect(corrected).toBeGreaterThan(1010);
    expect(corrected).toBeLessThan(1020);
  });
});

describe("downwind hazard", () => {
  const source = { lat: 45, lon: 10 };
  const westerly = { fromDegrees: 270, speedMs: 5 };

  it("travels opposite to where the wind comes from", () => {
    expect(downwindBearing(westerly)).toBe(90);
    expect(downwindBearing({ fromDegrees: 10, speedMs: 3 })).toBe(190);
  });

  it("puts points downwind in the path and gives arrival time", () => {
    const east = destination(source, 90, 3000);
    const r = assessPoint(source, westerly, east);
    expect(r.inPath).toBe(true);
    expect(r.minutesToArrival).toBeCloseTo(10, 1);
  });

  it("keeps upwind and crosswind points out of the path", () => {
    expect(assessPoint(source, westerly, destination(source, 270, 3000)).inPath).toBe(false);
    expect(assessPoint(source, westerly, destination(source, 0, 3000)).inPath).toBe(false);
    expect(assessPoint(source, westerly, destination(source, 0, 3000)).minutesToArrival).toBeNull();
  });

  it("widens the wedge in light wind", () => {
    expect(plumeHalfAngle(1)).toBeGreaterThan(plumeHalfAngle(8));
    const offAxis = destination(source, 90 + 40, 2000);
    expect(assessPoint(source, { fromDegrees: 270, speedMs: 1 }, offAxis).inPath).toBe(true);
    expect(assessPoint(source, { fromDegrees: 270, speedMs: 8 }, offAxis).inPath).toBe(false);
  });

  it("treats the source itself as in the path", () => {
    expect(assessPoint(source, westerly, destination(source, 270, 20)).inPath).toBe(true);
  });

  it("tells you to go crosswind or upwind, never downwind", () => {
    const e = escapeBearings(westerly);
    expect(e.crosswind.sort((a, b) => a - b)).toEqual([0, 180]);
    expect(e.upwind).toBe(270);
  });

  it("draws a closed wedge reaching as far as the wind has carried it", () => {
    const poly = plumePolygon(source, westerly, 10);
    expect(poly[0]).toEqual(source);
    expect(poly[poly.length - 1]).toEqual(source);
    const far = Math.max(...poly.map((p) => distanceMeters(source, p)));
    expect(far).toBeCloseTo(3000, -1);
  });

  it("converts wind speed units", () => {
    expect(toMetersPerSecond(36, "kmh")).toBeCloseTo(10, 6);
    expect(toMetersPerSecond(10, "knots")).toBeCloseTo(5.14444, 4);
    expect(toMetersPerSecond(10, "mph")).toBeCloseTo(4.4704, 4);
    expect(toMetersPerSecond(7, "ms")).toBe(7);
  });
});

describe("trusted time", () => {
  const T0 = 1_800_000_000_000;

  it("has an offset but no drift from a single check", () => {
    const m = fitClock([{ systemMs: T0, referenceMs: T0 + 4000 }])!;
    expect(m.offsetSeconds).toBe(4);
    expect(m.driftSecondsPerDay).toBeNull();
    expect(correctedTime(m, T0 + 5 * DAY)).toBe(T0 + 5 * DAY + 4000);
  });

  it("measures drift from checks days apart and extrapolates it", () => {
    // The clock loses 3 seconds a day.
    const checks = [0, 2, 4].map((d) => ({ systemMs: T0 + d * DAY, referenceMs: T0 + d * DAY + 1000 + d * 3000 }));
    const m = fitClock(checks)!;
    expect(m.driftSecondsPerDay).toBeCloseTo(3, 6);
    expect(m.offsetSeconds).toBeCloseTo(13, 6);
    // Two days after the last check the clock is a further 6 s slow.
    expect(correctedTime(m, T0 + 6 * DAY)).toBeCloseTo(T0 + 6 * DAY + 19_000, 3);
  });

  it("ignores checks too close together to measure drift", () => {
    const m = fitClock([
      { systemMs: T0, referenceMs: T0 + 1000 },
      { systemMs: T0 + 60_000, referenceMs: T0 + 61_500 },
    ])!;
    expect(m.driftSecondsPerDay).toBeNull();
    expect(m.offsetSeconds).toBe(1.5);
  });

  it("grows less certain the longer since the last check", () => {
    const m = fitClock([{ systemMs: T0, referenceMs: T0 }])!;
    expect(uncertaintySeconds(m, T0)).toBe(1);
    expect(uncertaintySeconds(m, T0 + 10 * DAY)).toBeCloseTo(21, 6);
    expect(fitClock([])).toBeNull();
  });
});
