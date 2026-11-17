import { useCallback, useEffect, useMemo, useState } from "react";
import { BookOpen, Brain, Folder, Map as MapIcon, RefreshCw } from "lucide-react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { formatBytesBinary } from "../../shared/format/formatBytesBinary";
import type { InventoryItem, StationInventory } from "./downloadsTypes";
import "./Downloads.css";

type Filter = "all" | "archive" | "map" | "model";

const STATUS_LABEL: Record<string, string> = {
  ready: "Ready",
  partial: "Not finished",
  unreadable: "Can't open",
  duplicate: "Duplicate",
};

const STATUS_TONE: Record<string, string> = {
  ready: "ok",
  partial: "warn",
  unreadable: "bad",
  duplicate: "warn",
};

const GROUPS: Array<{ kind: Exclude<Filter, "all">; title: string; empty: string; icon: typeof BookOpen }> = [
  {
    kind: "archive",
    title: "Library archives",
    empty: "No archives yet. Wikipedia, medical, repair and other libraries are on the Library tab.",
    icon: BookOpen,
  },
  { kind: "map", title: "Maps", empty: "No offline maps yet. Download your area on the Maps tab.", icon: MapIcon },
  { kind: "model", title: "AI models", empty: "No AI models yet. Choose one on the AI models tab.", icon: Brain },
];

/** Windows long-path prefixes (\\?\) mean nothing to people; hide them. */
function plainPath(p: string): string {
  return p.replace(/^\\\\\?\\UNC\\/, "\\\\").replace(/^\\\\\?\\/, "");
}

function ItemRow({ item }: { item: InventoryItem }) {
  const tone = STATUS_TONE[item.status] ?? "idle";
  return (
    <li className="dl-row">
      <div className="dl-row__main">
        <div className="dl-row__title">{item.title}</div>
        <div className="dl-row__meta">{formatBytesBinary(item.bytes)}</div>
        <div className="dl-row__path" title={plainPath(item.location)}>
          {plainPath(item.location)}
        </div>
        {item.note ? <div className={`dl-row__msg dl-row__msg--${tone}`}>{item.note}</div> : null}
      </div>
      <span className={`dl-chip dl-chip--${tone}`}>{STATUS_LABEL[item.status] ?? item.status}</span>
    </li>
  );
}

/**
 * Everything already on the station: archives, maps and models with size, folder and whether
 * they work, plus where each kind of download is saved.
 */
export function StationInventoryView({ onOpenTab }: { onOpenTab: (tab: "packages" | "maps" | "models") => void }) {
  const [inv, setInv] = useState<StationInventory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState<Filter>("all");

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setInv(await invoke<StationInventory>("station_inventory"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const totals = useMemo(() => {
    const t: Record<string, { n: number; bytes: number }> = {};
    for (const it of inv?.items ?? []) {
      const k = it.kind;
      t[k] = t[k] ?? { n: 0, bytes: 0 };
      t[k].n++;
      t[k].bytes += it.bytes;
    }
    return t;
  }, [inv]);

  const problems = (inv?.items ?? []).filter((i) => i.status !== "ready");

  return (
    <div className="dl-queue">
      <section className="dl-card">
        <div className="dl-card__head">
          <h2 className="dl-status__title">On this station</h2>
          <button type="button" className="dl-btn dl-btn--quiet" onClick={() => void load()} disabled={loading}>
            <RefreshCw size={16} aria-hidden className={loading ? "dl-spin" : undefined} /> Refresh
          </button>
        </div>
        {error ? (
          <p className="dl-warn" role="alert">
            {error}
          </p>
        ) : null}
        {inv ? (
          <div className="dl-tiles">
            {GROUPS.map((g) => {
              const t = totals[g.kind] ?? { n: 0, bytes: 0 };
              const Icon = g.icon;
              return (
                <button
                  key={g.kind}
                  type="button"
                  className={"dl-tile" + (filter === g.kind ? " dl-tile--on" : "")}
                  onClick={() => setFilter((f) => (f === g.kind ? "all" : g.kind))}
                  aria-pressed={filter === g.kind}
                >
                  <Icon size={18} aria-hidden />
                  <span className="dl-tile__n">{t.n}</span>
                  <span className="dl-tile__label">{g.title}</span>
                  <span className="dl-tile__size">{formatBytesBinary(t.bytes)}</span>
                </button>
              );
            })}
          </div>
        ) : loading ? (
          <p className="dl-status__detail">Looking through the station…</p>
        ) : null}
        {inv ? (
          <p className="dl-status__detail">
            {inv.libraryReaderRunning
              ? "The library reader is running: archives open in Academy and chat searches them."
              : inv.libraryReaderStarting
                ? "The library reader is opening the archives. With a large library this takes a few minutes after ATHENA starts; chat answers use your own documents until it is ready."
                : "The library reader is not running, so archives can't be opened or searched right now. ATHENA checks it every minute and starts it again."}{" "}
            {inv.indexedDocuments > 0 ? `${inv.indexedDocuments} of your own documents are in the chat index.` : ""}
          </p>
        ) : null}
        {problems.length > 0 ? (
          <p className="dl-warn">
            {problems.length} item{problems.length === 1 ? " needs" : "s need"} attention (marked below).
          </p>
        ) : null}
      </section>

      {inv
        ? GROUPS.filter((g) => filter === "all" || filter === g.kind).map((g) => {
            const items = inv.items.filter((i) => i.kind === g.kind);
            return (
              <section key={g.kind} className="dl-card">
                <div className="dl-card__head">
                  <h3 className="dl-card__title">{g.title}</h3>
                  <button
                    type="button"
                    className="dl-btn dl-btn--quiet"
                    onClick={() => onOpenTab(g.kind === "archive" ? "packages" : g.kind === "map" ? "maps" : "models")}
                  >
                    Get more
                  </button>
                </div>
                {items.length === 0 ? (
                  <p className="dl-status__detail">{g.empty}</p>
                ) : (
                  <ul className="dl-list">
                    {items.map((it) => (
                      <ItemRow key={it.location} item={it} />
                    ))}
                  </ul>
                )}
              </section>
            );
          })
        : null}

      {inv ? (
        <section className="dl-card">
          <h3 className="dl-card__title">
            <Folder size={16} aria-hidden /> Where downloads are saved
          </h3>
          <ul className="dl-list">
            {inv.destinations.map((d) => (
              <li key={d.label} className="dl-row">
                <div className="dl-row__main">
                  <div className="dl-row__title">{d.label}</div>
                  <div className="dl-row__path" title={plainPath(d.folder)}>
                    {plainPath(d.folder)}
                  </div>
                </div>
                {d.freeBytes != null ? <span className="dl-row__meta">{formatBytesBinary(d.freeBytes)} free</span> : null}
              </li>
            ))}
          </ul>
          <p className="dl-status__detail">Library archives and documents follow the download folder set on the Queue tab.</p>
        </section>
      ) : null}
    </div>
  );
}
