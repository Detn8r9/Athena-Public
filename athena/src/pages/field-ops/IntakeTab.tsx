import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDate, runAction, useCommand, type Stored } from "./ui";

type IntakeRow = Stored<{
  name: string;
  arrivedAt: number;
  partySize: number | null;
  skills: string[];
  medicalNotes: string | null;
  allergies: string | null;
  cameFrom: string | null;
  notes: string | null;
  quarantineDays: number;
  status: "quarantine" | "cleared" | "departed";
  clearedAt: number | null;
  clearedBy: string | null;
  quarantine: { endsAt: number; remainingSecs: number; ended: boolean; daysLeft: number };
}>;

const EMPTY = {
  name: "",
  partySize: "1",
  cameFrom: "",
  skills: "",
  medicalNotes: "",
  allergies: "",
  notes: "",
  quarantineDays: "5",
};

export function IntakeTab() {
  const { data, error, loading, reload } = useCommand<IntakeRow[]>("list_intake_records");
  const [actionError, setActionError] = useState<string | null>(null);
  const [form, setForm] = useState(EMPTY);
  const set = (key: keyof typeof EMPTY) => (e: { target: { value: string } }) =>
    setForm((f) => ({ ...f, [key]: e.target.value }));

  const rows = data ?? [];

  const add = () =>
    runAction(
      () =>
        invoke("save_intake_record", {
          record: {
            name: form.name,
            arrivedAt: Math.floor(Date.now() / 1000),
            partySize: Number.parseInt(form.partySize, 10) || 1,
            skills: form.skills
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean),
            medicalNotes: form.medicalNotes.trim() || null,
            allergies: form.allergies.trim() || null,
            cameFrom: form.cameFrom.trim() || null,
            notes: form.notes.trim() || null,
            quarantineDays: Number.parseInt(form.quarantineDays, 10) || 0,
            status: "",
          },
        }),
      setActionError,
      async () => {
        setForm(EMPTY);
        await reload();
      },
    );

  const setStatus = (id: string, status: string) =>
    runAction(() => invoke("set_intake_status", { id, status }), setActionError, reload);

  return (
    <section className="glass-panel fo-panel" aria-label="Newcomer intake">
      <h2>Newcomer intake</h2>
      <p className="fo-lead">
        Record everyone who arrives: what they can do, what they need, and how long to keep them apart from the group.
        The Dashboard tells you when a quarantine has run its course.
      </p>
      <ErrorNote message={error ?? actionError} />

      {rows.length === 0 && !loading ? (
        <p className="fo-empty">Nobody has been recorded yet.</p>
      ) : (
        <div className="fo-cards">
          {rows.map((r) => (
            <article key={r.id} className="glass-panel fo-card">
              <div className="fo-card-head">
                <strong>
                  {r.name}
                  {r.partySize && r.partySize > 1 ? ` (party of ${r.partySize})` : ""}
                </strong>
                {r.status === "quarantine" ? (
                  <Badge tone={r.quarantine.ended ? "warn" : "bad"}>
                    {r.quarantine.ended
                      ? "Quarantine over, needs review"
                      : `Quarantine: ${r.quarantine.daysLeft} day${r.quarantine.daysLeft === 1 ? "" : "s"} left`}
                  </Badge>
                ) : (
                  <Badge tone={r.status === "cleared" ? "good" : undefined}>
                    {r.status === "cleared" ? "Cleared" : "Left"}
                  </Badge>
                )}
              </div>
              <div className="fo-small fo-muted">
                Arrived {formatDate(r.arrivedAt)}
                {r.cameFrom ? ` from ${r.cameFrom}` : ""}
                {r.status === "quarantine" ? ` · quarantine ends ${formatDate(r.quarantine.endsAt)}` : ""}
                {r.status === "cleared" && r.clearedBy ? ` · cleared by ${r.clearedBy} on ${formatDate(r.clearedAt)}` : ""}
              </div>
              {r.skills.length > 0 ? <div className="fo-small">Skills: {r.skills.join(", ")}</div> : null}
              {r.allergies ? <div className="fo-small">Allergies: {r.allergies}</div> : null}
              {r.medicalNotes ? <div className="fo-small">Medical: {r.medicalNotes}</div> : null}
              {r.notes ? <p className="fo-small fo-prewrap">{r.notes}</p> : null}
              <div className="fo-actions">
                {r.status !== "cleared" ? (
                  <button type="button" className="secondary" onClick={() => void setStatus(r.id, "cleared")}>
                    Mark cleared
                  </button>
                ) : null}
                {r.status !== "departed" ? (
                  <button type="button" className="secondary" onClick={() => void setStatus(r.id, "departed")}>
                    Mark as left
                  </button>
                ) : null}
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    if (window.confirm(`Delete the intake record for ${r.name}?`)) {
                      void runAction(() => invoke("delete_intake_record", { id: r.id }), setActionError, reload);
                    }
                  }}
                >
                  Delete
                </button>
              </div>
            </article>
          ))}
        </div>
      )}

      <h3>Record an arrival</h3>
      <form
        className="fo-cards"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <div className="fo-row">
          <Field label="Name" grow>
            <input className="fo-input" value={form.name} onChange={set("name")} />
          </Field>
          <Field label="People in party">
            <input className="fo-input fo-input--narrow" type="number" min={1} max={200} value={form.partySize} onChange={set("partySize")} />
          </Field>
          <Field label="Quarantine (days, 0 for none)">
            <input className="fo-input fo-input--narrow" type="number" min={0} max={60} value={form.quarantineDays} onChange={set("quarantineDays")} />
          </Field>
        </div>
        <div className="fo-row">
          <Field label="Came from" grow>
            <input className="fo-input" value={form.cameFrom} onChange={set("cameFrom")} />
          </Field>
          <Field label="Skills (separate with commas)" grow>
            <input className="fo-input" value={form.skills} onChange={set("skills")} placeholder="nurse, welding" />
          </Field>
        </div>
        <div className="fo-row">
          <Field label="Allergies" grow>
            <input className="fo-input" value={form.allergies} onChange={set("allergies")} />
          </Field>
          <Field label="Medical needs" grow>
            <input className="fo-input" value={form.medicalNotes} onChange={set("medicalNotes")} />
          </Field>
        </div>
        <Field label="Notes">
          <textarea value={form.notes} onChange={set("notes")} />
        </Field>
        <div>
          <button type="submit" className="secondary" disabled={!form.name.trim()}>
            Save arrival
          </button>
        </div>
      </form>
    </section>
  );
}
