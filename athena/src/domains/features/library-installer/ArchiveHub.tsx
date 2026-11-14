import { lazy, Suspense, useState } from "react";
import { useLexicon } from "../../../shared/lexicon";
import { ArchiveLocalPanel } from "./LibraryInstaller";

const VaultPanel = lazy(() => import("./VaultPanel").then((m) => ({ default: m.VaultPanel })));

export type ArchiveTab = "files" | "vault";

export interface ArchiveHubProps {
  libraryError: string | null;
  /** Controlled by the app so Settings and search can open the Vault directly. */
  tab?: ArchiveTab;
  onTabChange?: (tab: ArchiveTab) => void;
  /** Vault section to open (coverage, scan, packs, stations, updates, alerts, ha, settings). */
  vaultSection?: string | null;
}

/**
 * Local Archive vault: indexed files on this station. Network acquisition lives under **Data Acquisition**.
 */
export function ArchiveHub({ libraryError, tab: controlledTab, onTabChange, vaultSection }: ArchiveHubProps) {
  const lx = useLexicon();
  const [localTab, setLocalTab] = useState<ArchiveTab>("files");
  const tab = controlledTab ?? localTab;
  const setTab = (t: ArchiveTab) => {
    setLocalTab(t);
    onTabChange?.(t);
  };

  const introBlock = (
    <p className="library-desc text-muted archive-hub-intro" style={{ margin: 0, flexShrink: 0 }}>
      {lx.archiveHubIntro}
    </p>
  );

  return (
    <div
      className="glass-panel app-view-placeholder archive-hub-root"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "0.75rem",
        padding: "1rem",
        position: "relative",
        minHeight: 0,
      }}
    >
      <div className="library-tab-panels archive-hub-scroll" style={{ flex: "1 1 0", minHeight: 0, overflowY: "auto", display: "flex", flexDirection: "column", gap: "0.65rem" }}>
        <h1 style={{ margin: 0, fontSize: "1.25rem", flexShrink: 0 }}>{lx.tabLocalArchive}</h1>
        <div role="tablist" aria-label="Library sections" style={{ display: "flex", gap: "0.5rem", flexShrink: 0 }}>
          {(
            [
              ["files", "Files"],
              ["vault", "Vault"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className={tab === id ? "glass-action-btn" : "secondary"}
              onClick={() => setTab(id)}
            >
              {label}
            </button>
          ))}
        </div>
        <p className="text-muted small" style={{ margin: 0, flexShrink: 0 }}>
          {tab === "files"
            ? "Files: every document on this station. Vault: what the library covers, packs, other stations, updates, alerts and Home Assistant."
            : "Vault: library coverage, packs, other stations and fleet link, updates, alerts, and Home Assistant. Its settings are near the bottom, and also listed in Settings → Settings on other screens."}
        </p>
        {tab === "files" ? (
          <>
            {introBlock}
            <div className="library-tab-panel" id="archive-panel-local" role="region" aria-label={lx.tabLocalArchive}>
              <ArchiveLocalPanel />
            </div>
          </>
        ) : (
          <Suspense fallback={<p className="text-muted">Loading the vault…</p>}>
            <VaultPanel initialSection={vaultSection} />
          </Suspense>
        )}
      </div>
      {libraryError !== null ? <p className="error library-tab-error">{libraryError}</p> : null}
    </div>
  );
}
