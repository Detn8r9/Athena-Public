import { lazy, Suspense, useState } from "react";
import "./fieldOps.css";
import { useFieldOpsBusy } from "./ui";
import type { CommsSection } from "../comms/CommunicationsHub";

const ProtocolsTab = lazy(() => import("./ProtocolsTab").then((m) => ({ default: m.ProtocolsTab })));
const PatientsTab = lazy(() => import("./PatientsTab").then((m) => ({ default: m.PatientsTab })));
const SuppliesTab = lazy(() => import("./SuppliesTab").then((m) => ({ default: m.SuppliesTab })));
const ToolsTab = lazy(() => import("./ToolsTab").then((m) => ({ default: m.ToolsTab })));
const RadioPlanTab = lazy(() => import("./RadioPlanTab").then((m) => ({ default: m.RadioPlanTab })));
const StationsTab = lazy(() => import("./StationsTab").then((m) => ({ default: m.StationsTab })));
const SensorsTab = lazy(() => import("./SensorsTab").then((m) => ({ default: m.SensorsTab })));
const RadioDataTab = lazy(() => import("./RadioDataTab").then((m) => ({ default: m.RadioDataTab })));
const BinderTab = lazy(() => import("./BinderTab").then((m) => ({ default: m.BinderTab })));
const CheckinsTab = lazy(() => import("./CheckinsTab").then((m) => ({ default: m.CheckinsTab })));
const RosterTab = lazy(() => import("./RosterTab").then((m) => ({ default: m.RosterTab })));
const SkillsTab = lazy(() => import("./SkillsTab").then((m) => ({ default: m.SkillsTab })));
const IntakeTab = lazy(() => import("./IntakeTab").then((m) => ({ default: m.IntakeTab })));
const MemoryTab = lazy(() => import("./MemoryTab").then((m) => ({ default: m.MemoryTab })));
const DecisionsTab = lazy(() => import("./DecisionsTab").then((m) => ({ default: m.DecisionsTab })));
const LedgerTab = lazy(() => import("./LedgerTab").then((m) => ({ default: m.LedgerTab })));

export const TABS = [
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
  { id: "sensors", label: "Sensors & map reports", Component: SensorsTab },
  { id: "radio", label: "Radio plan", Component: RadioPlanTab },
  { id: "radiodata", label: "Radio data & intercepts", Component: RadioDataTab },
  { id: "stations", label: "Other stations", Component: StationsTab },
  { id: "tools", label: "Field tools", Component: ToolsTab },
  { id: "binder", label: "Paper binder", Component: BinderTab },
] as const;

type TabId = (typeof TABS)[number]["id"];

/**
 * Tabs grouped into areas so a newcomer sees a handful of choices, not sixteen.
 * Every tab above belongs to exactly one group (checked by the test in FieldOpsHub.test.ts).
 */
type TabGroup = { id: string; label: string; hint: string; tabs: readonly TabId[] };

export const TAB_GROUPS: ReadonlyArray<TabGroup> = [
  { id: "people", label: "People", hint: "Who is here, who is on duty, and who can do what.", tabs: ["checkins", "roster", "skills", "intake"] },
  { id: "health", label: "Health", hint: "Medical protocols and patient records.", tabs: ["protocols", "patients"] },
  { id: "supplies", label: "Supplies", hint: "Food, water, fuel, power, and who owes what.", tabs: ["supplies", "ledger"] },
  { id: "records", label: "Records", hint: "What the group knows and has agreed, and a printable binder.", tabs: ["memory", "decisions", "binder"] },
  { id: "radio", label: "Radio & sensors", hint: "Radio plans, intercepted traffic, other stations, and sensor reports.", tabs: ["radio", "radiodata", "stations", "sensors"] },
  { id: "tools", label: "Field tools", hint: "Calculators and quick references.", tabs: ["tools"] },
];

/** Tabs whose home is now the Comms screen. Field Ops keeps them only for users who cannot open Comms. */
export const MOVED_TO_COMMS: ReadonlyArray<{ tab: TabId; section: CommsSection }> = [
  { tab: "radio", section: "radio_plan" },
  { tab: "radiodata", section: "radio_data" },
  { tab: "stations", section: "stations" },
];

/** The groups shown: when Comms is available the radio tabs leave and the area becomes just "Sensors". */
export function groupsFor(commsAvailable: boolean): ReadonlyArray<TabGroup> {
  if (!commsAvailable) return TAB_GROUPS;
  const moved = new Set(MOVED_TO_COMMS.map((m) => m.tab));
  return TAB_GROUPS.map((g) =>
    g.id === "radio"
      ? { ...g, label: "Sensors", hint: "Sensor readings and reports pinned to the map.", tabs: g.tabs.filter((t) => !moved.has(t)) }
      : g,
  );
}

function groupOf(groups: ReadonlyArray<TabGroup>, id: TabId) {
  return groups.find((g) => g.tabs.includes(id)) ?? groups[0];
}

const tabLabel = (id: TabId) => TABS.find((t) => t.id === id)?.label ?? id;

const STORAGE_KEY = "athena-field-ops-tab";

function initialTab(groups: ReadonlyArray<TabGroup>): TabId {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && groups.some((g) => g.tabs.includes(saved as TabId))) return saved as TabId;
  } catch {
    /* storage unavailable */
  }
  return "checkins";
}

/** Field Ops: the tools a group uses to coordinate people day to day. */
export function FieldOpsHub({
  onOpenComms,
  initialTab: requestedTab,
}: { onOpenComms?: (section: CommsSection) => void; initialTab?: string | null } = {}) {
  const groups = groupsFor(onOpenComms != null);
  const [tab, setTab] = useState<TabId>(() =>
    requestedTab && groups.some((g) => g.tabs.includes(requestedTab as TabId)) ? (requestedTab as TabId) : initialTab(groups),
  );
  const active = TABS.find((t) => t.id === tab) ?? TABS[0];
  const Active = active.Component;
  const busy = useFieldOpsBusy();
  const group = groupOf(groups, tab);
  // Remember the last tab used in each area so switching areas returns you where you were.
  const [lastInGroup, setLastInGroup] = useState<Record<string, TabId>>({});

  const choose = (id: TabId) => {
    setTab(id);
    setLastInGroup((m) => ({ ...m, [groupOf(groups, id).id]: id }));
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
        <div className="glass-panel fo-tabs" role="tablist" aria-label="Field Ops areas">
          {groups.map((g) => (
            <button
              key={g.id}
              type="button"
              role="tab"
              id={`fo-group-${g.id}`}
              aria-selected={g.id === group.id}
              className="fo-tab"
              onClick={() => choose(g.id === group.id ? tab : (lastInGroup[g.id] ?? g.tabs[0]))}
            >
              {g.label}
            </button>
          ))}
        </div>
        <p className="text-muted small" style={{ margin: "0.1rem 0.2rem" }}>
          {group.hint}
        </p>
        {group.id === "radio" && onOpenComms ? (
          <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap", alignItems: "center", margin: "0 0.2rem" }}>
            <span className="text-muted small">Radio plan, radio data and other stations now live in Comms:</span>
            {MOVED_TO_COMMS.map((m) => (
              <button key={m.tab} type="button" className="secondary" onClick={() => onOpenComms(m.section)}>
                {tabLabel(m.tab)}
              </button>
            ))}
          </div>
        ) : null}
        {group.tabs.length > 1 ? (
          <div className="glass-panel fo-tabs" role="tablist" aria-label={`${group.label} sections`}>
            {group.tabs.map((id) => (
              <button
                key={id}
                type="button"
                role="tab"
                id={`fo-tab-${id}`}
                aria-selected={id === tab}
                aria-controls="fo-tabpanel"
                className="fo-tab"
                onClick={() => choose(id)}
              >
                {tabLabel(id)}
              </button>
            ))}
          </div>
        ) : null}
        <div id="fo-tabpanel" role="tabpanel" aria-labelledby={group.tabs.length > 1 ? `fo-tab-${tab}` : `fo-group-${group.id}`} aria-busy={busy} className={busy ? "fo-busy" : undefined}>
          <p className="fo-busy-note" role="status" aria-live="polite">
            {busy ? "Saving…" : ""}
          </p>
          <Suspense fallback={<p className="fo-empty">Loading…</p>}>
            <Active />
          </Suspense>
        </div>
      </div>
    </div>
  );
}
