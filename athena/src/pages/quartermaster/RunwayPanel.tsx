import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";

type RunwayStatus = "out" | "critical" | "low" | "ok" | "unknown";

type ItemRunway = {
  id: string;
  name: string;
  category: string;
  division: string;
  quantity: number;
  dailyUse: number | null;
  daysLeft: number | null;
  runsOutAt: number | null;
  status: RunwayStatus;
  useEvents: number;
};

type RunwayReport = {
  items: ItemRunway[];
  windowDays: number;
  headcount: number;
  baselineHeadcount: number;
  outCount: number;
  criticalCount: number;
  lowCount: number;
  unknownCount: number;
};

const STATUS_LABEL: Record<RunwayStatus, string> = {
  out: "Out",
  critical: "Under a week",
  low: "Under a month",
  ok: "OK",
  unknown: "No estimate yet",
};

function formatDays(d: number | null): string {
  if (d == null) return "—";
  if (d < 1) return "less than a day";
  if (d < 14) return `${Math.round(d)} day${Math.round(d) === 1 ? "" : "s"}`;
  if (d < 90) return `${Math.round(d / 7)} weeks`;
  return `${Math.round(d / 30)} months`;
}

function formatDate(unixSeconds: number | null): string {
  if (unixSeconds == null) return "—";
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function formatRate(perDay: number | null): string {
  if (perDay == null) return "—";
  if (perDay >= 1) return `${perDay.toFixed(1)} / day`;
  return `${(perDay * 7).toFixed(1)} / week`;
}

/**
 * Supply runway: how long each stocked item lasts at the rate it is actually being used, and what
 * runs out first. Estimates come from the inventory log, so they need no setup.
 */
export function RunwayPanel() {
  const [report, setReport] = useState<RunwayReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [extraPeople, setExtraPeople] = useState(0);
  const [showAll, setShowAll] = useState(false);

  const load = useCallback(async (extra: number) => {
    setLoading(true);
    try {
      const r = await invoke<RunwayReport>("get_supply_runway", { extraPeople: extra });
      setReport(r);
      setError(null);
    } catch (e) {
      setError(e != null ? String(e) : "Could not load the supply runway.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(extraPeople);
  }, [load, extraPeople]);

  const estimated = report?.items.filter((i) => i.status !== "unknown") ?? [];
  const urgent = estimated.filter((i) => i.status !== "ok");
  const rows = showAll ? estimated : urgent.length > 0 ? urgent : estimated.slice(0, 5);
  const baseline = report?.baselineHeadcount ?? 1;

  return (
    <section className="library-section glass-panel" aria-label="Supply runway" style={{ display: "grid", gap: "0.75rem" }}>
      <div style={{ display: "flex", justifyContent: "space-between", gap: "1rem", flexWrap: "wrap", alignItems: "baseline" }}>
        <div>
          <h2 className="quartermaster-category-title" style={{ margin: 0 }}>
            Supply runway
          </h2>
          <p className="text-muted small" style={{ margin: "0.25rem 0 0" }}>
            How long each item lasts at the rate it has been used over the last {report?.windowDays ?? 30} days.
          </p>
        </div>
        <label className="small" style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
          Plan for
          <input
            type="number"
            min={1}
            max={999}
            value={baseline + extraPeople}
            onChange={(e) => {
              const n = Number.parseInt(e.target.value, 10);
              if (Number.isFinite(n) && n >= 1) setExtraPeople(n - baseline);
            }}
            className="map-chat-input"
            style={{ width: "5rem" }}
            aria-label="Number of people to plan for"
          />
          people
          {extraPeople !== 0 ? (
            <button type="button" className="secondary" onClick={() => setExtraPeople(0)}>
              Reset to {baseline}
            </button>
          ) : null}
        </label>
      </div>

      {error ? (
        <p className="small" role="alert">
          {error}
        </p>
      ) : null}

      {report && !error ? (
        <>
          <p className="small" style={{ margin: 0 }} aria-live="polite">
            {report.outCount + report.criticalCount + report.lowCount === 0
              ? estimated.length === 0
                ? "No estimates yet."
                : "Nothing is projected to run out within a month."
              : [
                  report.outCount > 0 ? `${report.outCount} out` : null,
                  report.criticalCount > 0 ? `${report.criticalCount} under a week` : null,
                  report.lowCount > 0 ? `${report.lowCount} under a month` : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
            {extraPeople !== 0 ? ` (scaled from ${baseline} to ${report.headcount} people)` : ""}
          </p>

          {rows.length > 0 ? (
            <div className="quartermaster-table-wrap">
              <table className="quartermaster-table">
                <thead>
                  <tr>
                    <th>Item</th>
                    <th>On hand</th>
                    <th>Use</th>
                    <th>Lasts</th>
                    <th>Runs out</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((i) => (
                    <tr key={i.id}>
                      <td>{i.name}</td>
                      <td>{i.quantity}</td>
                      <td>{formatRate(i.dailyUse)}</td>
                      <td>{i.status === "out" ? "—" : formatDays(i.daysLeft)}</td>
                      <td>{i.status === "out" ? "—" : formatDate(i.runsOutAt)}</td>
                      <td>
                        <strong>{STATUS_LABEL[i.status]}</strong>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap", alignItems: "center" }}>
            {estimated.length > rows.length || showAll ? (
              <button type="button" className="secondary" onClick={() => setShowAll((v) => !v)}>
                {showAll ? "Show only what is running low" : `Show all ${estimated.length} estimated items`}
              </button>
            ) : null}
            <button type="button" className="secondary" onClick={() => void load(extraPeople)} disabled={loading}>
              {loading ? "Refreshing…" : "Refresh"}
            </button>
          </div>

          {report.unknownCount > 0 ? (
            <p className="text-muted small" style={{ margin: 0 }}>
              {report.unknownCount} item{report.unknownCount === 1 ? " has" : "s have"} no estimate yet. An estimate
              appears once an item's quantity has been reduced at least twice, or once more than a week ago.
            </p>
          ) : null}
        </>
      ) : null}

      {loading && !report && !error ? <p className="text-muted small">Loading…</p> : null}
    </section>
  );
}
