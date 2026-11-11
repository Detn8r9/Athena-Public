import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, runAction, useCommand, usePeople, type Stored } from "./ui";

type Risk = "uncovered" | "single" | "thin" | "ok";

type SkillEntry = Stored<{ personId: string; personName: string; skill: string; level: number; note: string | null }>;

type Coverage = {
  skill: string;
  critical: boolean;
  risk: Risk;
  holders: Array<{ personId: string; personName: string; level: number }>;
  competent: number;
  suggestedTeacher: string | null;
  suggestedLearners: string[];
};

type Overview = { entries: SkillEntry[]; coverage: Coverage[]; criticalSkills: string[] };

const LEVEL_LABEL: Record<number, string> = { 1: "Learning", 2: "Can do it", 3: "Can teach it" };

const RISK_LABEL: Record<Risk, string> = {
  uncovered: "Nobody",
  single: "One person",
  thin: "Thin",
  ok: "Covered",
};

const RISK_TONE: Record<Risk, "bad" | "warn" | "good"> = {
  uncovered: "bad",
  single: "bad",
  thin: "warn",
  ok: "good",
};

function advice(c: Coverage): string {
  if (c.risk === "ok") return "";
  if (c.risk === "uncovered") {
    return c.suggestedLearners.length > 0
      ? `${c.suggestedLearners.join(", ")} ${c.suggestedLearners.length === 1 ? "is" : "are"} learning. Get one of them to the point of working unsupervised.`
      : "Nobody can do this yet. Pick two people to learn it from the library or the Academy.";
  }
  const teacher = c.suggestedTeacher ?? "The person who knows it";
  return c.suggestedLearners.length > 0
    ? `${teacher} should bring ${c.suggestedLearners.join(", ")} up to working unsupervised.`
    : `${teacher} should start teaching someone else.`;
}

export function SkillsTab() {
  const { data, error, loading, reload } = useCommand<Overview>("get_skills_overview");
  const people = usePeople();
  const [actionError, setActionError] = useState<string | null>(null);
  const [personId, setPersonId] = useState("");
  const [skill, setSkill] = useState("");
  const [level, setLevel] = useState("2");

  const coverage = data?.coverage ?? [];
  const entries = data?.entries ?? [];
  const atRisk = coverage.filter((c) => c.risk !== "ok");

  const add = () => {
    const person = people.find((p) => p.id === personId);
    if (!person) return;
    return runAction(
      () =>
        invoke("save_skill_entry", {
          entry: { personId: person.id, personName: person.name, skill, level: Number.parseInt(level, 10) },
        }),
      setActionError,
      async () => {
        setSkill("");
        await reload();
      },
    );
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Skills">
      <h2>Skills</h2>
      <p className="fo-lead">
        Record what each person can do. Any critical skill that only one person can do unsupervised is a risk: if
        they are hurt or away, the group loses it.
      </p>
      <ErrorNote message={error ?? actionError} />

      {entries.length > 0 ? (
        <p className="fo-small fo-muted" aria-live="polite">
          {atRisk.length === 0
            ? "Every skill has enough people."
            : `${atRisk.length} skill${atRisk.length === 1 ? " needs" : "s need"} more people.`}
        </p>
      ) : null}

      <div className="fo-table-wrap">
        <table className="fo-table">
          <thead>
            <tr>
              <th>Skill</th>
              <th>Coverage</th>
              <th>Who</th>
              <th>What to do</th>
            </tr>
          </thead>
          <tbody>
            {coverage.map((c) => (
              <tr key={c.skill}>
                <td>
                  {c.skill}
                  {c.critical ? <div className="fo-muted fo-small">Critical</div> : null}
                </td>
                <td>
                  <Badge tone={RISK_TONE[c.risk]}>{RISK_LABEL[c.risk]}</Badge>
                </td>
                <td>
                  {c.holders.length === 0 ? (
                    <span className="fo-muted">—</span>
                  ) : (
                    c.holders.map((h) => (
                      <div key={h.personId}>
                        {h.personName} <span className="fo-muted fo-small">({LEVEL_LABEL[h.level] ?? h.level})</span>
                      </div>
                    ))
                  )}
                </td>
                <td className="fo-small">{advice(c)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {loading && !data ? <p className="fo-empty">Loading…</p> : null}

      <h3>Record a skill</h3>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <Field label="Person">
          <select value={personId} onChange={(e) => setPersonId(e.target.value)}>
            <option value="">Choose…</option>
            {people.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Skill" grow>
          <input
            className="fo-input"
            list="fo-skill-suggestions"
            value={skill}
            onChange={(e) => setSkill(e.target.value)}
            placeholder="First aid"
          />
          <datalist id="fo-skill-suggestions">
            {(data?.criticalSkills ?? []).map((s) => (
              <option key={s} value={s} />
            ))}
          </datalist>
        </Field>
        <Field label="Level">
          <select value={level} onChange={(e) => setLevel(e.target.value)}>
            <option value="1">Learning</option>
            <option value="2">Can do it</option>
            <option value="3">Can teach it</option>
          </select>
        </Field>
        <button type="submit" className="secondary" disabled={!personId || !skill.trim()}>
          Add
        </button>
      </form>

      {entries.length > 0 ? (
        <details>
          <summary className="fo-muted fo-small" style={{ cursor: "pointer" }}>
            All {entries.length} recorded entries
          </summary>
          <div className="fo-table-wrap">
            <table className="fo-table">
              <thead>
                <tr>
                  <th>Person</th>
                  <th>Skill</th>
                  <th>Level</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((e) => (
                  <tr key={e.id}>
                    <td>{e.personName}</td>
                    <td>{e.skill}</td>
                    <td>{LEVEL_LABEL[e.level] ?? e.level}</td>
                    <td>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => void runAction(() => invoke("delete_skill_entry", { id: e.id }), setActionError, reload)}
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      ) : null}
    </section>
  );
}
