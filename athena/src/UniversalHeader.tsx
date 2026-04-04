import type { AppView } from "./appView";
import { ATHENA_FULL_NAME } from "./shared/branding";
import { substLexicon, useLexicon } from "./shared/lexicon";
import {
  Antenna,
  BookOpen,
  CalendarDays,
  HelpCircle,
  Home,
  Library,
  Map,
  MessageSquare,
  Radio,
  Settings,
} from "lucide-react";

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

export interface UniversalHeaderProps {
  currentView: AppView;
  setCurrentView: (view: AppView) => void;
  featureMapEnabled: boolean;
  meshRadioConnected: boolean;
  onOpenArchive: () => void;
  onOpenSettings: () => void;
  onOpenHelp: () => void;
  /** Open the full Meshtastic / mesh network screen (not only the quick-send modal). */
  onNavigateToMeshNetwork: () => void;
  onOpenCalendar: () => void;
  calendarDateTimeLabel: string;
  batteryState: UniversalHeaderBatteryState | null;
  headerDownloadBar: HeaderDownloadBarState | null;
  ingestionState: UniversalHeaderIngestionState | null;
}

function viewButtonClass(active: boolean): string {
  return `universal-header-icon-btn${active ? " universal-header-icon-btn-active" : ""}`;
}

export function UniversalHeader({
  currentView,
  setCurrentView,
  featureMapEnabled,
  meshRadioConnected,
  onOpenArchive,
  onOpenSettings,
  onOpenHelp,
  onNavigateToMeshNetwork,
  onOpenCalendar,
  calendarDateTimeLabel,
  batteryState,
  headerDownloadBar,
  ingestionState,
}: UniversalHeaderProps) {
  const lx = useLexicon();
  const pctRounded = batteryState?.percentage != null ? Math.round(batteryState.percentage) : 0;
  const mins = batteryState?.time_to_empty_minutes;
  const batteryAria =
    batteryState && batteryState.percentage !== null
      ? batteryState.is_charging
        ? substLexicon(lx.headerBatteryAriaCharging, { pct: pctRounded })
        : substLexicon(lx.headerBatteryAriaOnBattery, { pct: pctRounded }) +
          (mins != null ? substLexicon(lx.headerBatteryAriaMinsSuffix, { mins }) : "")
      : "";
  return (
    <header className="universal-header glass-panel" role="banner" aria-label={ATHENA_FULL_NAME}>
      <div className="universal-header-toolbar">
        <div className="universal-header-left">
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
          {ingestionState?.isIngesting ? (
            <div className="universal-header-ingestion" role="status" aria-live="polite">
              <span className="universal-header-ingestion-dot" aria-hidden />
              <span className="universal-header-ingestion-label">
                Ingesting: {ingestionState.currentFile ?? "—"}
              </span>
            </div>
          ) : null}
        </div>
        <div className="universal-header-right">
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
          {featureMapEnabled ? (
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
          <button
            type="button"
            className="universal-header-icon-btn"
            onClick={onOpenArchive}
            title={lx.navArchive}
            aria-label={lx.navArchive}
          >
            <Library size={22} strokeWidth={2} aria-hidden />
          </button>
          <button
            type="button"
            className={viewButtonClass(currentView === "mesh")}
            onClick={onNavigateToMeshNetwork}
            title={meshRadioConnected ? lx.navMeshTitle : `${lx.navMeshTitle} (open setup)`}
            aria-label={lx.navMeshAria}
            aria-current={currentView === "mesh" ? "page" : undefined}
          >
            <Antenna size={22} strokeWidth={2} aria-hidden />
          </button>
          <button
            type="button"
            className={viewButtonClass(currentView === "radio")}
            onClick={() => setCurrentView("radio")}
            title={lx.navRadio}
            aria-label={lx.navRadio}
            aria-current={currentView === "radio" ? "page" : undefined}
          >
            <Radio size={22} strokeWidth={2} aria-hidden />
          </button>
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
            className={viewButtonClass(currentView === "calendar")}
            onClick={onOpenCalendar}
            title={lx.navCalendar}
            aria-label={lx.navCalendar}
            aria-current={currentView === "calendar" ? "page" : undefined}
          >
            <CalendarDays size={22} strokeWidth={2} aria-hidden />
          </button>
          <button
            type="button"
            className="universal-header-clock-btn"
            onClick={onOpenCalendar}
            title={lx.navOpenCalendar}
            aria-label={lx.navOpenCalendarAria}
          >
            {calendarDateTimeLabel}
          </button>
          <button
            type="button"
            className="universal-header-icon-btn"
            onClick={onOpenSettings}
            title={lx.navSettings}
            aria-label={lx.navSettings}
          >
            <Settings size={22} strokeWidth={2} aria-hidden />
          </button>
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
