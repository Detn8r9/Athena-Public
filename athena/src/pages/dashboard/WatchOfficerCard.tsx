import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import type { AppView } from "../../app/appView";

type Severity = "critical" | "warning" | "info";

type Finding = {
  id: string;
  severity: Severity;
  area: string;
  title: string;
  detail: string;
  goTo: AppView | null;
};

type WatchBrief = {
  generatedAt: number;
  headline: string;
  findings: Finding[];
  aiSummary: string | null;
};

const SEVERITY_LABEL: Record<Severity, string> = {
  critical: "Act now",
  warning: "Watch",
  info: "Today",
};

/** How often the rules are re-checked while the dashboard is open. They cost no model time. */
const REFRESH_MS = 5 * 60_000;

/**
 * Watch officer: what needs attention across the station, most urgent first.
 *
 * Findings come from plain rules in the backend and never depend on the AI model. The model is
 * only used, on request, to write a short brief from those findings. Nothing here takes action.
 */
export function WatchOfficerCard({ onNavigate }: { onNavigate: (view: AppView) => void }) {
  const [brief, setBrief] = useState<WatchBrief | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [writing, setWriting] = useState(false);

  const load = useCallback(async (withAi: boolean) => {
    if (withAi) setWriting(true);
    try {
      const next = await invoke<WatchBrief>("get_watch_brief", { withAi });
      // A rules-only refresh must not erase a brief the model already wrote for the same findings.
      setBrief((prev) =>
        !withAi && prev?.aiSummary && sameFindings(prev.findings, next.findings)
          ? { ...next, aiSummary: prev.aiSummary }
          : next,
      );
      setError(null);
    } catch (e) {
      setError(e != null ? String(e) : "Could not load the watch brief.");
    } finally {
      if (withAi) setWriting(false);
    }
  }, []);

  useEffect(() => {
    void load(false);
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void load(false);
    }, REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const findings = brief?.findings ?? [];

  return (
    <section
      className="glass-panel dashboard-widget-sidebar__block"
      aria-label="Watch officer"
      style={{ padding: "0.7rem 0.85rem", display: "grid", gap: "0.55rem" }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem", flexWrap: "wrap", alignItems: "baseline" }}>
        <div>
          <div style={{ color: "var(--primary)", fontWeight: 800, letterSpacing: "0.06em", textTransform: "uppercase", fontSize: "0.72rem" }}>
            Watch officer
          </div>
          <div style={{ fontWeight: 700 }} aria-live="polite">
            {error ? "Watch brief unavailable" : brief ? brief.headline : "Checking the station…"}
          </div>
        </div>
        <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
          {findings.length > 0 ? (
            <button type="button" className="secondary" onClick={() => void load(true)} disabled={writing}>
              {writing ? "Writing brief…" : brief?.aiSummary ? "Rewrite brief" : "Write me a brief"}
            </button>
          ) : null}
          <button type="button" className="secondary" onClick={() => void load(false)}>
            Check now
          </button>
        </div>
      </div>

      {error ? (
        <p className="small" role="alert" style={{ margin: 0 }}>
          {error}
        </p>
      ) : null}

      {brief?.aiSummary ? (
        <p className="small" style={{ margin: 0, whiteSpace: "pre-wrap" }}>
          {brief.aiSummary}
          <span className="text-muted"> (Written by the AI from the items below.)</span>
        </p>
      ) : null}

      {findings.length > 0 ? (
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.4rem" }}>
          {findings.map((f) => (
            <li
              key={f.id}
              className="glass-panel"
              style={{ padding: "0.5rem 0.6rem", display: "flex", gap: "0.75rem", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap" }}
            >
              <div style={{ display: "grid", gap: 2, minWidth: 0, flex: "1 1 16rem" }}>
                <span style={{ fontWeight: 700, fontSize: "0.88rem" }}>
                  <span className="text-muted" style={{ fontWeight: 800, textTransform: "uppercase", fontSize: "0.68rem", letterSpacing: "0.06em" }}>
                    {SEVERITY_LABEL[f.severity]}
                  </span>{" "}
                  {f.title}
                </span>
                <span className="text-muted small" style={{ wordBreak: "break-word" }}>
                  {f.detail}
                </span>
              </div>
              {f.goTo ? (
                <button type="button" className="secondary" onClick={() => onNavigate(f.goTo as AppView)}>
                  Open
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}

function sameFindings(a: Finding[], b: Finding[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((f, i) => f.id === b[i]!.id && f.title === b[i]!.title && f.detail === b[i]!.detail);
}
