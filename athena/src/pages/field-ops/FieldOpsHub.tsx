import { lazy, Suspense, useState } from "react";
import "./fieldOps.css";

const ProtocolsTab = lazy(() => import("./ProtocolsTab").then((m) => ({ default: m.ProtocolsTab })));
const PatientsTab = lazy(() => import("./PatientsTab").then((m) => ({ default: m.PatientsTab })));
const SuppliesTab = lazy(() => import("./SuppliesTab").then((m) => ({ default: m.SuppliesTab })));
const ToolsTab = lazy(() => import("./ToolsTab").then((m) => ({ default: m.ToolsTab })));
const RadioPlanTab = lazy(() => import("./RadioPlanTab").then((m) => ({ default: m.RadioPlanTab })));
const StationsTab = lazy(() => import("./StationsTab").then((m) => ({ default: m.StationsTab })));
const CheckinsTab = lazy(() => import("./CheckinsTab").then((m) => ({ default: m.CheckinsTab })));
const RosterTab = lazy(() => import("./RosterTab").then((m) => ({ default: m.RosterTab })));
const SkillsTab = lazy(() => import("./SkillsTab").then((m) => ({ default: m.SkillsTab })));
const IntakeTab = lazy(() => import("./IntakeTab").then((m) => ({ default: m.IntakeTab })));
const MemoryTab = lazy(() => import("./MemoryTab").then((m) => ({ default: m.MemoryTab })));
const DecisionsTab = lazy(() => import("./DecisionsTab").then((m) => ({ default: m.DecisionsTab })));
const LedgerTab = lazy(() => import("./LedgerTab").then((m) => ({ default: m.LedgerTab })));

const TABS = [
  { id: "protocols", label: "Protocols", Component: ProtocolsTab },
  { id: "patients", label: "Patients", Component: PatientsTab },
  { id: "supplies", label: "Supplies & power", Component: SuppliesTab },
  { id: "checkins", label: "Check-ins", Component: CheckinsTab },
  { id: "roster", label: "Duty roster", Component: RosterTab },
  { id: "skills", label: "Skills", Component: SkillsTab },
  { id: "intake", label: "Newcomers", Component: IntakeTab },
  { id: "memory", label: "Group memory", Component: MemoryTab },
  { id: "decisions", label: "Decisions", Component: DecisionsTab },
  { id: "ledger", label: "Ledger", Component: LedgerTab },
  { id: "radio", label: "Radio plan", Component: RadioPlanTab },
  { id: "stations", label: "Other stations", Component: StationsTab },
  { id: "tools", label: "Field tools", Component: ToolsTab },
] as const;

type TabId = (typeof TABS)[number]["id"];

const STORAGE_KEY = "athena-field-ops-tab";

function initialTab(): TabId {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && TABS.some((t) => t.id === saved)) return saved as TabId;
  } catch {
    /* storage unavailable */
  }
  return "checkins";
}

/** Field Ops: the tools a group uses to coordinate people day to day. */
export function FieldOpsHub() {
  const [tab, setTab] = useState<TabId>(initialTab);
  const active = TABS.find((t) => t.id === tab) ?? TABS[0];
  const Active = active.Component;

  const choose = (id: TabId) => {
    setTab(id);
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {
      /* storage unavailable */
    }
  };

  return (
    <div className="home-view-scroll">
      <div className="fo-hub">
        <header className="fo-header">
          <h1>Field Ops</h1>
          <p>Who is where, who does what, and what the group knows and has agreed.</p>
        </header>
        <div className="glass-panel fo-tabs" role="tablist" aria-label="Field Ops sections">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              id={`fo-tab-${t.id}`}
              aria-selected={t.id === tab}
              aria-controls="fo-tabpanel"
              className="fo-tab"
              onClick={() => choose(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div id="fo-tabpanel" role="tabpanel" aria-labelledby={`fo-tab-${tab}`}>
          <Suspense fallback={<p className="fo-empty">Loading…</p>}>
            <Active />
          </Suspense>
        </div>
      </div>
    </div>
  );
}
