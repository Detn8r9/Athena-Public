/**
 * Trusted time (S23): keep an accurate clock without the internet.
 *
 * Every computer clock drifts. Each time you can check it against something better (a GPS fix, a
 * radio time signal, another station), record the pair. From those checks this works out how
 * fast the clock gains or loses and gives a corrected time in between.
 */

export type TimeCheck = {
  /** What this computer's clock said, Unix milliseconds. */
  systemMs: number;
  /** What the trusted source said at that moment, Unix milliseconds. */
  referenceMs: number;
  /** Where the reference came from, for the record. */
  source?: string;
};

export type ClockModel = {
  /** Reference minus system at the latest check, in seconds. Positive means the clock is slow. */
  offsetSeconds: number;
  /** Seconds the clock loses (positive) or gains (negative) per day. Null with a single check. */
  driftSecondsPerDay: number | null;
  /** System time of the latest check. */
  lastCheckMs: number;
  checks: number;
};

const DAY_MS = 86_400_000;

/** Fit offset and drift to the recorded checks. Returns null when there are none. */
export function fitClock(checks: TimeCheck[]): ClockModel | null {
  const valid = checks
    .filter((c) => Number.isFinite(c.systemMs) && Number.isFinite(c.referenceMs))
    .sort((a, b) => a.systemMs - b.systemMs);
  if (valid.length === 0) return null;
  const last = valid[valid.length - 1]!;
  const offsetSeconds = (last.referenceMs - last.systemMs) / 1000;
  const first = valid[0]!;
  // Drift needs checks at least an hour apart to mean anything.
  if (valid.length < 2 || last.systemMs - first.systemMs < 3_600_000) {
    return { offsetSeconds, driftSecondsPerDay: null, lastCheckMs: last.systemMs, checks: valid.length };
  }
  const xs = valid.map((c) => (c.systemMs - first.systemMs) / DAY_MS);
  const ys = valid.map((c) => (c.referenceMs - c.systemMs) / 1000);
  const mx = xs.reduce((a, b) => a + b, 0) / xs.length;
  const my = ys.reduce((a, b) => a + b, 0) / ys.length;
  let num = 0;
  let den = 0;
  for (let i = 0; i < xs.length; i++) {
    num += (xs[i]! - mx) * (ys[i]! - my);
    den += (xs[i]! - mx) ** 2;
  }
  return {
    offsetSeconds,
    driftSecondsPerDay: den > 0 ? num / den : null,
    lastCheckMs: last.systemMs,
    checks: valid.length,
  };
}

/** Best estimate of the true time for a given system clock reading. */
export function correctedTime(model: ClockModel, systemMs: number): number {
  const sinceCheckDays = (systemMs - model.lastCheckMs) / DAY_MS;
  const drift = model.driftSecondsPerDay ?? 0;
  return systemMs + (model.offsetSeconds + drift * sinceCheckDays) * 1000;
}

/**
 * How far the corrected time could be off, in seconds: it grows with time since the last check.
 * With no measured drift, assumes a typical uncorrected computer clock (about 2 s a day).
 */
export function uncertaintySeconds(model: ClockModel, systemMs: number): number {
  const days = Math.abs(systemMs - model.lastCheckMs) / DAY_MS;
  // A fitted drift still wanders with temperature; allow a quarter of it, at least 0.2 s a day.
  const perDay = model.driftSecondsPerDay == null ? 2 : Math.max(0.2, Math.abs(model.driftSecondsPerDay) * 0.25);
  return 1 + perDay * days;
}
