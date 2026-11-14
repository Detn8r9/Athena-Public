import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { isAthenaDesktopRuntime } from "../../../shared/ipc/athenaRuntime";

/**
 * Folder picker. On the station it opens the system dialog; in a browser on the network there is
 * no dialog for the station's disks, so it asks for the path instead.
 */
async function pickFolders(multiple: boolean): Promise<string[]> {
  if (isAthenaDesktopRuntime()) {
    const picked = await open({ directory: true, multiple });
    if (!picked) return [];
    return Array.isArray(picked) ? picked : [picked];
  }
  const typed = window.prompt(
    multiple
      ? "Folders on the station to look in (full paths, separated by commas):"
      : "Folder on the station (full path, for example D:\\Manuals):",
  );
  if (!typed) return [];
  const parts = multiple ? typed.split(",") : [typed];
  return parts.map((p) => p.trim()).filter(Boolean);
}
import { athenaInvoke as invoke, athenaListen as listen } from "../../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, messageOf } from "../../../pages/field-ops/ui";
import "../../../pages/field-ops/fieldOps.css";
import { formatBytes, levelTone, LEVEL_LABEL, ALERT_LABEL, alertTone, listingSize, type CoverageLevel, type AlertLevel } from "./vaultFormat";

type Suggestion = { title: string; folder: string; base: string; approxSize: string; why: string };
type TopicCoverage = {
  topic: string;
  label: string;
  level: CoverageLevel;
  documents: number;
  archives: number;
  examples: string[];
  suggestions: Suggestion[];
};
type PackView = {
  fileName: string;
  title: string;
  createdAt: number;
  docCount: number;
  packedBytes: number;
  originalBytes: number;
  textBytes: number;
  indexBytesEstimate: number;
  indexedDocs: number;
  onArchiveDrive: boolean;
  topics: Record<string, number>;
  problem: string | null;
};
type ScanItem = {
  id: string;
  path: string;
  name: string;
  bytes: number;
  topicLabel: string | null;
  score: number;
  private: boolean;
  fillsBlindSpot: boolean;
  large: boolean;
  status: "found" | "imported" | "dismissed";
  importedTo: string | null;
};
type UpdateInfo = {
  installedFile: string;
  installedPath: string;
  latestFile: string;
  latestDate: string;
  url: string;
  sizeText: string;
};
type Alert = { id: string; source: string; level: AlertLevel; title: string; detail: string; time: number; url: string | null };
type FleetStation = { station: string; lastSeen: number; topics: Record<string, string>; archives: string[] };
type Settings = {
  archiveDir: string | null;
  autoImportScan: boolean;
  meshShare: boolean;
  autoUpdate: boolean;
  alertsEnabled: boolean;
  alertRegion: string;
  alertLat: number | null;
  alertLon: number | null;
  fleetEnabled: boolean;
  fleetRelayUrl: string;
  faaEnabled: boolean;
  faaAirports: string;
  newsFeeds: string[];
  watchWords: string[];
  ha: HaSettings;
};
type HaSettings = {
  enabled: boolean;
  url: string;
  webhookId: string;
  /** Always empty from the station; typing one replaces the saved token. */
  token: string;
  tokenSet: boolean;
  notifyLevel: AlertLevel;
  publishSensors: boolean;
  entities: string[];
};
type HaReading = { entityId: string; name: string; state: string; unit: string | null; deviceClass: string | null };
type HaState = { readings: HaReading[]; lastRead: number; lastSent: number; lastError: string | null };
type Overview = {
  settings: Settings;
  station: string;
  coverage: TopicCoverage[];
  packs: PackView[];
  packsDir: string;
  freeBytes: number | null;
  archiveFreeBytes: number | null;
  scanItems: ScanItem[];
  defaultScanRoots: string[];
  updatesCheckedAt: number;
  updates: UpdateInfo[];
  alerts: { fetchedAt: number; alerts: Alert[]; errors: string[] };
  hunker: { active: boolean; startedAt: number; startedBy: string; reason: string };
  hunkerPhrase: string;
  fleet: {
    enabled: boolean;
    hasGroupKey: boolean;
    relayUrl: string;
    lastSync: number;
    lastError: string | null;
    stations: FleetStation[];
    missingHere: Array<[string, string[]]>;
    answers: Array<{ station: string; query: string; titles: string[]; at: number }>;
  };
  peers: number;
  haState: HaState;
};
type PeerAnswer = {
  peerUrl: string;
  station: string;
  hits: Array<{ title: string; relPath: string; fileType: string; bytes: number; snippet: string }>;
  error: string | null;
};
type Progress = { job: string; done: number; total: number; label: string };

export const SECTIONS = [
  ["coverage", "What the library covers"],
  ["scan", "Find on this computer"],
  ["packs", "Packs"],
  ["stations", "Other stations"],
  ["updates", "Updates"],
  ["alerts", "Alerts"],
  ["ha", "Home Assistant"],
  ["settings", "Vault settings"],
] as const;
type SectionId = (typeof SECTIONS)[number][0];

/**
 * The knowledge vault: where the library is thin, what this computer already holds, compressed
 * packs with staged chat search, other stations, updates, and warnings.
 */
export function VaultPanel({ initialSection }: { initialSection?: string | null } = {}) {
  const [data, setData] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // Links from Settings / "find anything" can open a specific Vault section.
  const [section, setSection] = useState<SectionId>(() =>
    SECTIONS.some(([id]) => id === initialSection) ? (initialSection as SectionId) : "coverage",
  );

  const load = useCallback(async () => {
    try {
      setData(await invoke<Overview>("vault_get_overview"));
      setError(null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, []);

  useEffect(() => {
    void load();
    let stop: (() => void) | undefined;
    void listen<Progress>("athena:vault-progress", (ev) => setProgress(ev.payload)).then((u) => {
      stop = u;
    });
    return () => stop?.();
  }, [load]);

  /** Run a vault command that returns the new overview (or something else, then reload). */
  const run = useCallback(
    async <T,>(label: string, action: () => Promise<T>, done?: (result: T) => string | null) => {
      setBusy(label);
      setNote(null);
      setProgress(null);
      try {
        const result = await action();
        if (result && typeof result === "object" && "coverage" in (result as object)) setData(result as unknown as Overview);
        else if (result && typeof result === "object" && "overview" in (result as object)) setData((result as unknown as { overview: Overview }).overview);
        else await load();
        setError(null);
        setNote(done?.(result) ?? null);
      } catch (e) {
        setError(messageOf(e));
      } finally {
        setBusy(null);
        setProgress(null);
      }
    },
    [load],
  );

  if (!data) {
    return (
      <section className="glass-panel fo-panel" aria-label="Vault">
        <h2>Vault</h2>
        <ErrorNote message={error} />
        {!error ? <p className="fo-empty">Loading…</p> : null}
      </section>
    );
  }

  const thin = data.coverage.filter((c) => c.level === "none" || c.level === "thin");
  const actAlerts = data.alerts.alerts.filter((a) => a.level === "act");

  return (
    <div className="fo-cards" aria-busy={busy != null}>
      {data.hunker.active ? (
        <section className="glass-panel fo-panel" role="alert">
          <h2>Hunker-down is active</h2>
          <p>
            Started by {data.hunker.startedBy} on {formatDateTime(data.hunker.startedAt)}. {data.hunker.reason}
          </p>
          <p className="fo-small fo-muted">The radio is silenced for a day. Other stations on the fleet link have been told.</p>
        </section>
      ) : actAlerts.length > 0 ? (
        <section className="glass-panel fo-panel" role="alert">
          <h2>{actAlerts.length === 1 ? "A serious alert needs a decision" : `${actAlerts.length} serious alerts need a decision`}</h2>
          <p>{actAlerts[0].title}</p>
          <button type="button" className="secondary" onClick={() => setSection("alerts")}>
            Go to alerts
          </button>
        </section>
      ) : null}

      <nav className="glass-panel fo-tabs" aria-label="Vault sections">
        {SECTIONS.map(([id, label]) => (
          <button key={id} type="button" className="fo-tab" aria-pressed={section === id} onClick={() => setSection(id)}>
            {label}
            {id === "coverage" && thin.length > 0 ? ` (${thin.length} thin)` : ""}
            {id === "scan" && data.scanItems.filter((i) => i.status === "found").length > 0
              ? ` (${data.scanItems.filter((i) => i.status === "found").length})`
              : ""}
            {id === "updates" && data.updates.length > 0 ? ` (${data.updates.length})` : ""}
          </button>
        ))}
      </nav>

      <ErrorNote message={error} />
      {note ? (
        <p className="fo-small" role="status">
          {note}
        </p>
      ) : null}
      {busy ? (
        <p className="fo-small fo-muted" role="status" aria-live="polite">
          {busy}
          {progress && progress.total > 1 ? ` ${progress.done + 1} of ${progress.total}` : progress && progress.job === "scan" && progress.total === 0 ? ` (${progress.done} files looked at)` : ""}
          {progress?.label ? `: ${progress.label}` : ""}{" "}
          {busy.startsWith("Scanning") || busy.startsWith("Packing") || busy.startsWith("Building") ? (
            <button type="button" className="secondary" onClick={() => void invoke("vault_cancel_job")}>
              Stop
            </button>
          ) : null}
        </p>
      ) : null}

      {section === "coverage" ? <CoverageSection data={data} busy={busy != null} run={run} /> : null}
      {section === "scan" ? <ScanSection data={data} busy={busy != null} run={run} /> : null}
      {section === "packs" ? <PacksSection data={data} busy={busy != null} run={run} /> : null}
      {section === "stations" ? <StationsSection data={data} busy={busy != null} run={run} setError={setError} /> : null}
      {section === "updates" ? <UpdatesSection data={data} busy={busy != null} run={run} /> : null}
      {section === "alerts" ? <AlertsSection data={data} busy={busy != null} run={run} /> : null}
      {section === "ha" ? <HomeAssistantSection data={data} busy={busy != null} run={run} /> : null}
      {section === "settings" ? <SettingsSection data={data} busy={busy != null} run={run} /> : null}
    </div>
  );
}

type Run = <T>(label: string, action: () => Promise<T>, done?: (result: T) => string | null) => Promise<void>;
type SectionProps = { data: Overview; busy: boolean; run: Run };

/** Download a Kiwix archive into the library (the download continues if the screen closes). */
function useArchiveDownload(onDone: (fileName: string) => void) {
  const [active, setActive] = useState<Record<string, number | null>>({});
  const doneRef = useRef(onDone);
  doneRef.current = onDone;
  const ids = useRef<Record<string, string>>({});

  useEffect(() => {
    const stops: Array<() => void> = [];
    void listen<{ downloadId: string; percent: number | null }>("zim_download_progress", (ev) => {
      const name = ids.current[ev.payload.downloadId];
      if (name) setActive((a) => ({ ...a, [name]: ev.payload.percent }));
    }).then((u) => stops.push(u));
    const finish = (id: string, ok: boolean) => {
      const name = ids.current[id];
      if (!name) return;
      delete ids.current[id];
      setActive((a) => {
        const next = { ...a };
        delete next[name];
        return next;
      });
      if (ok) doneRef.current(name);
    };
    void listen<{ downloadId: string }>("zim_download_complete", (ev) => finish(ev.payload.downloadId, true)).then((u) => stops.push(u));
    void listen<{ downloadId: string }>("zim_download_error", (ev) => finish(ev.payload.downloadId, false)).then((u) => stops.push(u));
    return () => stops.forEach((s) => s());
  }, []);

  const start = async (url: string, fileName: string) => {
    const downloadId = `vault-${fileName}-${Date.now()}`;
    ids.current[downloadId] = fileName;
    setActive((a) => ({ ...a, [fileName]: null }));
    try {
      await invoke("download_zim", { args: { url, fileName, downloadId } });
    } catch (e) {
      delete ids.current[downloadId];
      setActive((a) => {
        const next = { ...a };
        delete next[fileName];
        return next;
      });
      throw e;
    }
  };
  return { active, start };
}

function CoverageSection({ data, busy, run }: SectionProps) {
  const { active, start } = useArchiveDownload(() => void run("Refreshing", async () => invoke<Overview>("vault_get_overview")));
  return (
    <section className="glass-panel fo-panel" id="vault-coverage" aria-label="What the library covers">
      <h2>What the library covers</h2>
      <p className="fo-lead">
        Every book, document and encyclopedia on this station, sorted into survival topics. Thin topics come with
        suggested downloads. Downloads need an internet connection and run in the background.
      </p>
      <div className="fo-table-wrap">
        <table className="fo-table">
          <thead>
            <tr>
              <th>Topic</th>
              <th>Coverage</th>
              <th>Books and documents</th>
              <th>Encyclopedias and packs</th>
              <th>Examples</th>
            </tr>
          </thead>
          <tbody>
            {data.coverage.map((c) => (
              <tr key={c.topic}>
                <td>{c.label}</td>
                <td>
                  <Badge tone={levelTone(c.level)}>{LEVEL_LABEL[c.level]}</Badge>
                </td>
                <td>{c.documents}</td>
                <td>{c.archives}</td>
                <td className="fo-small">{c.examples.slice(0, 2).join("; ") || "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {data.coverage
        .filter((c) => c.suggestions.length > 0)
        .map((c) => (
          <div key={c.topic}>
            <h3>Fill the gap: {c.label}</h3>
            <ul className="fo-list">
              {c.suggestions.map((s) => {
                const downloading = Object.keys(active).find((k) => k.startsWith(s.base));
                return (
                  <li key={s.base}>
                    <strong>{s.title}</strong> (about {s.approxSize}). {s.why}{" "}
                    {downloading ? (
                      <span className="fo-small" role="status">
                        Downloading{active[downloading] != null ? ` ${Math.round(active[downloading] ?? 0)}%` : "…"}
                      </span>
                    ) : (
                      <button
                        type="button"
                        className="secondary"
                        disabled={busy}
                        onClick={() =>
                          void run(
                            `Finding the newest ${s.title}`,
                            async () => {
                              const src = await invoke<{ url: string; fileName: string; sizeText: string }>("vault_resolve_source", {
                                folder: s.folder,
                                base: s.base,
                              });
                              await start(src.url, src.fileName);
                              return src;
                            },
                            (src) => `Downloading ${src.fileName} (${listingSize(src.sizeText)}). It appears in the library when it finishes.`,
                          )
                        }
                      >
                        Download
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
    </section>
  );
}

function ScanSection({ data, busy, run }: SectionProps) {
  const [roots, setRoots] = useState<string[]>([]);
  const waiting = data.scanItems.filter((i) => i.status === "found");
  const imported = data.scanItems.filter((i) => i.status === "imported");
  const resolve = (ids: string[], action: "import" | "dismiss") =>
    void run(action === "import" ? "Copying into the library" : "Updating the list", () =>
      invoke<Overview>("vault_resolve_scan_items", { ids, action }),
    );

  return (
    <section className="glass-panel fo-panel" id="vault-scan" aria-label="Find on this computer">
      <h2>Find on this computer</h2>
      <p className="fo-lead">
        ATHENA looks through this computer's folders for manuals, guides, maps, plans and reference material, and
        copies them into the library. Originals stay where they are. Files that fill a thin topic are listed first.
      </p>
      <p className="fo-small fo-muted">
        Never copied: password stores, keys, wallets, bank and tax papers. Personal and location documents (such as a
        supply-cache list) go to a private shelf that is never offered to other stations.
        {data.settings.autoImportScan ? " Finds are copied straight away; files over 200 MB wait for you." : " Finds wait for you to choose."}
      </p>
      <div className="fo-row">
        <span className="fo-small">
          Folders: {(roots.length > 0 ? roots : data.defaultScanRoots).join(", ") || "none found"}
        </span>
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={async () => {
            const picked = await pickFolders(true);
            if (picked.length > 0) setRoots(picked);
          }}
        >
          Choose folders…
        </button>
        <button className="secondary"
          type="button"
          disabled={busy}
          onClick={() =>
            void run(
              "Scanning this computer",
              () => invoke<{ filesSeen: number; found: number; imported: number; waiting: number }>("vault_scan_device", { roots: roots.length > 0 ? roots : null }),
              (r) =>
                `Looked at ${r.filesSeen.toLocaleString()} files. ${r.found} new useful file${r.found === 1 ? "" : "s"}: ${r.imported} copied, ${r.waiting} waiting for you.`,
            )
          }
        >
          Scan now
        </button>
      </div>

      <h3>Waiting for you ({waiting.length})</h3>
      {waiting.length === 0 ? (
        <p className="fo-empty">Nothing waiting.</p>
      ) : (
        <>
          <div className="fo-row">
            <button type="button" className="secondary" disabled={busy} onClick={() => resolve(waiting.map((i) => i.id), "import")}>
              Copy all {waiting.length}
            </button>
          </div>
          <ScanTable items={waiting} busy={busy} onImport={(id) => resolve([id], "import")} onDismiss={(id) => resolve([id], "dismiss")} />
        </>
      )}
      <h3>Copied into the library ({imported.length})</h3>
      {imported.length === 0 ? (
        <p className="fo-empty">Nothing copied yet.</p>
      ) : (
        <ScanTable items={imported} busy={busy} onDismiss={(id) => resolve([id], "dismiss")} dismissLabel="Remove copy" />
      )}
    </section>
  );
}

function ScanTable({
  items,
  busy,
  onImport,
  onDismiss,
  dismissLabel = "Not useful",
}: {
  items: ScanItem[];
  busy: boolean;
  onImport?: (id: string) => void;
  onDismiss: (id: string) => void;
  dismissLabel?: string;
}) {
  return (
    <div className="fo-table-wrap">
      <table className="fo-table">
        <thead>
          <tr>
            <th>File</th>
            <th>Topic</th>
            <th>Size</th>
            <th>Notes</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {items.slice(0, 300).map((i) => (
            <tr key={i.id}>
              <td title={i.path}>{i.name}</td>
              <td>{i.topicLabel ?? "—"}</td>
              <td>{formatBytes(i.bytes)}</td>
              <td>
                {i.fillsBlindSpot ? <Badge tone="good">Fills a gap</Badge> : null} {i.private ? <Badge tone="warn">Private</Badge> : null}{" "}
                {i.large ? <Badge>Large</Badge> : null}
              </td>
              <td>
                {onImport ? (
                  <button type="button" className="secondary" disabled={busy} onClick={() => onImport(i.id)}>
                    Copy
                  </button>
                ) : null}{" "}
                <button type="button" className="secondary" disabled={busy} onClick={() => onDismiss(i.id)}>
                  {dismissLabel}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PacksSection({ data, busy, run }: SectionProps) {
  const [folder, setFolder] = useState("");
  const [title, setTitle] = useState("");
  const [toArchive, setToArchive] = useState(false);
  const totalOriginal = data.packs.reduce((n, p) => n + p.originalBytes, 0);
  const totalPacked = data.packs.reduce((n, p) => n + p.packedBytes, 0);
  const pendingIndex = data.packs.filter((p) => p.indexedDocs === 0).reduce((n, p) => n + p.indexBytesEstimate, 0);

  return (
    <section className="glass-panel fo-panel" id="vault-packs" aria-label="Packs">
      <h2>Packs</h2>
      <p className="fo-lead">
        A pack is a folder of documents compressed into one archive. It stays readable as it is: open it in Academy
        or any Kiwix reader. Every original is kept byte for byte, with a text copy that chat reads. Chat search is
        switched on per pack, when there is room for its index.
      </p>
      {data.packs.length > 0 ? (
        <p className="fo-small">
          {data.packs.length} pack{data.packs.length === 1 ? "" : "s"}: {formatBytes(totalOriginal)} of documents in{" "}
          {formatBytes(totalPacked)}.
          {pendingIndex > 0 ? ` Turning on chat search for the rest needs about ${formatBytes(pendingIndex)}.` : ""} Free on the
          data drive: {data.freeBytes != null ? formatBytes(data.freeBytes) : "unknown"}.
          {data.settings.archiveDir ? ` Free on the archive drive: ${data.archiveFreeBytes != null ? formatBytes(data.archiveFreeBytes) : "not connected"}.` : ""}
        </p>
      ) : null}
      <div className="fo-table-wrap">
        <table className="fo-table">
          <thead>
            <tr>
              <th>Pack</th>
              <th>Documents</th>
              <th>Size</th>
              <th>Chat search</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {data.packs.length === 0 ? (
              <tr>
                <td colSpan={5} className="fo-empty">
                  No packs yet.
                </td>
              </tr>
            ) : (
              data.packs.map((p) => (
                <tr key={p.fileName}>
                  <td title={p.fileName}>
                    {p.title}
                    {p.onArchiveDrive ? " (archive drive)" : ""}
                    {p.problem ? <div className="fo-error fo-small">{p.problem}</div> : null}
                  </td>
                  <td>{p.docCount}</td>
                  <td>
                    {formatBytes(p.packedBytes)}
                    {p.originalBytes > 0 ? <span className="fo-small fo-muted"> from {formatBytes(p.originalBytes)}</span> : null}
                  </td>
                  <td>
                    {p.indexedDocs > 0 ? (
                      <Badge tone="good">On ({p.indexedDocs} documents)</Badge>
                    ) : (
                      <span className="fo-small">Off. Needs about {formatBytes(p.indexBytesEstimate)}</span>
                    )}
                  </td>
                  <td>
                    <button
                      type="button"
                      className="secondary"
                      disabled={busy || p.problem != null}
                      onClick={() =>
                        void run(p.indexedDocs > 0 ? "Turning chat search off" : "Building chat search", () =>
                          invoke<Overview>("vault_set_pack_search", { fileName: p.fileName, enabled: p.indexedDocs === 0 }),
                        )
                      }
                    >
                      {p.indexedDocs > 0 ? "Turn search off" : "Turn search on"}
                    </button>{" "}
                    <button
                      type="button"
                      className="secondary"
                      disabled={busy || p.problem != null}
                      onClick={() =>
                        void run(
                          "Checking every document",
                          () => invoke<number>("vault_verify_pack", { fileName: p.fileName }),
                          (n) => `All ${n} documents in "${p.title}" match their recorded checksums.`,
                        )
                      }
                    >
                      Check
                    </button>{" "}
                    <button
                      type="button"
                      className="secondary"
                      disabled={busy}
                      onClick={() => {
                        if (!window.confirm(`Delete the pack "${p.title}"? The documents it was made from are not touched.`)) return;
                        void run("Deleting", () => invoke<Overview>("vault_delete_pack", { fileName: p.fileName }));
                      }}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <h3>Make a pack</h3>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void run(
            "Packing",
            () => invoke<Overview>("vault_create_pack", { sourceDir: folder, title, toArchiveDrive: toArchive }),
            () => `Packed "${title}". Turn on chat search when you want chat to read it.`,
          );
        }}
      >
        <Field label="Folder of documents" grow>
          <input className="fo-input" value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="Choose a folder" />
        </Field>
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={async () => {
            const picked = (await pickFolders(false))[0];
            if (picked) {
              setFolder(picked);
              if (!title) setTitle(picked.split(/[\\/]/).filter(Boolean).pop() ?? "");
            }
          }}
        >
          Browse…
        </button>
        <Field label="Pack name">
          <input className="fo-input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Vehicle manuals" />
        </Field>
        {data.settings.archiveDir ? (
          <label className="fo-field">
            <span>
              <input type="checkbox" checked={toArchive} onChange={(e) => setToArchive(e.target.checked)} /> Save on the archive drive
            </span>
          </label>
        ) : null}
        <button className="secondary" type="submit" disabled={busy || !folder.trim() || !title.trim()}>
          Make pack
        </button>
      </form>
      <p className="fo-small fo-muted">
        Packs are saved in {data.packsDir}. Plain text shrinks to about a third; PDFs are already compressed and stay
        about the same size, but keep their text copy for search.
      </p>
    </section>
  );
}

function StationsSection({ data, busy, run, setError }: SectionProps & { setError: (m: string | null) => void }) {
  const [query, setQuery] = useState("");
  const [answers, setAnswers] = useState<PeerAnswer[] | null>(null);
  const [copied, setCopied] = useState<Record<string, string>>({});

  return (
    <section className="glass-panel fo-panel" id="vault-stations" aria-label="Other stations">
      <h2>Other stations</h2>
      <p className="fo-lead">
        Ask other ATHENA stations whether they have something on a topic. They answer with titles and short passages;
        you choose what to copy. Private shelves, patient records and group memory are never shared.
      </p>
      <p className="fo-small">
        {data.peers} station{data.peers === 1 ? "" : "s"} on the local network now.
        {data.settings.meshShare ? " This station answers their questions too." : " This station does not answer their questions (Vault settings)."}
      </p>
      <form
        className="fo-row"
        onSubmit={(e) => {
          e.preventDefault();
          void run(
            "Asking the stations nearby",
            async () => {
              const a = await invoke<PeerAnswer[]>("vault_mesh_search", { query });
              setAnswers(a);
              return a;
            },
            (a) => (a.length === 0 ? "No station answered." : null),
          );
        }}
      >
        <Field label="What are you looking for?" grow>
          <input className="fo-input" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="alternator repair" />
        </Field>
        <button className="secondary" type="submit" disabled={busy || !query.trim()}>
          Ask nearby stations
        </button>
        {data.fleet.enabled ? (
          <button
            type="button"
            className="secondary"
            disabled={busy || !query.trim()}
            onClick={() =>
              void run(
                "Asking the fleet",
                () => invoke("vault_fleet_ask", { query }),
                () => "Asked the fleet over the internet. Answers appear below after the next sync (within 5 minutes).",
              )
            }
          >
            Ask the fleet
          </button>
        ) : null}
      </form>

      {answers?.map((a) => (
        <div key={a.peerUrl}>
          <h3>{a.station}</h3>
          {a.error ? <p className="fo-error fo-small">{a.error}</p> : null}
          {!a.error && a.hits.length === 0 ? <p className="fo-empty">Nothing on that.</p> : null}
          <ul className="fo-list">
            {a.hits.map((h) => {
              const key = `${a.peerUrl}|${h.relPath}`;
              return (
                <li key={key}>
                  <strong>{h.title}</strong> <span className="fo-small fo-muted">({formatBytes(h.bytes)})</span>
                  {h.snippet ? <div className="fo-small fo-muted">“{h.snippet}”</div> : null}
                  {copied[key] ? (
                    <span className="fo-small" role="status">
                      {copied[key]}
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="secondary"
                      disabled={busy}
                      onClick={async () => {
                        setCopied((c) => ({ ...c, [key]: "Copying…" }));
                        try {
                          await invoke<string>("vault_mesh_fetch", { peerUrl: a.peerUrl, relPath: h.relPath });
                          setCopied((c) => ({ ...c, [key]: "Copied into the library." }));
                        } catch (e) {
                          setCopied((c) => {
                            const n = { ...c };
                            delete n[key];
                            return n;
                          });
                          setError(messageOf(e));
                        }
                      }}
                    >
                      Copy here
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      ))}

      <h3>Fleet link</h3>
      {!data.fleet.enabled ? (
        <p className="fo-empty">
          Off. Stations far apart can stay in step over the internet through a relay you host. Set it up in Vault
          settings.
        </p>
      ) : (
        <>
          <p className="fo-small">
            Last sync: {data.fleet.lastSync ? formatDateTime(data.fleet.lastSync) : "never"}.
            {data.fleet.lastError ? <span className="fo-error"> {data.fleet.lastError}</span> : null}{" "}
            <button type="button" className="secondary" disabled={busy} onClick={() => void run("Syncing with the fleet", () => invoke<Overview>("vault_fleet_sync"))}>
              Sync now
            </button>
          </p>
          {data.fleet.stations.length > 0 ? (
            <div className="fo-table-wrap">
              <table className="fo-table">
                <thead>
                  <tr>
                    <th>Station</th>
                    <th>Last heard</th>
                    <th>Strong topics</th>
                    <th>Archives</th>
                  </tr>
                </thead>
                <tbody>
                  {data.fleet.stations.map((s) => (
                    <tr key={s.station}>
                      <td>{s.station}</td>
                      <td>{formatDateTime(s.lastSeen)}</td>
                      <td className="fo-small">
                        {Object.entries(s.topics)
                          .filter(([, l]) => l === "good")
                          .map(([t]) => t)
                          .join(", ") || "—"}
                      </td>
                      <td>{s.archives.length}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="fo-empty">No other station has reported yet.</p>
          )}
          {data.fleet.missingHere.length > 0 ? (
            <>
              <h3>Other stations have, this one does not</h3>
              <ul className="fo-list">
                {data.fleet.missingHere.slice(0, 20).map(([file, who]) => (
                  <li key={file}>
                    {file} <span className="fo-small fo-muted">({who.join(", ")})</span>
                  </li>
                ))}
              </ul>
              <p className="fo-small fo-muted">Download these from Updates, or copy them from a station on the same network.</p>
            </>
          ) : null}
          {data.fleet.answers.length > 0 ? (
            <>
              <h3>Answers from the fleet</h3>
              <ul className="fo-list">
                {data.fleet.answers.map((a, i) => (
                  <li key={i}>
                    <strong>{a.station}</strong> has, for “{a.query}”: {a.titles.join("; ")}
                  </li>
                ))}
              </ul>
            </>
          ) : null}
        </>
      )}
    </section>
  );
}

function UpdatesSection({ data, busy, run }: SectionProps) {
  const pendingFinish = useRef<Record<string, UpdateInfo>>({});
  const { active, start } = useArchiveDownload((fileName) => {
    const u = pendingFinish.current[fileName];
    if (!u) return;
    delete pendingFinish.current[fileName];
    void run(
      "Swapping in the new release",
      () => invoke<Overview>("vault_finish_update", { installedPath: u.installedPath, latestFile: u.latestFile }),
      () => `${u.latestFile} is installed and the older release was removed.`,
    );
  });

  return (
    <section className="glass-panel fo-panel" id="vault-updates" aria-label="Updates">
      <h2>Updates</h2>
      <p className="fo-lead">
        Newer releases of the installed encyclopedias. The old file is removed only after the new one has downloaded
        and opens.
      </p>
      <div className="fo-row">
        <span className="fo-small">Last checked: {data.updatesCheckedAt ? formatDateTime(data.updatesCheckedAt) : "never"}</span>
        <button className="secondary" type="button" disabled={busy} onClick={() => void run("Checking Kiwix for newer releases", () => invoke<Overview>("vault_check_updates"))}>
          Check now
        </button>
      </div>
      {data.updates.length === 0 ? (
        <p className="fo-empty">{data.updatesCheckedAt ? "Everything is current." : "Not checked yet."}</p>
      ) : (
        <div className="fo-table-wrap">
          <table className="fo-table">
            <thead>
              <tr>
                <th>Installed</th>
                <th>Newer release</th>
                <th>Size</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {data.updates.map((u) => (
                <tr key={u.installedPath}>
                  <td>{u.installedFile}</td>
                  <td>{u.latestFile}</td>
                  <td>{listingSize(u.sizeText)}</td>
                  <td>
                    {u.latestFile in active ? (
                      <span className="fo-small" role="status">
                        Downloading{active[u.latestFile] != null ? ` ${Math.round(active[u.latestFile] ?? 0)}%` : "…"}
                      </span>
                    ) : (
                      <button
                        type="button"
                        className="secondary"
                        disabled={busy}
                        onClick={() => {
                          pendingFinish.current[u.latestFile] = u;
                          void run("Starting the download", () => start(u.url, u.latestFile), () => `Downloading ${u.latestFile}.`);
                        }}
                      >
                        Update
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function AlertsSection({ data, busy, run }: SectionProps) {
  const [confirm, setConfirm] = useState("");
  const [reason, setReason] = useState("");
  const s = data.settings;
  return (
    <section className="glass-panel fo-panel" id="vault-alerts" aria-label="Alerts">
      <h2>Alerts</h2>
      <p className="fo-lead">
        Official warning feeds: earthquakes (USGS), disasters worldwide (GDACS), space weather (NOAA), US air traffic
        (FAA ground stops, delays and closures) and, if you set a US state, weather warnings (NWS). News headlines count
        only when they contain one of your watch words. Fixed rules grade each one by how severe and how close it is.
      </p>
      {!s.alertsEnabled ? (
        <p className="fo-empty">Alerts are off. Switch them on in Vault settings; they are checked every 15 minutes while online.</p>
      ) : null}
      <div className="fo-row">
        <span className="fo-small">
          Last checked: {data.alerts.fetchedAt ? formatDateTime(data.alerts.fetchedAt) : "never"}.
          {s.alertLat == null ? " Set this station's position in Vault settings so distance counts." : ""}
        </span>
        <button className="secondary" type="button" disabled={busy} onClick={() => void run("Reading the warning feeds", () => invoke<Overview>("vault_refresh_alerts"))}>
          Check now
        </button>
      </div>
      {data.alerts.errors.map((e) => (
        <p key={e} className="fo-error fo-small">
          {e}
        </p>
      ))}
      {data.alerts.alerts.length === 0 ? (
        <p className="fo-empty">No alerts.</p>
      ) : (
        <ul className="fo-list">
          {data.alerts.alerts.map((a) => (
            <li key={a.id}>
              <Badge tone={alertTone(a.level)}>{ALERT_LABEL[a.level]}</Badge> <strong>{a.title}</strong>{" "}
              <span className="fo-small fo-muted">{a.time ? formatDateTime(a.time) : ""}</span>
              <div className="fo-small">{a.detail}</div>
            </li>
          ))}
        </ul>
      )}

      <h3>Hunker-down</h3>
      {data.hunker.active ? (
        <>
          <p>
            Active since {formatDateTime(data.hunker.startedAt)}, started by {data.hunker.startedBy}: {data.hunker.reason}
          </p>
          <button className="secondary" type="button" disabled={busy} onClick={() => void run("Standing down", () => invoke<Overview>("vault_stand_down"))}>
            Stand down
          </button>
        </>
      ) : (
        <>
          <p className="fo-small">
            Nothing starts this automatically. A person with clearance 4 or higher decides. It silences the radio for a
            day, puts a notice on every screen, and tells the fleet so other operators can decide for their stations.
          </p>
          <form
            className="fo-row"
            onSubmit={(e) => {
              e.preventDefault();
              void run("Starting hunker-down", () => invoke<Overview>("vault_hunker_down", { confirm, reason }));
            }}
          >
            <Field label="Why" grow>
              <input className="fo-input" value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Grid down across the state" />
            </Field>
            <Field label={`Type ${data.hunkerPhrase} to confirm`}>
              <input className="fo-input" value={confirm} onChange={(e) => setConfirm(e.target.value)} autoComplete="off" />
            </Field>
            <button className="secondary" type="submit" disabled={busy || confirm.trim() !== data.hunkerPhrase || !reason.trim()}>
              Hunker down
            </button>
          </form>
        </>
      )}
    </section>
  );
}

function SettingsSection({ data, busy, run }: SectionProps) {
  const [s, setS] = useState<Settings>(data.settings);
  const [lat, setLat] = useState(data.settings.alertLat != null ? String(data.settings.alertLat) : "");
  const [lon, setLon] = useState(data.settings.alertLon != null ? String(data.settings.alertLon) : "");
  const [feeds, setFeeds] = useState(data.settings.newsFeeds.join("\n"));
  const [words, setWords] = useState(data.settings.watchWords.join("\n"));
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((x) => ({ ...x, [k]: v }));
  const check = (k: "autoImportScan" | "meshShare" | "autoUpdate" | "alertsEnabled" | "fleetEnabled" | "faaEnabled", label: string, hint: string) => (
    <label className="fo-field">
      <span>
        <input type="checkbox" checked={s[k]} onChange={(e) => set(k, e.target.checked)} /> {label}
      </span>
      <span className="fo-small fo-muted">{hint}</span>
    </label>
  );

  return (
    <section className="glass-panel fo-panel" id="vault-settings" aria-label="Vault settings">
      <h2>Vault settings</h2>
      <p className="fo-small fo-muted">Changing these needs clearance 4 or higher. Nothing here contacts the internet unless you switch it on.</p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const num = (v: string) => (v.trim() === "" ? null : Number.parseFloat(v));
          void run("Saving", async () => {
            const lines = (v: string) => v.split("\n").map((l) => l.trim()).filter(Boolean);
            await invoke("vault_save_settings", {
              settings: { ...s, alertLat: num(lat), alertLon: num(lon), newsFeeds: lines(feeds), watchWords: lines(words) },
            });
            return invoke<Overview>("vault_get_overview");
          }, () => "Saved.");
        }}
      >
        <div className="fo-row">
          <Field label="Archive folder (a large external drive, optional)" grow>
            <input className="fo-input" value={s.archiveDir ?? ""} onChange={(e) => set("archiveDir", e.target.value || null)} />
          </Field>
          <button
            type="button"
            className="secondary"
            onClick={async () => {
              const picked = (await pickFolders(false))[0];
              if (picked) set("archiveDir", picked);
            }}
          >
            Browse…
          </button>
        </div>
        {check("autoImportScan", "Copy scan finds straight away", "Off: every find waits for you to choose.")}
        {check("meshShare", "Answer other stations' questions", "They see titles and passages from shareable shelves only.")}
        {check("autoUpdate", "Check for newer releases daily", "Lists them under Updates; downloading still waits for you.")}
        {check("alertsEnabled", "Read the warning feeds every 15 minutes", "Sends nothing about this station except the state code below.")}
        <div className="fo-row">
          <Field label="US state for weather warnings (two letters, optional)">
            <input className="fo-input fo-input--narrow" maxLength={2} value={s.alertRegion} onChange={(e) => set("alertRegion", e.target.value.toUpperCase())} />
          </Field>
          <Field label="Station latitude (kept on this computer)">
            <input className="fo-input fo-input--narrow" type="number" step="any" value={lat} onChange={(e) => setLat(e.target.value)} />
          </Field>
          <Field label="Station longitude">
            <input className="fo-input fo-input--narrow" type="number" step="any" value={lon} onChange={(e) => setLon(e.target.value)} />
          </Field>
        </div>
        {check("faaEnabled", "US air traffic (FAA)", "Ground stops, delay programs and airport closures across the US.")}
        <Field label="Airports to watch closely (codes, optional)" grow>
          <input className="fo-input" value={s.faaAirports} onChange={(e) => set("faaAirports", e.target.value.toUpperCase())} placeholder="RAP, DEN, MSP" />
        </Field>
        <Field label="News feeds (one RSS or Atom address per line)" grow>
          <textarea className="fo-input" rows={3} value={feeds} onChange={(e) => setFeeds(e.target.value)} />
        </Field>
        <Field label="Watch words (one per line; a headline counts only if it has one)" grow>
          <textarea className="fo-input" rows={5} value={words} onChange={(e) => setWords(e.target.value)} />
        </Field>
        {check(
          "fleetEnabled",
          "Fleet link over the internet",
          data.fleet.hasGroupKey ? "Sealed with your group key; the relay cannot read it." : "Needs a group key first (Field Ops, Other stations).",
        )}
        <Field label="Relay address (https://…)" grow>
          <input className="fo-input" value={s.fleetRelayUrl} onChange={(e) => set("fleetRelayUrl", e.target.value)} placeholder="https://athena-fleet.example.workers.dev" />
        </Field>
        <p className="fo-small fo-muted">The relay is a small Cloudflare Worker you deploy yourself; see cloud/fleet-relay/README.md.</p>
        <button className="secondary" type="submit" disabled={busy}>
          Save settings
        </button>
      </form>
    </section>
  );
}

const HA_LEVELS: AlertLevel[] = ["watch", "warning", "act"];

function haAutomationYaml(webhookId: string): string {
  return `alias: ATHENA to my phone
trigger:
  - platform: webhook
    webhook_id: ${webhookId || "athena_alerts_CHANGE_ME"}
    allowed_methods: [POST]
    local_only: true
action:
  - service: notify.mobile_app_YOUR_PHONE
    data:
      title: "{{ trigger.json.title }}"
      message: "{{ trigger.json.message }}"
      data:
        tag: "{{ trigger.json.tag }}"
        push:
          interruption-level: >-
            {{ 'critical' if trigger.json.level == 'act' else 'time-sensitive' }}`;
}

/** Home Assistant: alerts to phones through a webhook, ATHENA sensors, and devices read back in. */
function HomeAssistantSection({ data, busy, run }: SectionProps) {
  const [h, setH] = useState<HaSettings>({ ...data.settings.ha, token: "" });
  const [entities, setEntities] = useState(data.settings.ha.entities.join("\n"));
  const set = <K extends keyof HaSettings>(k: K, v: HaSettings[K]) => setH((x) => ({ ...x, [k]: v }));
  const st = data.haState;
  const save = () =>
    invoke("vault_save_settings", {
      settings: { ...data.settings, ha: { ...h, entities: entities.split("\n").map((l) => l.trim()).filter(Boolean) } },
    });

  return (
    <section className="glass-panel fo-panel" id="vault-ha" aria-label="Home Assistant">
      <h2>Home Assistant</h2>
      <p className="fo-lead">
        Send ATHENA&apos;s alerts to your phone through Home Assistant&apos;s Companion app, wherever you are. Home
        Assistant also gets ATHENA sensors (alert level, serious alerts, hunker-down), and ATHENA reads the devices you
        list, such as smoke, leak or UPS sensors, into the Watch Officer. This works on the home network with the
        internet off.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void run("Saving", async () => {
            await save();
            return invoke<Overview>("vault_get_overview");
          }, () => "Saved.");
        }}
      >
        <label className="fo-field">
          <span>
            <input type="checkbox" checked={h.enabled} onChange={(e) => set("enabled", e.target.checked)} /> Link with Home Assistant
          </span>
        </label>
        <Field label="Home Assistant address" grow>
          <input className="fo-input" value={h.url} onChange={(e) => set("url", e.target.value)} placeholder="http://homeassistant.local:8123" />
        </Field>
        <Field label="Webhook id (from the automation below)" grow>
          <input className="fo-input" value={h.webhookId} onChange={(e) => set("webhookId", e.target.value)} placeholder="athena_alerts_x7k2" autoComplete="off" />
        </Field>
        <div className="fo-row">
          <Field label={h.tokenSet || data.settings.ha.tokenSet ? "Access token (saved; type a new one to replace it)" : "Long-lived access token (for sensors and devices)"} grow>
            <input
              className="fo-input"
              type="password"
              value={h.token}
              onChange={(e) => set("token", e.target.value)}
              placeholder={data.settings.ha.tokenSet ? "••••••• saved" : "Profile → Security → Long-lived access tokens"}
              autoComplete="new-password"
            />
          </Field>
          {data.settings.ha.tokenSet ? (
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={() =>
                void run("Forgetting the token", async () => {
                  await invoke("vault_ha_forget_token");
                  return invoke<Overview>("vault_get_overview");
                }, () => "Token deleted from this station.")
              }
            >
              Forget token
            </button>
          ) : null}
        </div>
        <Field label="Send to phones from this level up">
          <select className="fo-input fo-input--narrow" value={h.notifyLevel} onChange={(e) => set("notifyLevel", e.target.value as AlertLevel)}>
            {HA_LEVELS.map((l) => (
              <option key={l} value={l}>
                {ALERT_LABEL[l]}
              </option>
            ))}
          </select>
        </Field>
        <label className="fo-field">
          <span>
            <input type="checkbox" checked={h.publishSensors} onChange={(e) => set("publishSensors", e.target.checked)} /> Keep ATHENA sensors up to date in Home Assistant
          </span>
          <span className="fo-small fo-muted">sensor.athena_alert_level, sensor.athena_alerts, sensor.athena_status, binary_sensor.athena_hunker_down. Needs the token.</span>
        </label>
        <Field label="Devices to read (entity ids, one per line)" grow>
          <textarea className="fo-input" rows={4} value={entities} onChange={(e) => setEntities(e.target.value)} placeholder={"binary_sensor.kitchen_smoke\nsensor.ups_battery"} />
        </Field>
        <div className="fo-row">
          <button className="secondary" type="submit" disabled={busy}>
            Save
          </button>
          <button
            className="secondary"
            type="button"
            disabled={busy || !h.url.trim()}
            onClick={() =>
              void run("Testing the Home Assistant link", async () => {
                await save();
                return invoke<Overview>("vault_ha_test");
              }, () => "Home Assistant answered. A test message was sent if a webhook id is set.")
            }
          >
            Save and send a test
          </button>
          <button
            className="secondary"
            type="button"
            disabled={busy || !h.url.trim() || !(h.token.trim() || data.settings.ha.tokenSet)}
            onClick={() => {
              let place = "";
              void run("Reading Home Assistant's location", async () => {
                await save();
                const loc = await invoke<{ lat: number; lon: number; name: string | null }>("vault_ha_use_location");
                place = `${loc.name ? `${loc.name}, ` : ""}${loc.lat.toFixed(4)}, ${loc.lon.toFixed(4)}`;
                return invoke<Overview>("vault_get_overview");
              }, () => `Station position set to ${place}. Alerts and the map now use it.`);
            }}
          >
            Use Home Assistant&apos;s location
          </button>
        </div>
      </form>

      <p className="fo-small">
        Last message sent: {st.lastSent ? formatDateTime(st.lastSent) : "never"}. Devices last read: {st.lastRead ? formatDateTime(st.lastRead) : "never"}.
      </p>
      {st.lastError ? <p className="fo-error fo-small">{st.lastError}</p> : null}
      {st.readings.length > 0 ? (
        <ul className="fo-list">
          {st.readings.map((r) => (
            <li key={r.entityId}>
              <strong>{r.name}</strong>: {r.state}
              {r.unit ? ` ${r.unit}` : ""} <span className="fo-small fo-muted">{r.entityId}</span>
            </li>
          ))}
        </ul>
      ) : null}

      <h3>Set up the phone message in Home Assistant</h3>
      <ol className="fo-small">
        <li>Install the Home Assistant Companion app on your phone and sign in, so it shows up as a notify service.</li>
        <li>
          In Home Assistant, Settings → Automations → Create → Edit in YAML, paste the automation below, replace
          YOUR_PHONE with your phone&apos;s notify name and pick a hard-to-guess webhook id.
        </li>
        <li>Enter the same webhook id above, save, and press Save and send a test.</li>
      </ol>
      <pre className="fo-small" style={{ whiteSpace: "pre-wrap", overflowX: "auto" }}>
        {haAutomationYaml(h.webhookId.trim())}
      </pre>
      <p className="fo-small fo-muted">
        The webhook needs no token, so keep its id private. Leave local_only on: ATHENA talks to Home Assistant on your
        home network, and the Companion app delivers the message to your phone anywhere.
      </p>
    </section>
  );
}
