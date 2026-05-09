import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppView } from "../../app/appView";
import { ATHENA_FULL_NAME } from "../../shared/branding";
import { substLexicon, useLexicon } from "../../shared/lexicon";
import { useIngestionQueueStore, type IngestionQueueSnapshot } from "../../stores/ingestionQueueStore";
import {
  Bot,
  BookOpen,
  CalendarDays,
  HelpCircle,
  Home,
  Library,
  Map,
  MessageSquare,
  LogOut,
  Moon,
  Power,
  Settings,
  Sun,
  Users,
} from "lucide-react";
import { CommsHubIcon } from "../../shared/CommsHubIcon";
import { NotificationsCenter } from "./NotificationsCenter";

export type UniversalHeaderBatteryState = {
  percentage: number | null;
  is_charging: boolean | null;
  /** Minutes until empty when discharging, if reported by the OS. */
  time_to_empty_minutes?: number | null;
};

export type HeaderDownloadBarState =
  | {
      mode: "determinate";
      pct: number;
      subtitle: string;
      progressDetail: string | null;
    }
  | {
      mode: "indeterminate";
      subtitle: string;
      progressDetail: string | null;
    };

export type UniversalHeaderIngestionState = {
  isIngesting: boolean;
  currentFile: string | null;
};

/** Live zip/tar unpack progress (Chunk 3 library pipeline). */
export type UniversalHeaderArchiveExtractionState = {
  archiveLabel: string;
  phase: string;
  entriesProcessed: number;
  entriesTotal: number | null;
  bytesWritten: number;
  message?: string | null;
};

/** Local hot-swap override for the model effective in the current view (null = follow Settings routing). */
export type UniversalHeaderSessionModelRouting = {
  scopeLabel: string;
  activeLabel: string;
  /** `SESSION_MODEL_FOLLOW_SETTINGS` or a concrete `.gguf` basename */
  value: string;
  basenames: string[];
  onChange: (value: string) => void;
};

export interface UniversalHeaderProps {
  currentView: AppView;
  setCurrentView: (view: AppView) => void;
  featureMapEnabled: boolean;
  onOpenArchive: () => void;
  onOpenSettings: () => void;
  onOpenAiModelSettings: () => void;
  onOpenHelp: () => void;
  onOpenCalendar: () => void;
  calendarDateTimeLabel: string;
  batteryState: UniversalHeaderBatteryState | null;
  headerDownloadBar: HeaderDownloadBarState | null;
  ingestionState: UniversalHeaderIngestionState | null;
  archiveExtraction: UniversalHeaderArchiveExtractionState | null;
  authRank: number;
  authClearance: number;
  onLogout: () => void | Promise<void>;
  /** When true, local chat / Help AI are paused; mesh /ai queue may still run but auto-reply is off in minimal mode. */
  aiInferenceSleep: boolean;
  onToggleAiInferenceSleep: () => void | Promise<void>;
  /** Compact per-view model override control; null hides the control. */
  sessionModelRouting: UniversalHeaderSessionModelRouting | null;
  /** Ollama-only: shows whether traffic is using a remote Hive Mind host or the local sidecar. */
  ollamaHiveRoute?: "hive" | "local" | null;
}

function viewButtonClass(active: boolean): string {
  return `universal-header-icon-btn${active ? " universal-header-icon-btn-active" : ""}`;
}

function viewButtonClassBottomNav(active: boolean): string {
  return `universal-header-bottom-nav-btn${active ? " universal-header-bottom-nav-btn-active" : ""}`;
}

function clampPct(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(100, n));
}

function IndexingPill() {
  const queued = useIngestionQueueStore((s) => s.queued);
  const inProgress = useIngestionQueueStore((s) => s.inProgress);
  const total = queued.length + inProgress.length;
  const active = inProgress.length > 0;

  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocMouse = (e: MouseEvent) => {
      const t = e.target as Node | null;
      if (!t) return;
      if (wrapRef.current && !wrapRef.current.contains(t)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocMouse);
    return () => document.removeEventListener("mousedown", onDocMouse);
  }, [open]);

  if (total <= 0) return null;

  const overallIngestionProgress = clampPct(
    Math.round(
      (queued.length + inProgress.length) > 0
        ? [...inProgress, ...queued].reduce((s, x) => s + Math.max(0, Math.min(100, x.pct ?? 0)), 0) / (queued.length + inProgress.length)
        : 0,
    ),
  );

  return (
    <div className="universal-header-indexing-wrap" ref={wrapRef}>
      <div
        onClick={() => setOpen((v) => !v)}
        className="flex flex-col gap-1 w-40 px-3 py-2 rounded-lg hover:bg-white/5 transition-colors cursor-pointer"
        role="button"
        title="Click to view ingestion queue details"
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <span className="text-[11px] font-semibold text-slate-300 uppercase tracking-wider leading-none">
          Indexing...
        </span>
        <div className="w-full h-2 bg-slate-800 rounded-full overflow-hidden border border-white/5">
          <div
            className={"h-full bg-blue-500 transition-all duration-300 rounded-full" + (active ? "" : " opacity-70")}
            style={{ width: `${overallIngestionProgress}%` }}
          />
        </div>
      </div>

      {open ? (
        <div className="universal-header-indexing-menu" role="menu" aria-label="Ingestion queue">
          {[...inProgress.map((x) => ({ ...x, bucket: "In progress" as const })), ...queued.map((x) => ({ ...x, bucket: "Queued" as const }))].map(
            (item) => (
              <div key={item.id} className="universal-header-indexing-row" role="menuitem">
                <div className="universal-header-indexing-row__top">
                  <span className="universal-header-indexing-row__name">{item.displayName}</span>
                  <span className="universal-header-indexing-row__meta">{item.bucket}</span>
                </div>
                <div className="universal-header-indexing-row__bar" aria-hidden>
                  <div
                    className={`universal-header-indexing-row__fill${
                      (item.pct ?? 0) > 0 ? "" : " universal-header-indexing-row__fill--indeterminate"
                    }`}
                    style={(item.pct ?? 0) > 0 ? { width: `${Math.max(0, Math.min(100, item.pct))}%` } : undefined}
                  />
                </div>
                <div className="universal-header-indexing-row__sub">{item.sourcePackage}</div>
              </div>
            ),
          )}
        </div>
      ) : null}
    </div>
  );
}

/** Matches CSS mobile breakpoint for header / bottom nav (max-width: 768px). */
function useIsMobileHeaderLayout(): boolean {
  const [mobile, setMobile] = useState(
    () => (typeof window !== "undefined" ? window.matchMedia("(max-width: 768px)").matches : false),
  );
  useEffect(() => {
    const mq = window.matchMedia("(max-width: 768px)");
    const onChange = () => setMobile(mq.matches);
    onChange();
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return mobile;
}

export function UniversalHeader({
  currentView,
  setCurrentView,
  featureMapEnabled,
  onOpenArchive,
  onOpenSettings,
  onOpenAiModelSettings,
  onOpenHelp,
  onOpenCalendar,
  calendarDateTimeLabel,
  batteryState,
  headerDownloadBar,
  ingestionState,
  archiveExtraction,
  authRank,
  authClearance,
  onLogout,
  aiInferenceSleep,
  onToggleAiInferenceSleep,
  sessionModelRouting,
  ollamaHiveRoute = null,
}: UniversalHeaderProps) {
  const lx = useLexicon();
  const [powerMenuOpen, setPowerMenuOpen] = useState(false);
  const [powerSaveActive, setPowerSaveActive] = useState(false);
  const powerWrapRef = useRef<HTMLDivElement>(null);
  const powerMenuId = "universal-header-power-menu";
  const isMobileHeaderLayout = useIsMobileHeaderLayout();

  const pctRounded = batteryState?.percentage != null ? Math.round(batteryState.percentage) : 0;
  const mins = batteryState?.time_to_empty_minutes;
  const batteryAria =
    batteryState && batteryState.percentage !== null
      ? batteryState.is_charging
        ? substLexicon(lx.headerBatteryAriaCharging, { pct: pctRounded })
        : substLexicon(lx.headerBatteryAriaOnBattery, { pct: pctRounded }) +
          (mins != null ? substLexicon(lx.headerBatteryAriaMinsSuffix, { mins }) : "")
      : "";

  const miniProgress: HeaderDownloadBarState | null = useMemo(() => {
    if (headerDownloadBar) return headerDownloadBar;
    if (archiveExtraction) {
      const total = archiveExtraction.entriesTotal;
      if (total != null && total > 0) {
        const pct = clampPct(Math.round((archiveExtraction.entriesProcessed / total) * 100));
        return {
          mode: "determinate",
          pct,
          subtitle: archiveExtraction.archiveLabel,
          progressDetail: `${archiveExtraction.entriesProcessed.toLocaleString()} / ${total.toLocaleString()} entries`,
        };
      }
      return {
        mode: "indeterminate",
        subtitle: archiveExtraction.archiveLabel,
        progressDetail: archiveExtraction.phase || null,
      };
    }
    if (ingestionState?.isIngesting) {
      return {
        mode: "indeterminate",
        subtitle: "Indexing",
        progressDetail: ingestionState.currentFile ? ingestionState.currentFile : null,
      };
    }
    return null;
  }, [archiveExtraction, headerDownloadBar, ingestionState]);

  useEffect(() => {
    const u = listen<IngestionQueueSnapshot>("athena:ingestion-queue", (ev) => {
      const p = ev.payload;
      if (!p) return;
      useIngestionQueueStore.getState().setSnapshot({
        queued: p.queued ?? [],
        inProgress: p.inProgress ?? [],
      });
    });
    return () => {
      void u.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const u = listen<boolean>("athena:power-save-mode", (ev) => {
      setPowerSaveActive(Boolean(ev.payload));
    });
    return () => {
      void u.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!powerMenuOpen) return;
    const onDocMouse = (e: MouseEvent) => {
      const el = powerWrapRef.current;
      if (el && !el.contains(e.target as Node)) setPowerMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPowerMenuOpen(false);
    };
    document.addEventListener("mousedown", onDocMouse);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocMouse);
      document.removeEventListener("keydown", onKey);
    };
  }, [powerMenuOpen]);

  async function runStationMinimalMode() {
    setPowerMenuOpen(false);
    try {
      await invoke("enter_station_minimal_mode");
    } catch {
      /* still log out */
    }
    await Promise.resolve(onLogout());
  }

  async function runLogoutOnly() {
    setPowerMenuOpen(false);
    await Promise.resolve(onLogout());
  }

  async function runWakeAi() {
    setPowerMenuOpen(false);
    await Promise.resolve(onToggleAiInferenceSleep());
  }

  const powerHomeCluster = (
    <>
      <div className="universal-header-power-wrap" ref={powerWrapRef}>
        <button
          type="button"
          className={`universal-header-icon-btn${aiInferenceSleep ? " universal-header-ai-asleep" : ""}`}
          aria-expanded={powerMenuOpen}
          aria-haspopup="menu"
          aria-controls={powerMenuOpen ? powerMenuId : undefined}
          title="Power and station options"
          aria-label="Power and station options"
          onClick={() => setPowerMenuOpen((o) => !o)}
        >
          <Power size={22} strokeWidth={2} aria-hidden />
        </button>
        {powerMenuOpen ? (
          <div
            id={powerMenuId}
            className="universal-header-power-menu glass-panel"
            role="menu"
            aria-label="Station power options"
          >
            <button
              type="button"
              role="menuitem"
              className="universal-header-power-menu-item universal-header-power-menu-item-primary"
              onClick={() => void runStationMinimalMode()}
            >
              <span className="universal-header-power-menu-label">Station minimal mode</span>
              <span className="universal-header-power-menu-hint">
                Log out, pause local AI, disable mesh auto-reply, and stop the bundled Ollama sidecar if Athena started it.
              </span>
            </button>
            <button type="button" role="menuitem" className="universal-header-power-menu-item" onClick={() => void runLogoutOnly()}>
              <LogOut size={18} strokeWidth={2} aria-hidden className="universal-header-power-menu-icon" />
              <span>Log out only</span>
            </button>
            <button type="button" role="menuitem" className="universal-header-power-menu-item" onClick={() => void runWakeAi()}>
              {aiInferenceSleep ? (
                <Sun size={18} strokeWidth={2} aria-hidden className="universal-header-power-menu-icon" />
              ) : (
                <Moon size={18} strokeWidth={2} aria-hidden className="universal-header-power-menu-icon" />
              )}
              <span>{aiInferenceSleep ? "Wake local AI" : "Sleep local AI only"}</span>
            </button>
          </div>
        ) : null}
      </div>
      <button
        type="button"
        className={viewButtonClass(currentView === "home")}
        onClick={() => setCurrentView("home")}
        title={lx.navHome}
        aria-label={lx.navHome}
        aria-current={currentView === "home" ? "page" : undefined}
      >
        <Home size={22} strokeWidth={2} aria-hidden />
      </button>
    </>
  );

  const statusStrip = (
    <>
      <div
        className={"universal-header-mini-progress" + (miniProgress ? " universal-header-mini-progress--active" : "")}
        aria-hidden={!miniProgress}
      >
        <div className="universal-header-mini-progress__track" aria-hidden>
          <div
            className={
              "universal-header-mini-progress__fill" +
              (miniProgress?.mode === "indeterminate" ? " universal-header-mini-progress__fill--indeterminate" : "")
            }
            style={miniProgress?.mode === "determinate" ? { width: `${clampPct(miniProgress.pct)}%` } : undefined}
          />
        </div>
        <div className="universal-header-mini-progress__text" aria-live="polite">
          <span className="universal-header-mini-progress__sub">{miniProgress ? miniProgress.subtitle : ""}</span>
          <span className="universal-header-mini-progress__detail">
            {miniProgress ? (miniProgress.mode === "determinate" ? `${miniProgress.pct}%` : miniProgress.progressDetail || "…") : ""}
          </span>
        </div>
      </div>
      {powerSaveActive ? (
        <div className="universal-header-power-save-indicator" title="Power Save Mode Active" aria-label="Power Save Mode Active">
          Power Save Mode Active
        </div>
      ) : null}
      <div className="universal-header-ingestion-slot">
        {archiveExtraction ? (
          <div className="universal-header-ingestion" role="status" aria-live="polite">
            <span className="universal-header-ingestion-dot" aria-hidden />
            <span className="universal-header-ingestion-label">
              {archiveExtraction.phase === "error"
                ? `Unpack failed: ${archiveExtraction.archiveLabel}`
                : `Unpacking: ${archiveExtraction.archiveLabel}`}
              {archiveExtraction.phase !== "error" ? (
                <>
                  {" "}
                  <span className="universal-header-ingestion-detail">
                    {archiveExtraction.entriesTotal != null
                      ? `${archiveExtraction.entriesProcessed.toLocaleString()}/${archiveExtraction.entriesTotal.toLocaleString()} entries`
                      : `${archiveExtraction.entriesProcessed.toLocaleString()} entries`}
                    {archiveExtraction.bytesWritten > 0 ? ` · ${(archiveExtraction.bytesWritten / (1024 * 1024)).toFixed(1)} MiB` : null}
                  </span>
                </>
              ) : archiveExtraction.message ? (
                <>
                  {" "}
                  <span className="universal-header-ingestion-detail">{archiveExtraction.message}</span>
                </>
              ) : null}
            </span>
          </div>
        ) : ingestionState?.isIngesting ? (
          <div className="universal-header-ingestion" role="status" aria-live="polite">
            <span className="universal-header-ingestion-dot" aria-hidden />
            <span className="universal-header-ingestion-label">Ingesting: {ingestionState.currentFile ?? "—"}</span>
          </div>
        ) : null}
      </div>
    </>
  );

  const openModelSettings = onOpenAiModelSettings;
  const nodeState: "local" | "network" | "disconnected" =
    ollamaHiveRoute === "local" ? "local" : ollamaHiveRoute === "hive" ? "network" : "disconnected";

  const nodeDot = (
    <div
      className="w-2.5 h-2.5 rounded-full shadow-sm"
      style={{
        backgroundColor:
          nodeState === "local" ? "var(--primary)" : nodeState === "network" ? "#3b82f6" : "#ef4444",
      }}
      title={`AI Node Status: ${nodeState}`}
    />
  );

  const desktopNavRight = (
    <>
      {authClearance >= 4 ? <NotificationsCenter /> : null}
      <button
        type="button"
        className={viewButtonClass(currentView === "chat")}
        onClick={() => setCurrentView("chat")}
        title={lx.navChat}
        aria-label={lx.navChat}
        aria-current={currentView === "chat" ? "page" : undefined}
      >
        <MessageSquare size={22} strokeWidth={2} aria-hidden />
      </button>
      {featureMapEnabled && authRank >= 4 ? (
        <button
          type="button"
          className={viewButtonClass(currentView === "map")}
          onClick={() => setCurrentView("map")}
          title={lx.navMap}
          aria-label={lx.navMap}
          aria-current={currentView === "map" ? "page" : undefined}
        >
          <Map size={22} strokeWidth={2} aria-hidden />
        </button>
      ) : null}
      {authRank >= 2 ? (
        <button
          type="button"
          className={viewButtonClass(currentView === "comms_hub" || currentView === "mesh" || currentView === "radio")}
          onClick={() => setCurrentView("comms_hub")}
          title={lx.navRadio}
          aria-label={lx.navRadio}
          aria-current={currentView === "comms_hub" || currentView === "mesh" || currentView === "radio" ? "page" : undefined}
        >
          <CommsHubIcon size={22} strokeWidth={2} />
        </button>
      ) : null}
      <button
        type="button"
        className="universal-header-icon-btn"
        onClick={onOpenArchive}
        title={lx.navArchive}
        aria-label={lx.navArchive}
      >
        <Library size={22} strokeWidth={2} aria-hidden />
      </button>
      {authRank >= 4 ? (
        <button
          type="button"
          className={viewButtonClass(currentView === "personnel")}
          onClick={() => setCurrentView("personnel")}
          title="Personnel registry"
          aria-label="Personnel registry"
          aria-current={currentView === "personnel" ? "page" : undefined}
        >
          <Users size={22} strokeWidth={2} aria-hidden />
        </button>
      ) : null}
      <button
        type="button"
        className={viewButtonClass(currentView === "logbook")}
        onClick={() => setCurrentView("logbook")}
        title={lx.navLogbook}
        aria-label={lx.navLogbook}
        aria-current={currentView === "logbook" ? "page" : undefined}
      >
        <BookOpen size={22} strokeWidth={2} aria-hidden />
      </button>
      <button
        type="button"
        className={`universal-header-clock-btn universal-header-calendar-merge${
          currentView === "calendar" ? " universal-header-calendar-merge-active" : ""
        }`}
        onClick={onOpenCalendar}
        title={lx.navOpenCalendar}
        aria-label={lx.navOpenCalendarAria}
        aria-current={currentView === "calendar" ? "page" : undefined}
      >
        <CalendarDays size={18} strokeWidth={2} aria-hidden className="universal-header-calendar-merge-icon" />
        <span className="universal-header-calendar-merge-text">{calendarDateTimeLabel}</span>
      </button>
      <button
        type="button"
        onClick={openModelSettings}
        className="p-2 text-slate-400 hover:text-slate-200 rounded-lg hover:bg-white/5 transition-colors cursor-pointer"
        aria-label="Select AI Model"
        title="Select AI Model"
      >
        <Bot className="w-5 h-5" />
      </button>
      {nodeDot}
      {authClearance >= 1 ? (
        <button type="button" className="universal-header-icon-btn" onClick={onOpenSettings} title={lx.navSettings} aria-label={lx.navSettings}>
          <Settings size={22} strokeWidth={2} aria-hidden />
        </button>
      ) : null}
      <button type="button" className="universal-header-icon-btn" onClick={onOpenHelp} title={lx.navHelpTitle} aria-label={lx.navHelpAria}>
        <HelpCircle size={22} strokeWidth={2} aria-hidden />
      </button>
      {batteryState && batteryState.percentage !== null && (
        <div
          className={`battery-pill${batteryState.percentage < 20 && batteryState.is_charging === false ? " battery-pill-low" : ""}`}
          aria-label={batteryAria}
        >
          <span className="battery-icon" aria-hidden>
            🔋
          </span>
          <span className="battery-text">
            {Math.round(batteryState.percentage)}%
            {batteryState.is_charging ? lx.headerBatteryVisibleCharging : ""}
            {!batteryState.is_charging &&
            batteryState.time_to_empty_minutes != null &&
            batteryState.time_to_empty_minutes !== undefined
              ? substLexicon(lx.headerBatteryVisibleMins, { mins: batteryState.time_to_empty_minutes })
              : ""}
          </span>
        </div>
      )}
    </>
  );

  const mobilePrimaryBottomNav = (
    <div className="universal-header-bottom-nav-scroll">
      {authClearance >= 4 ? (
        <div className="universal-header-bottom-nav-item">
          <NotificationsCenter />
        </div>
      ) : null}
      <div className="universal-header-bottom-nav-item">
        <button
          type="button"
          className={viewButtonClassBottomNav(currentView === "chat")}
          onClick={() => setCurrentView("chat")}
          title={lx.navChat}
          aria-label={lx.navChat}
          aria-current={currentView === "chat" ? "page" : undefined}
        >
          <MessageSquare size={22} strokeWidth={2} aria-hidden />
        </button>
      </div>
      {featureMapEnabled && authRank >= 4 ? (
        <div className="universal-header-bottom-nav-item">
          <button
            type="button"
            className={viewButtonClassBottomNav(currentView === "map")}
            onClick={() => setCurrentView("map")}
            title={lx.navMap}
            aria-label={lx.navMap}
            aria-current={currentView === "map" ? "page" : undefined}
          >
            <Map size={22} strokeWidth={2} aria-hidden />
          </button>
        </div>
      ) : null}
      {authRank >= 2 ? (
        <div className="universal-header-bottom-nav-item">
          <button
            type="button"
            className={viewButtonClassBottomNav(currentView === "comms_hub" || currentView === "mesh" || currentView === "radio")}
            onClick={() => setCurrentView("comms_hub")}
            title={lx.navRadio}
            aria-label={lx.navRadio}
            aria-current={currentView === "comms_hub" || currentView === "mesh" || currentView === "radio" ? "page" : undefined}
          >
            <CommsHubIcon size={22} strokeWidth={2} />
          </button>
        </div>
      ) : null}
      <div className="universal-header-bottom-nav-item">
        <button type="button" className="universal-header-bottom-nav-btn" onClick={onOpenArchive} title={lx.navArchive} aria-label={lx.navArchive}>
          <Library size={22} strokeWidth={2} aria-hidden />
        </button>
      </div>
      {authRank >= 4 ? (
        <div className="universal-header-bottom-nav-item">
          <button
            type="button"
            className={viewButtonClassBottomNav(currentView === "personnel")}
            onClick={() => setCurrentView("personnel")}
            title="Personnel registry"
            aria-label="Personnel registry"
            aria-current={currentView === "personnel" ? "page" : undefined}
          >
            <Users size={22} strokeWidth={2} aria-hidden />
          </button>
        </div>
      ) : null}
      <div className="universal-header-bottom-nav-item">
        <button
          type="button"
          className={viewButtonClassBottomNav(currentView === "logbook")}
          onClick={() => setCurrentView("logbook")}
          title={lx.navLogbook}
          aria-label={lx.navLogbook}
          aria-current={currentView === "logbook" ? "page" : undefined}
        >
          <BookOpen size={22} strokeWidth={2} aria-hidden />
        </button>
      </div>
    </div>
  );

  const mobileRow1Right = (
    <div className="universal-header-mobile-row1-right">
      <button
        type="button"
        className={`universal-header-clock-btn universal-header-calendar-merge universal-header-mobile-calendar${
          currentView === "calendar" ? " universal-header-calendar-merge-active" : ""
        }`}
        onClick={onOpenCalendar}
        title={lx.navOpenCalendar}
        aria-label={lx.navOpenCalendarAria}
        aria-current={currentView === "calendar" ? "page" : undefined}
      >
        <CalendarDays size={18} strokeWidth={2} aria-hidden className="universal-header-calendar-merge-icon" />
        <span className="universal-header-calendar-merge-text">{calendarDateTimeLabel}</span>
      </button>
      <button
        type="button"
        onClick={openModelSettings}
        className="p-2 text-slate-400 hover:text-slate-200 rounded-lg hover:bg-white/5 transition-colors cursor-pointer"
        aria-label="Select AI Model"
        title="Select AI Model"
      >
        <Bot className="w-5 h-5" />
      </button>
      {nodeDot}
      {authClearance >= 1 ? (
        <button type="button" className="universal-header-icon-btn" onClick={onOpenSettings} title={lx.navSettings} aria-label={lx.navSettings}>
          <Settings size={22} strokeWidth={2} aria-hidden />
        </button>
      ) : null}
      <button type="button" className="universal-header-icon-btn" onClick={onOpenHelp} title={lx.navHelpTitle} aria-label={lx.navHelpAria}>
        <HelpCircle size={22} strokeWidth={2} aria-hidden />
      </button>
      {batteryState && batteryState.percentage !== null && (
        <div
          className={`battery-pill universal-header-mobile-battery${
            batteryState.percentage < 20 && batteryState.is_charging === false ? " battery-pill-low" : ""
          }`}
          aria-label={batteryAria}
        >
          <span className="battery-icon" aria-hidden>
            🔋
          </span>
          <span className="battery-text">
            {Math.round(batteryState.percentage)}%
            {batteryState.is_charging ? lx.headerBatteryVisibleCharging : ""}
            {!batteryState.is_charging &&
            batteryState.time_to_empty_minutes != null &&
            batteryState.time_to_empty_minutes !== undefined
              ? substLexicon(lx.headerBatteryVisibleMins, { mins: batteryState.time_to_empty_minutes })
              : ""}
          </span>
        </div>
      )}
    </div>
  );

  return (
    <header
      className={`universal-header glass-panel${isMobileHeaderLayout ? " universal-header--mobile" : ""}`}
      role="banner"
      aria-label={ATHENA_FULL_NAME}
    >
      {isMobileHeaderLayout ? (
        <>
          <div className="universal-header-mobile-stack">
            <div className="universal-header-mobile-row1">
              <div className="universal-header-mobile-row1-left" aria-label="Station navigation">
                {powerHomeCluster}
              </div>
              {mobileRow1Right}
            </div>
            {sessionModelRouting != null || ollamaHiveRoute != null ? (
              <div className="universal-header-mobile-row2">
                <div className="universal-header-model-route-row universal-header-model-route-row--mobile">
                  {nodeDot}
                  <IndexingPill />
                </div>
              </div>
            ) : null}
            <div className="universal-header-mobile-status" aria-label="Background status">
              {statusStrip}
            </div>
          </div>
          <nav className="universal-header-bottom-nav" aria-label="Primary modules">
            {mobilePrimaryBottomNav}
          </nav>
        </>
      ) : (
        <div className="universal-header-desktop-toolbar">
          <div className="universal-header-toolbar">
            <div className="universal-header-nav-left" aria-label="Station navigation">
              {powerHomeCluster}
            </div>
            <div className="universal-header-toolbar-center" aria-label="Status and session">
              {statusStrip}
              {sessionModelRouting != null || ollamaHiveRoute != null ? (
                <div className="universal-header-model-route-row">
                  {nodeDot}
                  <IndexingPill />
                </div>
              ) : null}
            </div>
            <div className="universal-header-nav-right" aria-label="Views and tools">
              {desktopNavRight}
            </div>
          </div>
        </div>
      )}
      {headerDownloadBar ? (
        <div className="main-header-download-wrap" role="status" aria-live="polite">
          <div
            className={
              headerDownloadBar.mode === "determinate"
                ? "main-header-download-fill"
                : "main-header-download-fill main-header-download-fill-indeterminate"
            }
            style={headerDownloadBar.mode === "determinate" ? { width: `${headerDownloadBar.pct}%` } : undefined}
          />
          <span className="main-header-download-label">
            {headerDownloadBar.mode === "determinate"
              ? `${lx.headerDownloadLead}${headerDownloadBar.pct}% · ${headerDownloadBar.subtitle}${
                  headerDownloadBar.progressDetail != null ? ` · ${headerDownloadBar.progressDetail}` : ""
                }`
              : `${lx.headerDownloadLead}${headerDownloadBar.subtitle}${
                  headerDownloadBar.progressDetail != null ? ` · ${headerDownloadBar.progressDetail}` : ""
                }`}
          </span>
        </div>
      ) : null}
    </header>
  );
}
