import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { ErrorNote, Field, formatHour, messageOf, runAction, useCommand, usePeople, type Stored } from "./ui";

type Duty = Stored<{
  name: string;
  shiftsPerDay: number;
  peoplePerShift: number;
  shiftHours: number | null;
  startHour: number | null;
}>;

type Exclusion = Stored<{ personId: string; reason: string | null }>;

type Setup = { duties: Duty[]; exclusions: Exclusion[] };

type Roster = {
  days: number;
  assignments: Array<{
    day: number;
    duty: string;
    shift: number;
    startHour: number;
    endHour: number;
    people: Array<{ id: string; name: string; shortRest: boolean }>;
    unfilled: number;
  }>;
  loads: Array<{ id: string; name: string; hours: number; shifts: number }>;
  unfilledPlaces: number;
  shortRestCount: number;
  spreadHours: number;
};

function dayLabel(offset: number): string {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

export function RosterTab() {
  const { data, error, reload } = useCommand<Setup>("get_roster_setup");
  const people = usePeople();
  const [actionError, setActionError] = useState<string | null>(null);
  const [roster, setRoster] = useState<Roster | null>(null);
  const [days, setDays] = useState("7");
  const [dutyName, setDutyName] = useState("");
  const [shifts, setShifts] = useState("3");
  const [perShift, setPerShift] = useState("1");
  const [shiftHours, setShiftHours] = useState("");
  const [startHour, setStartHour] = useState("");

  const duties = data?.duties ?? [];
  const excluded = new Set((data?.exclusions ?? []).map((x) => x.personId));

  const generate = async () => {
    try {
      const r = await invoke<Roster>("generate_duty_roster", { days: Number.parseInt(days, 10) || 7 });
      setRoster(r);
      setActionError(null);
    } catch (e) {
      setRoster(null);
      setActionError(messageOf(e));
    }
  };

  const addDuty = () =>
    runAction(
      () =>
        invoke("save_roster_duty", {
          duty: {
            name: dutyName,
            shiftsPerDay: Number.parseInt(shifts, 10),
            peoplePerShift: Number.parseInt(perShift, 10),
            shiftHours: shiftHours.trim() ? Number.parseFloat(shiftHours) : null,
            startHour: startHour.trim() ? Number.parseFloat(startHour) : null,
          },
        }),
      setActionError,
      async () => {
        setDutyName("");
        setShiftHours("");
        setStartHour("");
        setRoster(null);
        await reload();
      },
    );

  const byDay = new Map<number, Roster["assignments"]>();
  for (const a of roster?.assignments ?? []) {
    const list = byDay.get(a.day) ?? [];
    list.push(a);
    byDay.set(a.day, list);
  }

  return (
    <section className="glass-panel fo-panel" aria-label="Duty roster">
      <h2>Duty roster</h2>
      <p className="fo-lead">
        List the duties that need covering and ATHENA shares them out evenly, leaves at least 8 hours' rest between
        shifts where it can, and never rosters anyone for more than 16 hours without a break.
      </p>
      <ErrorNote message={error ?? actionError} />

      <h3>Duties</h3>
      {duties.length === 0 ? (
        <p className="fo-empty">No duties yet. Add one, for example "Watch" with 3 shifts a day.</p>
      ) : (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Duty</th>
                <th>Shifts a day</th>
                <th>Shift length</th>
                <th>First shift</th>
                <th>People per shift</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {duties.map((d) => (
                <tr key={d.id}>
                  <td>{d.name}</td>
                  <td>{d.shiftsPerDay}</td>
                  <td>{d.shiftHours != null ? `${d.shiftHours} h` : `${+(24 / d.shiftsPerDay).toFixed(1)} h (round the clock)`}</td>
                  <td>{formatHour(d.startHour ?? 0)}</td>
                  <td>{d.peoplePerShift}</td>
                  <td>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() =>
                        void runAction(() => invoke("delete_roster_duty", { id: d.id }), setActionError, async () => {
                          setRoster(null);
                          await reload();
                        })
                      }
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void addDuty();
        }}
      >
        <Field label="Duty" grow>
          <input className="fo-input" value={dutyName} onChange={(e) => setDutyName(e.target.value)} placeholder="Watch" />
        </Field>
        <Field label="Shifts a day">
          <input className="fo-input fo-input--narrow" type="number" min={1} max={24} value={shifts} onChange={(e) => setShifts(e.target.value)} />
        </Field>
        <Field label="People per shift">
          <input className="fo-input fo-input--narrow" type="number" min={1} max={50} value={perShift} onChange={(e) => setPerShift(e.target.value)} />
        </Field>
        <Field label="Shift length in hours (blank = round the clock)">
          <input className="fo-input fo-input--narrow" type="number" min={0.25} max={24} step={0.25} value={shiftHours} onChange={(e) => setShiftHours(e.target.value)} />
        </Field>
        <Field label="First shift starts (hour, 0–23)">
          <input className="fo-input fo-input--narrow" type="number" min={0} max={23.75} step={0.25} value={startHour} onChange={(e) => setStartHour(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!dutyName.trim()}>
          Add duty
        </button>
      </form>

      <h3>Who is available</h3>
      {people.length === 0 ? (
        <p className="fo-empty">No personnel found.</p>
      ) : (
        <div className="fo-row">
          {people.map((p) => (
            <label key={p.id} className="fo-small" style={{ display: "flex", gap: "0.35rem", alignItems: "center" }}>
              <input
                type="checkbox"
                checked={!excluded.has(p.id)}
                onChange={(e) =>
                  void runAction(
                    () => invoke("set_roster_exclusion", { personId: p.id, excluded: !e.target.checked }),
                    setActionError,
                    async () => {
                      setRoster(null);
                      await reload();
                    },
                  )
                }
              />
              {p.name}
            </label>
          ))}
        </div>
      )}

      <div className="fo-row">
        <Field label="Days to plan">
          <input className="fo-input fo-input--narrow" type="number" min={1} max={31} value={days} onChange={(e) => setDays(e.target.value)} />
        </Field>
        <button type="button" className="secondary" onClick={() => void generate()} disabled={duties.length === 0}>
          Build roster
        </button>
      </div>

      {roster ? (
        <>
          <p className="fo-small" aria-live="polite">
            {roster.unfilledPlaces === 0 && roster.shortRestCount === 0
              ? `Every shift is filled with full rest. The busiest and least busy person differ by ${roster.spreadHours.toFixed(1)} hours.`
              : [
                  roster.unfilledPlaces > 0
                    ? `${roster.unfilledPlaces} place${roster.unfilledPlaces === 1 ? "" : "s"} could not be filled: there are not enough people.`
                    : null,
                  roster.shortRestCount > 0
                    ? `${roster.shortRestCount} shift${roster.shortRestCount === 1 ? " starts" : "s start"} with less than 8 hours' rest.`
                    : null,
                ]
                  .filter(Boolean)
                  .join(" ")}
          </p>
          <div className="fo-table-wrap">
            <table className="fo-table">
              <thead>
                <tr>
                  <th>Person</th>
                  <th>Shifts</th>
                  <th>Hours</th>
                </tr>
              </thead>
              <tbody>
                {roster.loads.map((l) => (
                  <tr key={l.id}>
                    <td>{l.name}</td>
                    <td>{l.shifts}</td>
                    <td>{l.hours.toFixed(1)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {[...byDay.entries()].map(([day, list]) => (
            <div key={day} className="fo-table-wrap">
              <h3>{dayLabel(day)}</h3>
              <table className="fo-table">
                <thead>
                  <tr>
                    <th>Time</th>
                    <th>Duty</th>
                    <th>Who</th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((a) => (
                    <tr key={`${a.duty}-${a.shift}`}>
                      <td>
                        {formatHour(a.startHour)}–{formatHour(a.endHour)}
                      </td>
                      <td>{a.duty}</td>
                      <td>
                        {a.people.map((p) => `${p.name}${p.shortRest ? " (short rest)" : ""}`).join(", ") || "—"}
                        {a.unfilled > 0 ? (
                          <span className="fo-error">
                            {a.people.length > 0 ? " · " : ""}
                            {a.unfilled} unfilled
                          </span>
                        ) : null}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </>
      ) : null}
    </section>
  );
}
