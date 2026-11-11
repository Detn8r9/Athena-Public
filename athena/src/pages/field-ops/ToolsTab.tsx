import { useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, runAction, useCommand } from "./ui";
import {
  bearingDegrees,
  compassPoint,
  deadReckon,
  distanceMeters,
  formatDistance,
  formatLatLon,
  metersFromPaces,
  parseLatLon,
  sunPosition,
  trueToMagnetic,
  type Leg,
} from "./tools/geo";
import { assessPoint, downwindBearing, escapeBearings, toMetersPerSecond } from "./tools/hazard";
import { correctedTime, fitClock, uncertaintySeconds, type TimeCheck } from "./tools/timekeeping";
import { outlook, seaLevelPressure, type PressureReading } from "./tools/weather";

type Stored<V> = { id: string; createdAt: number; updatedAt: number; value: V };

function bearingText(b: number): string {
  return `${Math.round(b)}° (${compassPoint(b)})`;
}

function NavigationTool() {
  const [start, setStart] = useState("");
  const [declination, setDeclination] = useState("0");
  const [legs, setLegs] = useState<Leg[]>([]);
  const [bearing, setBearing] = useState("");
  const [distance, setDistance] = useState("");
  const [unit, setUnit] = useState<"m" | "paces">("m");
  const [paceLength, setPaceLength] = useState("1.5");
  const [target, setTarget] = useState("");

  const startPos = parseLatLon(start);
  const decl = Number.parseFloat(declination) || 0;
  const reckoned = startPos && legs.length > 0 ? deadReckon(startPos, legs, decl) : null;
  const here = reckoned?.position ?? startPos;
  const targetPos = parseLatLon(target);
  const sun = here ? sunPosition(new Date(), here) : null;

  const addLeg = () => {
    const b = Number.parseFloat(bearing);
    const d = Number.parseFloat(distance);
    if (!Number.isFinite(b) || !(d > 0)) return;
    const meters = unit === "paces" ? metersFromPaces(d, Number.parseFloat(paceLength) || 0) : d;
    if (!(meters > 0)) return;
    setLegs((l) => [...l, { bearing: b, meters }]);
    setBearing("");
    setDistance("");
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Navigation without GPS">
      <h2>Navigation without GPS</h2>
      <p className="fo-lead">
        Start from a place you know, enter each stretch you walk as a compass bearing and a distance, and ATHENA keeps
        track of where you are and the bearing home. Nothing here needs a satellite.
      </p>
      <div className="fo-row">
        <Field label="Start position (latitude, longitude)" grow>
          <input className="fo-input" value={start} onChange={(e) => setStart(e.target.value)} placeholder="44.4280, -110.5885" />
        </Field>
        <Field label="Declination (° east; west is negative)">
          <input className="fo-input fo-input--narrow" type="number" step="any" value={declination} onChange={(e) => setDeclination(e.target.value)} />
        </Field>
      </div>
      {start.trim() && !startPos ? <p className="fo-error">Enter the start as two numbers, for example 44.4280, -110.5885.</p> : null}

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          addLeg();
        }}
      >
        <Field label="Compass bearing (°)">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={360} step="any" value={bearing} onChange={(e) => setBearing(e.target.value)} />
        </Field>
        <Field label="Distance">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={distance} onChange={(e) => setDistance(e.target.value)} />
        </Field>
        <Field label="In">
          <select value={unit} onChange={(e) => setUnit(e.target.value as "m" | "paces")}>
            <option value="m">metres</option>
            <option value="paces">paces</option>
          </select>
        </Field>
        {unit === "paces" ? (
          <Field label="Metres per pace (double step)">
            <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={paceLength} onChange={(e) => setPaceLength(e.target.value)} />
          </Field>
        ) : null}
        <button type="submit" className="secondary" disabled={!startPos}>
          Add leg
        </button>
        {legs.length > 0 ? (
          <>
            <button type="button" className="secondary" onClick={() => setLegs((l) => l.slice(0, -1))}>
              Undo last leg
            </button>
            <button type="button" className="secondary" onClick={() => setLegs([])}>
              Clear
            </button>
          </>
        ) : null}
      </form>

      {legs.length > 0 ? (
        <p className="fo-small fo-muted">
          {legs.map((l, i) => `${i + 1}. ${Math.round(l.bearing)}° for ${formatDistance(l.meters)}`).join("   ")}
        </p>
      ) : null}

      {reckoned ? (
        <div className="fo-table-wrap" aria-live="polite">
          <table className="fo-table">
            <tbody>
              <tr>
                <td>Estimated position</td>
                <td>{formatLatLon(reckoned.position)}</td>
              </tr>
              <tr>
                <td>Distance walked</td>
                <td>{formatDistance(reckoned.travelledMeters)}</td>
              </tr>
              <tr>
                <td>Straight line back to start</td>
                <td>
                  {formatDistance(reckoned.metersFromStart)} on compass bearing{" "}
                  {bearingText(trueToMagnetic(reckoned.bearingToStart, decl))}
                </td>
              </tr>
            </tbody>
          </table>
          <p className="fo-small fo-muted">
            Dead reckoning drifts. Expect an error of roughly 5 to 10 percent of the distance walked, and correct it
            whenever you reach a landmark you can identify.
          </p>
        </div>
      ) : null}

      <div className="fo-row">
        <Field label="Bearing and distance to another point (latitude, longitude)" grow>
          <input className="fo-input" value={target} onChange={(e) => setTarget(e.target.value)} />
        </Field>
      </div>
      {here && targetPos ? (
        <p className="fo-small" aria-live="polite">
          From your {reckoned ? "estimated position" : "start"}: {formatDistance(distanceMeters(here, targetPos))} on
          compass bearing {bearingText(trueToMagnetic(bearingDegrees(here, targetPos), decl))}.
        </p>
      ) : null}

      {sun ? (
        <p className="fo-small">
          <strong>Sun as a compass:</strong>{" "}
          {sun.elevation > 0
            ? `right now the sun is at true bearing ${bearingText(sun.azimuth)}, ${Math.round(sun.elevation)}° above the horizon. Face the sun and that is the direction you are looking.`
            : "the sun is below the horizon here right now."}{" "}
          <span className="fo-muted">This uses this computer's clock and the position above.</span>
        </p>
      ) : null}
    </section>
  );
}

function WeatherTool() {
  const { data, error, reload } = useCommand<Array<Stored<{ at: number; hpa: number }>>>("list_field_records", { kind: "pressure_reading" });
  const [actionError, setActionError] = useState<string | null>(null);
  const [pressure, setPressure] = useState("");
  const [altitude, setAltitude] = useState("0");

  const readings: PressureReading[] = useMemo(() => (data ?? []).map((r) => r.value), [data]);
  const forecast = outlook(readings);
  const recent = [...(data ?? [])].sort((a, b) => b.value.at - a.value.at).slice(0, 8);

  const add = () => {
    const station = Number.parseFloat(pressure);
    if (!(station > 300 && station < 1100)) {
      setActionError("Enter the barometer reading in hPa (millibars), between 300 and 1100.");
      return;
    }
    const hpa = seaLevelPressure(station, Number.parseFloat(altitude) || 0);
    return runAction(
      () => invoke("save_field_record", { kind: "pressure_reading", value: { at: Date.now(), hpa: Math.round(hpa * 10) / 10 } }),
      setActionError,
      async () => {
        setPressure("");
        await reload();
      },
    );
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Weather outlook">
      <h2>Weather outlook</h2>
      <p className="fo-lead">
        Enter a barometer reading every hour or two. From how fast the pressure is rising or falling, ATHENA gives an
        outlook for the next 12 to 24 hours. Any cheap barometer or a weather-station sensor will do.
      </p>
      <ErrorNote message={error ?? actionError} />

      {forecast ? (
        <article className="glass-panel fo-card" aria-live="polite">
          <div className="fo-card-head">
            <strong>{forecast.headline}</strong>
            <Badge tone={forecast.warning ? "bad" : "good"}>{forecast.warning ? "Prepare" : "No warning"}</Badge>
          </div>
          <p className="fo-small fo-prewrap">{forecast.detail}</p>
          <p className="fo-small fo-muted">
            {forecast.latestHpa.toFixed(1)} hPa, {forecast.changePer3h >= 0 ? "up" : "down"}{" "}
            {Math.abs(forecast.changePer3h).toFixed(1)} hPa per 3 hours, from {forecast.spanHours.toFixed(1)} hours of
            readings. This is a rule-of-thumb outlook, not a guarantee.
          </p>
        </article>
      ) : (
        <p className="fo-empty">
          {readings.length === 0
            ? "No readings yet."
            : "Add another reading at least an hour after the first to get an outlook."}
        </p>
      )}

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <Field label="Barometer reading (hPa)">
          <input className="fo-input fo-input--narrow" type="number" step="any" value={pressure} onChange={(e) => setPressure(e.target.value)} />
        </Field>
        <Field label="Your height above sea level (m)">
          <input className="fo-input fo-input--narrow" type="number" step="any" value={altitude} onChange={(e) => setAltitude(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!pressure.trim()}>
          Add reading
        </button>
      </form>

      {recent.length > 0 ? (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>When</th>
                <th>Sea-level pressure</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {recent.map((r) => (
                <tr key={r.id}>
                  <td>{formatDateTime(Math.floor(r.value.at / 1000))}</td>
                  <td>{r.value.hpa.toFixed(1)} hPa</td>
                  <td>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => void runAction(() => invoke("delete_field_record", { kind: "pressure_reading", id: r.id }), setActionError, reload)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </section>
  );
}

function HazardTool() {
  const [source, setSource] = useState("");
  const [you, setYou] = useState("");
  const [from, setFrom] = useState("");
  const [speed, setSpeed] = useState("");
  const [unit, setUnit] = useState<"ms" | "kmh" | "knots" | "mph">("kmh");

  const sourcePos = parseLatLon(source);
  const youPos = parseLatLon(you);
  const fromDeg = Number.parseFloat(from);
  const speedMs = toMetersPerSecond(Number.parseFloat(speed) || 0, unit);
  const wind = Number.isFinite(fromDeg) ? { fromDegrees: fromDeg, speedMs } : null;
  const risk = sourcePos && youPos && wind ? assessPoint(sourcePos, wind, youPos) : null;
  const escape = wind ? escapeBearings(wind) : null;

  return (
    <section className="glass-panel fo-panel" aria-label="Downwind hazard">
      <h2>Downwind hazard</h2>
      <p className="fo-lead">
        For smoke, a chemical release or fallout: enter where it is coming from and the wind, and see whether you are
        in its path, how long you have, and which way to move. This is a direction aid. It does not tell you how
        dangerous the hazard is or how far it stays dangerous.
      </p>
      <div className="fo-row">
        <Field label="Where the hazard is (latitude, longitude)" grow>
          <input className="fo-input" value={source} onChange={(e) => setSource(e.target.value)} />
        </Field>
        <Field label="Where you are (latitude, longitude)" grow>
          <input className="fo-input" value={you} onChange={(e) => setYou(e.target.value)} />
        </Field>
      </div>
      <div className="fo-row">
        <Field label="Wind is blowing FROM (° true)">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={360} step="any" value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label="Wind speed">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={speed} onChange={(e) => setSpeed(e.target.value)} />
        </Field>
        <Field label="In">
          <select value={unit} onChange={(e) => setUnit(e.target.value as typeof unit)}>
            <option value="kmh">km/h</option>
            <option value="mph">mph</option>
            <option value="knots">knots</option>
            <option value="ms">m/s</option>
          </select>
        </Field>
      </div>

      {wind && escape ? (
        <article className="glass-panel fo-card" aria-live="polite">
          {risk ? (
            <div className="fo-card-head">
              <strong>
                {risk.inPath
                  ? risk.minutesToArrival != null
                    ? `You are in its path. It could reach you in about ${Math.max(1, Math.round(risk.minutesToArrival))} minutes.`
                    : "You are in its path."
                  : "You are outside the downwind path."}
              </strong>
              <Badge tone={risk.inPath ? "bad" : "good"}>{risk.inPath ? "Move now" : "Stay alert"}</Badge>
            </div>
          ) : null}
          <p className="fo-small">
            The hazard is travelling towards {bearingText(downwindBearing(wind))}. To get out of its path, move
            crosswind: towards {bearingText(escape.crosswind[0])} or {bearingText(escape.crosswind[1])}. Do not move
            downwind.
          </p>
          {risk ? (
            <p className="fo-small fo-muted">
              You are {formatDistance(risk.distanceMeters)} from the source, {Math.round(risk.offAxisDegrees)}° off the
              downwind line. Wind shifts move the path, so keep checking the wind.
            </p>
          ) : null}
        </article>
      ) : (
        <p className="fo-empty">Enter the wind direction to see which way the hazard is heading.</p>
      )}
    </section>
  );
}

function TimeTool() {
  const { data, error, reload } = useCommand<Array<Stored<TimeCheck>>>("list_field_records", { kind: "time_check" });
  const [actionError, setActionError] = useState<string | null>(null);
  const [reference, setReference] = useState("");
  const [sourceName, setSourceName] = useState("GPS");
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);

  const checks = useMemo(() => (data ?? []).map((r) => r.value), [data]);
  const model = fitClock(checks);
  const corrected = model ? correctedTime(model, now) : null;

  const add = () => {
    // Time of day typed as HH:MM:SS, taken to be today in local time.
    const m = /^(\d{1,2}):(\d{2})(?::(\d{2}))?$/.exec(reference.trim());
    if (!m) {
      setActionError("Enter the trusted time as HH:MM:SS, then press the button at that exact moment.");
      return;
    }
    const systemMs = Date.now();
    const ref = new Date(systemMs);
    ref.setHours(Number(m[1]), Number(m[2]), Number(m[3] ?? 0), 0);
    // If the typed time is just past midnight relative to the clock (or vice versa), pick the nearer day.
    let referenceMs = ref.getTime();
    if (referenceMs - systemMs > 12 * 3_600_000) referenceMs -= 86_400_000;
    if (systemMs - referenceMs > 12 * 3_600_000) referenceMs += 86_400_000;
    return runAction(
      () => invoke("save_field_record", { kind: "time_check", value: { systemMs, referenceMs, source: sourceName.trim() || "manual" } }),
      setActionError,
      async () => {
        setReference("");
        await reload();
      },
    );
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Trusted time">
      <h2>Trusted time</h2>
      <p className="fo-lead">
        Computer clocks drift, and radio schedules and check-ins depend on everyone agreeing on the time. Whenever you
        can see a time you trust (a GPS receiver, a radio time signal, another station), type it in and press the
        button at that moment. After two checks a few days apart, ATHENA corrects for this clock's drift.
      </p>
      <ErrorNote message={error ?? actionError} />

      <div className="fo-table-wrap" aria-live="off">
        <table className="fo-table">
          <tbody>
            <tr>
              <td>This computer's clock</td>
              <td>{new Date(now).toLocaleTimeString()}</td>
            </tr>
            <tr>
              <td>Corrected time</td>
              <td>
                {corrected != null && model
                  ? `${new Date(corrected).toLocaleTimeString()} (within about ${Math.round(uncertaintySeconds(model, now))} s)`
                  : "No time checks yet"}
              </td>
            </tr>
            {model ? (
              <tr>
                <td>This clock</td>
                <td>
                  {Math.abs(model.offsetSeconds) < 0.5
                    ? "was right at the last check"
                    : `was ${Math.abs(model.offsetSeconds).toFixed(1)} s ${model.offsetSeconds > 0 ? "slow" : "fast"} at the last check`}
                  {model.driftSecondsPerDay != null
                    ? `, and ${model.driftSecondsPerDay >= 0 ? "loses" : "gains"} ${Math.abs(model.driftSecondsPerDay).toFixed(2)} s a day`
                    : ""}
                  . {model.checks} check{model.checks === 1 ? "" : "s"} recorded.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <Field label="Trusted time (HH:MM:SS, today)">
          <input className="fo-input" value={reference} onChange={(e) => setReference(e.target.value)} placeholder="14:30:00" />
        </Field>
        <Field label="Where it came from">
          <input className="fo-input" value={sourceName} onChange={(e) => setSourceName(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!reference.trim()}>
          Record check now
        </button>
      </form>
      {(data ?? []).length > 0 ? (
        <button
          type="button"
          className="secondary"
          style={{ justifySelf: "start" }}
          onClick={() => {
            if (window.confirm("Remove all recorded time checks?")) {
              void runAction(
                async () => {
                  for (const r of data ?? []) await invoke("delete_field_record", { kind: "time_check", id: r.id });
                },
                setActionError,
                reload,
              );
            }
          }}
        >
          Clear time checks
        </button>
      ) : null}
    </section>
  );
}

export function ToolsTab() {
  return (
    <div className="fo-cards">
      <NavigationTool />
      <WeatherTool />
      <HazardTool />
      <TimeTool />
    </div>
  );
}
