/**
 * One "Downloads" screen with two sides:
 *  - Browse catalog: the curated store (models, maps, apps, library packs) — `marketplace` view.
 *  - Files & queue: raw files, packages, the download queue and disk — `data_acquisition` view.
 * Both views keep their own tabs; this bar is the shared title and the switch between them.
 */
export type DownloadsMode = "browse" | "files";

const MODES: ReadonlyArray<{ id: DownloadsMode; label: string; hint: string }> = [
  {
    id: "browse",
    label: "Browse catalog",
    hint: "Pick AI models, maps, apps and library packs from the catalog.",
  },
  {
    id: "files",
    label: "Files & queue",
    hint: "Fetch specific files and courses, watch the download queue, and check disk space.",
  },
];

export function DownloadsSwitcher({ mode, onChange }: { mode: DownloadsMode; onChange: (mode: DownloadsMode) => void }) {
  const active = MODES.find((m) => m.id === mode) ?? MODES[0];
  return (
    <header
      className="glass-panel downloads-switch"
      style={{ flexShrink: 0, display: "grid", gap: "0.4rem", padding: "0.7rem 1rem", margin: "0.5rem 0.75rem 0" }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "0.75rem", flexWrap: "wrap" }}>
        <h1 style={{ margin: 0, fontSize: "1.25rem" }}>Downloads</h1>
        <div role="tablist" aria-label="Downloads sections" style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
          {MODES.map((m) => (
            <button
              key={m.id}
              type="button"
              role="tab"
              aria-selected={m.id === mode}
              className={m.id === mode ? "glass-action-btn" : "secondary"}
              onClick={() => {
                if (m.id !== mode) onChange(m.id);
              }}
            >
              {m.label}
            </button>
          ))}
        </div>
      </div>
      <p className="text-muted small" style={{ margin: 0 }}>
        {active.hint}
      </p>
    </header>
  );
}
