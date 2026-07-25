import { useCallback, useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { Bell, BookOpen, ClipboardList, Package, Shield } from "lucide-react";
import { useAuth } from "../../domains/auth/AuthContext";
import { LogbookView } from "../logbook/LogbookView";
import { LogisticsLibraryView } from "./LogisticsLibraryView";
import { QuartermasterHub, type QuartermasterTabId } from "../quartermaster/QuartermasterHub";
import { ApprovalInbox } from "../../domains/features/map/ApprovalInbox";
import { InvQueueButton } from "../../domains/features/logistics/InvQueueButton";
import { useLexicon } from "../../shared/lexicon";
import { canAccessTacticalLogbook } from "../../shared/tacticalLogbookAccess";

type HubTab = "notes" | "tactical" | "inventory" | "audit";

export function LogisticsLogbookHub({ meshConnected }: { meshConnected: boolean }) {
  const lx = useLexicon();
  const { rankLevel } = useAuth();
  const r = rankLevel ?? 0;
  const [occupation, setOccupation] = useState("");
  const [hubTab, setHubTab] = useState<HubTab>("notes");
  const [approvalInboxOpen, setApprovalInboxOpen] = useState(false);
  const [aiPendingCount, setAiPendingCount] = useState(0);

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
  // Backend permits inventory list at rank 2+; edits remain rank-gated inside Quartermaster.
  const canInventory = r >= 2;
  const canAudit = r >= 4;

  useEffect(() => {
    if (!canAudit) {
      setAiPendingCount(0);
      return;
    }
    const loadN = () => {
      void invoke<number>("ai_count_pending")
        .then((n) => setAiPendingCount(typeof n === "number" ? n : 0))
        .catch(() => setAiPendingCount(0));
    };
    loadN();
    const id = window.setInterval(loadN, 45_000);
    return () => window.clearInterval(id);
  }, [canAudit]);

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
        {canInventory ? <InvQueueButton /> : null}
        {canAudit ? (
          <>
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
            <button
              type="button"
              className="llh-tab"
              onClick={() => setApprovalInboxOpen(true)}
              title="AI approval inbox"
              aria-label="Open AI approval inbox"
            >
              <Bell size={18} aria-hidden />
              Inbox
              {aiPendingCount > 0 ? (
                <span
                  aria-label={`${aiPendingCount} pending`}
                  style={{
                    marginLeft: 6,
                    padding: "0.1rem 0.45rem",
                    borderRadius: 999,
                    fontSize: "0.75rem",
                    color: "var(--text)",
                    background: "color-mix(in srgb, var(--primary) 20%, transparent)",
                    border: "1px solid color-mix(in srgb, var(--primary) 35%, var(--border))",
                  }}
                >
                  {aiPendingCount}
                </span>
              ) : null}
            </button>
          </>
        ) : null}
      </div>

      <div className="logistics-logbook-hub-body">
        {hubTab === "notes" ? (
          <LogisticsLibraryView meshConnected={meshConnected} onProfileSaved={() => void refreshOcc()} />
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
      <ApprovalInbox
        open={approvalInboxOpen}
        onClose={() => setApprovalInboxOpen(false)}
        onChanged={() => {
          void invoke<number>("ai_count_pending")
            .then((n) => setAiPendingCount(typeof n === "number" ? n : 0))
            .catch(() => {});
        }}
      />
    </div>
  );
}
