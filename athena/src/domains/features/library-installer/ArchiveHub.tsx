import { useLexicon } from "../../../shared/lexicon";
import { ArchiveLocalPanel } from "./LibraryInstaller";

export interface ArchiveHubProps {
  libraryError: string | null;
}

/**
 * Local Archive vault: indexed files on this station. Network acquisition lives under **Data Acquisition**.
 */
export function ArchiveHub({ libraryError }: ArchiveHubProps) {
  const lx = useLexicon();

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
        {introBlock}
        <div className="library-tab-panel" id="archive-panel-local" role="region" aria-label={lx.tabLocalArchive}>
          <ArchiveLocalPanel />
        </div>
      </div>
      {libraryError !== null ? <p className="error library-tab-error">{libraryError}</p> : null}
    </div>
  );
}
