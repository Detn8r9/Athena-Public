import { useMemo, type ReactNode, type RefObject } from "react";
import {
  Activity,
  Brain,
  Calendar as CalendarIcon,
  CircleHelp,
  Compass,
  KeyRound,
  Monitor,
  Palette,
  RadioTower,
  Route,
  Search,
  Usb,
  Volume2,
} from "lucide-react";
import "./settings.css";
import { SettingsSidebarItem } from "./SettingsSidebarItem";
import {
  SETTINGS_GROUP_LABELS,
  SETTINGS_SIDEBAR_PANELS,
  matchSettingsSearch,
  type SettingsPanelId,
  type SettingsSearchEntry,
} from "./settingsSearchIndex";

const PANEL_ICONS: Partial<Record<SettingsPanelId, ReactNode>> = {
  general: <Activity size={14} strokeWidth={2.25} />,
  dateTime: <CalendarIcon size={14} strokeWidth={2.25} />,
  calendar: <CalendarIcon size={14} strokeWidth={2.25} />,
  mesh: <RadioTower size={14} strokeWidth={2.25} />,
  location: <Compass size={14} strokeWidth={2.25} />,
  maps: <Route size={14} strokeWidth={2.25} />,
  ai: <Brain size={14} strokeWidth={2.25} />,
  apiKeys: <KeyRound size={14} strokeWidth={2.25} />,
  voice: <Volume2 size={14} strokeWidth={2.25} />,
  appearance: <Monitor size={14} strokeWidth={2.25} />,
  colorsThemes: <Palette size={14} strokeWidth={2.25} />,
  themes: <Palette size={14} strokeWidth={2.25} />,
  usbExport: <Usb size={14} strokeWidth={2.25} />,
  troubleshooting: <CircleHelp size={14} strokeWidth={2.25} />,
  developer: <CircleHelp size={14} strokeWidth={2.25} />,
};

const PANEL_TONES: Partial<Record<SettingsPanelId, string>> = {
  general: "color-mix(in srgb, var(--primary) 70%, #64748b)",
  dateTime: "color-mix(in srgb, #f59e0b 75%, #334155)",
  calendar: "color-mix(in srgb, #ef4444 70%, #334155)",
  mesh: "color-mix(in srgb, #22c55e 70%, #14532d)",
  location: "color-mix(in srgb, #06b6d4 70%, #164e63)",
  maps: "color-mix(in srgb, #3b82f6 75%, #1e3a8a)",
  ai: "color-mix(in srgb, #a855f7 70%, #4c1d95)",
  apiKeys: "color-mix(in srgb, #eab308 70%, #713f12)",
  voice: "color-mix(in srgb, #ec4899 70%, #831843)",
  appearance: "color-mix(in srgb, #64748b 80%, #0f172a)",
  colorsThemes: "color-mix(in srgb, var(--primary) 55%, #db2777)",
  usbExport: "color-mix(in srgb, #0ea5e9 75%, #0c4a6e)",
  troubleshooting: "color-mix(in srgb, #94a3b8 70%, #334155)",
};

const GROUP_ORDER: SettingsSearchEntry["group"][] = [
  "system",
  "tactical",
  "intelligence",
  "appearance",
  "data",
];

export type SettingsShellMe = {
  name: string;
  displayName: string | null;
  division: string;
  rankLabel: string;
  avatar: ReactNode;
  accountHint: string;
};

type Props = {
  panel: SettingsPanelId;
  onSelectPanel: (panel: SettingsPanelId) => void;
  query: string;
  onQueryChange: (q: string) => void;
  onSelectSearchResult: (entry: SettingsSearchEntry) => void;
  detailTitle: string;
  detailBack?: { label: string; ariaLabel: string; onClick: () => void } | null;
  me: SettingsShellMe;
  canUseUsbExport: boolean;
  searchPlaceholder: string;
  searchAria: string;
  searchNoMatch: string;
  navAria: string;
  chromeTitle: string;
  chromeTitleAria: string;
  onChromeTitleClick?: () => void;
  closeAria: string;
  onClose: () => void;
  children: ReactNode;
  detailScrollRef?: RefObject<HTMLDivElement | null>;
};

export function SettingsShell({
  panel,
  onSelectPanel,
  query,
  onQueryChange,
  onSelectSearchResult,
  detailTitle,
  detailBack,
  me,
  canUseUsbExport,
  searchPlaceholder,
  searchAria,
  searchNoMatch,
  navAria,
  chromeTitle,
  chromeTitleAria,
  onChromeTitleClick,
  closeAria,
  onClose,
  children,
  detailScrollRef,
}: Props) {
  const searching = query.trim().length > 0;
  const results = useMemo(() => {
    if (!searching) return [];
    return matchSettingsSearch(query).filter((e) => {
      if (e.hostOnly === "usbExport" && !canUseUsbExport) return false;
      return true;
    });
  }, [query, searching, canUseUsbExport]);

  const sidebarByGroup = useMemo(() => {
    const items = SETTINGS_SIDEBAR_PANELS.filter((p) => {
      if (p.panel === "me") return false;
      if (p.hostOnly === "usbExport" && !canUseUsbExport) return false;
      return true;
    });
    return GROUP_ORDER.map((group) => ({
      group,
      label: SETTINGS_GROUP_LABELS[group],
      items: items.filter((i) => i.group === group),
    })).filter((g) => g.items.length > 0);
  }, [canUseUsbExport]);

  const meSelected = panel === "me" && !searching;

  return (
    <div className="settings-shell">
      <div className="settings-shell__chrome">
        <h2
          className={onChromeTitleClick ? "settings-shell__chrome-title settings-title-tappable" : "settings-shell__chrome-title"}
          onClick={onChromeTitleClick}
          role={onChromeTitleClick ? "button" : undefined}
          aria-label={chromeTitleAria}
        >
          {chromeTitle}
        </h2>
        <button type="button" className="close-btn" onClick={onClose} aria-label={closeAria}>
          ×
        </button>
      </div>

      <div className="settings-shell__body">
        <aside className="settings-shell__sidebar" aria-label={navAria}>
          <div className="settings-shell__search">
            <Search className="settings-shell__search-icon" size={15} strokeWidth={2} aria-hidden />
            <input
              id="settings-shell-search"
              type="search"
              className="settings-input settings-shell__search-input"
              value={query}
              onChange={(e) => onQueryChange(e.target.value)}
              placeholder={searchPlaceholder}
              aria-label={searchAria}
              autoComplete="off"
              autoCorrect="off"
              spellCheck={false}
            />
          </div>

          <div className="settings-shell__sidebar-scroll">
            {searching ? (
              results.length === 0 ? (
                <p className="settings-search-empty" role="status">
                  {searchNoMatch}
                </p>
              ) : (
                results.map((entry) => (
                  <button
                    key={entry.id}
                    type="button"
                    className="settings-search-result"
                    onClick={() => onSelectSearchResult(entry)}
                  >
                    <span className="settings-search-result__title">{entry.title}</span>
                    <span className="settings-search-result__path">
                      {entry.pathLabel}
                      {entry.title !== entry.pathLabel ? ` › ${entry.title}` : ""}
                    </span>
                  </button>
                ))
              )
            ) : (
              <>
                <button
                  type="button"
                  className={[
                    "settings-shell__me-card",
                    meSelected ? "settings-shell__me-card--selected" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  onClick={() => onSelectPanel("me")}
                  aria-current={meSelected ? "page" : undefined}
                >
                  <div className="settings-shell__me-avatar" aria-hidden>
                    {me.avatar}
                  </div>
                  <div className="settings-shell__me-text">
                    <span className="settings-shell__me-name">
                      {(me.displayName?.trim() || me.name || "").trim() || "—"}
                    </span>
                    <span className="settings-shell__me-sub">{me.accountHint}</span>
                    <span className="settings-shell__me-meta">
                      {me.division} · {me.rankLabel}
                    </span>
                  </div>
                </button>

                {sidebarByGroup.map(({ group, label, items }) => (
                  <div key={group} className="settings-sidebar-group">
                    <div className="settings-sidebar-group__label">{label}</div>
                    {items.map((item) => (
                      <SettingsSidebarItem
                        key={item.panel}
                        label={item.title}
                        icon={PANEL_ICONS[item.panel] ?? <Activity size={14} />}
                        iconTone={PANEL_TONES[item.panel]}
                        selected={panel === item.panel || (item.panel === "colorsThemes" && panel === "themes")}
                        onClick={() => onSelectPanel(item.panel)}
                      />
                    ))}
                  </div>
                ))}
              </>
            )}
          </div>
        </aside>

        <section className="settings-shell__detail" aria-label={detailTitle}>
          <header className="settings-shell__detail-header">
            {detailBack ? (
              <button
                type="button"
                className="settings-back-btn settings-shell__detail-back"
                onClick={detailBack.onClick}
                aria-label={detailBack.ariaLabel}
              >
                {detailBack.label}
              </button>
            ) : null}
            <h3 className="settings-shell__detail-title">{detailTitle}</h3>
          </header>
          <div className="settings-shell__detail-scroll" ref={detailScrollRef}>
            {children}
          </div>
        </section>
      </div>
    </div>
  );
}
