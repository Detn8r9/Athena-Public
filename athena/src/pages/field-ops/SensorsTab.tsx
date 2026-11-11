import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, formatSpan, runAction, useCommand, type Stored } from "./ui";

type SensorState = "no_data" | "stale" | "low" | "high" | "ok";

type SensorRow = Stored<{
  name: string;
  metric: string;
  unit: string;
  nodeId: number | null;
  minAlert: number | null;
  maxAlert: number | null;
  staleAfterMinutes: number | null;
  urgent: boolean;
  state: SensorState;
  latestAt: number | null;
  latestValue: number | null;
  recent: number[];
}>;

type Freshness = "fresh" | "ageing" | "stale";

type PictureMarker = {
  id: string;
  lat: number;
  lng: number;
  title: string;
  category: string;
  createdUnix: number;
  freshness: Freshness;
  fromRadio: boolean;
};

const STATE_LABEL: Record<SensorState, string> = {
  no_data: "No readings",
  stale: "Gone quiet",
  low: "Too low",
  high: "Too high",
  ok: "In range",
};

const STATE_TONE: Record<SensorState, "bad" | "warn" | "good" | undefined> = {
  no_data: undefined,
  stale: "warn",
  low: "bad",
  high: "bad",
  ok: "good",
};

/** Tiny trend line drawn from the recent readings. Uses the current text colour. */
function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return null;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const w = 90;
  const h = 22;
  const points = values
    .map((v, i) => `${((i / (values.length - 1)) * w).toFixed(1)},${(h - 2 - ((v - min) / span) * (h - 4)).toFixed(1)}`)
    .join(" ");
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} role="img" aria-label={`Trend over the last ${values.length} readings`}>
      <polyline points={points} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
    </svg>
  );
}

function numOrNull(s: string): number | null {
  const n = Number.parseFloat(s);
  return s.trim() && Number.isFinite(n) ? n : null;
}

function SensorsSection() {
  const { data, error, loading, reload } = useCommand<SensorRow[]>("list_sensors");
  const [actionError, setActionError] = useState<string | null>(null);
  const [form, setForm] = useState({ name: "", unit: "", min: "", max: "", stale: "", urgent: false });
  const [manual, setManual] = useState<Record<string, string>>({});

  const rows = data ?? [];
  const now = Date.now() / 1000;

  const limits = (r: SensorRow) =>
    [
      r.minAlert != null ? `low ${r.minAlert}` : null,
      r.maxAlert != null ? `high ${r.maxAlert}` : null,
      r.staleAfterMinutes ? `quiet after ${r.staleAfterMinutes} min` : null,
    ]
      .filter(Boolean)
      .join(", ") || "No limits set";

  const editLimits = (r: SensorRow) => {
    const min = window.prompt(`Low limit for ${r.name} (${r.unit}). Leave blank for none.`, r.minAlert?.toString() ?? "");
    if (min == null) return;
    const max = window.prompt(`High limit for ${r.name} (${r.unit}). Leave blank for none.`, r.maxAlert?.toString() ?? "");
    if (max == null) return;
    const stale = window.prompt("Raise an alert when nothing is heard for how many minutes? Leave blank for never.", r.staleAfterMinutes?.toString() ?? "");
    if (stale == null) return;
    void runAction(
      () =>
        invoke("save_sensor", {
          id: r.id,
          sensor: {
            name: r.name,
            metric: r.metric,
            unit: r.unit,
            nodeId: r.nodeId,
            minAlert: numOrNull(min),
            maxAlert: numOrNull(max),
            staleAfterMinutes: numOrNull(stale),
            urgent: r.urgent,
          },
        }),
      setActionError,
      reload,
    );
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Sensors">
      <h2>Sensors</h2>
      <p className="fo-lead">
        Anything that gives a number over time: a water tank, a soil probe, a Geiger counter, a radio's battery.
        Meshtastic nodes that report telemetry appear here on their own. Set limits, and the Dashboard tells you when
        one is crossed or a sensor goes quiet.
      </p>
      <ErrorNote message={error ?? actionError} />

      {rows.length === 0 ? (
        <p className="fo-empty">{loading ? "Loading…" : "No sensors yet. Add one below, or wait for a radio node to report."}</p>
      ) : (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Sensor</th>
                <th>Latest</th>
                <th>Trend</th>
                <th>Status</th>
                <th>Limits</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id}>
                  <td>
                    {r.name}
                    <div className="fo-muted fo-small">
                      {r.nodeId != null ? `Radio node ${r.nodeId}` : "Entered by hand"}
                      {r.urgent ? " · urgent" : ""}
                    </div>
                  </td>
                  <td>
                    {r.latestValue != null ? `${+r.latestValue.toFixed(2)} ${r.unit}` : "—"}
                    {r.latestAt ? <div className="fo-muted fo-small">{formatSpan(now - r.latestAt)} ago</div> : null}
                  </td>
                  <td>
                    <Sparkline values={r.recent} />
                  </td>
                  <td>
                    <Badge tone={STATE_TONE[r.state]}>{STATE_LABEL[r.state]}</Badge>
                  </td>
                  <td className="fo-small">{limits(r)}</td>
                  <td>
                    <div className="fo-actions">
                      <input
                        className="fo-input fo-input--narrow"
                        inputMode="decimal"
                        placeholder="Reading"
                        aria-label={`New reading for ${r.name}`}
                        value={manual[r.id] ?? ""}
                        onChange={(e) => setManual((m) => ({ ...m, [r.id]: e.target.value }))}
                      />
                      <button
                        type="button"
                        className="secondary"
                        disabled={numOrNull(manual[r.id] ?? "") == null}
                        onClick={() =>
                          void runAction(
                            () => invoke("add_sensor_reading", { sensorId: r.id, value: numOrNull(manual[r.id] ?? "") }),
                            setActionError,
                            async () => {
                              setManual((m) => ({ ...m, [r.id]: "" }));
                              await reload();
                            },
                          )
                        }
                      >
                        Add
                      </button>
                      <button type="button" className="secondary" onClick={() => editLimits(r)}>
                        Limits
                      </button>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          if (window.confirm(`Remove ${r.name} and its readings?`)) {
                            void runAction(() => invoke("delete_sensor", { id: r.id }), setActionError, reload);
                          }
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3>Add a sensor</h3>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void runAction(
            () =>
              invoke("save_sensor", {
                sensor: {
                  name: form.name,
                  metric: "value",
                  unit: form.unit,
                  nodeId: null,
                  minAlert: numOrNull(form.min),
                  maxAlert: numOrNull(form.max),
                  staleAfterMinutes: numOrNull(form.stale),
                  urgent: form.urgent,
                },
              }),
            setActionError,
            async () => {
              setForm({ name: "", unit: "", min: "", max: "", stale: "", urgent: false });
              await reload();
            },
          );
        }}
      >
        <Field label="Name" grow>
          <input className="fo-input" value={form.name} onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))} placeholder="Main water tank" />
        </Field>
        <Field label="Unit">
          <input className="fo-input fo-input--narrow" value={form.unit} onChange={(e) => setForm((f) => ({ ...f, unit: e.target.value }))} placeholder="%" />
        </Field>
        <Field label="Low limit">
          <input className="fo-input fo-input--narrow" inputMode="decimal" value={form.min} onChange={(e) => setForm((f) => ({ ...f, min: e.target.value }))} />
        </Field>
        <Field label="High limit">
          <input className="fo-input fo-input--narrow" inputMode="decimal" value={form.max} onChange={(e) => setForm((f) => ({ ...f, max: e.target.value }))} />
        </Field>
        <Field label="Quiet after (min)">
          <input className="fo-input fo-input--narrow" inputMode="numeric" value={form.stale} onChange={(e) => setForm((f) => ({ ...f, stale: e.target.value }))} />
        </Field>
        <label className="fo-small" style={{ display: "flex", gap: "0.35rem", alignItems: "center" }}>
          <input type="checkbox" checked={form.urgent} onChange={(e) => setForm((f) => ({ ...f, urgent: e.target.checked }))} />
          Urgent when out of range
        </label>
        <button type="submit" className="secondary" disabled={!form.name.trim()}>
          Add sensor
        </button>
      </form>
    </section>
  );
}

const FRESH_LABEL: Record<Freshness, string> = { fresh: "Current", ageing: "Confirm before relying on it", stale: "Probably out of date" };
const FRESH_TONE: Record<Freshness, "good" | "warn" | "bad"> = { fresh: "good", ageing: "warn", stale: "bad" };

function PictureSection() {
  const { data, error, loading, reload } = useCommand<PictureMarker[]>("list_shared_picture");
  const [actionError, setActionError] = useState<string | null>(null);
  const [sent, setSent] = useState<string | null>(null);
  const markers = data ?? [];

  return (
    <section className="glass-panel fo-panel" aria-label="Map reports">
      <div className="fo-card-head">
        <h2>Map reports</h2>
        <button type="button" className="secondary" onClick={() => void reload()}>
          Refresh
        </button>
      </div>
      <p className="fo-lead">
        Every marker on the map, newest first, with how far it can still be trusted: a hazard goes out of date in
        days, a water source in months. Send a marker over the mesh and it appears on every station's map. Markers are
        added on the Map screen.
      </p>
      <ErrorNote message={error ?? actionError} />
      {sent ? <p className="fo-small">{sent}</p> : null}
      {markers.length === 0 ? (
        <p className="fo-empty">{loading ? "Loading…" : "No markers on the map yet."}</p>
      ) : (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Report</th>
                <th>Type</th>
                <th>Where</th>
                <th>Reported</th>
                <th>Trust</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {markers.map((m) => (
                <tr key={m.id}>
                  <td>
                    {m.title || "Untitled"}
                    {m.fromRadio ? <div className="fo-muted fo-small">Received by radio</div> : null}
                  </td>
                  <td>{m.category}</td>
                  <td className="fo-small">
                    {m.lat.toFixed(5)}, {m.lng.toFixed(5)}
                  </td>
                  <td className="fo-small">{m.createdUnix > 0 ? formatDateTime(m.createdUnix) : "—"}</td>
                  <td>
                    <Badge tone={FRESH_TONE[m.freshness]}>{FRESH_LABEL[m.freshness]}</Badge>
                  </td>
                  <td>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() =>
                        void runAction(
                          () => invoke("share_marker_over_mesh", { id: m.id }),
                          (msg) => {
                            setActionError(msg);
                            if (msg) setSent(null);
                          },
                          () => setSent(`Sent "${m.title || "marker"}" to the mesh.`),
                        )
                      }
                    >
                      Send over mesh
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function SensorsTab() {
  return (
    <div className="fo-cards">
      <SensorsSection />
      <PictureSection />
    </div>
  );
}
