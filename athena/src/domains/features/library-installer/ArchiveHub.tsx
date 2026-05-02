import { KnowledgeCatalogView } from "../../../pages/knowledge-catalog/KnowledgeCatalogView";
import { useLexicon } from "../../../shared/lexicon";
import { ArchiveAcademyDownloadsPanel, ArchiveLocalPanel } from "./LibraryInstaller";
import { ArchiveMapsTab, type ArchiveMapsTabProps } from "./ArchiveMapsTab";
import { DynamicLibraryInstaller } from "./DynamicLibraryInstaller";

export type ArchivePrimaryTabId = "local" | "network";

export type ArchiveNetworkSubTabId = "packages" | "academy" | "documents" | "maps";

export interface ArchiveHubProps {
  primaryTab: ArchivePrimaryTabId;
  onPrimaryTabChange: (tab: ArchivePrimaryTabId) => void;
  networkSubTab: ArchiveNetworkSubTabId;
  onNetworkSubTabChange: (tab: ArchiveNetworkSubTabId) => void;
  mapsTabProps: ArchiveMapsTabProps;
  libraryError: string | null;
}

export function ArchiveHub({
  primaryTab,
  onPrimaryTabChange,
  networkSubTab,
  onNetworkSubTabChange,
  mapsTabProps,
  libraryError,
}: ArchiveHubProps) {
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
      <div className="library-tablist" role="tablist" aria-label={lx.archivePrimaryTablistAria}>
        <button
          type="button"
          role="tab"
          id="archive-tab-local"
          aria-selected={primaryTab === "local"}
          aria-controls="archive-panel-local"
          className={"library-tab" + (primaryTab === "local" ? " library-tab-active" : "")}
          onClick={() => onPrimaryTabChange("local")}
        >
          {lx.tabLocalArchive}
        </button>
        <button
          type="button"
          role="tab"
          id="archive-tab-network"
          aria-selected={primaryTab === "network"}
          aria-controls="archive-panel-network"
          className={"library-tab" + (primaryTab === "network" ? " library-tab-active" : "")}
          onClick={() => onPrimaryTabChange("network")}
        >
          {lx.tabDownloadNetwork}
        </button>
      </div>
      <div
        className="library-tab-panels archive-hub-scroll"
        style={{
          flex: "1 1 0",
          minHeight: 0,
          overflowY: "auto",
          display: "flex",
          flexDirection: "column",
          gap: "0.65rem",
        }}
      >
        {primaryTab === "local" ? (
          <>
            {introBlock}
            <div className="library-tab-panel" id="archive-panel-local" role="tabpanel" aria-labelledby="archive-tab-local">
              <ArchiveLocalPanel />
            </div>
          </>
        ) : null}
        {primaryTab === "network" ? (
          <div className="library-tab-panel" id="archive-panel-network" role="tabpanel" aria-labelledby="archive-tab-network">
            {introBlock}
            <div className="library-tablist archive-hub-network-subtabs" role="tablist" aria-label={lx.archiveNetworkTablistAria} style={{ flexShrink: 0 }}>
              <button
                type="button"
                role="tab"
                id="archive-sub-packages"
                aria-selected={networkSubTab === "packages"}
                aria-controls="archive-subpanel-packages"
                className={"library-tab" + (networkSubTab === "packages" ? " library-tab-active" : "")}
                onClick={() => onNetworkSubTabChange("packages")}
              >
                {lx.tabArchivePackages}
              </button>
              <button
                type="button"
                role="tab"
                id="archive-sub-academy"
                aria-selected={networkSubTab === "academy"}
                aria-controls="archive-subpanel-academy"
                className={"library-tab" + (networkSubTab === "academy" ? " library-tab-active" : "")}
                onClick={() => onNetworkSubTabChange("academy")}
              >
                {lx.tabArchiveAcademy}
              </button>
              <button
                type="button"
                role="tab"
                id="archive-sub-documents"
                aria-selected={networkSubTab === "documents"}
                aria-controls="archive-subpanel-documents"
                className={"library-tab" + (networkSubTab === "documents" ? " library-tab-active" : "")}
                onClick={() => onNetworkSubTabChange("documents")}
              >
                {lx.tabArchiveDocuments}
              </button>
              <button
                type="button"
                role="tab"
                id="archive-sub-maps"
                aria-selected={networkSubTab === "maps"}
                aria-controls="archive-subpanel-maps"
                className={"library-tab" + (networkSubTab === "maps" ? " library-tab-active" : "")}
                onClick={() => onNetworkSubTabChange("maps")}
              >
                {lx.tabArchiveMaps}
              </button>
            </div>
            {networkSubTab === "packages" ? (
              <div id="archive-subpanel-packages" role="tabpanel" aria-labelledby="archive-sub-packages">
                <DynamicLibraryInstaller />
              </div>
            ) : null}
            {networkSubTab === "academy" ? (
              <div id="archive-subpanel-academy" role="tabpanel" aria-labelledby="archive-sub-academy">
                <ArchiveAcademyDownloadsPanel />
              </div>
            ) : null}
            {networkSubTab === "documents" ? (
              <div id="archive-subpanel-documents" role="tabpanel" aria-labelledby="archive-sub-documents">
                <KnowledgeCatalogView />
              </div>
            ) : null}
            {networkSubTab === "maps" ? (
              <div id="archive-subpanel-maps" role="tabpanel" aria-labelledby="archive-sub-maps">
                <ArchiveMapsTab {...mapsTabProps} />
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
      {libraryError !== null ? <p className="error library-tab-error">{libraryError}</p> : null}
    </div>
  );
}
