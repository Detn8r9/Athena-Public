import { useMemo, useState } from "react";
import type { GoToTarget } from "../../shared/navigation/goTo";
import { placesForRank, type Place } from "../../shared/navigation/places";

/** Screen name = first part of the breadcrumb ("Library › Vault" → "Library"). */
function screenOf(p: Place): string {
  return p.where.split("›")[0]!.trim();
}

/**
 * Settings → "Settings on other screens": every setting and tool that lives on its own screen,
 * grouped by screen, each with a button that opens the exact screen, tab and section.
 */
export function ElsewherePanel({ rank, onOpen }: { rank: number; onOpen: (target: GoToTarget) => void }) {
  const [showSections, setShowSections] = useState(false);
  const groups = useMemo(() => {
    const wanted = placesForRank(rank).filter((p) => p.kind === "setting" || (showSections && p.kind === "section"));
    const byScreen = new Map<string, Place[]>();
    for (const p of wanted) {
      const key = screenOf(p);
      byScreen.set(key, [...(byScreen.get(key) ?? []), p]);
    }
    return Array.from(byScreen.entries());
  }, [rank, showSections]);

  return (
    <div className="settings-elsewhere" style={{ display: "grid", gap: "1rem" }}>
      <p className="text-muted small" style={{ margin: 0 }}>
        Some settings live on the screen they control. They are all listed here, so this is the one place to look. Open takes you
        straight there. You can also type any setting into the search box above, or use Find anything (🔍) in the top bar.
      </p>
      <label className="small" style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
        <input type="checkbox" checked={showSections} onChange={(e) => setShowSections(e.target.checked)} />
        Also list every tab and section (not only settings)
      </label>
      {groups.map(([screen, items]) => (
        <section key={screen} className="settings-elsewhere__group" aria-label={screen}>
          <h4 style={{ margin: "0 0 0.4rem" }}>{screen}</h4>
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.45rem" }}>
            {items.map((p) => (
              <li
                key={p.id}
                className="settings-elsewhere__item"
                style={{ display: "flex", gap: "0.75rem", alignItems: "center", flexWrap: "wrap" }}
              >
                <div style={{ display: "grid", gap: 2, flex: "1 1 16rem", minWidth: 0 }}>
                  <strong>{p.title}</strong>
                  <span className="text-muted small">
                    {p.where} — {p.detail}
                  </span>
                </div>
                <button type="button" className="secondary" onClick={() => onOpen(p.target)} aria-label={`Open ${p.title}`}>
                  Open
                </button>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
