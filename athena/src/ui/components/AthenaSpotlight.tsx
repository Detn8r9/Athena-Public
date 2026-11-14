import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { Globe, Search } from "lucide-react";
import {
  runSpotlightSearch,
  type SpotlightAction,
  type SpotlightResult,
} from "../../shared/search/spotlightSearch";

type Props = {
  internetAllowed: boolean;
  onAction: (action: SpotlightAction) => void;
  /** Compact trigger for mobile header. */
  compact?: boolean;
  /** Signed-in rank, so results never point at a screen the user cannot open. */
  rank?: number;
};

export function AthenaSpotlight({ internetAllowed, onAction, compact = false, rank }: Props) {
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SpotlightResult[]>([]);
  const [activeIdx, setActiveIdx] = useState(0);
  const [searching, setSearching] = useState(false);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setResults([]);
    setActiveIdx(0);
  }, []);

  const openPanel = useCallback(() => {
    setOpen(true);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, []);

  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (open) {
          close();
        } else {
          openPanel();
        }
      }
      if (e.key === "Escape" && open) {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, close, openPanel]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open, close]);

  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (!q) {
      setResults([]);
      setSearching(false);
      return;
    }
    let cancelled = false;
    setSearching(true);
    const t = window.setTimeout(() => {
      void runSpotlightSearch({ query: q, internetAllowed, rank }).then((rows) => {
        if (cancelled) return;
        setResults(rows);
        setActiveIdx(0);
        setSearching(false);
      });
    }, 120);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [query, internetAllowed, open, rank]);

  const runResult = useCallback(
    (r: SpotlightResult) => {
      onAction(r.action);
      close();
    },
    [onAction, close],
  );

  const onKeyDownInput = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, Math.max(0, results.length - 1)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const r = results[activeIdx];
      if (r) runResult(r);
    }
  };

  const placeholder = internetAllowed
    ? "Find any screen, setting, person or item… (and the web)"
    : "Find any screen, setting, person or item…";

  return (
    <div className={`athena-spotlight${compact ? " athena-spotlight--compact" : ""}`} ref={rootRef}>
      {!open ? (
        <button
          type="button"
          className="athena-spotlight-trigger"
          onClick={openPanel}
          title="Find anything: screens, settings, people, items (⌘K / Ctrl+K)"
          aria-label="Find anything"
        >
          <Search size={compact ? 18 : 16} strokeWidth={2} aria-hidden />
          {!compact ? <span className="athena-spotlight-trigger-label">Find anything</span> : null}
          {!compact ? <kbd className="athena-spotlight-kbd">⌘K</kbd> : null}
        </button>
      ) : (
        <div className="athena-spotlight-panel" role="combobox" aria-expanded aria-owns={listId} aria-haspopup="listbox">
          <div className="athena-spotlight-input-row">
            <Search size={16} strokeWidth={2} aria-hidden className="athena-spotlight-input-icon" />
            <input
              ref={inputRef}
              type="search"
              className="athena-spotlight-input"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onKeyDownInput}
              placeholder={placeholder}
              aria-autocomplete="list"
              aria-controls={listId}
              aria-activedescendant={results[activeIdx] ? `${listId}-${results[activeIdx].id}` : undefined}
              autoComplete="off"
              autoCorrect="off"
              spellCheck={false}
            />
            {!internetAllowed ? (
              <span className="athena-spotlight-offline-pill" title="Local search only">
                Offline
              </span>
            ) : (
              <span title="Local + web" aria-label="Online search available">
                <Globe size={14} strokeWidth={2} aria-hidden className="athena-spotlight-online-icon" />
              </span>
            )}
          </div>
          {!query.trim() && !searching ? (
            <p className="athena-spotlight-empty text-muted" style={{ margin: "0.5rem 0.75rem" }}>
              Type what you are looking for: a screen, a setting or a thing. For example: vault, home assistant,
              satellite, road data, inventory, radio plan.
            </p>
          ) : null}
          {(query.trim() || searching) && (
            <ul id={listId} className="athena-spotlight-results" role="listbox" aria-label="Search results">
              {searching && results.length === 0 ? (
                <li className="athena-spotlight-empty text-muted">Searching…</li>
              ) : null}
              {!searching && query.trim() && results.length === 0 ? (
                <li className="athena-spotlight-empty text-muted">
                  {internetAllowed ? "No local matches — try a web search below if it appears." : "No matches (offline)."}
                </li>
              ) : null}
              {results.map((r, idx) => (
                <li key={r.id} role="option" id={`${listId}-${r.id}`} aria-selected={idx === activeIdx}>
                  <button
                    type="button"
                    className={`athena-spotlight-result${idx === activeIdx ? " athena-spotlight-result--active" : ""}`}
                    onMouseEnter={() => setActiveIdx(idx)}
                    onClick={() => runResult(r)}
                  >
                    <span className="athena-spotlight-result-main">
                      <span className="athena-spotlight-result-title">{r.title}</span>
                      {r.subtitle ? <span className="athena-spotlight-result-sub text-muted">{r.subtitle}</span> : null}
                    </span>
                    {r.badge ? <span className="athena-spotlight-badge">{r.badge}</span> : null}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
