import { createContext, useContext, useMemo, type ReactNode } from "react";

export type LexiconThemeId = "survival" | "fallout";

/** Replace `{{key}}` placeholders in lexicon strings. */
export function substLexicon(template: string, vars: Record<string, string | number>): string {
  let out = template;
  for (const [k, v] of Object.entries(vars)) {
    out = out.split(`{{${k}}}`).join(String(v));
  }
  return out;
}

export type Lexicon = {
  navArchiveView: string;
  archiveScreenTitle: string;
  archiveScreenSubtitle: string;
  tabLocalArchive: string;
  tabDownloadNetwork: string;
  archiveHubIntro: string;
  archivePrimaryTablistAria: string;
  archiveNetworkTablistAria: string;
  tabArchivePackages: string;
  tabArchiveDocuments: string;
  tabArchiveMaps: string;
  tabArchiveAll: string;
  tabArchiveAcademy: string;
  archiveLocalTablistAria: string;
  archiveSortLabel: string;
  archiveSortName: string;
  archiveSortSize: string;
  archiveSortPath: string;
  archiveSortDirAsc: string;
  archiveSortDirDesc: string;
  archiveSearchFilesPlaceholder: string;
  sectionAcademyCurriculum: string;
  sectionAcademyCurriculumBody: string;
  academyOtherDownloads: string;
  archiveLocalNoMatches: string;
  archiveLocalTitle: string;
  archiveLocalSubtitle: string;
  regionalMapsNotSet: string;
  sectionTierPackages: string;
  sectionAlacarte: string;
  dataSources: string;
  openInFinder: string;
  actionDeployTier: string;
  actionRequisitionFile: string;
  cancelDownload: string;
  emptyLocalArchive: string;
  noDataSourcesHint: string;
  addDataSource: string;
  removeDataSource: string;
  pathPlaceholder: string;
  closeModal: string;
  regionalMaps: string;
  setDownloadArea: string;
  setDownloadAreaBody: string;
  locationFieldLabel: string;
  lookUpAndSave: string;
  useNycDefault: string;
  notNow: string;
  setLocation: string;
  changeArea: string;
  noAreaWarning: string;
  destinationSummary: string;
  watchFoldersLabel: string;
  fileMissingOnDisk: string;
  installingLabel: string;
  tierFilesCount: string;
  sizeUnknown: string;
  catalogDuplicateNote: string;

  navHome: string;
  navChat: string;
  navMap: string;
  navArchive: string;
  navMeshTitle: string;
  navMeshAria: string;
  navRadio: string;
  navLogbook: string;
  navCalendar: string;
  navOpenCalendar: string;
  navOpenCalendarAria: string;
  navSettings: string;
  navHelpTitle: string;
  navHelpAria: string;

  actionSave: string;
  actionCancel: string;
  actionDelete: string;
  stateSaving: string;
  stateApplying: string;
  stateStarting: string;
  stateSending: string;
  stateRequesting: string;
  actionConnect: string;
  actionConnected: string;
  actionConnecting: string;

  mapDisabledTitle: string;
  mapDisabledBodyBefore: string;
  mapDisabledLocationWord: string;
  mapDisabledBodyAfter: string;
  mapDisabledOpenLocation: string;
  mapDisabledBackHome: string;

  settingsBackLabel: string;
  settingsBackAria: string;
  settingsCloseAria: string;
  settingsMenuTitle: string;
  settingsMenuTitleAria: string;
  settingsPanelGeneral: string;
  settingsPanelAi: string;
  settingsPanelLocation: string;
  settingsPanelApiKeys: string;
  settingsPanelMaps: string;
  settingsPanelMesh: string;
  settingsPanelVoice: string;
  settingsPanelCalendar: string;
  settingsPanelDateTime: string;
  settingsPanelAppearance: string;
  settingsPanelThemes: string;
  settingsPanelThemeStudio: string;
  settingsPanelTroubleshooting: string;
  settingsPanelUsbExport: string;
  settingsPanelDeveloper: string;
  settingsNavAria: string;
  settingsNavIntroLead: string;
  settingsNavMeshConnectedNote: string;
  settingsNavMeshDisconnectedNote: string;
  settingsNavGroupProfileCalendar: string;
  settingsNavGroupAiKnowledge: string;
  settingsNavGroupMapsRadio: string;
  settingsNavGroupLookFeel: string;
  settingsNavGroupBackupHelp: string;
  settingsNavGeneral: string;
  settingsNavGeneralDesc: string;
  settingsNavCalendar: string;
  settingsNavCalendarDesc: string;
  settingsNavDateTime: string;
  settingsNavDateTimeDesc: string;
  settingsNavAiModels: string;
  settingsNavAiModelsDesc: string;
  settingsNavApiKeys: string;
  settingsNavApiKeysDesc: string;
  settingsNavLocation: string;
  settingsNavLocationDesc: string;
  settingsNavOfflineMaps: string;
  settingsNavOfflineMapsDesc: string;
  settingsNavOfflineMapsNoteMapOn: string;
  settingsNavOfflineMapsNoteMapOff: string;
  settingsNavMesh: string;
  settingsNavMeshChecking: string;
  settingsNavMeshNone: string;
  settingsNavMeshOk: string;
  settingsNavVoice: string;
  settingsNavVoiceDesc: string;
  settingsNavVoiceHiddenNote: string;
  settingsNavWindowDisplay: string;
  settingsNavWindowDisplayDesc: string;
  settingsNavThemes: string;
  settingsNavThemesDesc: string;
  settingsNavTroubleshooting: string;
  settingsNavTroubleshootingDesc: string;
  settingsNavUsbExport: string;
  settingsNavUsbExportDesc: string;

  helpModalTitle: string;
  helpCloseAria: string;
  helpSearchLabel: string;
  helpSearchPlaceholder: string;
  helpNavIntroMatching: string;
  helpNavIntroChoose: string;
  helpEmptyNav: string;
  helpEmptyDetail: string;
  helpNavSectionsAria: string;
  helpTopicOverviewLabel: string;
  helpTopicOverviewBlurb: string;
  helpTopicHomeLabel: string;
  helpTopicHomeBlurb: string;
  helpTopicTacticalMapLabel: string;
  helpTopicTacticalMapBlurb: string;
  helpTopicMapsOfflineLabel: string;
  helpTopicMapsOfflineBlurb: string;
  helpTopicChatLabel: string;
  helpTopicChatBlurb: string;
  helpTopicToolsLabel: string;
  helpTopicToolsBlurb: string;
  helpTopicSidebarLabel: string;
  helpTopicSidebarBlurb: string;
  helpTopicHeaderLabel: string;
  helpTopicHeaderBlurb: string;
  helpTopicLibraryLabel: string;
  helpTopicLibraryBlurb: string;
  helpTopicMeshtasticLabel: string;
  helpTopicMeshtasticBlurb: string;
  helpTopicRadioCommsLabel: string;
  helpTopicRadioCommsBlurb: string;
  helpTopicSettingsLabel: string;
  helpTopicSettingsBlurb: string;
  helpTopicRepliesLabel: string;
  helpTopicRepliesBlurb: string;
  helpTopicHelpAssistantLabel: string;
  helpTopicHelpAssistantBlurb: string;

  headerBatteryAriaCharging: string;
  headerBatteryAriaOnBattery: string;
  headerBatteryAriaMinsSuffix: string;
  headerBatteryVisibleCharging: string;
  headerBatteryVisibleMins: string;
  headerDownloadLead: string;

  chatSend: string;
  citationOpenFile: string;
  citationPageLabel: string;

  calendarAriaPrev: string;
  calendarAriaNext: string;
  calendarToday: string;
  calendarTabMonth: string;
  calendarTabWeek: string;
  calendarTabDay: string;
  calendarTabAgenda: string;
  calendarSearchPlaceholder: string;
  calendarSearchAria: string;
  calendarMore: string;
  calendarQuickAddPlaceholder: string;
  calendarParse: string;
  calendarPullMesh: string;
  calendarExportIcs: string;
  calendarImportIcs: string;
  calendarAllDay: string;
  calendarHolidayTag: string;
  calendarUpcoming: string;
  calendarThisDay: string;
  calendarNoEvents: string;
  calendarMapBtn: string;
  calendarPinBtn: string;
  calendarNewEvent: string;
  calendarTemplateMedName: string;
  calendarTemplateMedTitle: string;
  calendarTemplatePatrolName: string;
  calendarTemplatePatrolTitle: string;
  calendarTemplateSupplyName: string;
  calendarTemplateSupplyTitle: string;
  calendarTemplateRadioName: string;
  calendarTemplateRadioTitle: string;
  calendarFieldTitle: string;
  calendarFieldNotes: string;
  calendarFieldStarts: string;
  calendarFieldEnds: string;
  calendarFieldLocation: string;
  calendarFieldMapPin: string;
  calendarPinNone: string;
  calendarNoPinsHint: string;
  calendarOpenMapPins: string;
  calendarFieldParticipants: string;
  calendarParticipantsPlaceholder: string;
  calendarFieldRepeat: string;
  calendarFieldAlert: string;
  calendarRepeatNone: string;
  calendarRepeatDaily: string;
  calendarRepeatWeekly: string;
  calendarRepeatMonthly: string;
  calendarRepeatYearly: string;
  calendarAlertNone: string;
  calendarAlert10: string;
  calendarAlert30: string;
  calendarAlert60: string;
  calendarAlert1440: string;
  calendarAddToCalendar: string;
  calendarUntitledEvent: string;
  calendarImportedEventDefault: string;
  /** Short line above template chips in the new-event sheet */
  calendarTemplatesHint: string;

  logbookProfileTitle: string;
  logbookProfileBlurb: string;
  logbookLabelName: string;
  logbookLabelOccupation: string;
  logbookSaveContinue: string;
  logbookSidebarAria: string;
  logbookTitle: string;
  logbookTagline: string;
  logbookSearchPlaceholder: string;
  logbookSearchAria: string;
  logbookSortLabel: string;
  logbookSortDate: string;
  logbookSortAuthor: string;
  logbookSortOccupation: string;
  logbookNewNoteTitle: string;
  logbookFocusNewNote: string;
  logbookLoading: string;
  logbookNoMatching: string;
  logbookEmptyPreview: string;
  logbookEntryHeader: string;
  logbookPullMesh: string;
  logbookPullMeshTitleOff: string;
  logbookPullMeshTitleOn: string;
  logbookHelpToggleTitle: string;
  logbookHelpAria: string;
  logbookHelpBody: string;
  logbookUnknownAuthor: string;
  logbookThisDevice: string;
  logbookMeshNodeLabel: string;
  logbookFollowUpTag: string;
  logbookAddFollowUp: string;
  logbookDeleteEntry: string;
  logbookSelectOrWrite: string;
  logbookFollowUpHint: string;
  logbookComposeAria: string;
  logbookNewNoteLabel: string;
  logbookDraftPlaceholder: string;
  logbookProfileCompleteModal: string;
  logbookProfileCompleteGeneral: string;
  logbookMeshBudget: string;
  logbookMeshNoRadioHint: string;
  logbookMeshTruncateHint: string;
  logbookCancelFollowUp: string;
  logbookSaveNote: string;
  logbookErrSaveProfile: string;
  logbookErrSaveNote: string;
  logbookErrDelete: string;
  logbookErrMeshPull: string;
  logbookErrLoad: string;

  radioTitle: string;
  radioSignalHealth: string;
  radioNoTelemetry: string;
  radioEncryptedChannelHeader: string;
  radioProfileLabel: string;
  radioNoProfilesOption: string;
  radioApplyToRadio: string;
  radioDeleteProfile: string;
  radioNewProfile: string;
  radioPlaceholderName: string;
  radioPlaceholderFreqMhz: string;
  radioPlaceholderEncryptionKey: string;
  radioAriaChannelName: string;
  radioAriaFreqMhz: string;
  radioAriaEncryptionKey: string;
  radioSaveProfile: string;
  radioSdrTitle: string;
  radioSdrBlurb: string;
  radioDongle: string;
  radioRefreshDevices: string;
  radioMhzLabel: string;
  radioModeLabel: string;
  radioModeNfm: string;
  radioModeWbfm: string;
  radioModeAm: string;
  radioStart: string;
  radioStop: string;
  radioAriaSdrSpectrum: string;
  radioSdrLogPlaceholder: string;
  radioAriaSerialPorts: string;
  radioNoSerialPorts: string;
  radioRefresh: string;
  radioConnect: string;
  radioConnected: string;
  radioOutboundPackets: string;
  radioNoTrackedPackets: string;
  radioLanClusterTitle: string;
  radioNoLanPeers: string;
  radioTerminalOutputAria: string;
  radioTerminalWaiting: string;
  radioTxPlaceholder: string;
  radioTxAria: string;
  radioSend: string;
  radioConfirmDeleteChannel: string;
  radioErrValidFreqMhz: string;
  radioErrSdrStart: string;
  radioErrLoadPorts: string;
  radioErrConnectionFailed: string;
  radioErrSendFailed: string;
  radioErrApplyChannel: string;
  radioErrChannelFields: string;
  radioErrSaveChannel: string;
  radioErrDeleteChannel: string;
  radioAriaRtlDevice: string;
  radioAriaFreqMegahertz: string;
  radioAriaDemodMode: string;
};

const SURVIVAL_LEXICON: Lexicon = {
  navArchiveView: "The Archive",
  archiveScreenTitle: "The Forge",
  archiveScreenSubtitle:
    "Autonomous Tactical Heuristic Emergency Network Archive (ATHENA) — unified offline library and data sources.",
  tabLocalArchive: "Local Archive",
  tabDownloadNetwork: "Download Network",
  archiveHubIntro:
    "Local Archive lists files already in your data sources — use All, Packages, Documents, Maps, and Academy tabs with search and sort. Download Network adds Packages (tiers, Academy by grade/subject, à la carte), Documents (RAG / Kiwix), and Maps.",
  archivePrimaryTablistAria: "Archive: local files or download network",
  archiveNetworkTablistAria: "Download network sections",
  tabArchivePackages: "Packages",
  tabArchiveDocuments: "Documents",
  tabArchiveMaps: "Maps",
  tabArchiveAll: "All",
  tabArchiveAcademy: "Academy",
  archiveLocalTablistAria: "Local archive file categories",
  archiveSortLabel: "Sort",
  archiveSortName: "Name",
  archiveSortSize: "Size",
  archiveSortPath: "Path",
  archiveSortDirAsc: "Ascending",
  archiveSortDirDesc: "Descending",
  archiveSearchFilesPlaceholder: "Search file name or path…",
  sectionAcademyCurriculum: "Academy downloads",
  sectionAcademyCurriculumBody:
    "Kiwix archives for the Academy curriculum, grouped by grade band and subject. They install into library/academy/ like other tier files.",
  academyOtherDownloads: "Other academy files",
  archiveLocalNoMatches: "No files match this category or search.",
  archiveLocalTitle: "Your archive",
  archiveLocalSubtitle:
    "Data source folders and indexed files. Use the same categories as Download Network, plus search and sort, to find what you already have.",
  regionalMapsNotSet: "Not set",
  sectionTierPackages: "The Forge",
  sectionAlacarte: "The Cache",
  dataSources: "Data Sources",
  openInFinder: "Open location",
  actionDeployTier: "Deploy",
  actionRequisitionFile: "Requisition",
  cancelDownload: "Cancel download",
  emptyLocalArchive:
    "No files were found under your data sources yet. Add folders in Data Sources or install packages from Download Network.",
  noDataSourcesHint:
    "Add at least one data source folder so downloads know where to land. Network and shared volumes can be used if they are mounted as a path on this Mac.",
  addDataSource: "Add folder",
  removeDataSource: "Remove",
  pathPlaceholder: "Paste path (for example /Volumes/Share/Athena)",
  closeModal: "Close",
  regionalMaps: "Regional maps",
  setDownloadArea: "Set your download area",
  setDownloadAreaBody:
    "The Satchel adds offline state maps (Protomaps) for where you are, plus nearby states when you are close to a border. Wikipedia and other tiers are unchanged.",
  locationFieldLabel: "US city, address, or ZIP",
  lookUpAndSave: "Look up and save",
  useNycDefault: "Use NYC default (offline)",
  notNow: "Not now",
  setLocation: "Set location",
  changeArea: "Change area",
  noAreaWarning:
    "No download area set — The Satchel will only include Wikipedia until you choose a US location for state maps.",
  destinationSummary: "Primary destination",
  watchFoldersLabel: "Data source folders",
  fileMissingOnDisk: "Missing on disk (still in index)",
  installingLabel: "Installing",
  tierFilesCount: "files",
  sizeUnknown: "size unknown",
  catalogDuplicateNote: "Individual files also appear below in The Cache.",

  navHome: "Home",
  navChat: "Chat",
  navMap: "Map",
  navArchive: "Archive",
  navMeshTitle: "Meshtastic (USB mesh radio)",
  navMeshAria: "Meshtastic mesh radio",
  navRadio: "Radio",
  navLogbook: "Logbook",
  navCalendar: "Calendar",
  navOpenCalendar: "Open Calendar",
  navOpenCalendarAria: "Open calendar and date time settings",
  navSettings: "Settings",
  navHelpTitle: "Help & features guide",
  navHelpAria: "Help and learn how to use the app",

  actionSave: "Save",
  actionCancel: "Cancel",
  actionDelete: "Delete",
  stateSaving: "Saving…",
  stateApplying: "Applying…",
  stateStarting: "Starting…",
  stateSending: "Sending…",
  stateRequesting: "Requesting…",
  actionConnect: "Connect",
  actionConnected: "Connected",
  actionConnecting: "Connecting…",

  mapDisabledTitle: "Maps are off",
  mapDisabledBodyBefore: "Enable Maps under ",
  mapDisabledLocationWord: "Location",
  mapDisabledBodyAfter: " in Settings to open the map viewer.",
  mapDisabledOpenLocation: "Open Location settings",
  mapDisabledBackHome: "Back to Home",

  settingsBackLabel: "← Back",
  settingsBackAria: "Back to settings menu",
  settingsCloseAria: "Close settings",
  settingsMenuTitle: "Settings",
  settingsMenuTitleAria: "Settings",
  settingsPanelGeneral: "General",
  settingsPanelAi: "AI & Models",
  settingsPanelLocation: "Location",
  settingsPanelApiKeys: "API keys",
  settingsPanelMaps: "Offline maps",
  settingsPanelMesh: "Mesh",
  settingsPanelVoice: "Voice",
  settingsPanelCalendar: "Calendar",
  settingsPanelDateTime: "Date & Time",
  settingsPanelAppearance: "Window & Display",
  settingsPanelThemes: "Themes & Colors",
  settingsPanelThemeStudio: "Theme Studio",
  settingsPanelTroubleshooting: "Troubleshooting",
  settingsPanelUsbExport: "USB export",
  settingsPanelDeveloper: "Developer",
  settingsNavAria: "Settings sections",
  settingsNavIntroLead:
    "Pick a section below. New to ATHENA? Start with General (what the name means and your library folder), then AI & Models for chat. Use Backup & help for USB transfer or the error log.",
  settingsNavMeshConnectedNote: " The mesh radio button appears when a Meshtastic device is connected.",
  settingsNavMeshDisconnectedNote:
    " No Meshtastic radio detected—mesh options are unavailable until a device is connected.",
  settingsNavGroupProfileCalendar: "Profile & Calendar",
  settingsNavGroupAiKnowledge: "AI & Knowledge",
  settingsNavGroupMapsRadio: "Maps, Radio & Devices",
  settingsNavGroupLookFeel: "Look & feel",
  settingsNavGroupBackupHelp: "Backup & help",
  settingsNavGeneral: "General",
  settingsNavGeneralDesc: "About ATHENA, profile, startup view, help, maintenance",
  settingsNavCalendar: "Calendar",
  settingsNavCalendarDesc: "Holiday region, reminders, templates, ICS",
  settingsNavDateTime: "Date & Time",
  settingsNavDateTimeDesc: "12/24-hour clock and quiet reminder hours",
  settingsNavAiModels: "AI & Models",
  settingsNavAiModelsDesc: "Ollama, personality, model size, memory, citations",
  settingsNavApiKeys: "API keys",
  settingsNavApiKeysDesc: "Paste and store provider keys for advanced features",
  settingsNavLocation: "Location",
  settingsNavLocationDesc:
    "AI place context and Kiwix local-data sheet (not The Archive download-area pin)",
  settingsNavOfflineMaps: "Offline maps",
  settingsNavOfflineMapsDesc:
    "Tactical basemaps, Kiwix map ZIMs, imports (Satchel state tiles: The Archive)",
  settingsNavOfflineMapsNoteMapOn: " · Use the Map icon in the top bar when Maps is enabled",
  settingsNavOfflineMapsNoteMapOff: " · Turn on Maps under Location to show the map button",
  settingsNavMesh: "Mesh",
  settingsNavMeshChecking: "Checking for USB radio…",
  settingsNavMeshNone: "No Meshtastic radio connected",
  settingsNavMeshOk: "Meshtastic auto-reply and mesh traffic",
  settingsNavVoice: "Voice",
  settingsNavVoiceDesc: "Read-aloud (TTS)",
  settingsNavVoiceHiddenNote: " · Hidden in this build",
  settingsNavWindowDisplay: "Window & Display",
  settingsNavWindowDisplayDesc: "Fullscreen and night vision",
  settingsNavThemes: "Themes & Colors",
  settingsNavThemesDesc: "Experience style, color palette, custom themes",
  settingsNavTroubleshooting: "Troubleshooting",
  settingsNavTroubleshootingDesc: "Error log, shortcuts to Library and Help",
  settingsNavUsbExport: "USB export",
  settingsNavUsbExportDesc: "Portable copy to a flash drive (optional format on macOS)",

  helpModalTitle: "Help & features",
  helpCloseAria: "Close help",
  helpSearchLabel: "Search topics",
  helpSearchPlaceholder: "Filter topics (e.g. map, mesh, library)…",
  helpNavIntroMatching: "Matching topics:",
  helpNavIntroChoose: "Choose a topic:",
  helpEmptyNav: "No topics match. Clear the search or try Ask Athena.",
  helpEmptyDetail:
    "No topics match this filter. Clear the search box above, or open Ask Athena from the sidebar once it appears again.",
  helpNavSectionsAria: "Help sections",
  helpTopicOverviewLabel: "Overview",
  helpTopicOverviewBlurb: "What Athena is and how it works",
  helpTopicHomeLabel: "Home",
  helpTopicHomeBlurb: "Dashboard, CPU/RAM, quick launch",
  helpTopicTacticalMapLabel: "Tactical map",
  helpTopicTacticalMapBlurb: "Pins, layers, mesh, right-click",
  helpTopicMapsOfflineLabel: "Maps & offline data",
  helpTopicMapsOfflineBlurb: "Tactical basemaps vs Library tiers",
  helpTopicChatLabel: "Chat & composer",
  helpTopicChatBlurb: "Sending messages and shortcuts",
  helpTopicToolsLabel: "Tools",
  helpTopicToolsBlurb: "Attachments and the Tools menu",
  helpTopicSidebarLabel: "Sidebar",
  helpTopicSidebarBlurb: "Chats list and organization",
  helpTopicHeaderLabel: "Top bar",
  helpTopicHeaderBlurb: "Icons across the header",
  helpTopicLibraryLabel: "Library & Kiwix",
  helpTopicLibraryBlurb: "Watch folder, Archive, indexing, ZIMs",
  helpTopicMeshtasticLabel: "Meshtastic",
  helpTopicMeshtasticBlurb: "USB LoRa mesh radio",
  helpTopicRadioCommsLabel: "Comms (radio)",
  helpTopicRadioCommsBlurb: "Serial radio terminal, channels",
  helpTopicSettingsLabel: "Settings",
  helpTopicSettingsBlurb: "Profile, AI, maps, mesh, themes, USB export",
  helpTopicRepliesLabel: "Replies & playback",
  helpTopicRepliesBlurb: "TTS, copy, and citations",
  helpTopicHelpAssistantLabel: "Ask Athena",
  helpTopicHelpAssistantBlurb: "AI help for using the app (beta)",

  headerBatteryAriaCharging: "Battery {{pct}}%, charging",
  headerBatteryAriaOnBattery: "Battery {{pct}}%, on battery",
  headerBatteryAriaMinsSuffix: ", about {{mins}} minutes to empty",
  headerBatteryVisibleCharging: " • Charging",
  headerBatteryVisibleMins: " · ~{{mins}}m",
  headerDownloadLead: "Downloading local data — ",

  chatSend: "Send",
  citationOpenFile: "Open Source",
  citationPageLabel: "Page",

  calendarAriaPrev: "Previous",
  calendarAriaNext: "Next",
  calendarToday: "Today",
  calendarTabMonth: "Month",
  calendarTabWeek: "Week",
  calendarTabDay: "Day",
  calendarTabAgenda: "Agenda",
  calendarSearchPlaceholder: "Search",
  calendarSearchAria: "Search calendar",
  calendarMore: "More",
  calendarQuickAddPlaceholder: 'Quick add: "2026-04-11 7pm check generator at Camp Alpha"',
  calendarParse: "Parse",
  calendarPullMesh: "Pull mesh",
  calendarExportIcs: "Export ICS",
  calendarImportIcs: "Import ICS",
  calendarAllDay: "All day",
  calendarHolidayTag: "Holiday",
  calendarUpcoming: "Upcoming",
  calendarThisDay: "This day",
  calendarNoEvents: "No events",
  calendarMapBtn: "Map",
  calendarPinBtn: "Pin",
  calendarNewEvent: "New event",
  calendarTemplateMedName: "Medication",
  calendarTemplateMedTitle: "Medication check",
  calendarTemplatePatrolName: "Patrol",
  calendarTemplatePatrolTitle: "Patrol route",
  calendarTemplateSupplyName: "Supply",
  calendarTemplateSupplyTitle: "Supply inventory",
  calendarTemplateRadioName: "Radio watch",
  calendarTemplateRadioTitle: "Radio watch",
  calendarFieldTitle: "Title",
  calendarFieldNotes: "Notes",
  calendarFieldStarts: "Starts",
  calendarFieldEnds: "Ends",
  calendarFieldLocation: "Location",
  calendarFieldMapPin: "Map pin",
  calendarPinNone: "None",
  calendarNoPinsHint:
    "No map pins yet. Open the Map to place markers (double-click), then return here to link an event.",
  calendarOpenMapPins: "Open Map to add pins",
  calendarFieldParticipants: "Participants",
  calendarParticipantsPlaceholder: "Comma separated",
  calendarFieldRepeat: "Repeat",
  calendarFieldAlert: "Alert",
  calendarRepeatNone: "None",
  calendarRepeatDaily: "Daily",
  calendarRepeatWeekly: "Weekly",
  calendarRepeatMonthly: "Monthly",
  calendarRepeatYearly: "Yearly",
  calendarAlertNone: "None",
  calendarAlert10: "10 min",
  calendarAlert30: "30 min",
  calendarAlert60: "1 hr",
  calendarAlert1440: "1 day",
  calendarAddToCalendar: "Add to calendar",
  calendarUntitledEvent: "Untitled event",
  calendarImportedEventDefault: "Imported event",
  calendarTemplatesHint: "Quick presets — tap to fill title and category, then set times.",

  logbookProfileTitle: "Logbook profile",
  logbookProfileBlurb:
    "Add your name and occupation so new notes are labeled clearly. This stays on this device.",
  logbookLabelName: "Name",
  logbookLabelOccupation: "Occupation",
  logbookSaveContinue: "Save and continue",
  logbookSidebarAria: "Logbook entries",
  logbookTitle: "Logbook",
  logbookTagline: "Notes on device · syncs over mesh",
  logbookSearchPlaceholder: "Search note text…",
  logbookSearchAria: "Search logbook body",
  logbookSortLabel: "Sort",
  logbookSortDate: "Date",
  logbookSortAuthor: "Author",
  logbookSortOccupation: "Occupation",
  logbookNewNoteTitle: "New note",
  logbookFocusNewNote: "Focus new note",
  logbookLoading: "Loading…",
  logbookNoMatching: "No matching entries.",
  logbookEmptyPreview: "(empty)",
  logbookEntryHeader: "Entry",
  logbookPullMesh: "Pull from mesh",
  logbookPullMeshTitleOff: "Connect a Meshtastic USB radio on Primary channel",
  logbookPullMeshTitleOn: "Broadcast a sync request; other Athena devices reply with recent notes",
  logbookHelpToggleTitle: "How logbook sync works on the mesh",
  logbookHelpAria: "Logbook mesh sync help",
  logbookHelpBody:
    "Saving sends new notes on the Primary channel when a Meshtastic radio is connected. Use Pull from mesh to ask other Athena devices to share recent entries. Entries use the same id everywhere, so you will not get duplicates. Long text may be truncated for one radio packet.",
  logbookUnknownAuthor: "Unknown author",
  logbookThisDevice: "This device",
  logbookMeshNodeLabel: "Mesh node",
  logbookFollowUpTag: "Follow-up",
  logbookAddFollowUp: "Add follow-up",
  logbookDeleteEntry: "Delete entry",
  logbookSelectOrWrite: "Select an entry on the left, or write a new note below.",
  logbookFollowUpHint: "Follow-up to a previous entry. Saving links this note as a reply.",
  logbookComposeAria: "Compose new log entry",
  logbookNewNoteLabel: "New note",
  logbookDraftPlaceholder: "Weather, inventory, contacts, route changes…",
  logbookProfileCompleteModal: "Complete your logbook profile above to compose.",
  logbookProfileCompleteGeneral: "Set your logbook profile to compose notes.",
  logbookMeshBudget: "One mesh packet budget (auto-send on save):",
  logbookMeshNoRadioHint: " · Connect a Meshtastic USB radio to auto-send and pull",
  logbookMeshTruncateHint: " · Longer notes are truncated for mesh when needed",
  logbookCancelFollowUp: "Cancel follow-up",
  logbookSaveNote: "Save note",
  logbookErrSaveProfile: "Could not save profile.",
  logbookErrSaveNote: "Could not save note.",
  logbookErrDelete: "Could not delete entry.",
  logbookErrMeshPull: "Could not request notes from the mesh.",
  logbookErrLoad: "Could not load logbook.",

  radioTitle: "Radio Terminal",
  radioSignalHealth: "Signal health",
  radioNoTelemetry: "No telemetry yet. Expect serial lines:",
  radioEncryptedChannelHeader: "Encrypted channel profile",
  radioProfileLabel: "Profile",
  radioNoProfilesOption: "No profiles — add one below",
  radioApplyToRadio: "Apply to radio",
  radioDeleteProfile: "Delete profile",
  radioNewProfile: "New profile",
  radioPlaceholderName: "Name",
  radioPlaceholderFreqMhz: "Frequency MHz",
  radioPlaceholderEncryptionKey: "Encryption key",
  radioAriaChannelName: "Channel name",
  radioAriaFreqMhz: "Frequency MHz",
  radioAriaEncryptionKey: "Encryption key",
  radioSaveProfile: "Save profile",
  radioSdrTitle: "SDR scanner (rtl_fm)",
  radioSdrBlurb:
    "Requires rtl_fm on PATH. Raw PCM is streamed over an ephemeral loopback port; playback uses Web Audio with a scheduled buffer queue.",
  radioDongle: "Dongle",
  radioRefreshDevices: "Refresh devices",
  radioMhzLabel: "MHz",
  radioModeLabel: "Mode",
  radioModeNfm: "NFM (fm)",
  radioModeWbfm: "WBFM",
  radioModeAm: "AM",
  radioStart: "Start",
  radioStop: "Stop",
  radioAriaSdrSpectrum: "SDR spectrum",
  radioSdrLogPlaceholder: "rtl_fm stderr will appear here.",
  radioAriaSerialPorts: "Available serial ports",
  radioNoSerialPorts: "No serial ports found",
  radioRefresh: "Refresh",
  radioConnect: "Connect",
  radioConnected: "Connected",
  radioOutboundPackets: "Outbound packets",
  radioNoTrackedPackets: "No tracked packets yet.",
  radioLanClusterTitle: "LAN AI cluster peers",
  radioNoLanPeers: "No other Athena nodes on LAN yet (same model required for pooled inference).",
  radioTerminalOutputAria: "Radio terminal output",
  radioTerminalWaiting: "(waiting for radio data)",
  radioTxPlaceholder: "Type a radio message…",
  radioTxAria: "Radio transmit message",
  radioSend: "Send",
  radioConfirmDeleteChannel: "Delete this channel profile from the database?",
  radioErrValidFreqMhz: "Enter a valid frequency in MHz.",
  radioErrSdrStart: "SDR start failed.",
  radioErrLoadPorts: "Could not load serial ports.",
  radioErrConnectionFailed: "Connection failed.",
  radioErrSendFailed: "Send failed.",
  radioErrApplyChannel: "Could not apply channel profile.",
  radioErrChannelFields: "Channel name, frequency (MHz), and encryption key are required.",
  radioErrSaveChannel: "Could not save channel.",
  radioErrDeleteChannel: "Could not delete channel.",
  radioAriaRtlDevice: "RTL-SDR device index",
  radioAriaFreqMegahertz: "Frequency megahertz",
  radioAriaDemodMode: "Demodulation mode",
};

const FALLOUT_LEXICON: Lexicon = {
  ...SURVIVAL_LEXICON,
  navArchiveView: "The Archive",
  archiveScreenTitle: "The Workbench",
  archiveScreenSubtitle:
    "Vault-Tec sanctioned offline archive — stash your holotapes, manuals, and scavenged packs.",
  tabLocalArchive: "Vault Storage",
  tabDownloadNetwork: "Wasteland Network",
  archiveHubIntro:
    "Vault Storage: same category tabs as the network (All, Packages, Documents, Maps, Academy) plus search and sort. Wasteland Network: Packages (bundles, Academy drops by clearance, single pulls), Documents, Maps.",
  archivePrimaryTablistAria: "Archive: vault storage or wasteland network",
  archiveNetworkTablistAria: "Wasteland network sections",
  tabArchivePackages: "Packages",
  tabArchiveDocuments: "Documents",
  tabArchiveMaps: "Maps",
  tabArchiveAll: "All",
  tabArchiveAcademy: "Academy",
  archiveLocalTablistAria: "Vault inventory categories",
  archiveSortLabel: "Sort",
  archiveSortName: "Name",
  archiveSortSize: "Size",
  archiveSortPath: "Path",
  archiveSortDirAsc: "Ascending",
  archiveSortDirDesc: "Descending",
  archiveSearchFilesPlaceholder: "Search holotape name or path…",
  sectionAcademyCurriculum: "Academy holotapes",
  sectionAcademyCurriculumBody:
    "Kiosk ZIMs by clearance band and course track. Deliveries route to library/academy/ like other sanctioned packs.",
  academyOtherDownloads: "Unfiled academy payloads",
  archiveLocalNoMatches: "No holotapes match this filter.",
  archiveLocalTitle: "Vault inventory",
  archiveLocalSubtitle: "Registered tower paths and indexed files — filter like the Wasteland Network tabs, with search and sort.",
  regionalMapsNotSet: "Not plotted",
  sectionTierPackages: "The Workbench",
  sectionAlacarte: "The Stash",
  dataSources: "Radio Towers",
  openInFinder: "Show in Finder",
  actionDeployTier: "Holotape",
  actionRequisitionFile: "Scavenge",
  cancelDownload: "Cancel download",
  emptyLocalArchive:
    "No files detected in your vault paths. Add a Radio Tower path or pull packs from the Wasteland Network.",
  noDataSourcesHint:
    "Register at least one vault path so deliveries land somewhere safe. Mounted network drives count if macOS shows them as a folder path.",
  addDataSource: "Add vault path",
  removeDataSource: "Strike entry",
  pathPlaceholder: "Paste path (for example /Volumes/Share/Vault)",
  closeModal: "Close panel",
  regionalMaps: "Regional maps",
  setDownloadArea: "Set your surface coordinates",
  setDownloadAreaBody:
    "The Satchel still pulls US state Protomaps from this pin; other tiers behave the same as before.",
  locationFieldLabel: "US city, address, or ZIP",
  lookUpAndSave: "Query atlas and save",
  useNycDefault: "Default to New York (offline)",
  notNow: "Skip",
  setLocation: "Set coordinates",
  changeArea: "Replot",
  noAreaWarning:
    "No surface pin saved — The Satchel only queues Wikipedia until you pick a US spot for state maps.",
  destinationSummary: "Primary vault path",
  watchFoldersLabel: "Radio tower paths",
  fileMissingOnDisk: "File missing (ghost index)",
  installingLabel: "Encoding",
  tierFilesCount: "holotapes",
  sizeUnknown: "size unknown",
  catalogDuplicateNote: "Same asset may appear in The Stash for single pulls.",

  navHome: "Home",
  navChat: "Comms",
  navMap: "Surface map",
  navArchive: "Archive",
  navMeshTitle: "Meshtastic (vault uplink)",
  navMeshAria: "Meshtastic mesh uplink",
  navRadio: "Radio",
  navLogbook: "Field log",
  navCalendar: "Schedule",
  navOpenCalendar: "Open schedule",
  navOpenCalendarAria: "Open schedule and chronometer settings",
  navSettings: "Vault settings",
  navHelpTitle: "Pip-Boy manual",
  navHelpAria: "Help and operating procedures",

  actionSave: "Commit",
  actionCancel: "Abort",
  actionDelete: "Purge",
  stateSaving: "Archiving…",
  stateApplying: "Tuning…",
  stateStarting: "Spinning up…",
  stateSending: "Transmitting…",
  stateRequesting: "Hailing…",
  actionConnect: "Link",
  actionConnected: "Linked",
  actionConnecting: "Linking…",

  mapDisabledTitle: "Surface cartography offline",
  mapDisabledBodyBefore: "Enable surface maps under ",
  mapDisabledLocationWord: "Location",
  mapDisabledBodyAfter: " in Vault settings to open the tactical viewer.",
  mapDisabledOpenLocation: "Open Location panel",
  mapDisabledBackHome: "Return to Home",

  settingsBackLabel: "← Back",
  settingsBackAria: "Back to vault settings menu",
  settingsCloseAria: "Close vault settings",
  settingsMenuTitle: "Vault settings",
  settingsMenuTitleAria: "Vault settings",
  settingsPanelGeneral: "General",
  settingsPanelAi: "AI & Models",
  settingsPanelLocation: "Location",
  settingsPanelApiKeys: "API keys",
  settingsPanelMaps: "Offline maps",
  settingsPanelMesh: "Mesh uplink",
  settingsPanelVoice: "Voice",
  settingsPanelCalendar: "Schedule",
  settingsPanelDateTime: "Date & Time",
  settingsPanelAppearance: "Window & Display",
  settingsPanelThemes: "Themes & Colors",
  settingsPanelThemeStudio: "Theme Studio",
  settingsPanelTroubleshooting: "Diagnostics",
  settingsPanelUsbExport: "Holotape export",
  settingsPanelDeveloper: "Overseer",
  settingsNavAria: "Vault settings sections",
  settingsNavIntroLead:
    "Pick a section. New here? General explains ATHENA and your stash path; AI & Archives powers chat. Holotapes & diagnostics covers USB transfer and the error log.",
  settingsNavMeshConnectedNote: " The mesh uplink icon appears when a Meshtastic device is tethered.",
  settingsNavMeshDisconnectedNote:
    " No Meshtastic uplink detected—mesh options stay locked until hardware is connected.",
  settingsNavGroupProfileCalendar: "Dweller & Schedule",
  settingsNavGroupAiKnowledge: "AI & Archives",
  settingsNavGroupMapsRadio: "Maps, Radio & Gear",
  settingsNavGroupLookFeel: "Look & feel",
  settingsNavGroupBackupHelp: "Holotapes & diagnostics",
  settingsNavGeneral: "General",
  settingsNavGeneralDesc: "About ATHENA, dweller profile, startup view, help, maintenance",
  settingsNavCalendar: "Schedule",
  settingsNavCalendarDesc: "Holiday region, reminders, templates, ICS",
  settingsNavDateTime: "Date & Time",
  settingsNavDateTimeDesc: "12/24-hour chronometer and quiet hours",
  settingsNavAiModels: "AI & Models",
  settingsNavAiModelsDesc: "Ollama, personality, model size, memory, citations",
  settingsNavApiKeys: "API keys",
  settingsNavApiKeysDesc: "Store vendor keys for advanced uplinks",
  settingsNavLocation: "Location",
  settingsNavLocationDesc: "AI place context and Kiwix installer (not The Archive pin)",
  settingsNavOfflineMaps: "Offline maps",
  settingsNavOfflineMapsDesc: "Tactical basemaps, Kiwix atlas ZIMs, imports (state tiles: The Archive)",
  settingsNavOfflineMapsNoteMapOn: " · Use the Surface map icon when maps are enabled",
  settingsNavOfflineMapsNoteMapOff: " · Enable maps under Location to show the map control",
  settingsNavMesh: "Mesh uplink",
  settingsNavMeshChecking: "Scanning for USB radio…",
  settingsNavMeshNone: "No Meshtastic uplink",
  settingsNavMeshOk: "Meshtastic auto-reply and mesh traffic",
  settingsNavVoice: "Voice",
  settingsNavVoiceDesc: "Read-aloud (TTS)",
  settingsNavVoiceHiddenNote: " · Classified in this build",
  settingsNavWindowDisplay: "Window & Display",
  settingsNavWindowDisplayDesc: "Fullscreen and night-ops tint",
  settingsNavThemes: "Themes & Colors",
  settingsNavThemesDesc: "Experience style, palette, custom themes",
  settingsNavTroubleshooting: "Diagnostics",
  settingsNavTroubleshootingDesc: "Error log and deep probes",
  settingsNavUsbExport: "Holotape export",
  settingsNavUsbExportDesc: "Copy the app and vault data to removable storage",

  helpModalTitle: "Manual & features",
  helpCloseAria: "Close manual",
  helpSearchLabel: "Search topics",
  helpSearchPlaceholder: "Filter topics (map, mesh, archive)…",
  helpNavIntroMatching: "Matching topics:",
  helpNavIntroChoose: "Choose a topic:",
  helpEmptyNav: "No topics match. Clear the search or try Ask Athena.",
  helpEmptyDetail:
    "No topics match this filter. Clear the search above, or open Ask Athena from the sidebar when it returns.",
  helpNavSectionsAria: "Manual sections",
  helpTopicOverviewLabel: "Overview",
  helpTopicOverviewBlurb: "What Athena is and how the vault works",
  helpTopicHomeLabel: "Home",
  helpTopicHomeBlurb: "Dashboard, vitals, quick launch",
  helpTopicTacticalMapLabel: "Tactical map",
  helpTopicTacticalMapBlurb: "Pins, layers, mesh, right-click",
  helpTopicMapsOfflineLabel: "Maps & offline data",
  helpTopicMapsOfflineBlurb: "Basemaps vs Library tiers",
  helpTopicChatLabel: "Chat & composer",
  helpTopicChatBlurb: "Messages and shortcuts",
  helpTopicToolsLabel: "Tools",
  helpTopicToolsBlurb: "Attachments and Tools menu",
  helpTopicSidebarLabel: "Sidebar",
  helpTopicSidebarBlurb: "Chats list and organization",
  helpTopicHeaderLabel: "Top bar",
  helpTopicHeaderBlurb: "Icons across the header",
  helpTopicLibraryLabel: "Library & Kiwix",
  helpTopicLibraryBlurb: "Watch folder, Archive, indexing, ZIMs",
  helpTopicMeshtasticLabel: "Meshtastic",
  helpTopicMeshtasticBlurb: "USB LoRa mesh uplink",
  helpTopicRadioCommsLabel: "Comms (radio)",
  helpTopicRadioCommsBlurb: "Serial terminal and channel profiles",
  helpTopicSettingsLabel: "Settings",
  helpTopicSettingsBlurb: "Profile, AI, maps, mesh, themes, USB export",
  helpTopicRepliesLabel: "Replies & playback",
  helpTopicRepliesBlurb: "TTS, copy, citations",
  helpTopicHelpAssistantLabel: "Ask Athena",
  helpTopicHelpAssistantBlurb: "AI help for operating the app (beta)",

  headerBatteryAriaCharging: "Cell {{pct}}%, charging",
  headerBatteryAriaOnBattery: "Cell {{pct}}%, on battery",
  headerBatteryAriaMinsSuffix: ", roughly {{mins}} minutes remaining",
  headerBatteryVisibleCharging: " · Charging",
  headerBatteryVisibleMins: " · ~{{mins}}m",
  headerDownloadLead: "Pulling local data — ",

  chatSend: "Transmit",
  citationOpenFile: "Access Record",
  citationPageLabel: "Log",

  calendarAriaPrev: "Earlier",
  calendarAriaNext: "Later",
  calendarToday: "Today",
  calendarTabMonth: "Month",
  calendarTabWeek: "Week",
  calendarTabDay: "Day",
  calendarTabAgenda: "Agenda",
  calendarSearchPlaceholder: "Search",
  calendarSearchAria: "Search schedule",
  calendarMore: "More",
  calendarQuickAddPlaceholder: 'Quick add: "2287-04-11 7pm reactor check at Vault door"',
  calendarParse: "Decode",
  calendarPullMesh: "Pull mesh",
  calendarExportIcs: "Export ICS",
  calendarImportIcs: "Import ICS",
  calendarAllDay: "All day",
  calendarHolidayTag: "Holiday",
  calendarUpcoming: "Upcoming",
  calendarThisDay: "This day",
  calendarNoEvents: "No events",
  calendarMapBtn: "Map",
  calendarPinBtn: "Pin",
  calendarNewEvent: "New event",
  calendarTemplateMedName: "Stims check",
  calendarTemplateMedTitle: "Med-X inventory",
  calendarTemplatePatrolName: "Patrol",
  calendarTemplatePatrolTitle: "Perimeter sweep",
  calendarTemplateSupplyName: "Supplies",
  calendarTemplateSupplyTitle: "Stock tally",
  calendarTemplateRadioName: "Radio watch",
  calendarTemplateRadioTitle: "Comms watch",
  calendarFieldTitle: "Title",
  calendarFieldNotes: "Notes",
  calendarFieldStarts: "Starts",
  calendarFieldEnds: "Ends",
  calendarFieldLocation: "Location",
  calendarFieldMapPin: "Map pin",
  calendarPinNone: "None",
  calendarNoPinsHint:
    "No surface pins yet. Open the map to drop markers (double-click), then link them here.",
  calendarOpenMapPins: "Open map to add pins",
  calendarFieldParticipants: "Participants",
  calendarParticipantsPlaceholder: "Comma separated",
  calendarFieldRepeat: "Repeat",
  calendarFieldAlert: "Alert",
  calendarRepeatNone: "None",
  calendarRepeatDaily: "Daily",
  calendarRepeatWeekly: "Weekly",
  calendarRepeatMonthly: "Monthly",
  calendarRepeatYearly: "Yearly",
  calendarAlertNone: "None",
  calendarAlert10: "10 min",
  calendarAlert30: "30 min",
  calendarAlert60: "1 hr",
  calendarAlert1440: "1 day",
  calendarAddToCalendar: "Commit to schedule",
  calendarUntitledEvent: "Unnamed event",
  calendarImportedEventDefault: "Imported event",
  calendarTemplatesHint: "Quick presets — tap to fill title and category, then set times.",

  logbookProfileTitle: "Field log profile",
  logbookProfileBlurb:
    "Register your name and role so entries show clear attribution. Data stays on this terminal.",
  logbookLabelName: "Name",
  logbookLabelOccupation: "Occupation",
  logbookSaveContinue: "Save and continue",
  logbookSidebarAria: "Field log entries",
  logbookTitle: "Field log",
  logbookTagline: "Local notes · mesh relay",
  logbookSearchPlaceholder: "Search log text…",
  logbookSearchAria: "Search log body",
  logbookSortLabel: "Sort",
  logbookSortDate: "Date",
  logbookSortAuthor: "Author",
  logbookSortOccupation: "Occupation",
  logbookNewNoteTitle: "New entry",
  logbookFocusNewNote: "Focus new entry",
  logbookLoading: "Loading…",
  logbookNoMatching: "No matching entries.",
  logbookEmptyPreview: "(empty)",
  logbookEntryHeader: "Entry",
  logbookPullMesh: "Pull from mesh",
  logbookPullMeshTitleOff: "Connect a Meshtastic USB radio on Primary channel",
  logbookPullMeshTitleOn: "Ping the mesh; other Athena nodes may push recent entries",
  logbookHelpToggleTitle: "How field log sync works",
  logbookHelpAria: "Field log mesh help",
  logbookHelpBody:
    "Saving pushes new entries on the Primary channel when Meshtastic is connected. Pull from mesh requests copies from other Athena devices. IDs match everywhere to avoid duplicates. Long text may be clipped for one packet.",
  logbookUnknownAuthor: "Unknown author",
  logbookThisDevice: "This terminal",
  logbookMeshNodeLabel: "Mesh node",
  logbookFollowUpTag: "Follow-up",
  logbookAddFollowUp: "Add follow-up",
  logbookDeleteEntry: "Delete entry",
  logbookSelectOrWrite: "Pick an entry on the left, or draft a new one below.",
  logbookFollowUpHint: "Follow-up to a prior entry. Saving links this as a reply.",
  logbookComposeAria: "Compose log entry",
  logbookNewNoteLabel: "New entry",
  logbookDraftPlaceholder: "Weather, stock, contacts, route changes…",
  logbookProfileCompleteModal: "Finish your field log profile above to compose.",
  logbookProfileCompleteGeneral: "Set your field log profile to compose entries.",
  logbookMeshBudget: "One mesh packet budget (auto-send on save):",
  logbookMeshNoRadioHint: " · Link a Meshtastic USB radio to auto-send and pull",
  logbookMeshTruncateHint: " · Long entries may be truncated for mesh",
  logbookCancelFollowUp: "Cancel follow-up",
  logbookSaveNote: "Save entry",
  logbookErrSaveProfile: "Could not save profile.",
  logbookErrSaveNote: "Could not save entry.",
  logbookErrDelete: "Could not delete entry.",
  logbookErrMeshPull: "Could not request notes from the mesh.",
  logbookErrLoad: "Could not load logbook.",

  radioTitle: "Radio terminal",
  radioSignalHealth: "Signal health",
  radioNoTelemetry: "No telemetry yet. Expect serial lines:",
  radioEncryptedChannelHeader: "Encrypted channel profile",
  radioProfileLabel: "Profile",
  radioNoProfilesOption: "No profiles — add one below",
  radioApplyToRadio: "Apply to radio",
  radioDeleteProfile: "Delete profile",
  radioNewProfile: "New profile",
  radioPlaceholderName: "Name",
  radioPlaceholderFreqMhz: "Frequency MHz",
  radioPlaceholderEncryptionKey: "Encryption key",
  radioAriaChannelName: "Channel name",
  radioAriaFreqMhz: "Frequency MHz",
  radioAriaEncryptionKey: "Encryption key",
  radioSaveProfile: "Save profile",
  radioSdrTitle: "SDR scanner (rtl_fm)",
  radioSdrBlurb:
    "Requires rtl_fm on PATH. Raw PCM streams over a loopback port; Web Audio plays a buffered queue.",
  radioDongle: "Dongle",
  radioRefreshDevices: "Refresh devices",
  radioMhzLabel: "MHz",
  radioModeLabel: "Mode",
  radioModeNfm: "NFM (fm)",
  radioModeWbfm: "WBFM",
  radioModeAm: "AM",
  radioStart: "Start",
  radioStop: "Stop",
  radioAriaSdrSpectrum: "SDR spectrum",
  radioSdrLogPlaceholder: "rtl_fm stderr will appear here.",
  radioAriaSerialPorts: "Available serial ports",
  radioNoSerialPorts: "No serial ports found",
  radioRefresh: "Refresh",
  radioConnect: "Link",
  radioConnected: "Linked",
  radioOutboundPackets: "Outbound packets",
  radioNoTrackedPackets: "No tracked packets yet.",
  radioLanClusterTitle: "LAN AI cluster peers",
  radioNoLanPeers: "No other Athena nodes on LAN yet (same model required for pooled inference).",
  radioTerminalOutputAria: "Radio terminal output",
  radioTerminalWaiting: "(waiting for radio data)",
  radioTxPlaceholder: "Type a radio message…",
  radioTxAria: "Radio transmit message",
  radioSend: "Transmit",
  radioConfirmDeleteChannel: "Delete this channel profile from the database?",
  radioErrValidFreqMhz: "Enter a valid frequency in MHz.",
  radioErrSdrStart: "SDR start failed.",
  radioErrLoadPorts: "Could not load serial ports.",
  radioErrConnectionFailed: "Connection failed.",
  radioErrSendFailed: "Send failed.",
  radioErrApplyChannel: "Could not apply channel profile.",
  radioErrChannelFields: "Channel name, frequency (MHz), and encryption key are required.",
  radioErrSaveChannel: "Could not save channel.",
  radioErrDeleteChannel: "Could not delete channel.",
  radioAriaRtlDevice: "RTL-SDR device index",
  radioAriaFreqMegahertz: "Frequency megahertz",
  radioAriaDemodMode: "Demodulation mode",
};

export function getLexicon(themeId: LexiconThemeId): Lexicon {
  return themeId === "fallout" ? FALLOUT_LEXICON : SURVIVAL_LEXICON;
}

const LexiconContext = createContext<Lexicon>(SURVIVAL_LEXICON);

export function LexiconProvider({
  themeId,
  children,
}: {
  themeId: LexiconThemeId;
  children: ReactNode;
}) {
  const value = useMemo(() => getLexicon(themeId), [themeId]);
  return <LexiconContext.Provider value={value}>{children}</LexiconContext.Provider>;
}

export function useLexicon(): Lexicon {
  return useContext(LexiconContext);
}
