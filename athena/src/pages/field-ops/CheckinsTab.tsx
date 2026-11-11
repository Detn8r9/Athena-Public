import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, formatSpan, runAction, useCommand, type Stored } from "./ui";

type CheckinState = "paused" | "ok" | "due_soon" | "late" | "overdue";

type CheckinRow = Stored<{
  name: string;
  nodeId: number | null;
  intervalHours: number;
  graceMinutes: number;
  active: boolean;
  lastManualAt: number | null;
  lastNote: string | null;
  startedAt: number | null;
  status: {
    state: CheckinState;
    lastContactAt: number | null;
    dueAt: number;
    lateBySecs: number;
    lastWasRadio: boolean;
  };
}>;

const STATE_LABEL: Record<CheckinState, string> = {
  paused: "Paused",
  ok: "On time",
  due_soon: "Due soon",
  late: "Late",
  overdue: "Overdue",
};

const STATE_TONE: Record<CheckinState, "bad" | "warn" | "good" | undefined> = {
  paused: undefined,
  ok: "good",
  due_soon: "warn",
  late: "warn",
  overdue: "bad",
};

export function CheckinsTab() {
  const { data, error, loading, reload } = useCommand<CheckinRow[]>("list_checkin_plans");
  const [actionError, setActionError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [intervalHours, setIntervalHours] = useState("12");
  const [graceMinutes, setGraceMinutes] = useState("30");
  const [nodeId, setNodeId] = useState("");

  const rows = data ?? [];

  const add = () =>
    runAction(
      () =>
        invoke("save_checkin_plan", {
          plan: {
            name,
            nodeId: nodeId.trim() ? Number.parseInt(nodeId, 10) : null,
            intervalHours: Number.parseFloat(intervalHours),
            graceMinutes: Number.parseInt(graceMinutes, 10) || 0,
            active: true,
          },
        }),
      setActionError,
      async () => {
        setName("");
        setNodeId("");
        await reload();
      },
    );

  return (
    <section className="glass-panel fo-panel" aria-label="Check-ins">
      <h2>Check-ins</h2>
      <p className="fo-lead">
        Set how often each person or team should report in. A message from their radio counts automatically; for
        anything else, press "Checked in". Overdue check-ins appear on the Dashboard.
      </p>
      <ErrorNote message={error ?? actionError} />

      {rows.length === 0 && !loading ? (
        <p className="fo-empty">No check-in plans yet. Add the first one below.</p>
      ) : (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Who</th>
                <th>Every</th>
                <th>Last contact</th>
                <th>Next due</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id}>
                  <td>
                    {r.name}
                    {r.nodeId != null ? <div className="fo-muted fo-small">Radio {r.nodeId}</div> : null}
                  </td>
                  <td>{r.intervalHours} h</td>
                  <td>
                    {formatDateTime(r.status.lastContactAt)}
                    {r.status.lastContactAt ? (
                      <div className="fo-muted fo-small">
                        {r.status.lastWasRadio ? "by radio" : "marked by hand"}
                        {!r.status.lastWasRadio && r.lastNote ? ` · ${r.lastNote}` : ""}
                      </div>
                    ) : null}
                  </td>
                  <td>{r.active ? formatDateTime(r.status.dueAt) : "—"}</td>
                  <td>
                    <Badge tone={STATE_TONE[r.status.state]}>{STATE_LABEL[r.status.state]}</Badge>
                    {r.status.lateBySecs > 0 && r.active ? (
                      <div className="fo-muted fo-small">{formatSpan(r.status.lateBySecs)} late</div>
                    ) : null}
                  </td>
                  <td>
                    <div className="fo-actions">
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => void runAction(() => invoke("record_checkin", { id: r.id }), setActionError, reload)}
                      >
                        Checked in
                      </button>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() =>
                          void runAction(
                            () =>
                              invoke("save_checkin_plan", {
                                id: r.id,
                                plan: {
                                  name: r.name,
                                  nodeId: r.nodeId,
                                  intervalHours: r.intervalHours,
                                  graceMinutes: r.graceMinutes,
                                  active: !r.active,
                                  lastManualAt: r.lastManualAt,
                                  lastNote: r.lastNote,
                                  // Resuming starts the clock again from now.
                                  startedAt: r.active ? r.startedAt : Math.floor(Date.now() / 1000),
                                },
                              }),
                            setActionError,
                            reload,
                          )
                        }
                      >
                        {r.active ? "Pause" : "Resume"}
                      </button>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          if (window.confirm(`Remove the check-in plan for ${r.name}?`)) {
                            void runAction(() => invoke("delete_checkin_plan", { id: r.id }), setActionError, reload);
                          }
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3>Add a check-in plan</h3>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <Field label="Person or team" grow>
          <input className="fo-input" value={name} onChange={(e) => setName(e.target.value)} placeholder="North patrol" />
        </Field>
        <Field label="Every (hours)">
          <input
            className="fo-input fo-input--narrow"
            type="number"
            min={0.25}
            max={168}
            step={0.25}
            value={intervalHours}
            onChange={(e) => setIntervalHours(e.target.value)}
          />
        </Field>
        <Field label="Grace (minutes)">
          <input
            className="fo-input fo-input--narrow"
            type="number"
            min={0}
            max={720}
            value={graceMinutes}
            onChange={(e) => setGraceMinutes(e.target.value)}
          />
        </Field>
        <Field label="Radio node number (optional)">
          <input
            className="fo-input"
            inputMode="numeric"
            value={nodeId}
            onChange={(e) => setNodeId(e.target.value.replace(/[^0-9]/g, ""))}
            placeholder="e.g. 3735928559"
          />
        </Field>
        <button type="submit" className="secondary" disabled={!name.trim()}>
          Add plan
        </button>
      </form>
    </section>
  );
}
