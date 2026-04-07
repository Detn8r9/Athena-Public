import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, ClipboardList, Package, Shield } from "lucide-react";
import { useAuth } from "./auth/AuthContext";
import { LogbookView } from "./LogbookView";
import { QuartermasterHub, type QuartermasterTabId } from "./QuartermasterHub";
import { useLexicon } from "./shared/lexicon";
import { canAccessTacticalLogbook } from "./shared/tacticalLogbookAccess";

type HubTab = "notes" | "tactical" | "inventory" | "audit";

export function LogisticsLogbookHub({ meshConnected }: { meshConnected: boolean }) {
  const lx = useLexicon();
  const { rankLevel } = useAuth();
  const r = rankLevel ?? 0;
  const [occupation, setOccupation] = useState("");
  const [hubTab, setHubTab] = useState<HubTab>("notes");

  const refreshOcc = useCallback(async () => {
    try {
      const p = await invoke<{ occupation: string }>("get_logbook_author_profile");
      setOccupation((p.occupation ?? "").trim());
    } catch {
      setOccupation("");
    }
  }, []);

  useEffect(() => {
    void refreshOcc();
  }, [refreshOcc]);

  useEffect(() => {
    const un = listen("athena:logbook-updated", () => {
      void refreshOcc();
    });
    return () => {
      void un.then((fn) => fn());
    };
  }, [refreshOcc]);

  const canTactical = useMemo(() => canAccessTacticalLogbook(r, occupation), [r, occupation]);
  const canInventory = r >= 3;
  const canAudit = r >= 4;

  useEffect(() => {
    if (hubTab === "tactical" && !canTactical) setHubTab("notes");
  }, [hubTab, canTactical]);

  useEffect(() => {
    if ((hubTab === "inventory" || hubTab === "audit") && !canInventory) setHubTab("notes");
  }, [hubTab, canInventory]);

  useEffect(() => {
    if (hubTab === "audit" && !canAudit) setHubTab("inventory");
  }, [hubTab, canAudit]);

  const qmTab: QuartermasterTabId = hubTab === "audit" ? "log" : "inventory";

  function tabClass(active: boolean): string {
    return active ? "llh-tab llh-tab-active" : "llh-tab";
  }

  return (
    <div className="logistics-logbook-hub home-view-scroll">
      <header className="logistics-logbook-hub-header">
        <h1 className="logistics-logbook-hub-title">{lx.logisticsHubTitle}</h1>
        <p className="logistics-logbook-hub-subtitle text-muted">{lx.logisticsHubSubtitle}</p>
      </header>

      <div className="logistics-logbook-hub-tabs glass-panel" role="tablist" aria-label={lx.logisticsHubTabsAria}>
        <button
          type="button"
          role="tab"
          aria-selected={hubTab === "notes"}
          className={tabClass(hubTab === "notes")}
          onClick={() => setHubTab("notes")}
        >
          <BookOpen size={18} aria-hidden />
          {lx.logisticsHubTabNotes}
        </button>
        {canTactical ? (
          <button
            type="button"
            role="tab"
            aria-selected={hubTab === "tactical"}
            className={tabClass(hubTab === "tactical")}
            onClick={() => {
              void refreshOcc();
              setHubTab("tactical");
            }}
          >
            <Shield size={18} aria-hidden />
            {lx.logisticsHubTabTactical}
          </button>
        ) : null}
        {canInventory ? (
          <button
            type="button"
            role="tab"
            aria-selected={hubTab === "inventory"}
            className={tabClass(hubTab === "inventory")}
            onClick={() => setHubTab("inventory")}
          >
            <Package size={18} aria-hidden />
            {lx.logisticsHubTabInventory}
          </button>
        ) : null}
        {canAudit ? (
          <button
            type="button"
            role="tab"
            aria-selected={hubTab === "audit"}
            className={tabClass(hubTab === "audit")}
            onClick={() => setHubTab("audit")}
          >
            <ClipboardList size={18} aria-hidden />
            {lx.logisticsHubTabAudit}
          </button>
        ) : null}
      </div>

      <div className="logistics-logbook-hub-body">
        {hubTab === "notes" ? (
          <LogbookView
            meshConnected={meshConnected}
            notebookScope="general"
            onProfileSaved={() => void refreshOcc()}
          />
        ) : null}
        {hubTab === "tactical" && canTactical ? (
          <LogbookView
            meshConnected={meshConnected}
            notebookScope="tactical"
            onProfileSaved={() => void refreshOcc()}
          />
        ) : null}
        {hubTab === "inventory" || hubTab === "audit" ? (
          <QuartermasterHub
            embedded
            activeTab={qmTab}
            onActiveTabChange={(t) => setHubTab(t === "log" ? "audit" : "inventory")}
          />
        ) : null}
      </div>
    </div>
  );
}
