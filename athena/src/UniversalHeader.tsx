import type { AppView } from "./appView";
import {
  Antenna,
  BookOpen,
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

export interface UniversalHeaderProps {
  currentView: AppView;
  setCurrentView: (view: AppView) => void;
  featureMapEnabled: boolean;
  meshRadioConnected: boolean;
  onOpenLibrary: () => void;
  onOpenSettings: () => void;
  onOpenHelp: () => void;
  onOpenMesh: () => void;
  batteryState: UniversalHeaderBatteryState | null;
  headerDownloadBar: HeaderDownloadBarState | null;
}

function viewButtonClass(active: boolean): string {
  return `universal-header-icon-btn${active ? " universal-header-icon-btn-active" : ""}`;
}

export function UniversalHeader({
  currentView,
  setCurrentView,
  featureMapEnabled,
  meshRadioConnected,
  onOpenLibrary,
  onOpenSettings,
  onOpenHelp,
  onOpenMesh,
  batteryState,
  headerDownloadBar,
}: UniversalHeaderProps) {
  return (
    <header className="universal-header glass-panel" role="banner">
      <div className="universal-header-toolbar">
        <div className="universal-header-left">
          <button
            type="button"
            className={viewButtonClass(currentView === "home")}
            onClick={() => setCurrentView("home")}
            title="Home"
            aria-label="Home"
            aria-current={currentView === "home" ? "page" : undefined}
          >
            <Home size={22} strokeWidth={2} aria-hidden />
          </button>
        </div>
        <div className="universal-header-right">
          <button
            type="button"
            className={viewButtonClass(currentView === "chat")}
            onClick={() => setCurrentView("chat")}
            title="Chat"
            aria-label="Chat"
            aria-current={currentView === "chat" ? "page" : undefined}
          >
            <MessageSquare size={22} strokeWidth={2} aria-hidden />
          </button>
          {featureMapEnabled ? (
            <button
              type="button"
              className={viewButtonClass(currentView === "map")}
              onClick={() => setCurrentView("map")}
              title="Map"
              aria-label="Map"
              aria-current={currentView === "map" ? "page" : undefined}
            >
              <Map size={22} strokeWidth={2} aria-hidden />
            </button>
          ) : null}
          <button
            type="button"
            className="universal-header-icon-btn"
            onClick={onOpenLibrary}
            title="Library"
            aria-label="Library"
          >
            <Library size={22} strokeWidth={2} aria-hidden />
          </button>
          {meshRadioConnected ? (
            <button
              type="button"
              className="universal-header-icon-btn"
              onClick={onOpenMesh}
              title="Meshtastic (USB mesh radio)"
              aria-label="Meshtastic mesh radio"
            >
              <Antenna size={22} strokeWidth={2} aria-hidden />
            </button>
          ) : null}
          <button
            type="button"
            className={viewButtonClass(currentView === "radio")}
            onClick={() => setCurrentView("radio")}
            title="Radio"
            aria-label="Radio"
            aria-current={currentView === "radio" ? "page" : undefined}
          >
            <Radio size={22} strokeWidth={2} aria-hidden />
          </button>
          <button
            type="button"
            className={viewButtonClass(currentView === "logbook")}
            onClick={() => setCurrentView("logbook")}
            title="Logbook"
            aria-label="Logbook"
            aria-current={currentView === "logbook" ? "page" : undefined}
          >
            <BookOpen size={22} strokeWidth={2} aria-hidden />
          </button>
          <button
            type="button"
            className="universal-header-icon-btn"
            onClick={onOpenSettings}
            title="Settings"
            aria-label="Settings"
          >
            <Settings size={22} strokeWidth={2} aria-hidden />
          </button>
          <button
            type="button"
            className="universal-header-icon-btn"
            onClick={onOpenHelp}
            title="Help & features guide"
            aria-label="Help and learn how to use the app"
          >
            <HelpCircle size={22} strokeWidth={2} aria-hidden />
          </button>
          {batteryState && batteryState.percentage !== null && (
            <div
              className={`battery-pill${
                batteryState.percentage < 20 && batteryState.is_charging === false ? " battery-pill-low" : ""
              }`}
              aria-label={
                batteryState.is_charging
                  ? `Battery ${Math.round(batteryState.percentage)}%, charging`
                  : `Battery ${Math.round(batteryState.percentage)}%, on battery${
                      batteryState.time_to_empty_minutes != null
                        ? `, about ${batteryState.time_to_empty_minutes} minutes to empty`
                        : ""
                    }`
              }
            >
              <span className="battery-icon" aria-hidden>
                🔋
              </span>
              <span className="battery-text">
                {Math.round(batteryState.percentage)}%
                {batteryState.is_charging ? " • Charging" : ""}
                {!batteryState.is_charging &&
                batteryState.time_to_empty_minutes != null &&
                batteryState.time_to_empty_minutes !== undefined
                  ? ` · ~${batteryState.time_to_empty_minutes}m`
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
              ? `Downloading local data — ${headerDownloadBar.pct}% · ${headerDownloadBar.subtitle}${
                  headerDownloadBar.progressDetail != null ? ` · ${headerDownloadBar.progressDetail}` : ""
                }`
              : `Downloading local data — ${headerDownloadBar.subtitle}${
                  headerDownloadBar.progressDetail != null ? ` · ${headerDownloadBar.progressDetail}` : ""
                }`}
          </span>
        </div>
      ) : null}
    </header>
  );
}
