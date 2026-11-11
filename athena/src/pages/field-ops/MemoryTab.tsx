import { useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { ErrorNote, Field, formatDate, runAction, useCommand, type Stored } from "./ui";

type MemoryNote = Stored<{ title: string; body: string; tags: string[]; author: string | null }>;

export function MemoryTab() {
  const { data, error, loading, reload } = useCommand<MemoryNote[]>("list_memory_notes");
  const [actionError, setActionError] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [tags, setTags] = useState("");
  const [saving, setSaving] = useState(false);
  const [filter, setFilter] = useState("");

  const notes = data ?? [];
  const q = filter.trim().toLowerCase();
  const shown = q
    ? notes.filter((n) => `${n.title} ${n.body} ${n.tags.join(" ")}`.toLowerCase().includes(q))
    : notes;

  const save = async () => {
    setSaving(true);
    await runAction(
      () =>
        invoke("save_memory_note", {
          note: {
            title,
            body,
            tags: tags
              .split(",")
              .map((t) => t.trim())
              .filter(Boolean),
          },
        }),
      setActionError,
      async () => {
        setTitle("");
        setBody("");
        setTags("");
        await reload();
      },
    );
    setSaving(false);
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Group memory">
      <h2>Group memory</h2>
      <p className="fo-lead">
        Write down what the group has learned: which well is bad, how the pump was fixed, where the safe route runs.
        Each note goes into the library, so anyone can ask about it in Chat and get it back with your name and the date.
      </p>
      <ErrorNote message={error ?? actionError} />

      <form
        className="fo-cards"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <div className="fo-row">
          <Field label="What is this about?" grow>
            <input className="fo-input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="North well is contaminated" />
          </Field>
          <Field label="Topics (separate with commas)" grow>
            <input className="fo-input" value={tags} onChange={(e) => setTags(e.target.value)} placeholder="water, hazard" />
          </Field>
        </div>
        <Field label="What should the group remember?">
          <textarea value={body} onChange={(e) => setBody(e.target.value)} />
        </Field>
        <div>
          <button type="submit" className="secondary" disabled={saving || !title.trim() || !body.trim()}>
            {saving ? "Saving…" : "Save to group memory"}
          </button>
        </div>
      </form>

      <div className="fo-row">
        <h3>
          {notes.length} note{notes.length === 1 ? "" : "s"}
        </h3>
        {notes.length > 3 ? (
          <Field label="Filter">
            <input className="fo-input" value={filter} onChange={(e) => setFilter(e.target.value)} />
          </Field>
        ) : null}
      </div>
      {shown.length === 0 ? (
        <p className="fo-empty">{loading ? "Loading…" : notes.length === 0 ? "Nothing recorded yet." : "No notes match."}</p>
      ) : (
        <div className="fo-cards">
          {shown.map((n) => (
            <article key={n.id} className="glass-panel fo-card">
              <div className="fo-card-head">
                <strong>{n.title}</strong>
                <span className="fo-small fo-muted">
                  {n.author ? `${n.author} · ` : ""}
                  {formatDate(n.createdAt)}
                </span>
              </div>
              <p className="fo-small fo-prewrap">{n.body}</p>
              {n.tags.length > 0 ? <div className="fo-small fo-muted">{n.tags.join(" · ")}</div> : null}
              <div className="fo-actions">
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    if (window.confirm(`Delete "${n.title}" from group memory?`)) {
                      void runAction(() => invoke("delete_memory_note", { id: n.id }), setActionError, reload);
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
    </section>
  );
}
