import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { ErrorNote, Field, formatDate, runAction, useCommand, usePeople, type Stored } from "./ui";

type LedgerEntry = Stored<{ from: string; to: string; amount: number; unit: string; note: string | null; settled: boolean }>;

type LedgerView = {
  entries: LedgerEntry[];
  balances: Array<{ person: string; unit: string; net: number }>;
};

function amount(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
}

export function LedgerTab() {
  const { data, error, loading, reload } = useCommand<LedgerView>("get_ledger");
  const people = usePeople();
  const [actionError, setActionError] = useState<string | null>(null);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [qty, setQty] = useState("");
  const [unit, setUnit] = useState("hours");
  const [note, setNote] = useState("");

  const entries = data?.entries ?? [];
  const balances = data?.balances ?? [];

  const add = () =>
    runAction(
      () =>
        invoke("save_ledger_entry", {
          entry: { from, to, amount: Number.parseFloat(qty), unit, note: note.trim() || null, settled: false },
        }),
      setActionError,
      async () => {
        setQty("");
        setNote("");
        await reload();
      },
    );

  const toggleSettled = (e: LedgerEntry) =>
    runAction(
      () =>
        invoke("save_ledger_entry", {
          id: e.id,
          entry: { from: e.from, to: e.to, amount: e.amount, unit: e.unit, note: e.note, settled: !e.settled },
        }),
      setActionError,
      reload,
    );

  return (
    <section className="glass-panel fo-panel" aria-label="Ledger">
      <h2>Ledger</h2>
      <p className="fo-lead">
        Keep track of labour and goods that change hands, so debts are remembered fairly. Each unit (hours, kg of
        rice, litres of fuel) is balanced on its own.
      </p>
      <ErrorNote message={error ?? actionError} />

      <h3>Where things stand</h3>
      {balances.length === 0 ? (
        <p className="fo-empty">{loading ? "Loading…" : "Everything is even."}</p>
      ) : (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Person</th>
                <th>Position</th>
              </tr>
            </thead>
            <tbody>
              {balances.map((b) => (
                <tr key={`${b.person}-${b.unit}`}>
                  <td>{b.person}</td>
                  <td>{b.net > 0 ? `Is owed ${amount(b.net)} ${b.unit}` : `Owes ${amount(-b.net)} ${b.unit}`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3>Record an exchange</h3>
      <datalist id="fo-ledger-people">
        {people.map((p) => (
          <option key={p.id} value={p.name} />
        ))}
      </datalist>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <Field label="Who gave" grow>
          <input className="fo-input" list="fo-ledger-people" value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label="Who received" grow>
          <input className="fo-input" list="fo-ledger-people" value={to} onChange={(e) => setTo(e.target.value)} />
        </Field>
        <Field label="Amount">
          <input className="fo-input fo-input--narrow" type="number" min={0} step="any" value={qty} onChange={(e) => setQty(e.target.value)} />
        </Field>
        <Field label="Of what">
          <input className="fo-input" value={unit} onChange={(e) => setUnit(e.target.value)} placeholder="hours" />
        </Field>
        <Field label="Note (optional)" grow>
          <input className="fo-input" value={note} onChange={(e) => setNote(e.target.value)} />
        </Field>
        <button type="submit" className="secondary" disabled={!from.trim() || !to.trim() || !qty.trim() || !unit.trim()}>
          Record
        </button>
      </form>

      {entries.length > 0 ? (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Date</th>
                <th>Exchange</th>
                <th>Note</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr key={e.id} className={e.settled ? "fo-muted" : undefined}>
                  <td>{formatDate(e.createdAt)}</td>
                  <td>
                    {e.from} gave {e.to} {amount(e.amount)} {e.unit}
                    {e.settled ? " (settled)" : ""}
                  </td>
                  <td>{e.note ?? ""}</td>
                  <td>
                    <div className="fo-actions">
                      <button type="button" className="secondary" onClick={() => void toggleSettled(e)}>
                        {e.settled ? "Reopen" : "Mark settled"}
                      </button>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => void runAction(() => invoke("delete_ledger_entry", { id: e.id }), setActionError, reload)}
                      >
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </section>
  );
}
