import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, messageOf, type Stored } from "./ui";
import { useCommand } from "./ui";

type SilenceWindow = Stored<{ name: string; startMinute: number; endMinute: number; active: boolean }>;

type View = { windows: SilenceWindow[]; silentUntil: number | null; silentNow: string | null };

const COMMANDS: Array<[string, string]> = [
  ["?inv rice", "How many of an item the station has."],
  ["?runway", "Days of water, food and fuel left, and what is running low."],
  ["?checkin all quiet", "Record your check-in, with an optional note."],
  ["?overdue", "Who has missed a check-in."],
  ["?brief", "The watch officer's one-line status and urgent items."],
  ["?find how long to boil water", "The best-matching passage from the station library."],
  ["/ai your question", "Ask the station's AI (when it is switched on)."],
  ["SOS your message", "Raise an emergency. Always sent, even during radio silence."],
];

function toMinutes(hhmm: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(hhmm.trim());
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  return h < 24 && min < 60 ? h * 60 + min : null;
}

function toClock(minute: number): string {
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
}

export function RadioPlanTab() {
  const { data, error, reload } = useCommand<View>("get_radio_discipline");
  const [view, setView] = useState<View | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [start, setStart] = useState("22:00");
  const [end, setEnd] = useState("05:00");

  const current = view ?? data;

  const run = async (command: string, args: Record<string, unknown>) => {
    try {
      setView(await invoke<View>(command, args));
      setActionError(null);
    } catch (e) {
      setActionError(messageOf(e));
    }
  };

  const add = () => {
    const s = toMinutes(start);
    const e = toMinutes(end);
    if (s == null || e == null) {
      setActionError("Enter times as HH:MM, for example 22:00.");
      return;
    }
    void run("save_silence_window", { window: { name, startMinute: s, endMinute: e, active: true } }).then(() => setName(""));
  };

  return (
    <div className="fo-cards">
      <section className="glass-panel fo-panel" aria-label="Radio silence">
        <div className="fo-card-head">
          <h2>Radio silence</h2>
          {current ? (
            <Badge tone={current.silentNow ? "warn" : "good"}>{current.silentNow ? "Silent now" : "Transmitting allowed"}</Badge>
          ) : null}
        </div>
        <p className="fo-lead">
          Every transmission tells anyone listening that you are there. Set times when ATHENA must not transmit, or
          silence the radio straight away. Receiving carries on as normal, and SOS messages are always sent.
        </p>
        <ErrorNote message={error ?? actionError} />
        {current?.silentNow ? <p className="fo-small">{current.silentNow}</p> : null}

        <div className="fo-row">
          {[30, 60, 240].map((m) => (
            <button key={m} type="button" className="secondary" onClick={() => void run("set_radio_silence", { minutes: m })}>
              Go silent for {m < 60 ? `${m} minutes` : `${m / 60} hour${m === 60 ? "" : "s"}`}
            </button>
          ))}
          {current?.silentUntil ? (
            <button type="button" className="secondary" onClick={() => void run("set_radio_silence", { minutes: 0 })}>
              Lift silence (set until {formatDateTime(current.silentUntil)})
            </button>
          ) : null}
        </div>

        <h3>Daily silence windows</h3>
        {current && current.windows.length > 0 ? (
          <div className="fo-table-wrap">
            <table className="fo-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>From</th>
                  <th>Until</th>
                  <th>Status</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {current.windows.map((w) => (
                  <tr key={w.id}>
                    <td>{w.name}</td>
                    <td>{toClock(w.startMinute)}</td>
                    <td>
                      {toClock(w.endMinute)}
                      {w.endMinute < w.startMinute ? " (next day)" : w.endMinute === w.startMinute ? " (all day)" : ""}
                    </td>
                    <td>{w.active ? "On" : "Off"}</td>
                    <td>
                      <div className="fo-actions">
                        <button
                          type="button"
                          className="secondary"
                          onClick={() =>
                            void run("save_silence_window", {
                              id: w.id,
                              window: { name: w.name, startMinute: w.startMinute, endMinute: w.endMinute, active: !w.active },
                            })
                          }
                        >
                          {w.active ? "Turn off" : "Turn on"}
                        </button>
                        <button type="button" className="secondary" onClick={() => void run("delete_silence_window", { id: w.id })}>
                          Remove
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="fo-empty">No silence windows set.</p>
        )}
        <form
          className="fo-row"
          onSubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <Field label="Name" grow>
            <input className="fo-input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Night silence" />
          </Field>
          <Field label="From (HH:MM)">
            <input className="fo-input fo-input--narrow" value={start} onChange={(e) => setStart(e.target.value)} />
          </Field>
          <Field label="Until (HH:MM)">
            <input className="fo-input fo-input--narrow" value={end} onChange={(e) => setEnd(e.target.value)} />
          </Field>
          <button type="submit" className="secondary" disabled={!name.trim()}>
            Add window
          </button>
          <button type="button" className="secondary" onClick={() => void reload().then(() => setView(null))}>
            Refresh
          </button>
        </form>
      </section>

      <section className="glass-panel fo-panel" aria-label="Radio commands">
        <h2>Ask the base by radio</h2>
        <p className="fo-lead">
          Anyone registered on the mesh can send these from a bare radio and get an answer from this station. They are
          answered from station records, so they work even when the AI is switched off.
        </p>
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Send</th>
                <th>You get back</th>
              </tr>
            </thead>
            <tbody>
              {COMMANDS.map(([cmd, what]) => (
                <tr key={cmd}>
                  <td>
                    <code>{cmd}</code>
                  </td>
                  <td>{what}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
