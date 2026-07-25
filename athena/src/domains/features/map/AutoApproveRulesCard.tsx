import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";

type RuleRow = {
  id: string;
  entityType: string;
  action: string;
  predicateJson: string;
  enabled: boolean;
  createdAt: string;
};

/**
 * Rank 5: manage `ai_autoapprove_rules`.
 */
export function AutoApproveRulesCard() {
  const [rules, setRules] = useState<RuleRow[]>([]);
  const [entityType, setEntityType] = useState("map_marker");
  const [action, setAction] = useState("create");
  const [pred, setPred] = useState('{"field":"name","op":"=","value":"test"}');
  const [err, setErr] = useState("");

  const load = useCallback(async () => {
    try {
      const list = await invoke<RuleRow[]>("ai_get_autoapprove_rules");
      setRules(Array.isArray(list) ? list : []);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <section className="library-section">
      <h3>AI auto-approve (Rank 5)</h3>
      <p className="text-muted small">When a staged change matches a rule predicate, it is auto-approved. Use simple JSON predicates evaluated server-side.</p>
      {err ? <p className="error small">{err}</p> : null}
      <form
        style={{ display: "grid", gap: 8, maxWidth: 480, marginBottom: 16 }}
        onSubmit={async (e) => {
          e.preventDefault();
          setErr("");
          try {
            await invoke("ai_set_autoapprove_rule", {
              args: {
                id: null,
                entityType: entityType.trim(),
                action: action.trim(),
                predicateJson: pred,
                enabled: true,
              },
            });
            setPred("{}");
            void load();
          } catch (e2) {
            setErr(String(e2));
          }
        }}
      >
        <label>
          Entity
          <input className="settings-input" value={entityType} onChange={(e) => setEntityType(e.target.value)} />
        </label>
        <label>
          Action
          <input className="settings-input" value={action} onChange={(e) => setAction(e.target.value)} />
        </label>
        <label>
          Predicate JSON
          <textarea className="settings-input" rows={4} value={pred} onChange={(e) => setPred(e.target.value)} />
        </label>
        <button type="submit">Add rule</button>
      </form>
      <ul className="text-muted small" style={{ listStyle: "none", padding: 0 }}>
        {rules.map((r) => (
          <li
            key={r.id}
            style={{
              borderTop: "1px solid var(--border)",
              padding: "6px 0",
              display: "flex",
              justifyContent: "space-between",
              gap: 8,
              flexWrap: "wrap",
            }}
          >
            <span>
              {r.entityType} / {r.action} {r.enabled ? "" : "(off)"}
            </span>
            <button
              type="button"
              className="secondary"
              onClick={async () => {
                if (!window.confirm("Delete this rule?")) return;
                try {
                  await invoke("ai_delete_autoapprove_rule", { id: r.id });
                  void load();
                } catch (e2) {
                  window.alert(String(e2));
                }
              }}
            >
              Delete
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
