import { useEffect, useMemo, useRef, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { BUILTIN_PROTOCOLS } from "./protocols/builtin";
import {
  advance,
  currentStep,
  goBack,
  isFinished,
  runTranscript,
  startRun,
  type Protocol,
  type ProtocolRun,
} from "./protocols/engine";
import { ErrorNote, Field, messageOf, useCommand, type Stored } from "./ui";

type PatientRow = Stored<{ name: string }>;

function speak(text: string) {
  try {
    if (!("speechSynthesis" in window)) return;
    window.speechSynthesis.cancel();
    window.speechSynthesis.speak(new SpeechSynthesisUtterance(text));
  } catch {
    /* speech is a convenience; never let it break the protocol */
  }
}

function stopSpeaking() {
  try {
    if ("speechSynthesis" in window) window.speechSynthesis.cancel();
  } catch {
    /* ignore */
  }
}

function countdown(seconds: number): string {
  const s = Math.max(0, Math.ceil(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function ProtocolsTab() {
  const protocols = BUILTIN_PROTOCOLS;
  const { data: patients } = useCommand<PatientRow[]>("list_patients");
  const [protocol, setProtocol] = useState<Protocol | null>(null);
  const [run, setRun] = useState<ProtocolRun | null>(null);
  const [patientId, setPatientId] = useState("");
  const [readAloud, setReadAloud] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const stepStartedAt = useRef(Date.now());

  const step = protocol && run ? currentStep(protocol, run) : null;
  const finished = protocol && run ? isFinished(protocol, run) : false;

  // Restart the step clock and read the step aloud whenever the step changes.
  const stepKey = step ? `${protocol?.id}:${step.id}:${run?.history.length}` : "";
  useEffect(() => {
    if (!step) return;
    stepStartedAt.current = Date.now();
    setNow(Date.now());
    if (readAloud) speak(step.text);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stepKey]);

  useEffect(() => {
    if (!readAloud) stopSpeaking();
  }, [readAloud]);

  useEffect(() => () => stopSpeaking(), []);

  // Tick once a second only while a timer step is showing.
  const timing = step?.kind === "timer";
  useEffect(() => {
    if (!timing) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [timing, stepKey]);

  const remaining = step?.kind === "timer" ? step.seconds - (now - stepStartedAt.current) / 1000 : 0;
  const timerDone = step?.kind === "timer" && remaining <= 0;

  const begin = (p: Protocol) => {
    setProtocol(p);
    setRun(startRun(p, Date.now()));
    setSaved(null);
    setError(null);
  };

  const saveToChart = async (p: Protocol, r: ProtocolRun) => {
    if (!patientId) return;
    try {
      await invoke("add_chart_entry", {
        entry: {
          patientId,
          at: Math.floor(r.startedAt / 1000),
          entryType: "protocol",
          text: runTranscript(p, r),
        },
      });
      const name = patients?.find((x) => x.id === patientId)?.name ?? "the patient";
      setSaved(`Saved to ${name}'s chart.`);
      setError(null);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const close = async (save: boolean) => {
    stopSpeaking();
    if (save && protocol && run && run.history.length > 0) await saveToChart(protocol, run);
    setProtocol(null);
    setRun(null);
  };

  const next = useMemo(
    () => (protocol && step?.kind === "end" && step.continueWith ? protocols.find((p) => p.id === step.continueWith) : undefined),
    [protocol, step, protocols],
  );

  if (!protocol || !run || !step) {
    return (
      <section className="glass-panel fo-panel" aria-label="Guided protocols">
        <h2>Guided protocols</h2>
        <p className="fo-lead">
          Follow an emergency one step at a time. Each protocol asks one thing, waits for you, and keeps a timed record
          you can save to a patient's chart. These are the first-aid checklists built into ATHENA. They are educational
          and do not replace trained medical care.
        </p>
        <ErrorNote message={error} />
        {saved ? <p className="fo-small">{saved}</p> : null}
        <div className="fo-row">
          <Field label="Patient (optional, to keep a record)">
            <select value={patientId} onChange={(e) => setPatientId(e.target.value)}>
              <option value="">No record</option>
              {(patients ?? []).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <label className="fo-small" style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
            <input type="checkbox" checked={readAloud} onChange={(e) => setReadAloud(e.target.checked)} />
            Read each step aloud
          </label>
        </div>
        <div className="fo-cards">
          {protocols.map((p) => (
            <article key={p.id} className="glass-panel fo-card">
              <div className="fo-card-head">
                <strong>{p.title}</strong>
                <button type="button" className="secondary" onClick={() => begin(p)}>
                  Start
                </button>
              </div>
              <p className="fo-small fo-muted fo-prewrap">{p.summary}</p>
            </article>
          ))}
        </div>
      </section>
    );
  }

  const stepNumber = run.history.length + 1;

  return (
    <section className="glass-panel fo-panel" aria-label={`${protocol.title} protocol`}>
      <div className="fo-card-head">
        <h2>{protocol.title}</h2>
        <span className="fo-small fo-muted">{finished ? "Finished" : `Step ${stepNumber}`}</span>
      </div>
      <ErrorNote message={error} />

      <div className="glass-panel fo-card" aria-live="assertive">
        <p className="fo-prewrap" style={{ fontSize: "1.15rem", lineHeight: 1.45 }}>
          {step.text}
        </p>
        {step.kind === "timer" ? (
          <p style={{ fontSize: "1.6rem", fontWeight: 800, margin: 0 }} aria-label="Time remaining">
            {timerDone ? "Time is up" : countdown(remaining)}
          </p>
        ) : null}

        <div className="fo-actions">
          {step.kind === "action" ? (
            <button type="button" className="secondary" onClick={() => setRun(advance(protocol, run, Date.now()))}>
              Done, next step
            </button>
          ) : null}
          {step.kind === "question"
            ? step.options.map((o, i) => (
                <button key={o.label} type="button" className="secondary" onClick={() => setRun(advance(protocol, run, Date.now(), { choice: i }))}>
                  {o.label}
                </button>
              ))
            : null}
          {step.kind === "timer" ? (
            <button
              type="button"
              className="secondary"
              onClick={() => setRun(advance(protocol, run, Date.now(), { skipped: !timerDone }))}
            >
              {timerDone ? "Next step" : "Move on early"}
            </button>
          ) : null}
          {step.kind === "end" && next ? (
            <button
              type="button"
              className="secondary"
              onClick={() => {
                void saveToChart(protocol, run).then(() => begin(next));
              }}
            >
              Continue with {next.title}
            </button>
          ) : null}
          {step.kind === "end" ? (
            <button type="button" className="secondary" onClick={() => void close(true)}>
              {patientId ? "Save to chart and close" : "Close"}
            </button>
          ) : null}
        </div>
      </div>

      <div className="fo-row">
        <button type="button" className="secondary" disabled={run.history.length === 0} onClick={() => setRun(goBack(run))}>
          Back one step
        </button>
        {readAloud ? (
          <button type="button" className="secondary" onClick={() => speak(step.text)}>
            Say it again
          </button>
        ) : null}
        <label className="fo-small" style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
          <input type="checkbox" checked={readAloud} onChange={(e) => setReadAloud(e.target.checked)} />
          Read aloud
        </label>
        {!finished ? (
          <button
            type="button"
            className="secondary"
            onClick={() => {
              if (window.confirm("Stop this protocol?")) void close(true);
            }}
          >
            Stop
          </button>
        ) : null}
      </div>

      {protocol.doNot.length > 0 ? (
        <div>
          <h3>Do not</h3>
          <ul className="fo-small">
            {protocol.doNot.map((d) => (
              <li key={d}>{d}</li>
            ))}
          </ul>
        </div>
      ) : null}
      {protocol.doList.length > 0 ? (
        <details>
          <summary className="fo-small fo-muted" style={{ cursor: "pointer" }}>
            Full checklist (all steps on one page)
          </summary>
          <ul className="fo-small">
            {protocol.doList.map((d) => (
              <li key={d}>{d}</li>
            ))}
          </ul>
        </details>
      ) : null}
      <p className="fo-small fo-muted">Source: {protocol.source}</p>
    </section>
  );
}
