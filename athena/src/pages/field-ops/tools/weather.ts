/**
 * Local weather outlook from a barometer (S20).
 *
 * Pressure and how fast it is changing are the oldest reliable short-range forecast there is.
 * This applies the traditional barometer rules to your own readings. It is an outlook for the
 * next 12 to 24 hours, not a guarantee.
 */

export type PressureReading = {
  /** Unix milliseconds. */
  at: number;
  /** Sea-level pressure in hPa (millibars). */
  hpa: number;
};

export type Tendency = "rising_fast" | "rising" | "steady" | "falling" | "falling_fast" | "falling_very_fast";

export type Outlook = {
  tendency: Tendency;
  /** Change in hPa per 3 hours; positive is rising. */
  changePer3h: number;
  latestHpa: number;
  /** Hours of readings the tendency is based on. */
  spanHours: number;
  headline: string;
  detail: string;
  /** True when the outlook warrants preparing for bad weather. */
  warning: boolean;
};

/**
 * Convert a station reading to sea-level pressure so readings taken at height compare with
 * the thresholds below. Uses the standard barometric formula.
 */
export function seaLevelPressure(stationHpa: number, altitudeMeters: number, tempC = 15): number {
  const h = Math.max(0, altitudeMeters);
  return stationHpa * (1 - (0.0065 * h) / (tempC + 0.0065 * h + 273.15)) ** -5.257;
}

/**
 * Rate of change over the recent readings, as hPa per 3 hours, by least-squares fit so one
 * noisy reading does not swing the result. Needs at least an hour of readings.
 */
export function pressureTrend(readings: PressureReading[], windowHours = 6): { changePer3h: number; spanHours: number } | null {
  const sorted = [...readings].filter((r) => Number.isFinite(r.hpa)).sort((a, b) => a.at - b.at);
  if (sorted.length < 2) return null;
  const latest = sorted[sorted.length - 1]!;
  const recent = sorted.filter((r) => latest.at - r.at <= windowHours * 3_600_000);
  if (recent.length < 2) return null;
  const spanHours = (latest.at - recent[0]!.at) / 3_600_000;
  if (spanHours < 1) return null;
  const xs = recent.map((r) => (r.at - recent[0]!.at) / 3_600_000);
  const ys = recent.map((r) => r.hpa);
  const mx = xs.reduce((a, b) => a + b, 0) / xs.length;
  const my = ys.reduce((a, b) => a + b, 0) / ys.length;
  let num = 0;
  let den = 0;
  for (let i = 0; i < xs.length; i++) {
    num += (xs[i]! - mx) * (ys[i]! - my);
    den += (xs[i]! - mx) ** 2;
  }
  if (den === 0) return null;
  return { changePer3h: (num / den) * 3, spanHours };
}

/** Classify a 3-hour change using the bands used in marine forecasts. */
export function classifyTendency(changePer3h: number): Tendency {
  const c = changePer3h;
  if (c <= -6) return "falling_very_fast";
  if (c <= -3.6) return "falling_fast";
  if (c <= -1.6) return "falling";
  if (c >= 3.6) return "rising_fast";
  if (c >= 1.6) return "rising";
  return "steady";
}

export function outlook(readings: PressureReading[]): Outlook | null {
  const trend = pressureTrend(readings);
  if (!trend) return null;
  const latest = [...readings].sort((a, b) => a.at - b.at).pop()!;
  const tendency = classifyTendency(trend.changePer3h);
  const p = latest.hpa;
  const high = p >= 1022;
  const low = p < 1009;

  let headline: string;
  let detail: string;
  let warning = false;

  switch (tendency) {
    case "falling_very_fast":
      headline = "Storm likely within hours";
      detail = "Pressure is dropping very rapidly. Expect strong wind and heavy rain or snow soon. Secure shelter, tarps and anything loose, and bring people in.";
      warning = true;
      break;
    case "falling_fast":
      headline = "Bad weather approaching";
      detail = "Pressure is falling quickly. Wind and rain are likely within about 6 to 12 hours. Finish outdoor work and prepare shelter.";
      warning = true;
      break;
    case "falling":
      if (low) {
        headline = "Rain or wind likely";
        detail = "Pressure is low and still falling. Unsettled weather is likely within the day.";
        warning = true;
      } else if (high) {
        headline = "Fair for now, change coming";
        detail = "Pressure is high but falling. Expect the fair spell to end within a day or two.";
      } else {
        headline = "Turning unsettled";
        detail = "Pressure is falling. Cloud and possibly rain are likely within 12 to 24 hours.";
      }
      break;
    case "rising_fast":
      headline = low ? "Clearing, but windy" : "Clearing quickly";
      detail = "Pressure is rising quickly. The weather should improve, often with strong gusty wind while it does.";
      break;
    case "rising":
      headline = "Improving";
      detail = low
        ? "Pressure is low but rising. Conditions should gradually improve."
        : "Pressure is rising. Expect drier, more settled weather.";
      break;
    default:
      if (high) {
        headline = "Settled";
        detail = "Pressure is high and steady. Fair weather is likely to continue. On clear nights expect it to be cold.";
      } else if (low) {
        headline = "Unsettled";
        detail = "Pressure is low and steady. Expect continued cloud and showers.";
      } else {
        headline = "Little change";
        detail = "Pressure is steady. Expect weather much like the present for the next 12 hours.";
      }
  }

  return { tendency, changePer3h: trend.changePer3h, latestHpa: p, spanHours: trend.spanHours, headline, detail, warning };
}
