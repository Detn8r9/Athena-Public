import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDate, runAction, useCommand, type Stored } from "./ui";

type DecisionRow = Stored<{
  title: string;
  detail: string | null;
  options: string[];
  votes: Array<{ voterId: string; voterName: string; option: number }>;
  status: "open" | "closed";
  outcome: string | null;
  recordedBy: string | null;
  tally: { counts: number[]; voters: number; leading: number | null; tied: boolean };
}>;

export function DecisionsTab() {
  const { data, error, loading, reload } = useCommand<DecisionRow[]>("list_decisions");
  const [actionError, setActionError] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [detail, setDetail] = useState("");
  const [options, setOptions] = useState("Yes, No");

  const rows = data ?? [];

  const add = () =>
    runAction(
      () =>
        invoke("save_decision", {
          decision: {
            title,
            detail: detail.trim() || null,
            options: options
              .split(",")
              .map((o) => o.trim())
              .filter(Boolean),
            votes: [],
            status: "open",
          },
        }),
      setActionError,
      async () => {
        setTitle("");
        setDetail("");
        await reload();
      },
    );

  const close = (d: DecisionRow) => {
    const suggested = d.tally.leading != null ? d.options[d.tally.leading] : "";
    const outcome = window.prompt("What was decided?", suggested ?? "");
    if (outcome == null) return;
    void runAction(() => invoke("close_decision", { id: d.id, outcome }), setActionError, reload);
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Decisions">
      <h2>Decisions</h2>
      <p className="fo-lead">
        Put a question to the group, let each signed-in person vote once, and record what was decided so it can be
        checked later.
      </p>
      <ErrorNote message={error ?? actionError} />

      {rows.length === 0 && !loading ? (
        <p className="fo-empty">No decisions recorded yet.</p>
      ) : (
        <div className="fo-cards">
          {rows.map((d) => (
            <article key={d.id} className="glass-panel fo-card">
              <div className="fo-card-head">
                <strong>{d.title}</strong>
                <Badge tone={d.status === "open" ? "warn" : "good"}>{d.status === "open" ? "Open" : "Decided"}</Badge>
              </div>
              <div className="fo-small fo-muted">
                {formatDate(d.createdAt)}
                {d.recordedBy ? ` · raised by ${d.recordedBy}` : ""} · {d.tally.voters} vote
                {d.tally.voters === 1 ? "" : "s"}
                {d.status === "open" && d.tally.tied ? " · tied" : ""}
              </div>
              {d.detail ? <p className="fo-small fo-prewrap">{d.detail}</p> : null}
              {d.status === "closed" && d.outcome ? (
                <p className="fo-prewrap">
                  <strong>Decided:</strong> {d.outcome}
                </p>
              ) : null}
              <div className="fo-actions">
                {d.options.map((o, i) => (
                  <button
                    key={o}
                    type="button"
                    className="secondary"
                    disabled={d.status !== "open"}
                    onClick={() => void runAction(() => invoke("cast_decision_vote", { id: d.id, option: i }), setActionError, reload)}
                  >
                    {o}: {d.tally.counts[i] ?? 0}
                    {d.tally.leading === i ? " (leading)" : ""}
                  </button>
                ))}
              </div>
              {d.votes.length > 0 ? (
                <div className="fo-small fo-muted">
                  {d.votes.map((v) => `${v.voterName}: ${d.options[v.option] ?? "?"}`).join(" · ")}
                </div>
              ) : null}
              <div className="fo-actions">
                {d.status === "open" ? (
                  <button type="button" className="secondary" onClick={() => close(d)}>
                    Close and record outcome
                  </button>
                ) : null}
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    if (window.confirm(`Delete "${d.title}"?`)) {
                      void runAction(() => invoke("delete_decision", { id: d.id }), setActionError, reload);
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

      <h3>Raise a decision</h3>
      <form
        className="fo-cards"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <div className="fo-row">
          <Field label="Question" grow>
            <input className="fo-input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Move camp before winter?" />
          </Field>
          <Field label="Options (separate with commas)" grow>
            <input className="fo-input" value={options} onChange={(e) => setOptions(e.target.value)} />
          </Field>
        </div>
        <Field label="Background (optional)">
          <textarea value={detail} onChange={(e) => setDetail(e.target.value)} />
        </Field>
        <div>
          <button type="submit" className="secondary" disabled={!title.trim()}>
            Raise decision
          </button>
        </div>
      </form>
    </section>
  );
}
