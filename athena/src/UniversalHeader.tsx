import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppView } from "./appView";
import { ATHENA_FULL_NAME } from "./shared/branding";
import { substLexicon, useLexicon } from "./shared/lexicon";
import {
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
import { CommsHubIcon } from "./shared/CommsHubIcon";
import { NotificationsCenter } from "./components/NotificationsCenter";

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

export interface UniversalHeaderProps {
  currentView: AppView;
  setCurrentView: (view: AppView) => void;
  featureMapEnabled: boolean;
  onOpenArchive: () => void;
  onOpenSettings: () => void;
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
}

function viewButtonClass(active: boolean): string {
  return `universal-header-icon-btn${active ? " universal-header-icon-btn-active" : ""}`;
}

export function UniversalHeader({
  currentView,
  setCurrentView,
  featureMapEnabled,
  onOpenArchive,
  onOpenSettings,
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
}: UniversalHeaderProps) {
  const lx = useLexicon();
  const [powerMenuOpen, setPowerMenuOpen] = useState(false);
  const powerWrapRef = useRef<HTMLDivElement>(null);
  const powerMenuId = "universal-header-power-menu";

  const pctRounded = batteryState?.percentage != null ? Math.round(batteryState.percentage) : 0;
  const mins = batteryState?.time_to_empty_minutes;
  const batteryAria =
    batteryState && batteryState.percentage !== null
      ? batteryState.is_charging
        ? substLexicon(lx.headerBatteryAriaCharging, { pct: pctRounded })
        : substLexicon(lx.headerBatteryAriaOnBattery, { pct: pctRounded }) +
          (mins != null ? substLexicon(lx.headerBatteryAriaMinsSuffix, { mins }) : "")
      : "";

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

  return (
    <header className="universal-header glass-panel" role="banner" aria-label={ATHENA_FULL_NAME}>
      <div className="universal-header-toolbar">
        <div className="universal-header-left">
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
                    Log out, pause local AI, disable mesh auto-reply, and stop the bundled Ollama sidecar if Athena
                    started it.
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
        </div>
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
                      {archiveExtraction.bytesWritten > 0
                        ? ` · ${(archiveExtraction.bytesWritten / (1024 * 1024)).toFixed(1)} MiB`
                        : null}
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
              <span className="universal-header-ingestion-label">
                Ingesting: {ingestionState.currentFile ?? "—"}
              </span>
            </div>
          ) : null}
        </div>
        <div className="universal-header-right">
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
          {authClearance >= 1 ? (
            <button
              type="button"
              className="universal-header-icon-btn"
              onClick={onOpenSettings}
              title={lx.navSettings}
              aria-label={lx.navSettings}
            >
              <Settings size={22} strokeWidth={2} aria-hidden />
            </button>
          ) : null}
          <button
            type="button"
            className="universal-header-icon-btn"
            onClick={onOpenHelp}
            title={lx.navHelpTitle}
            aria-label={lx.navHelpAria}
          >
            <HelpCircle size={22} strokeWidth={2} aria-hidden />
          </button>
          {batteryState && batteryState.percentage !== null && (
            <div
              className={`battery-pill${
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
      </div>
      {headerDownloadBar ? (
        <div className="main-header-download-wrap" role="status" aria-live="polite">
          <div
            className={
              headerDownloadBar.mode === "determinate"
                ? "main-header-download-fill"
                : "main-header-download-fill main-header-download-fill-indeterminate"
            }
            style={
              headerDownloadBar.mode === "determinate" ? { width: `${headerDownloadBar.pct}%` } : undefined
            }
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
