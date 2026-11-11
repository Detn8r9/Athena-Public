import { useCallback, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";

type LibrarySearchTestHit = {
  title: string;
  filePath: string;
  pageNumber: number | null;
  score: number;
  relevant: boolean;
  reranked: boolean;
  preview: string;
};

type Status =
  | { kind: "idle" }
  | { kind: "searching" }
  | { kind: "done"; hits: LibrarySearchTestHit[]; query: string }
  | { kind: "error"; message: string };

/**
 * Runs the same retrieval chat uses and shows what it found, so you can confirm a document is
 * findable before asking the assistant about it.
 */
export function LibrarySearchTest() {
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  const run = useCallback(async () => {
    const q = query.trim();
    if (!q) return;
    setStatus({ kind: "searching" });
    try {
      const hits = await invoke<LibrarySearchTestHit[]>("test_library_search", { query: q });
      setStatus({ kind: "done", hits: Array.isArray(hits) ? hits : [], query: q });
    } catch (e) {
      setStatus({ kind: "error", message: e != null ? String(e) : "Search failed." });
    }
  }, [query]);

  const used = status.kind === "done" ? status.hits.filter((h) => h.relevant) : [];
  const skipped = status.kind === "done" ? status.hits.filter((h) => !h.relevant) : [];
  const noReranker = status.kind === "done" && status.hits.length > 0 && !status.hits[0]!.reranked;

  return (
    <div className="glass-panel" style={{ padding: "0.6rem", display: "grid", gap: "0.45rem" }}>
      <div style={{ color: "var(--primary)" }}>Test library search</div>
      <div className="text-muted small">
        Ask a question the way you would in chat to see which passages the assistant would be given.
      </div>
      <form
        style={{ display: "grid", gap: "0.4rem" }}
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="e.g. how often do I change the generator oil?"
          className="map-chat-input"
          aria-label="Question to test against the library"
        />
        <button type="submit" className="secondary" disabled={status.kind === "searching" || !query.trim()}>
          {status.kind === "searching" ? "Searching…" : "Search library"}
        </button>
      </form>

      {status.kind === "error" ? (
        <div className="small" role="alert" style={{ color: "var(--danger, var(--error, inherit))" }}>
          {status.message}
        </div>
      ) : null}

      {status.kind === "done" ? (
        <div style={{ display: "grid", gap: "0.45rem" }} aria-live="polite">
          <div className="small">
            {used.length === 0
              ? "Nothing in your library matches this question, so chat would answer without your documents."
              : `Chat would use ${used.length} passage${used.length === 1 ? "" : "s"} for this question.`}
          </div>
          {noReranker ? (
            <div className="text-muted small">
              The reranker model is not installed, so results are ordered by keyword and meaning match only.
              Install it from the Marketplace for more accurate answers.
            </div>
          ) : null}
          {used.map((h, i) => (
            <HitRow key={`u-${i}`} hit={h} />
          ))}
          {skipped.length > 0 ? (
            <details>
              <summary className="text-muted small" style={{ cursor: "pointer" }}>
                {skipped.length} weaker match{skipped.length === 1 ? "" : "es"} not used
              </summary>
              <div style={{ display: "grid", gap: "0.45rem", marginTop: "0.45rem" }}>
                {skipped.map((h, i) => (
                  <HitRow key={`s-${i}`} hit={h} />
                ))}
              </div>
            </details>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function HitRow({ hit }: { hit: LibrarySearchTestHit }) {
  return (
    <button
      type="button"
      className="glass-panel"
      title="Open this document"
      onClick={() =>
        void invoke("open_library_source", { path: hit.filePath, page: hit.pageNumber }).catch(() => {})
      }
      style={{
        padding: "0.45rem",
        textAlign: "left",
        cursor: "pointer",
        display: "grid",
        gap: 3,
        opacity: hit.relevant ? 1 : 0.7,
        font: "inherit",
        color: "inherit",
      }}
    >
      <span style={{ fontWeight: 700, fontSize: "0.82rem", wordBreak: "break-word" }}>
        {hit.title}
        {hit.pageNumber != null ? ` · page ${hit.pageNumber}` : ""}
      </span>
      <span className="text-muted small" style={{ wordBreak: "break-word" }}>
        {hit.preview}
        {hit.preview.length >= 280 ? "…" : ""}
      </span>
    </button>
  );
}
