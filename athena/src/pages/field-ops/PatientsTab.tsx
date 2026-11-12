import { useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, messageOf, runAction, useCommand, usePeople, type Stored } from "./ui";

type Patient = Stored<{
  name: string;
  personId: string | null;
  allergies: string | null;
  bloodType: string | null;
  conditions: string | null;
  regularMedication: string | null;
  notes: string | null;
}>;

type ChartEntry = Stored<{
  patientId: string;
  at: number;
  entryType: "protocol" | "vitals" | "medication" | "note";
  text: string;
  recordedBy: string | null;
}>;

const TYPE_LABEL: Record<ChartEntry["entryType"], string> = {
  protocol: "Protocol",
  vitals: "Vitals",
  medication: "Medication",
  note: "Note",
};

const NO_VITALS = { pulse: "", breathing: "", tempC: "", systolic: "", diastolic: "", spo2: "" };

function numberOrNull(s: string): number | null {
  const n = Number.parseFloat(s);
  return s.trim() && Number.isFinite(n) ? n : null;
}

export function PatientsTab() {
  const { data, error, loading, reload } = useCommand<Patient[]>("list_patients");
  const people = usePeople();
  const [actionError, setActionError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [entries, setEntries] = useState<ChartEntry[]>([]);
  const [newName, setNewName] = useState("");
  const [newAllergies, setNewAllergies] = useState("");
  const [newBlood, setNewBlood] = useState("");
  const [vitals, setVitals] = useState(NO_VITALS);
  const [medication, setMedication] = useState("");
  const [note, setNote] = useState("");

  const patients = data ?? [];
  const selected = patients.find((p) => p.id === selectedId) ?? null;

  const loadEntries = async (patientId: string) => {
    try {
      setEntries(await invoke<ChartEntry[]>("list_chart_entries", { patientId }));
    } catch (e) {
      setActionError(messageOf(e));
    }
  };

  // With a single patient on file there is nothing to choose: open their chart.
  useEffect(() => {
    if (!selectedId && patients.length === 1) setSelectedId(patients[0].id);
  }, [patients, selectedId]);

  useEffect(() => {
    if (selectedId) void loadEntries(selectedId);
    else setEntries([]);
  }, [selectedId]);

  const addPatient = () => {
    const person = people.find((p) => p.name.toLowerCase() === newName.trim().toLowerCase());
    return runAction(
      async () => {
        const saved = await invoke<Patient>("save_patient", {
          patient: {
            name: newName,
            personId: person?.id ?? null,
            allergies: newAllergies.trim() || null,
            bloodType: newBlood.trim() || null,
          },
        });
        setSelectedId(saved.id);
      },
      setActionError,
      async () => {
        setNewName("");
        setNewAllergies("");
        setNewBlood("");
        await reload();
      },
    );
  };

  const addEntry = (entry: Record<string, unknown>, after: () => void) => {
    if (!selected) return;
    return runAction(
      () => invoke("add_chart_entry", { entry: { patientId: selected.id, at: Math.floor(Date.now() / 1000), text: "", ...entry } }),
      setActionError,
      async () => {
        after();
        await loadEntries(selected.id);
      },
    );
  };

  const setVital = (key: keyof typeof NO_VITALS) => (e: { target: { value: string } }) =>
    setVitals((v) => ({ ...v, [key]: e.target.value }));

  return (
    <section className="glass-panel fo-panel" aria-label="Patient charts">
      <h2>Patient charts</h2>
      <p className="fo-lead">
        Keep a timeline for anyone being treated: readings, what was given, what was done. Whoever takes over care can
        see everything in order.
      </p>
      <ErrorNote message={error ?? actionError} />

      <div className="fo-row">
        <Field label="Patient">
          <select value={selectedId ?? ""} onChange={(e) => setSelectedId(e.target.value || null)}>
            <option value="">{loading ? "Loading…" : patients.length === 0 ? "No patients yet" : "Choose…"}</option>
            {patients.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Field>
      </div>

      {selected ? (
        <>
          <article className="glass-panel fo-card">
            <div className="fo-card-head">
              <strong>{selected.name}</strong>
              {selected.allergies ? <Badge tone="bad">Allergies: {selected.allergies}</Badge> : <Badge>No allergies recorded</Badge>}
            </div>
            <div className="fo-small fo-muted">
              {[
                selected.bloodType ? `Blood type ${selected.bloodType}` : null,
                selected.conditions ? `Conditions: ${selected.conditions}` : null,
                selected.regularMedication ? `Regular medication: ${selected.regularMedication}` : null,
              ]
                .filter(Boolean)
                .join(" · ") || "No other details recorded."}
            </div>
            <div className="fo-actions">
              <button
                type="button"
                className="secondary"
                onClick={() => {
                  if (window.confirm(`Delete ${selected.name} and their whole chart?`)) {
                    void runAction(() => invoke("delete_patient", { id: selected.id }), setActionError, async () => {
                      setSelectedId(null);
                      await reload();
                    });
                  }
                }}
              >
                Delete patient
              </button>
            </div>
          </article>

          <h3>Record vitals</h3>
          <form
            className="fo-row"
            onSubmit={(e) => {
              e.preventDefault();
              void addEntry(
                {
                  entryType: "vitals",
                  vitals: {
                    pulse: numberOrNull(vitals.pulse),
                    breathing: numberOrNull(vitals.breathing),
                    tempC: numberOrNull(vitals.tempC),
                    systolic: numberOrNull(vitals.systolic),
                    diastolic: numberOrNull(vitals.diastolic),
                    spo2: numberOrNull(vitals.spo2),
                  },
                },
                () => setVitals(NO_VITALS),
              );
            }}
          >
            <Field label="Pulse (per min)">
              <input className="fo-input fo-input--narrow" inputMode="decimal" value={vitals.pulse} onChange={setVital("pulse")} />
            </Field>
            <Field label="Breaths (per min)">
              <input className="fo-input fo-input--narrow" inputMode="decimal" value={vitals.breathing} onChange={setVital("breathing")} />
            </Field>
            <Field label="Temperature (°C)">
              <input className="fo-input fo-input--narrow" inputMode="decimal" value={vitals.tempC} onChange={setVital("tempC")} />
            </Field>
            <Field label="BP upper">
              <input className="fo-input fo-input--narrow" inputMode="decimal" value={vitals.systolic} onChange={setVital("systolic")} />
            </Field>
            <Field label="BP lower">
              <input className="fo-input fo-input--narrow" inputMode="decimal" value={vitals.diastolic} onChange={setVital("diastolic")} />
            </Field>
            <Field label="Oxygen (%)">
              <input className="fo-input fo-input--narrow" inputMode="decimal" value={vitals.spo2} onChange={setVital("spo2")} />
            </Field>
            <button type="submit" className="secondary">
              Save reading
            </button>
          </form>

          <form
            className="fo-row"
            onSubmit={(e) => {
              e.preventDefault();
              void addEntry({ entryType: "medication", text: medication }, () => setMedication(""));
            }}
          >
            <Field label="Medication given (what, how much, how)" grow>
              <input className="fo-input" value={medication} onChange={(e) => setMedication(e.target.value)} />
            </Field>
            <button type="submit" className="secondary" disabled={!medication.trim()}>
              Record medication
            </button>
          </form>

          <form
            className="fo-row"
            onSubmit={(e) => {
              e.preventDefault();
              void addEntry({ entryType: "note", text: note }, () => setNote(""));
            }}
          >
            <Field label="Note" grow>
              <input className="fo-input" value={note} onChange={(e) => setNote(e.target.value)} />
            </Field>
            <button type="submit" className="secondary" disabled={!note.trim()}>
              Add note
            </button>
          </form>

          <h3>Timeline</h3>
          {entries.length === 0 ? (
            <p className="fo-empty">Nothing recorded yet.</p>
          ) : (
            <div className="fo-cards">
              {entries.map((en) => (
                <article key={en.id} className="glass-panel fo-card">
                  <div className="fo-card-head">
                    <span>
                      <Badge>{TYPE_LABEL[en.entryType]}</Badge> <span className="fo-small">{formatDateTime(en.at)}</span>
                    </span>
                    <span className="fo-small fo-muted">{en.recordedBy ?? ""}</span>
                  </div>
                  <p className="fo-small fo-prewrap">{en.text}</p>
                  <div className="fo-actions">
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => {
                        if (window.confirm("Remove this entry from the chart?")) {
                          void runAction(() => invoke("delete_chart_entry", { id: en.id }), setActionError, () => loadEntries(selected.id));
                        }
                      }}
                    >
                      Remove
                    </button>
                  </div>
                </article>
              ))}
            </div>
          )}
        </>
      ) : null}

      <h3>Add a patient</h3>
      <datalist id="fo-patient-people">
        {people.map((p) => (
          <option key={p.id} value={p.name} />
        ))}
      </datalist>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void addPatient();
        }}
      >
        <Field label="Name" grow>
          <input className="fo-input" list="fo-patient-people" value={newName} onChange={(e) => setNewName(e.target.value)} />
        </Field>
        <Field label="Allergies" grow>
          <input className="fo-input" value={newAllergies} onChange={(e) => setNewAllergies(e.target.value)} />
        </Field>
        <Field label="Blood type">
          <input className="fo-input fo-input--narrow" value={newBlood} onChange={(e) => setNewBlood(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!newName.trim()}>
          Add patient
        </button>
      </form>
    </section>
  );
}
