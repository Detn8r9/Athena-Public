import { useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, runAction, useCommand, type Stored } from "./ui";

type Resource = "water" | "food" | "fuel";
type NeedStatus = "untracked" | "out" | "critical" | "low" | "ok";

type NeedsOverview = {
  report: {
    headcount: number;
    resources: Array<{
      resource: Resource;
      label: string;
      unit: string;
      total: number;
      dailyNeed: number;
      daysLeft: number | null;
      status: NeedStatus;
      itemCount: number;
    }>;
  };
  settings: { waterLPerPersonDay: number; foodKcalPerPersonDay: number; fuelLPerDay: number };
  profiles: Array<Stored<{ itemId: string; resource: Resource; amountPerUnit: number }>>;
  inventory: Array<{ id: string; name: string; quantity: number }>;
};

type PowerOverview = {
  system: {
    batteryWh: number;
    usableFraction: number;
    chargePercent: number;
    panelWatts: number;
    sunHours: number;
    efficiency: number;
  };
  configured: boolean;
  loads: Array<Stored<{ name: string; watts: number; hoursPerDay: number }>>;
  budget: {
    generationWhPerDay: number;
    useWhPerDay: number;
    netWhPerDay: number;
    storedUsableWh: number;
    daysWithoutSun: number | null;
    daysUntilEmpty: number | null;
    sustainable: boolean;
    shortfallWhPerDay: number;
    loads: Array<{ name: string; whPerDay: number; percent: number }>;
  };
};

const STATUS_LABEL: Record<NeedStatus, string> = {
  untracked: "Not counted yet",
  out: "Out",
  critical: "Under a week",
  low: "Under a month",
  ok: "OK",
};

const STATUS_TONE: Record<NeedStatus, "bad" | "warn" | "good" | undefined> = {
  untracked: undefined,
  out: "bad",
  critical: "bad",
  low: "warn",
  ok: "good",
};

const UNIT_HINT: Record<Resource, string> = {
  water: "litres of water in one unit",
  food: "kcal of food in one unit",
  fuel: "litres of fuel in one unit",
};

function days(d: number | null): string {
  if (d == null) return "—";
  if (d < 1) return "less than a day";
  if (d < 60) return `${Math.floor(d)} day${Math.floor(d) === 1 ? "" : "s"}`;
  return `${Math.round(d / 30)} months`;
}

function NeedsSection() {
  const { data, error, reload } = useCommand<NeedsOverview>("get_needs_overview");
  const [actionError, setActionError] = useState<string | null>(null);
  const [itemId, setItemId] = useState("");
  const [resource, setResource] = useState<Resource>("water");
  const [amount, setAmount] = useState("");
  const [settings, setSettings] = useState({ water: "", food: "", fuel: "" });

  useEffect(() => {
    if (!data) return;
    setSettings({
      water: String(data.settings.waterLPerPersonDay),
      food: String(data.settings.foodKcalPerPersonDay),
      fuel: String(data.settings.fuelLPerDay),
    });
  }, [data]);

  const nameOf = (id: string) => data?.inventory.find((i) => i.id === id)?.name ?? "Removed item";

  return (
    <section className="glass-panel fo-panel" aria-label="Water, food and fuel">
      <h2>Water, food and fuel</h2>
      <p className="fo-lead">
        Tell ATHENA what one unit of an inventory item holds, and it works out how many days the group can last. This
        needs no usage history, so it works from day one.
      </p>
      <ErrorNote message={error ?? actionError} />

      {data ? (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Resource</th>
                <th>On hand</th>
                <th>Group needs per day</th>
                <th>Lasts</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {data.report.resources.map((r) => (
                <tr key={r.resource}>
                  <td>{r.label}</td>
                  <td>{r.itemCount > 0 ? `${Math.round(r.total).toLocaleString()} ${r.unit}` : "—"}</td>
                  <td>{r.dailyNeed > 0 ? `${Math.round(r.dailyNeed).toLocaleString()} ${r.unit}` : "Not set"}</td>
                  <td>{days(r.daysLeft)}</td>
                  <td>
                    <Badge tone={STATUS_TONE[r.status]}>{STATUS_LABEL[r.status]}</Badge>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="fo-small fo-muted">Planning for {data.report.headcount} people (everyone in Personnel).</p>
        </div>
      ) : null}

      <h3>What each item holds</h3>
      {data && data.profiles.length > 0 ? (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Item</th>
                <th>Counts as</th>
                <th>One unit holds</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {data.profiles.map((p) => (
                <tr key={p.id}>
                  <td>{nameOf(p.itemId)}</td>
                  <td>{p.resource}</td>
                  <td>
                    {p.amountPerUnit} {p.resource === "food" ? "kcal" : "litres"}
                  </td>
                  <td>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => void runAction(() => invoke("delete_supply_profile", { id: p.id }), setActionError, reload)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="fo-empty">Nothing counted yet. Pick an inventory item below.</p>
      )}
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void runAction(
            () => invoke("save_supply_profile", { profile: { itemId, resource, amountPerUnit: Number.parseFloat(amount) } }),
            setActionError,
            async () => {
              setAmount("");
              setItemId("");
              await reload();
            },
          );
        }}
      >
        <Field label="Inventory item" grow>
          <select value={itemId} onChange={(e) => setItemId(e.target.value)}>
            <option value="">{data && data.inventory.length === 0 ? "Add items in Logistics first" : "Choose…"}</option>
            {(data?.inventory ?? []).map((i) => (
              <option key={i.id} value={i.id}>
                {i.name} ({i.quantity})
              </option>
            ))}
          </select>
        </Field>
        <Field label="Counts as">
          <select value={resource} onChange={(e) => setResource(e.target.value as Resource)}>
            <option value="water">Water</option>
            <option value="food">Food</option>
            <option value="fuel">Fuel</option>
          </select>
        </Field>
        <Field label={`Amount (${UNIT_HINT[resource]})`}>
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={amount} onChange={(e) => setAmount(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!itemId || !amount.trim()}>
          Save
        </button>
      </form>

      <h3>Daily needs</h3>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void runAction(
            () =>
              invoke("save_needs_settings", {
                settings: {
                  waterLPerPersonDay: Number.parseFloat(settings.water) || 0,
                  foodKcalPerPersonDay: Number.parseFloat(settings.food) || 0,
                  fuelLPerDay: Number.parseFloat(settings.fuel) || 0,
                },
              }),
            setActionError,
            reload,
          );
        }}
      >
        <Field label="Water per person (litres a day)">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={settings.water} onChange={(e) => setSettings((s) => ({ ...s, water: e.target.value }))} />
        </Field>
        <Field label="Food per person (kcal a day)">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={settings.food} onChange={(e) => setSettings((s) => ({ ...s, food: e.target.value }))} />
        </Field>
        <Field label="Fuel for the whole group (litres a day)">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={settings.fuel} onChange={(e) => setSettings((s) => ({ ...s, fuel: e.target.value }))} />
        </Field>
        <button type="submit" className="secondary">
          Save needs
        </button>
      </form>
      <p className="fo-small fo-muted">
        The starting figures are common planning values: about 3.8 litres (one US gallon) of water and 2,000 kcal per
        person per day. Raise them for heat, heavy work, illness or washing.
      </p>
    </section>
  );
}

function PowerSection() {
  const { data, error, reload } = useCommand<PowerOverview>("get_power_overview");
  const [actionError, setActionError] = useState<string | null>(null);
  const [form, setForm] = useState({ batteryWh: "", usable: "", charge: "", panel: "", sun: "", eff: "" });
  const [loadName, setLoadName] = useState("");
  const [watts, setWatts] = useState("");
  const [hours, setHours] = useState("");

  useEffect(() => {
    if (!data) return;
    const s = data.system;
    setForm({
      batteryWh: String(s.batteryWh),
      usable: String(Math.round(s.usableFraction * 100)),
      charge: String(s.chargePercent),
      panel: String(s.panelWatts),
      sun: String(s.sunHours),
      eff: String(Math.round(s.efficiency * 100)),
    });
  }, [data]);

  const set = (key: keyof typeof form) => (e: { target: { value: string } }) => setForm((f) => ({ ...f, [key]: e.target.value }));
  const b = data?.budget;

  return (
    <section className="glass-panel fo-panel" aria-label="Power budget">
      <h2>Power budget</h2>
      <p className="fo-lead">
        Enter your battery, panels and what is plugged in. ATHENA shows whether the sun keeps up, how long the battery
        carries you through bad weather, and which device to switch off first.
      </p>
      <ErrorNote message={error ?? actionError} />

      {b && data?.configured ? (
        <>
          <p aria-live="polite">
            <strong>
              {b.useWhPerDay <= 0
                ? "Add what is plugged in to see the budget."
                : b.sustainable
                  ? `The sun keeps up, with ${Math.round(b.netWhPerDay)} Wh a day to spare.`
                  : `You are using ${Math.round(b.shortfallWhPerDay)} Wh a day more than you generate. The battery will be empty in ${days(b.daysUntilEmpty)}.`}
            </strong>
          </p>
          <div className="fo-table-wrap">
            <table className="fo-table">
              <tbody>
                <tr>
                  <td>Generated per day</td>
                  <td>{Math.round(b.generationWhPerDay)} Wh</td>
                </tr>
                <tr>
                  <td>Used per day</td>
                  <td>{Math.round(b.useWhPerDay)} Wh</td>
                </tr>
                <tr>
                  <td>Usable energy in the battery now</td>
                  <td>{Math.round(b.storedUsableWh)} Wh</td>
                </tr>
                <tr>
                  <td>Battery alone lasts (no sun)</td>
                  <td>{days(b.daysWithoutSun)}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </>
      ) : null}

      <h3>What is plugged in</h3>
      {data && data.loads.length > 0 ? (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Device</th>
                <th>Watts</th>
                <th>Hours a day</th>
                <th>Energy a day</th>
                <th>Share</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {[...data.loads]
                .sort((a, c) => c.watts * c.hoursPerDay - a.watts * a.hoursPerDay)
                .map((l) => {
                  const share = b?.loads.find((s) => s.name === l.name);
                  return (
                    <tr key={l.id}>
                      <td>{l.name}</td>
                      <td>{l.watts}</td>
                      <td>{l.hoursPerDay}</td>
                      <td>{Math.round(l.watts * l.hoursPerDay)} Wh</td>
                      <td>{share ? `${Math.round(share.percent)}%` : "—"}</td>
                      <td>
                        <button
                          type="button"
                          className="secondary"
                          onClick={() => void runAction(() => invoke("delete_power_load", { id: l.id }), setActionError, reload)}
                        >
                          Remove
                        </button>
                      </td>
                    </tr>
                  );
                })}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="fo-empty">No devices listed yet.</p>
      )}
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void runAction(
            () => invoke("save_power_load", { load: { name: loadName, watts: Number.parseFloat(watts), hoursPerDay: Number.parseFloat(hours) } }),
            setActionError,
            async () => {
              setLoadName("");
              setWatts("");
              setHours("");
              await reload();
            },
          );
        }}
      >
        <Field label="Device" grow>
          <input className="fo-input" value={loadName} onChange={(e) => setLoadName(e.target.value)} placeholder="Laptop running ATHENA" />
        </Field>
        <Field label="Watts">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={watts} onChange={(e) => setWatts(e.target.value)} />
        </Field>
        <Field label="Hours a day">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={24} step="any" value={hours} onChange={(e) => setHours(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!loadName.trim() || !watts.trim() || !hours.trim()}>
          Add device
        </button>
      </form>

      <h3>Battery and panels</h3>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void runAction(
            () =>
              invoke("save_power_system", {
                system: {
                  batteryWh: Number.parseFloat(form.batteryWh) || 0,
                  usableFraction: (Number.parseFloat(form.usable) || 0) / 100,
                  chargePercent: Number.parseFloat(form.charge) || 0,
                  panelWatts: Number.parseFloat(form.panel) || 0,
                  sunHours: Number.parseFloat(form.sun) || 0,
                  efficiency: (Number.parseFloat(form.eff) || 0) / 100,
                },
              }),
            setActionError,
            reload,
          );
        }}
      >
        <Field label="Battery size (Wh = Ah × volts)">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={form.batteryWh} onChange={set("batteryWh")} />
        </Field>
        <Field label="Usable share (%)">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={100} value={form.usable} onChange={set("usable")} />
        </Field>
        <Field label="Charge now (%)">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={100} value={form.charge} onChange={set("charge")} />
        </Field>
        <Field label="Panels (watts)">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={form.panel} onChange={set("panel")} />
        </Field>
        <Field label="Full-sun hours a day">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={24} step="any" value={form.sun} onChange={set("sun")} />
        </Field>
        <Field label="System efficiency (%)">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={100} value={form.eff} onChange={set("eff")} />
        </Field>
        <button type="submit" className="secondary">
          Save system
        </button>
      </form>
      <p className="fo-small fo-muted">
        Usable share: lead-acid batteries are usually run down to about half; lithium iron phosphate to 80% or more.
        Efficiency covers losses in the charge controller, wiring and inverter; 75% is a cautious starting point.
      </p>
    </section>
  );
}

export function SuppliesTab() {
  return (
    <div className="fo-cards">
      <NeedsSection />
      <PowerSection />
    </div>
  );
}
