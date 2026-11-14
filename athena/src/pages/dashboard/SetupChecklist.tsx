import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import type { DashboardDestination } from "./DashboardView";
import { buildSetupSteps, setupComplete, type SetupSignals, type SetupStep } from "./setupSteps";

const HIDDEN_KEY = "athena-setup-checklist-hidden";

/** Read by screen readers, not shown (the ✓ mark is decorative). */
const VISUALLY_HIDDEN = {
  position: "absolute",
  width: 1,
  height: 1,
  overflow: "hidden",
  clip: "rect(0 0 0 0)",
  whiteSpace: "nowrap",
} as const;

function readHidden(): boolean {
  try {
    return localStorage.getItem(HIDDEN_KEY) === "1";
  } catch {
    return false;
  }
}

async function countOf(command: string): Promise<number | null> {
  try {
    const rows = await invoke<unknown>(command);
    return Array.isArray(rows) ? rows.length : null;
  } catch {
    return null;
  }
}

async function loadSignals(): Promise<SetupSignals> {
  const [brief, libraryFiles, maps, people, inventoryItems, hardware] = await Promise.all([
    invoke<{ findings: Array<{ id: string }> }>("get_watch_brief", { withAi: false }).catch(() => null),
    countOf("list_library_contents"),
    countOf("get_downloaded_maps"),
    countOf("list_personnel"),
    countOf("list_inventory"),
    invoke<{ meshData?: boolean }>("check_hardware_status").catch(() => null),
  ]);
  return {
    aiReady: brief ? !brief.findings.some((f) => f.id === "station.ai") : null,
    libraryFiles,
    maps,
    people,
    inventoryItems,
    radioConnected: hardware ? hardware.meshData === true : null,
  };
}

/**
 * First-run "Get set up" list on Home: the few things a new station needs, each with a button
 * to the screen that does it. Ticks itself off from live station data; hides when finished or dismissed.
 */
export function SetupChecklist({ rank, onNavigate }: { rank: number; onNavigate: (d: DashboardDestination) => void }) {
  const [hidden, setHidden] = useState(readHidden);
  const [steps, setSteps] = useState<SetupStep[] | null>(null);

  const refresh = useCallback(async () => {
    const signals = await loadSignals();
    setSteps(buildSetupSteps(signals, rank));
  }, [rank]);

  useEffect(() => {
    if (hidden) return;
    void refresh();
    // Re-check when the user comes back to the window (they may have just finished a step elsewhere).
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [hidden, refresh]);

  if (hidden || !steps || setupComplete(steps)) return null;

  const required = steps.filter((s) => !s.optional);
  const doneCount = required.filter((s) => s.done).length;
  const nextId = steps.find((s) => !s.done)?.id;

  const hide = () => {
    setHidden(true);
    try {
      localStorage.setItem(HIDDEN_KEY, "1");
    } catch {
      /* storage unavailable */
    }
  };

  return (
    <section
      className="glass-panel setup-checklist"
      aria-label="Get set up"
      style={{ padding: "0.7rem 0.85rem", display: "grid", gap: "0.55rem", marginBottom: "0.6rem" }}
    >
      <div className="setup-checklist__head" style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: "0.75rem", flexWrap: "wrap" }}>
        <div>
          <h2 style={{ margin: 0, fontSize: "1.05rem" }}>Get set up</h2>
          <span className="setup-checklist__progress text-muted small">
            {doneCount} of {required.length} done. Work down the list; each step opens the screen that does it.
          </span>
        </div>
        <button type="button" className="secondary" onClick={hide} title="Hide this list. Help (?) explains every screen.">
          Hide
        </button>
      </div>
      <ol className="setup-checklist__list" style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.4rem" }}>
        {steps.map((s, i) => (
          <li
            key={s.id}
            className="setup-checklist__item"
            data-done={s.done ? "true" : "false"}
            aria-current={s.id === nextId ? "step" : undefined}
            style={{ display: "flex", gap: "0.65rem", alignItems: "center", flexWrap: "wrap", opacity: s.done ? 0.6 : 1 }}
          >
            <span className="setup-checklist__mark" aria-hidden style={{ width: "1.4rem", textAlign: "center", fontWeight: 800 }}>
              {s.done ? "✓" : i + 1}
            </span>
            <div className="setup-checklist__text" style={{ display: "grid", gap: 2, flex: "1 1 14rem", minWidth: 0 }}>
              <strong style={{ fontSize: "0.9rem" }}>
                {s.title}
                {s.optional ? <span className="text-muted" style={{ fontWeight: 400 }}> (optional)</span> : null}
                {s.done ? <span style={VISUALLY_HIDDEN}> (done)</span> : null}
              </strong>
              {s.done ? null : <span className="text-muted small">{s.detail}</span>}
            </div>
            {s.done ? null : (
              <button type="button" className="secondary setup-checklist__go" onClick={() => onNavigate(s.goTo)}>
                {s.goLabel}
              </button>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
