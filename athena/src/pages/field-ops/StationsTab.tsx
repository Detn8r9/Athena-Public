import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDate, formatDateTime, messageOf, runAction, useCommand } from "./ui";

type TrustView = {
  hasKey: boolean;
  keyFingerprint: string | null;
  signOutgoing: boolean;
  hasDuressWord: boolean;
  stationName: string | null;
};

type GroupInfo = { id: string; label: string; count: number };

type BundleTrust = "verified" | "unsigned" | "cannot_check" | "tampered";

type ImportSummary = {
  station: string;
  createdAt: number;
  trust: BundleTrust;
  recordsInBundle: number;
  merged: { added: number; updated: number; unchanged: number } | null;
};

type Stored<V> = { id: string; createdAt: number; updatedAt: number; value: V };

const TRUST_TEXT: Record<BundleTrust, string> = {
  verified: "Signed with your group key and unchanged.",
  unsigned: "Not signed. There is no way to tell who made it.",
  cannot_check: "Signed, but this station has no group key to check it with.",
  tampered: "Does not match your group key. It was changed, or it came from another group.",
};

function TrustSection() {
  const { data, error, reload } = useCommand<TrustView>("get_trust_settings");
  const [view, setView] = useState<TrustView | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [newKey, setNewKey] = useState<string | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [duress, setDuress] = useState("");
  const [station, setStation] = useState("");

  const current = view ?? data;

  const apply = async (command: string, args: Record<string, unknown>) => {
    try {
      setView(await invoke<TrustView>(command, args));
      setActionError(null);
      return true;
    } catch (e) {
      setActionError(messageOf(e));
      return false;
    }
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Group key">
      <div className="fo-card-head">
        <h2>Group key and duress word</h2>
        {current ? <Badge tone={current.hasKey ? "good" : undefined}>{current.hasKey ? `Key set · ${current.keyFingerprint}` : "No key"}</Badge> : null}
      </div>
      <p className="fo-lead">
        Stations that share a key can tell each other's messages and courier bundles from forgeries. Make the key on
        one station and carry it to the others by hand. If a radio or laptop is lost or taken, make a new key and
        share it again. Only senior members can change these.
      </p>
      <ErrorNote message={error ?? actionError} />

      {newKey ? (
        <article className="glass-panel fo-card">
          <strong>New group key. Copy it to your other stations now; it will not be shown again.</strong>
          <p className="fo-prewrap" style={{ fontFamily: "ui-monospace, monospace", userSelect: "all" }}>
            {newKey}
          </p>
          <div className="fo-actions">
            <button type="button" className="secondary" onClick={() => setNewKey(null)}>
              I have copied it
            </button>
          </div>
        </article>
      ) : null}

      <div className="fo-row">
        <button
          type="button"
          className="secondary"
          onClick={() => {
            if (current?.hasKey && !window.confirm("Replace the group key? Every other station will need the new key.")) return;
            void invoke<string>("generate_group_key")
              .then(async (k) => {
                setNewKey(k);
                setActionError(null);
                setView(null);
                await reload();
              })
              .catch((e) => setActionError(messageOf(e)));
          }}
        >
          {current?.hasKey ? "Make a new key" : "Make a group key"}
        </button>
        {current?.hasKey ? (
          <button
            type="button"
            className="secondary"
            onClick={() => {
              if (window.confirm("Remove the group key from this station?")) void apply("set_group_key", { key: null });
            }}
          >
            Remove key
          </button>
        ) : null}
      </div>

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void apply("set_group_key", { key: keyInput }).then((ok) => ok && setKeyInput(""));
        }}
      >
        <Field label="Enter a key from another station, or a shared passphrase (12+ characters)" grow>
          <input className="fo-input" value={keyInput} onChange={(e) => setKeyInput(e.target.value)} autoComplete="off" />
        </Field>
        <button type="submit" className="secondary" disabled={!keyInput.trim()}>
          Use this key
        </button>
      </form>
      {current?.hasKey ? (
        <p className="fo-small fo-muted">
          To confirm two stations hold the same key, compare the four-character code shown above ({current.keyFingerprint}).
        </p>
      ) : null}

      <label className="fo-small" style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
        <input
          type="checkbox"
          checked={current?.signOutgoing ?? false}
          disabled={!current?.hasKey}
          onChange={(e) => void apply("set_trust_options", { signOutgoing: e.target.checked })}
        />
        Sign the radio messages this station sends (adds 14 characters; station clocks must agree within 30 minutes)
      </label>

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void apply("set_trust_options", { duressWord: duress }).then((ok) => ok && setDuress(""));
        }}
      >
        <Field label={current?.hasDuressWord ? "Duress word is set. Enter a new one to change it" : "Duress word (one ordinary-sounding word)"} grow>
          <input className="fo-input" value={duress} onChange={(e) => setDuress(e.target.value)} autoComplete="off" />
        </Field>
        <button type="submit" className="secondary" disabled={!duress.trim()}>
          Save duress word
        </button>
        {current?.hasDuressWord ? (
          <button type="button" className="secondary" onClick={() => void apply("set_trust_options", { duressWord: "" })}>
            Remove
          </button>
        ) : null}
      </form>
      <p className="fo-small fo-muted">
        Anyone forced to send a message works the duress word into it. The message looks normal and gets a normal
        reply, and every station that knows the word raises an urgent alert.
      </p>

      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void apply("set_trust_options", { stationName: station }).then((ok) => ok && setStation(""));
        }}
      >
        <Field label={`This station's name${current?.stationName ? ` (now "${current.stationName}")` : ""}`} grow>
          <input className="fo-input" value={station} onChange={(e) => setStation(e.target.value)} placeholder="Ridge outpost" />
        </Field>
        <button type="submit" className="secondary" disabled={!station.trim()}>
          Save name
        </button>
      </form>
    </section>
  );
}

function CourierSection() {
  const { data: groups, error, reload } = useCommand<GroupInfo[]>("list_courier_groups");
  const [chosen, setChosen] = useState<string[]>(["knowledge", "coordination", "federation"]);
  const [actionError, setActionError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [pending, setPending] = useState<{ path: string; summary: ImportSummary } | null>(null);

  const toggle = (id: string) => setChosen((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id]));

  const exportBundle = async () => {
    try {
      const path = await save({
        defaultPath: `athena-courier-${new Date().toISOString().slice(0, 10)}.athena-bundle`,
        filters: [{ name: "ATHENA courier bundle", extensions: ["athena-bundle"] }],
      });
      if (!path) return;
      const out = await invoke<{ path: string; records: number; signed: boolean }>("export_courier_bundle", { path, groups: chosen });
      setMessage(`Wrote ${out.records} record${out.records === 1 ? "" : "s"} to ${out.path}${out.signed ? ", signed with your group key." : ". It is not signed because this station has no group key."}`);
      setActionError(null);
    } catch (e) {
      setActionError(messageOf(e));
    }
  };

  const pick = async () => {
    try {
      const path = await open({ multiple: false, filters: [{ name: "ATHENA courier bundle", extensions: ["athena-bundle", "json"] }] });
      if (typeof path !== "string") return;
      const summary = await invoke<ImportSummary>("import_courier_bundle", { path, apply: false });
      setPending({ path, summary });
      setMessage(null);
      setActionError(null);
    } catch (e) {
      setPending(null);
      setActionError(messageOf(e));
    }
  };

  const importNow = async (acceptUnverified: boolean) => {
    if (!pending) return;
    try {
      const done = await invoke<ImportSummary>("import_courier_bundle", { path: pending.path, apply: true, acceptUnverified });
      const m = done.merged;
      setMessage(m ? `Imported from ${done.station}: ${m.added} new, ${m.updated} updated, ${m.unchanged} already up to date.` : "Imported.");
      setPending(null);
      setActionError(null);
      await reload();
    } catch (e) {
      setActionError(messageOf(e));
    }
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Courier sync">
      <h2>Courier sync</h2>
      <p className="fo-lead">
        Carry records to another station on a USB stick when there is no radio or network link. Import merges them:
        the newest version of each record wins and deletions carry over, so the same stick can go back and forth.
      </p>
      <ErrorNote message={error ?? actionError} />
      {message ? <p className="fo-small">{message}</p> : null}

      <h3>Send</h3>
      <div className="fo-cards">
        {(groups ?? []).map((g) => (
          <label key={g.id} className="fo-small" style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
            <input type="checkbox" checked={chosen.includes(g.id)} onChange={() => toggle(g.id)} />
            {g.label} <span className="fo-muted">({g.count})</span>
          </label>
        ))}
      </div>
      <div className="fo-row">
        <button type="button" className="secondary" disabled={chosen.length === 0} onClick={() => void exportBundle()}>
          Save bundle to a drive…
        </button>
      </div>
      <p className="fo-small fo-muted">Group keys, settings and passwords are never included.</p>

      <h3>Receive</h3>
      <div className="fo-row">
        <button type="button" className="secondary" onClick={() => void pick()}>
          Open a bundle…
        </button>
      </div>
      {pending ? (
        <article className="glass-panel fo-card">
          <div className="fo-card-head">
            <strong>
              From {pending.summary.station}, made {formatDateTime(pending.summary.createdAt)}
            </strong>
            <Badge tone={pending.summary.trust === "verified" ? "good" : pending.summary.trust === "tampered" ? "bad" : "warn"}>
              {pending.summary.trust === "verified" ? "Verified" : pending.summary.trust === "tampered" ? "Do not trust" : "Unverified"}
            </Badge>
          </div>
          <p className="fo-small">
            {pending.summary.recordsInBundle} record{pending.summary.recordsInBundle === 1 ? "" : "s"}. {TRUST_TEXT[pending.summary.trust]}
          </p>
          <div className="fo-actions">
            {pending.summary.trust === "verified" ? (
              <button type="button" className="secondary" onClick={() => void importNow(false)}>
                Import
              </button>
            ) : null}
            {pending.summary.trust === "unsigned" || pending.summary.trust === "cannot_check" ? (
              <button
                type="button"
                className="secondary"
                onClick={() => {
                  if (window.confirm("This bundle cannot be verified. Import it only if you trust where the drive came from. Import anyway?")) {
                    void importNow(true);
                  }
                }}
              >
                Import anyway
              </button>
            ) : null}
            <button type="button" className="secondary" onClick={() => setPending(null)}>
              Cancel
            </button>
          </div>
        </article>
      ) : null}
    </section>
  );
}

type FedPost = { kind: "need" | "offer"; what: string; group: string; contact: string; open: boolean };
type FedPerson = { name: string; description: string; lastSeen: string; soughtBy: string; contact: string; found: boolean };

function FederationSection() {
  const posts = useCommand<Array<Stored<FedPost>>>("list_field_records", { kind: "fed_post" });
  const people = useCommand<Array<Stored<FedPerson>>>("list_field_records", { kind: "fed_person" });
  const [actionError, setActionError] = useState<string | null>(null);
  const [post, setPost] = useState<FedPost>({ kind: "need", what: "", group: "", contact: "", open: true });
  const [person, setPerson] = useState<FedPerson>({ name: "", description: "", lastSeen: "", soughtBy: "", contact: "", found: false });

  const savePost = (value: FedPost, id?: string) =>
    runAction(() => invoke("save_field_record", { kind: "fed_post", id, value }), setActionError, posts.reload);
  const savePerson = (value: FedPerson, id?: string) =>
    runAction(() => invoke("save_field_record", { kind: "fed_person", id, value }), setActionError, people.reload);

  const sortedPosts = [...(posts.data ?? [])].sort((a, b) => Number(b.value.open) - Number(a.value.open) || b.updatedAt - a.updatedAt);
  const sortedPeople = [...(people.data ?? [])].sort((a, b) => Number(a.value.found) - Number(b.value.found) || b.updatedAt - a.updatedAt);

  return (
    <>
      <section className="glass-panel fo-panel" aria-label="Needs and offers">
        <h2>Needs and offers</h2>
        <p className="fo-lead">
          What your group needs and what it can spare. Include this board in a courier bundle and other groups see it
          alongside their own, so trades can be arranged between settlements.
        </p>
        <ErrorNote message={posts.error ?? actionError} />
        {sortedPosts.length === 0 ? (
          <p className="fo-empty">Nothing posted yet.</p>
        ) : (
          <div className="fo-table-wrap">
            <table className="fo-table">
              <thead>
                <tr>
                  <th>Type</th>
                  <th>What</th>
                  <th>Group</th>
                  <th>Contact</th>
                  <th>Posted</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {sortedPosts.map((p) => (
                  <tr key={p.id} className={p.value.open ? undefined : "fo-muted"}>
                    <td>
                      <Badge tone={p.value.kind === "need" ? "warn" : "good"}>{p.value.kind === "need" ? "Need" : "Offer"}</Badge>
                    </td>
                    <td>
                      {p.value.what}
                      {p.value.open ? "" : " (closed)"}
                    </td>
                    <td>{p.value.group}</td>
                    <td>{p.value.contact}</td>
                    <td>{formatDate(p.createdAt)}</td>
                    <td>
                      <div className="fo-actions">
                        <button type="button" className="secondary" onClick={() => void savePost({ ...p.value, open: !p.value.open }, p.id)}>
                          {p.value.open ? "Close" : "Reopen"}
                        </button>
                        <button
                          type="button"
                          className="secondary"
                          onClick={() => void runAction(() => invoke("delete_field_record", { kind: "fed_post", id: p.id }), setActionError, posts.reload)}
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
        )}
        <form
          className="fo-row"
          onSubmit={(e) => {
            e.preventDefault();
            void savePost(post).then((ok) => ok && setPost((p) => ({ ...p, what: "" })));
          }}
        >
          <Field label="Type">
            <select value={post.kind} onChange={(e) => setPost((p) => ({ ...p, kind: e.target.value as "need" | "offer" }))}>
              <option value="need">We need</option>
              <option value="offer">We can offer</option>
            </select>
          </Field>
          <Field label="What" grow>
            <input className="fo-input" value={post.what} onChange={(e) => setPost((p) => ({ ...p, what: e.target.value }))} placeholder="Antibiotics; can trade diesel" />
          </Field>
          <Field label="Group">
            <input className="fo-input" value={post.group} onChange={(e) => setPost((p) => ({ ...p, group: e.target.value }))} />
          </Field>
          <Field label="How to reach you">
            <input className="fo-input" value={post.contact} onChange={(e) => setPost((p) => ({ ...p, contact: e.target.value }))} placeholder="Channel 3 at 19:00" />
          </Field>
          <button type="submit" className="secondary" disabled={!post.what.trim()}>
            Post
          </button>
        </form>
      </section>

      <section className="glass-panel fo-panel" aria-label="People finder">
        <h2>People finder</h2>
        <p className="fo-lead">
          People who are being looked for. Carried between groups in courier bundles, so someone searching in one place
          can be matched with someone found in another.
        </p>
        <ErrorNote message={people.error} />
        {sortedPeople.length === 0 ? (
          <p className="fo-empty">Nobody listed.</p>
        ) : (
          <div className="fo-cards">
            {sortedPeople.map((p) => (
              <article key={p.id} className="glass-panel fo-card">
                <div className="fo-card-head">
                  <strong>{p.value.name}</strong>
                  <Badge tone={p.value.found ? "good" : "warn"}>{p.value.found ? "Found" : "Missing"}</Badge>
                </div>
                {p.value.description ? <p className="fo-small fo-prewrap">{p.value.description}</p> : null}
                <div className="fo-small fo-muted">
                  {[
                    p.value.lastSeen ? `Last seen: ${p.value.lastSeen}` : null,
                    p.value.soughtBy ? `Sought by: ${p.value.soughtBy}` : null,
                    p.value.contact ? `Contact: ${p.value.contact}` : null,
                    `Listed ${formatDate(p.createdAt)}`,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </div>
                <div className="fo-actions">
                  <button type="button" className="secondary" onClick={() => void savePerson({ ...p.value, found: !p.value.found }, p.id)}>
                    {p.value.found ? "Mark as still missing" : "Mark as found"}
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => {
                      if (window.confirm(`Remove ${p.value.name} from the list?`)) {
                        void runAction(() => invoke("delete_field_record", { kind: "fed_person", id: p.id }), setActionError, people.reload);
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
        <form
          className="fo-cards"
          onSubmit={(e) => {
            e.preventDefault();
            void savePerson(person).then((ok) => ok && setPerson({ name: "", description: "", lastSeen: "", soughtBy: person.soughtBy, contact: person.contact, found: false }));
          }}
        >
          <div className="fo-row">
            <Field label="Name" grow>
              <input className="fo-input" value={person.name} onChange={(e) => setPerson((p) => ({ ...p, name: e.target.value }))} />
            </Field>
            <Field label="Last seen (where and when)" grow>
              <input className="fo-input" value={person.lastSeen} onChange={(e) => setPerson((p) => ({ ...p, lastSeen: e.target.value }))} />
            </Field>
          </div>
          <div className="fo-row">
            <Field label="Description (age, appearance, anything that identifies them)" grow>
              <input className="fo-input" value={person.description} onChange={(e) => setPerson((p) => ({ ...p, description: e.target.value }))} />
            </Field>
          </div>
          <div className="fo-row">
            <Field label="Who is looking for them" grow>
              <input className="fo-input" value={person.soughtBy} onChange={(e) => setPerson((p) => ({ ...p, soughtBy: e.target.value }))} />
            </Field>
            <Field label="How to reach the searcher" grow>
              <input className="fo-input" value={person.contact} onChange={(e) => setPerson((p) => ({ ...p, contact: e.target.value }))} />
            </Field>
            <button type="submit" className="secondary" disabled={!person.name.trim()}>
              Add to list
            </button>
          </div>
        </form>
      </section>
    </>
  );
}

export function StationsTab() {
  return (
    <div className="fo-cards">
      <CourierSection />
      <FederationSection />
      <TrustSection />
    </div>
  );
}
