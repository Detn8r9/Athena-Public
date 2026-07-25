import { useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { ATHENA_ACRONYM_PARTS, ATHENA_FULL_NAME } from "../../shared/branding";
import { useLexicon } from "../../shared/lexicon";

export type HelpSectionId =
  | "overview"
  | "login_security"
  | "home"
  | "calendar"
  | "tactical_map"
  | "maps_offline"
  | "chat"
  | "tools"
  | "sidebar"
  | "header"
  | "library"
  | "meshtastic"
  | "radio_comms"
  | "settings"
  | "personnel_hub"
  | "replies"
  | "help_assistant";

type SectionDef = {
  id: HelpSectionId;
  label: string;
  blurb: string;
  /** Extra words for search (lowercase phrases). */
  keywords?: string[];
};

function sectionMatches(query: string, s: SectionDef): boolean {
  const t = query.trim().toLowerCase();
  if (!t) return true;
  const hay = [s.label, s.blurb, s.id, ...(s.keywords ?? [])].join(" ").toLowerCase();
  return hay.includes(t);
}

interface HelpModalProps {
  open: boolean;
  onClose: () => void;
}

export function HelpModal({ open, onClose }: HelpModalProps) {
  const lx = useLexicon();
  const [active, setActive] = useState<HelpSectionId>("overview");
  const [search, setSearch] = useState("");

  const sections = useMemo<SectionDef[]>(
    () => [
      {
        id: "overview",
        label: lx.helpTopicOverviewLabel,
        blurb: lx.helpTopicOverviewBlurb,
        keywords: ["ollama", "local", "offline"],
      },
      {
        id: "login_security",
        label: lx.helpTopicLoginSecurityLabel,
        blurb: lx.helpTopicLoginSecurityBlurb,
        keywords: ["pin", "password", "lock", "login", "operator", "rank", "audit", "personnel"],
      },
      {
        id: "home",
        label: lx.helpTopicHomeLabel,
        blurb: lx.helpTopicHomeBlurb,
        keywords: ["dashboard", "launcher", "system", "academy", "knowledge catalog", "pdf"],
      },
      {
        id: "calendar",
        label: lx.helpTopicCalendarHelpLabel,
        blurb: lx.helpTopicCalendarHelpBlurb,
        keywords: ["schedule", "event", "ics", "reminder", "holiday", "mesh"],
      },
      {
        id: "tactical_map",
        label: lx.helpTopicTacticalMapLabel,
        blurb: lx.helpTopicTacticalMapBlurb,
        keywords: ["map", "pin", "tactical", "poi", "mesh", "maplibre"],
      },
      {
        id: "maps_offline",
        label: lx.helpTopicMapsOfflineLabel,
        blurb: lx.helpTopicMapsOfflineBlurb,
        keywords: ["kiwix", "zim", "mbtiles", "pmtiles", "download", "basemap", "archive", "installer", "state maps"],
      },
      {
        id: "chat",
        label: lx.helpTopicChatLabel,
        blurb: lx.helpTopicChatBlurb,
        keywords: ["message", "keyboard"],
      },
      {
        id: "tools",
        label: lx.helpTopicToolsLabel,
        blurb: lx.helpTopicToolsBlurb,
        keywords: ["attach", "file", "image"],
      },
      {
        id: "sidebar",
        label: lx.helpTopicSidebarLabel,
        blurb: lx.helpTopicSidebarBlurb,
        keywords: ["chat list", "pin"],
      },
      {
        id: "header",
        label: lx.helpTopicHeaderLabel,
        blurb: lx.helpTopicHeaderBlurb,
        keywords: ["export", "pdf", "triage"],
      },
      {
        id: "library",
        label: lx.helpTopicLibraryLabel,
        blurb: lx.helpTopicLibraryBlurb,
        keywords: ["index", "rag", "zim", "installer", "watch folder", "satchel", "protomaps", "pmtiles"],
      },
      {
        id: "meshtastic",
        label: lx.helpTopicMeshtasticLabel,
        blurb: lx.helpTopicMeshtasticBlurb,
        keywords: ["mesh", "lora", "usb"],
      },
      {
        id: "radio_comms",
        label: lx.helpTopicRadioCommsLabel,
        blurb: lx.helpTopicRadioCommsBlurb,
        keywords: ["radio", "serial", "comms", "mesh", "packet"],
      },
      {
        id: "settings",
        label: lx.helpTopicSettingsLabel,
        blurb: lx.helpTopicSettingsBlurb,
        keywords: [
          "preferences",
          "theme",
          "theme studio",
          "watch folder",
          "location",
          "api key",
          "aegis",
          "usb",
          "export",
          "backup",
          "scorched earth",
          "format drive",
          "quiet hours",
          "date time",
        ],
      },
      {
        id: "personnel_hub",
        label: lx.helpTopicPersonnelHubLabel,
        blurb: lx.helpTopicPersonnelHubBlurb,
        keywords: ["roster", "quartermaster", "logistics", "inventory", "division", "admin"],
      },
      {
        id: "replies",
        label: lx.helpTopicRepliesLabel,
        blurb: lx.helpTopicRepliesBlurb,
        keywords: ["speech", "copy"],
      },
      {
        id: "help_assistant",
        label: lx.helpTopicHelpAssistantLabel,
        blurb: lx.helpTopicHelpAssistantBlurb,
        keywords: ["ai", "question", "search", "how do i"],
      },
    ],
    [lx],
  );

  const visibleSections = useMemo(() => sections.filter((s) => sectionMatches(search, s)), [sections, search]);

  useEffect(() => {
    if (open) {
      setActive("overview");
      setSearch("");
    }
  }, [open]);

  useEffect(() => {
    if (visibleSections.length === 0) return;
    if (!visibleSections.some((s) => s.id === active)) {
      setActive(visibleSections[0].id);
    }
  }, [visibleSections, active]);

  if (!open) return null;

  return (
    <div className="modal-backdrop help-modal-backdrop" onClick={onClose}>
      <div className="library-modal help-modal" onClick={(e) => e.stopPropagation()}>
        <div className="library-header">
          <h2 id="help-modal-title">{lx.helpModalTitle}</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label={lx.helpCloseAria}>
            ×
          </button>
        </div>

        <div className="help-modal-search-wrap">
          <label htmlFor="help-modal-search" className="help-modal-search-label">
            {lx.helpSearchLabel}
          </label>
          <input
            id="help-modal-search"
            type="search"
            className="help-modal-search-input map-chat-input"
            placeholder={lx.helpSearchPlaceholder}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            autoComplete="off"
          />
        </div>

        <div className="help-modal-shell">
          <nav className="help-modal-nav" aria-labelledby="help-modal-title">
            <p className="help-modal-nav-intro">{search.trim() ? lx.helpNavIntroMatching : lx.helpNavIntroChoose}</p>
            {visibleSections.length === 0 ? (
              <p className="text-muted small help-modal-no-match">{lx.helpEmptyNav}</p>
            ) : (
              <ul className="help-modal-nav-list" role="tablist" aria-label={lx.helpNavSectionsAria}>
                {visibleSections.map((s) => (
                  <li key={s.id} role="none">
                    <button
                      type="button"
                      role="tab"
                      id={`help-tab-${s.id}`}
                      aria-selected={active === s.id}
                      aria-controls={`help-panel-${s.id}`}
                      className={`help-modal-nav-btn${active === s.id ? " is-active" : ""}`}
                      onClick={() => setActive(s.id)}
                    >
                      <span className="help-modal-nav-btn-label">{s.label}</span>
                      <span className="help-modal-nav-btn-blurb">{s.blurb}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </nav>

          <div
            className="help-modal-detail"
            role="tabpanel"
            id={`help-panel-${active}`}
            aria-labelledby={`help-tab-${active}`}
          >
            {visibleSections.length === 0 ? (
              <p className="help-modal-empty-detail text-muted">{lx.helpEmptyDetail}</p>
            ) : (
              <>
                {active === "overview" && <OverviewPanel />}
                {active === "login_security" && <LoginSecurityPanel />}
                {active === "home" && <HomePanel />}
                {active === "calendar" && <CalendarHelpPanel />}
                {active === "tactical_map" && <TacticalMapPanel />}
                {active === "maps_offline" && <MapsOfflinePanel />}
                {active === "chat" && <ChatPanel />}
                {active === "tools" && <ToolsPanel />}
                {active === "sidebar" && <SidebarPanel />}
                {active === "header" && <HeaderPanel />}
                {active === "library" && <LibraryPanel />}
                {active === "meshtastic" && <MeshtasticPanel />}
                {active === "radio_comms" && <RadioCommsPanel />}
                {active === "settings" && <SettingsPanel />}
                {active === "personnel_hub" && <PersonnelHubPanel />}
                {active === "replies" && <RepliesPanel />}
                {active === "help_assistant" && <AskAthenaPanel />}
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function AthenaAcronymCheatsheet() {
  return (
    <ul className="settings-tips-list text-muted" style={{ marginTop: "0.35rem" }}>
      {ATHENA_ACRONYM_PARTS.map((row, i) => (
        <li key={`${row.word}-${i}`}>
          <strong>{row.letter}</strong> — <strong>{row.word}:</strong> {row.meaning}
        </li>
      ))}
    </ul>
  );
}

function LoginSecurityPanel() {
  return (
    <>
      <h3 className="help-detail-title">Login &amp; access</h3>
      <p>
        The app opens on a <strong>login</strong> screen. Enter your <strong>operator name</strong> and <strong>PIN</strong> exactly as
        stored in the local personnel roster. PIN verification runs on this device only.
      </p>
      <h4>Rank levels (summary)</h4>
      <ul>
        <li>
          Lower ranks (typically levels 1–3) use the app day-to-day; saved <strong>offline chat</strong> history is scoped to their own
          account.
        </li>
        <li>
          Higher ranks (typically 4–5) can open an <strong>Offline chat history</strong> selector in the chat sidebar to review another
          operator&apos;s saved local threads (read-only while viewing someone else). Sending, new chat, delete, and saves apply only
          to your own history.
        </li>
      </ul>
      <h4>Lock screen tips</h4>
      <ul>
        <li>
          The <strong>moon / sun</strong> control toggles <strong>local AI sleep</strong> so the main model stays idle; mesh-related AI
          paths may still run depending on your settings.
        </li>
        <li>
          The small <strong>CPU</strong> graph and system lines are a live snapshot only—they are not uploaded anywhere.
        </li>
      </ul>
    </>
  );
}

function CalendarHelpPanel() {
  return (
    <>
      <h3 className="help-detail-title">Calendar</h3>
      <p>
        Open <strong>Calendar</strong> from the navigation strip. You get month, week, day, and agenda views, quick templates for common
        field tasks, and optional <strong>holiday</strong> overlays based on your region in Settings.
      </p>
      <ul>
        <li>
          <strong>ICS</strong> — Import or export standard <code>.ics</code> files for interoperability with other calendar apps.
        </li>
        <li>
          <strong>Mesh</strong> — When your build exposes it, you can pull or share schedule-related payloads over the mesh stack;
          treat mesh as best-effort and verify critical times locally.
        </li>
        <li>
          <strong>Map pins</strong> — Events can reference tactical map POIs where that integration is enabled.
        </li>
      </ul>
    </>
  );
}

function PersonnelHubPanel() {
  return (
    <>
      <h3 className="help-detail-title">Personnel &amp; supply</h3>
      <p>
        <strong>Personnel</strong> (from Home or the dashboard) is where administrators manage the roster: divisions, display names,
        rank levels, and PINs. Access is restricted to elevated ranks.
      </p>
      <ul>
        <li>
          <strong>Logistics Hub</strong> — Inventory and supply workflows for tracked items and logs; open it from the header or dashboard when your rank allows.
        </li>
        <li>
          Changes apply to this installation&apos;s database; back up before bulk edits (see Settings → USB export / Aegis Protocol).
        </li>
      </ul>
    </>
  );
}

function OverviewPanel() {
  return (
    <>
      <h3 className="help-detail-title">Overview</h3>
      <p>
        <strong>{ATHENA_FULL_NAME}</strong> — in the UI the product is usually called <strong>Athena</strong> (the assistant). It is
        a local, offline-first survival and repair assistant: a large language model on your machine through <strong>Ollama</strong>,
        grounded in documents in your <strong>Library</strong> (a folder you choose as the watch folder). Inference stays on your device;
        only optional features (geocoding The Archive download-area pin, map tile APIs, etc.) use the network when you trigger them.
      </p>
      <h4>What ATHENA stands for</h4>
      <AthenaAcronymCheatsheet />
      <h4>Portable install (USB)</h4>
      <p>
        If you place a folder named <code>data</code> next to the ATHENA app (for example on a USB stick), the app can run in{" "}
        <strong>portable mode</strong> and keep settings and indexes on that drive. Use{" "}
        <strong>Settings → Backup &amp; diagnostics → USB export</strong> to open <strong>Aegis Protocol</strong>: choose an export tier and
        destination drive; optional whole-disk formatting is available on macOS from that dialog.
      </p>
      <h4>Login</h4>
      <p>
        Each session starts at the <strong>login</strong> screen (operator name + PIN). See <strong>Login &amp; access</strong> in this help
        for ranks, chat history audit rules, and lock-screen controls.
      </p>
      <h4>What you need</h4>
      <ul>
        <li>
          <strong>Ollama</strong> running on this machine, with at least one model pulled (e.g.{" "}
          <code>ollama run llama3.2:3b</code>). If nothing is listening on port 11434, Athena can download the official Ollama
          CLI into its app data folder and start <code>ollama serve</code> for you. If <code>ollama</code> is only on your disk in
          an unusual place, the app may offer to <strong>copy</strong> it into that managed folder (your original stays put).
          When Ollama isn&apos;t running, a banner appears at the top of the app.
        </li>
        <li>
          A <strong>watch folder</strong> (Library): set it from the header <strong>Library</strong> sheet or related Settings notes.
          PDFs, text, and Kiwix <code>.zim</code> files there are indexed so answers can cite your material.
        </li>
        <li>
          <strong>The Archive</strong> (Home → The Archive): downloads bundled <em>tiers</em> (e.g. Wikipedia Simple English
          plus US state <strong>Protomaps</strong> extracts) into that same watch folder. Filter <strong>Local Archive</strong> and the
          download network by Packages, Documents, Maps, and <strong>Academy</strong> (curriculum ZIMs and related payloads land under{" "}
          <code>library/academy/</code> when you install them). The first time you open it, set a <strong>US download-area pin</strong>{" "}
          (default suggestion New York City, or geocode your town). That pin picks your home state map and can add neighboring states when
          you are near a border—separate from the chat <strong>Location</strong> field in Settings.
        </li>
      </ul>
      <h4>How answers are built</h4>
      <p>
        When you ask a question, Athena searches your indexed library for relevant passages, then the model generates a reply
        using that context. Follow-up questions in the same chat reuse conversation history (see <strong>Model &amp; Memory</strong>{" "}
        in Settings) so pronouns like &quot;it&quot; or &quot;they&quot; still make sense.
      </p>
      <h4>Asking about the app itself: <code>/athena</code></h4>
      <p>
        Start a message with <strong>
          <code>/athena</code>
        </strong>{" "}
        (then your question) so the model uses the built-in <strong>ATHENA project reference</strong> instead of relying on
        random library documents—best for &quot;what can Athena do?&quot;, settings locations, Meshtastic behavior, etc. If you
        send only <code>/athena</code>, you get a short overview of capabilities.
      </p>
    </>
  );
}

function ChatPanel() {
  return (
    <>
      <h3 className="help-detail-title">Chat &amp; composer</h3>
      <p>
        The main area shows your messages and Athena&apos;s replies. Use the <strong>Offline chat</strong> / <strong>LoRa mesh</strong>{" "}
        tabs to switch between local LLM threads and mesh-integrated chat when both are available.
      </p>
      <p>
        Type in the box at the bottom and press <kbd>Enter</kbd> to send, or click <strong>Send</strong>. Use{" "}
        <kbd>Shift</kbd>+<kbd>Enter</kbd> for a new line without sending.
      </p>
      <h4>Keyboard shortcuts</h4>
      <ul>
        <li>
          <kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>K</kbd> or <kbd>/</kbd> (when not typing in another field) — focus the message input.
        </li>
        <li>
          <kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>F</kbd> — open or toggle <strong>Find in chat</strong> to search text in the current
          conversation. Matches are highlighted; use the controls to jump between occurrences.
        </li>
        <li>
          <kbd>Escape</kbd> — closes certain overlays (e.g. triage) when open; behavior depends on context.
        </li>
      </ul>
      <h4>Attachments</h4>
      <p>
        When you attach files from <strong>Tools</strong>, small chips appear above the input showing what&apos;s attached. Those
        items are included in the next request so Athena can use them.
      </p>
      <h4>Voice &amp; conversation (when enabled)</h4>
      <p>
        If your build exposes microphone features, you may see <strong>Conversation</strong> mode and dictation under Tools.
        Conversation mode listens for speech, sends it as a message, and can read replies aloud—useful for hands-free use. Grant
        microphone permission in your OS settings if prompted.
      </p>
      <h4>Officer review of saved offline chats</h4>
      <p>
        If your account rank is high enough, an <strong>Offline chat history</strong> dropdown appears above the chat list. Pick another
        operator to load their saved threads for audit; the UI switches to read-only for their history. Lower ranks never see this
        control.
      </p>
    </>
  );
}

function ToolsPanel() {
  return (
    <>
      <h3 className="help-detail-title">Tools &amp; attachments</h3>
      <p>
        Next to the input, the <strong>Tools</strong> button (<span className="help-inline-mock">＋ Tools</span>) opens a menu.
        From there you can:
      </p>
      <ul>
        <li>
          <strong>Attach images</strong> — Add images; they&apos;re processed and indexed so Athena can refer to them.
        </li>
        <li>
          <strong>Attach files</strong> — Add documents from disk into the current prompt context and library pipeline as
          appropriate.
        </li>
        <li>
          <strong>Voice / dictation</strong> — When available, speak instead of typing; requires microphone access.
        </li>
      </ul>
      <p>
        Attached content is associated with your next message. Large libraries benefit from letting files sit in your Library
        folder so they&apos;re fully indexed—not only one-off attachments.
      </p>
    </>
  );
}

function SidebarPanel() {
  return (
    <>
      <h3 className="help-detail-title">Sidebar (chats)</h3>
      <p>
        The left panel lists saved conversations. Click a chat to open it. Use the <strong>+</strong> button to start a{" "}
        <strong>new chat</strong> (saved). The <strong>temp</strong> control starts a quick session you can promote to saved
        later.
      </p>
      <ul>
        <li>
          <strong>Offline chat history</strong> — When your rank allows it, choose whose saved local chats to list (see Chat topic).
        </li>
        <li>
          <strong>Pin</strong> (<span aria-hidden>📌</span>) — Keep important chats at the top of the list.
        </li>
        <li>
          <strong>Delete</strong> — Remove a chat permanently (disabled while auditing another user&apos;s history).
        </li>
        <li>
          <strong>Search</strong> — Filter the chat list by title or content.
        </li>
        <li>
          <strong>Collapse</strong> — The arrow button hides the sidebar to give more room to the conversation.
        </li>
      </ul>
      <p>
        Mesh-originated AI conversations appear here too as separate chats (e.g. per remote node), named like{" "}
        <code>Athena (private) · …</code> when created from Meshtastic auto-reply.
      </p>
    </>
  );
}

function HeaderPanel() {
  return (
    <>
      <h3 className="help-detail-title">Top bar (left to right)</h3>
      <p>After the sidebar collapse control, the header icons are:</p>
      <ul className="help-detail-icon-list">
        <li>
          <strong>Home</strong> — Opens the dashboard (CPU/RAM, quick tiles to Chat, Map, Comms, Library, etc.). Scrolls on small
          screens.
        </li>
        <li>
          <strong>Export PDF</strong> — Saves the <em>current</em> conversation as a PDF file. Disabled when there are no messages
          or while an export is in progress.
        </li>
        <li>
          <strong>Emergency triage</strong> — Opens a menu of emergency checklists (first aid, bleeding, shock, poisoning, etc.).
          Pick a topic to see step-by-step &quot;Do this first&quot;, Do / Do NOT lists, and notes. Not a substitute for professional
          care or 911 when appropriate.
        </li>
        <li>
          <strong>Library</strong> — Choose your <strong>watch folder</strong>, see indexed files, open offline data links, scan for
          local ZIM files, and rebuild the vector index when you add many documents.
        </li>
        <li>
          <strong>The Archive</strong> — Opens from <strong>Home</strong>. Download curated tiers into your data sources; set your US
          regional-maps pin there (not in Settings → Location).
        </li>
        <li>
          <strong>Map</strong> (when enabled) — Tactical map with pins and layers. See <strong>Tactical map</strong> in Help.
        </li>
        <li>
          <strong>Meshtastic</strong> — Opens the USB mesh radio panel: Primary-channel broadcast send/receive. See the{" "}
          <strong>Meshtastic</strong> topic in this help for detail.
        </li>
        <li>
          <strong>Settings</strong> — Grouped menu: account &amp; calendar, AI, location &amp; maps, radio &amp; voice, look &amp; feel
          (window, themes, Theme studio), and backup &amp; diagnostics (error log, USB / <strong>Aegis Protocol</strong>).{" "}
          <strong>General → About ATHENA</strong> spells out the acronym.
        </li>
        <li>
          <strong>Help</strong> — This guide.
        </li>
      </ul>
      <p>
        If your system reports battery status, a <strong>battery</strong> indicator may appear near the end of the header.
      </p>
    </>
  );
}

function LibraryPanel() {
  return (
    <>
      <h3 className="help-detail-title">Library &amp; Kiwix</h3>
      <h4>Watch folder (Library path)</h4>
      <p>
        Open <strong>Library</strong> from the header, then <strong>Choose folder</strong>. Athena watches that folder and indexes
        supported documents so semantic search can find relevant chunks. <strong>Clear</strong> removes the path (not the files on
        disk). <strong>The Archive</strong> also writes into this folder—pick a disk with enough free space before
        downloading large tiers.
      </p>
      <h4>The Archive vs Library sheet</h4>
      <p>
        <strong>The Archive</strong> (from <strong>Home</strong>) is for <em>bundled</em> downloads: Kiwix archives in tiers and US
        state <strong>Protomaps</strong> <code>.pmtiles</code> chosen from your Archive download-area pin. The header <strong>Library</strong>{" "}
        sheet is for day-to-day file lists, scanning, and index maintenance. Both use the same watch folder path.
      </p>
      <h4>Kiwix / Wikipedia offline</h4>
      <p>
        <strong>Download more data</strong> (in the Files section) opens the <strong>Offline ZIM (Kiwix)</strong> dialog: link to
        the Kiwix Library site, then place <code>.zim</code> files inside your library folder and use{" "}
        <strong>Scan for Local Files</strong> to discover them. Academy and curriculum ZIMs installed via <strong>The Archive</strong>{" "}
        often land under <code>library/academy/</code>; they are indexed like other library files.
      </p>
      <h4>Indexing</h4>
      <p>
        When you add or change files, the app indexes them in the background. Already-indexed files whose size and modification time
        have not changed are skipped on startup so large libraries load faster. If something still seems out of date, use{" "}
        <strong>Rebuild vector index</strong> under Vector index (this re-embeds everything and can take a while).
      </p>
      <p>
        The <strong>Files in library</strong> list shows detected files; use <strong>View more</strong> when you have many.
      </p>
      <h4>Quarantine</h4>
      <p>
        If a file fails safety checks, the app may quarantine it and show an alert—follow the on-screen message if that happens.
      </p>
    </>
  );
}

function MeshtasticPanel() {
  return (
    <>
      <h3 className="help-detail-title">Meshtastic (USB mesh)</h3>
      <p>
        With a Meshtastic device connected over <strong>USB</strong>, Athena decodes plain text from the radio. The mesh panel (
        <strong>tower / broadcast</strong> icon) shows <strong>Primary channel</strong> <em>broadcast</em> traffic in{" "}
        <strong>Received</strong>—messages everyone on that channel can see.
      </p>
      <ul>
        <li>
          <strong>Send on mesh</strong> — Broadcasts a short text on the Primary channel (optional ACK). Wait until the node ID is
          known (a few seconds after plug-in) if send fails.
        </li>
        <li>
          <strong>Encrypted</strong> packets are not shown in the log.
        </li>
        <li>
          In <strong>Settings → Mesh</strong>, <strong>Auto-reply to mesh</strong> uses Ollama to answer other nodes&apos; Primary
          traffic; replies go as <strong>direct messages</strong> to each sender, and a <strong>private chat</strong> per node is
          saved in the sidebar—separate from the public mesh log.
        </li>
        <li>
          The mesh panel&apos;s <strong>?</strong> button has extra tips (e.g. USB vs BLE with a phone).
        </li>
        <li>
          <strong>Mesh network</strong> (from <strong>Chat</strong> or <strong>Comms</strong>) is the guided inbox: it shows which radio is plugged in, opens
          offline mesh chat, and links to Comms for technical logs.
        </li>
      </ul>
    </>
  );
}

function SettingsPanel() {
  return (
    <>
      <h3 className="help-detail-title">Settings</h3>
      <p>
        Open <strong>Settings</strong> from the gear icon. The main menu is grouped with dividers: <strong>General &amp; calendar</strong>{" "}
        (identity and schedule), <strong>AI &amp; Knowledge</strong>, <strong>Location &amp; maps</strong>, <strong>Radio &amp; voice</strong>,{" "}
        <strong>Look &amp; feel</strong>, and <strong>Backup &amp; diagnostics</strong>. The top <strong>Me</strong> card jumps straight to your
        operator profile (login name, PIN, display name, rank).
      </p>
      <h4>About ATHENA</h4>
      <p className="text-muted">
        Under <strong>General</strong>, see the full product name and a letter-by-letter breakdown of{" "}
        <strong>{ATHENA_FULL_NAME}</strong> so you know how Autonomous, Tactical, Heuristic, Emergency, Network, and Archive map to
        features.
      </p>

      <h4>General</h4>
      <ul>
        <li>
          <strong>About ATHENA</strong> — Official acronym expansion and what each letter emphasizes in this app.
        </li>
        <li>
          <strong>Default startup page</strong> — Which screen (Chat, Home, etc.) opens when you launch the app after login.
        </li>
        <li>
          <strong>System maintenance</strong> — Backup or restore the local index database; export serial <strong>Comms</strong> logs as CSV
          via the system file dialog.
        </li>
        <li>
          <strong>Me</strong> (nav card) — Lock screen <strong>login name</strong>, <strong>PIN</strong>, roster display name, and rank
          summary; save changes from this pane.
        </li>
        <li>
          <strong>User profile</strong> — Optional display name (used in AI context) and birthdate (stored only on this device). At age
          18+ you can enable the unrestricted model catalog for <strong>local</strong> uncensored models you install yourself.
        </li>
        <li>
          <strong>Help &amp; shortcuts</strong> — Quick reminders (focus input, find in chat, <code>/athena</code>, <code>/rag-count</code>
          ) and buttons to open Help or Library.
        </li>
        <li>
          <strong>Device stats overlay</strong> — Optional small CPU/RAM/network readout in the corner.
        </li>
      </ul>

      <h4>Calendar &amp; Date &amp; Time</h4>
      <ul>
        <li>
          <strong>Calendar</strong> — Holiday region, reminders, event templates, and <strong>ICS</strong> import/export. Mesh-related
          calendar options appear when your build exposes them.
        </li>
        <li>
          <strong>Date &amp; Time</strong> — 12- or 24-hour clock display and <strong>quiet hours</strong> so reminder notifications
          respect local sleep windows.
        </li>
      </ul>

      <h4>AI &amp; models</h4>
      <ul>
        <li>
          <strong>Inference backend</strong> — Default <strong>Ollama</strong>, or <strong>OpenAI-compatible (local)</strong> for LM
          Studio, llama.cpp server, etc. (localhost URLs only). Test connection from Settings when using the latter.
        </li>
        <li>
          <strong>Ollama</strong> — Status and a &quot;Try to start Ollama&quot; action when the server isn&apos;t running.
        </li>
        <li>
          <strong>Model size &amp; conversation memory</strong> — Tier selection (including extra-large 34B+ when installed) and how many
          past messages to include; <strong>maximum</strong> context scales with this device&apos;s RAM. Hardware estimate when available.
        </li>
        <li>
          <strong>Personality &amp; style</strong> — Assistant tone presets tied to themes when available.
        </li>
        <li>
          <strong>Library citations</strong> — Toggle &quot;From your library&quot; under assistant messages.
        </li>
      </ul>

      <h4>API keys</h4>
      <ul>
        <li>
          <strong>API keys</strong> — Separate panel for provider keys (e.g. geocoding or map tile APIs when your build uses them). Values
          stay on this device; only features you trigger send requests.
        </li>
      </ul>

      <h4>Location &amp; maps</h4>
      <ul>
        <li>
          <strong>Location</strong> — Toggle <strong>location-aware answers</strong> and your <strong>address or area</strong> (stored
          locally; not live cloud GPS). Used for AI context and the Kiwix “install local data” flow. This is separate from the{" "}
          <strong>The Archive</strong> regional-maps pin.
        </li>
        <li>
          <strong>Offline maps</strong> — Tactical basemap packs, Kiwix regional map ZIMs, imports, and the <strong>offline map manager</strong>{" "}
          when present. Does not replace Archive tier downloads; use <strong>Home → The Archive</strong> for bundled packages and US state{" "}
          Protomaps. Turn on <strong>Maps</strong> under Location to show the tactical map icon in the header.
        </li>
      </ul>

      <h4>Radio &amp; voice</h4>
      <ul>
        <li>
          <strong>Mesh</strong> — Meshtastic USB detection, channel/signup phrase, and <strong>auto-reply to mesh</strong> (Ollama-powered
          replies as DMs). The header mesh panel and <strong>Mesh network</strong> inbox use these settings.
        </li>
        <li>
          <strong>Voice</strong> — Text-to-speech: read replies aloud and voice presets when your build exposes them. (Microphone /
          conversation features, when enabled, live under <strong>Tools</strong> in chat—see the Chat topic.)
        </li>
      </ul>

      <h4>Look &amp; feel</h4>
      <ul>
        <li>
          <strong>Window &amp; Display</strong> — Fullscreen, night vision, and related display options.
        </li>
        <li>
          <strong>Themes &amp; Colors</strong> — Presets, <strong>personality</strong>, palette controls, and <strong>custom themes</strong>.
        </li>
        <li>
          <strong>Theme studio</strong> — Create or edit named themes (colors, scanlines, vignette, grain, corner style). New themes appear
          under Themes &amp; Colors after you save.
        </li>
      </ul>

      <h4>Backup &amp; diagnostics</h4>
      <ul>
        <li>
          <strong>Troubleshooting</strong> — Error log you can copy when reporting a problem; shortcuts to Library and Help; optional
          diagnostics exports when your build includes them.
        </li>
        <li>
          <strong>USB export</strong> — Opens <strong>Aegis Protocol</strong> immediately (no extra menu). Choose a{" "}
          <strong>tier</strong> — <em>Essentials</em> (compact portable pack and proximity map extract), <em>Recommended</em> (adds models and
          priority payload fill to a free-space target), or <em>Absolute</em> (attempts full payload and library mirrors)—then pick a
          detected removable drive. The dialog warns if <strong>free space</strong> is below the minimum for the selected tier.
        </li>
        <li>
          <strong>Format drive</strong> (macOS only) — Optional whole-disk erase as ExFAT labeled ATHENA before copy; requires a resolvable
          raw device path for the volume.
        </li>
        <li>
          <strong>Scorched Earth</strong> — Shown in <strong>red</strong> in the dialog. When enabled, local intelligence and documents can
          be wiped <em>after</em> a successful export; confirmations are explicit and irreversible.
        </li>
      </ul>

      <h4>Developer (hidden)</h4>
      <p>
        From the main Settings menu, tap the <strong>Settings</strong> title in the header seven times to open raw diagnostics.
      </p>
    </>
  );
}

function RepliesPanel() {
  return (
    <>
      <h3 className="help-detail-title">Replies &amp; playback</h3>
      <p>Under Athena&apos;s assistant messages you may see:</p>
      <ul>
        <li>
          <strong>Read aloud</strong> (<span aria-hidden>🔈</span>) — Text-to-speech for that reply (when TTS is enabled in
          Settings).
        </li>
        <li>
          <strong>Pause / Stop</strong> — Control ongoing speech.
        </li>
        <li>
          <strong>Copy</strong> (<span aria-hidden>⎘</span>) — Copy the reply to the clipboard.
        </li>
      </ul>
      <p>
        When <strong>Show library sources in chat</strong> is on, a <strong>From your library</strong> section lists documents that
        contributed to the answer—useful for verification.
      </p>
      <p>
        Streaming replies appear as they generate; if something fails, check the error banner or the error log in Settings.
      </p>
    </>
  );
}

function HomePanel() {
  return (
    <>
      <h3 className="help-detail-title">Home (dashboard)</h3>
      <p>
        The <strong>Home</strong> view is the launch pad: system readouts (CPU, RAM, and related stats when available), shortcuts to{" "}
        <strong>Chat</strong>, <strong>Tactical map</strong> (if Maps is on), <strong>Comms</strong>, <strong>Library</strong>,{" "}
        <strong>The Archive</strong> (bundled Wikipedia + US state map tiers into your watch folder), <strong>Knowledge catalog</strong>,
        and more.
      </p>
      <ul>
        <li>
          <strong>Academy</strong> (when shown on your dashboard) opens guided lessons inside the app—structured modules, optional PDF-style
          units when your build includes them, and links into The Archive for curriculum ZIMs and payloads under{" "}
          <code>library/academy/</code>.
        </li>
        <li>
          <strong>Knowledge catalog</strong> — Browse bundled reference and installable packages from the dashboard when available; it
          complements Chat and The Archive rather than replacing your Library watch folder.
        </li>
        <li>
          <strong>Calendar</strong> opens the schedule workspace (events, ICS, reminders)—see the Calendar topic in Help.
        </li>
        <li>
          <strong>Personnel</strong> and <strong>Logistics Hub</strong> (when visible) depend on your rank and build; use them for roster and supply workflows.
        </li>
        <li>
          On <strong>short or narrow screens</strong>, the page scrolls so nothing is cut off below the header.
        </li>
        <li>
          <strong>Recent Comms</strong> shows a short log of recent serial/mesh traffic when the radio stack has data—tap through to
          the full Comms view for terminals and channels.
        </li>
      </ul>
    </>
  );
}

function TacticalMapPanel() {
  return (
    <>
      <h3 className="help-detail-title">Tactical map</h3>
      <p>
        Open the map from <strong>Home</strong> or the <strong>map</strong> icon in the header (when Maps is enabled in Settings →
        Location). You pan and zoom the basemap; <strong>right‑click</strong> (or long‑press on touch) drops or edits a point of
        interest.
      </p>
      <h4>Pin picker (Resources · Network · Custom)</h4>
      <p>
        When you add or edit a pin, the icon chooser has three tabs—aligned with how pins are stored:
      </p>
      <ul>
        <li>
          <strong>Resources</strong> — Common sites and supplies (shelter, water, medical, fuel, etc.). Saved as{" "}
          <em>resource</em> pins.
        </li>
        <li>
          <strong>Network</strong> — Meshtastic / RF / comms‑style icons (mesh nodes, antennas, repeaters). Saved as{" "}
          <em>network</em> pins so they filter separately from supplies.
        </li>
        <li>
          <strong>Custom</strong> — Full Lucide library plus search—pick any icon when the presets don&apos;t fit.
        </li>
      </ul>
      <h4>Overlays &amp; layers</h4>
      <p>
        Use <strong>Overlays</strong> on the map left rail to toggle tactical mesh, resource pins, hazard markers, drawings, and
        route blockages. Mesh overlays (when connected) show node positions on the map where supported by your build.
      </p>
    </>
  );
}

function MapsOfflinePanel() {
  return (
    <>
      <h3 className="help-detail-title">Maps &amp; offline data</h3>
      <p>Athena uses several kinds of “offline map” data—each has its own job:</p>
      <ul>
        <li>
          <strong>Kiwix / ZIM maps</strong> — Wikipedia‑style <code>.zim</code> files in your Library. Great for place names and
          reference. Add them via Library → offline data, The Archive tiers, or manual download, then index as usual.
        </li>
        <li>
          <strong>Basemap tiles (MBTiles / PMTiles / archives)</strong> — Raster or vector tiles for the <em>tactical moving map</em>.
          Pick regions or import files under <strong>Settings → Offline maps</strong> and (when enabled) the Library <strong>Maps</strong>{" "}
          tab. These are what you pan/zoom in the map viewer—not encyclopedia ZIMs.
        </li>
        <li>
          <strong>The Archive state extracts</strong> — Large US state <code>.pmtiles</code> from Protomaps, saved under{" "}
          <code>library/maps/</code> inside your watch folder. They are selected from the <strong>Archive download-area pin</strong> (home
          state plus border‑proximity neighbors). They are <em>not</em> the same as the tactical basemap packs in Settings → Offline
          maps—both can coexist.
        </li>
      </ul>
      <p>
        <strong>Settings → Location</strong> controls AI place-aware answers and the Kiwix “install local data” sheet—it does{" "}
        <strong>not</strong> change The Archive’s regional Protomaps list. Change the download-area pin inside{" "}
        <strong>The Archive</strong> (Change area).
      </p>
      <p>
        If something looks empty, check that the right tile pack is <strong>enabled</strong> for the main map and that downloads
        finished. Satellite or API keys (when used) live under Settings → API keys.
      </p>
    </>
  );
}

function RadioCommsPanel() {
  return (
    <>
      <h3 className="help-detail-title">Comms (serial radio)</h3>
      <p>
        <strong>Comms</strong> is the full‑screen serial radio workspace (from Home or the dashboard). It is separate from the small{" "}
        <strong>Meshtastic</strong> header panel: Comms is for choosing a <strong>serial port</strong>, viewing a live{" "}
        <strong>terminal</strong> of RX lines, optional telemetry (RSSI/SNR), saved <strong>mesh message</strong> history, and{" "}
        <strong>radio channels</strong> you define in-app.
      </p>
      <ul>
        <li>
          <strong>Connect</strong> — Pick a port, connect, and watch traffic scroll. Disconnect when you&apos;re done to free the
          port.
        </li>
        <li>
          <strong>Send</strong> — Type a line to transmit when your hardware/firmware path supports it; follow local regulations and
          device limits.
        </li>
        <li>
          <strong>Logs</strong> — Recent traffic can also feed AI context when enabled; you can export serial logs from Settings →
          Troubleshooting for debugging.
        </li>
      </ul>
      <p>
        For USB Meshtastic <em>broadcast/DM</em> chat integration and auto‑reply, see <strong>Meshtastic</strong> in this help—that
        flow uses the mesh panel and Settings → Mesh, not only the Comms terminal.
      </p>
    </>
  );
}

function AskAthenaPanel() {
  const [q, setQ] = useState("");
  const [answer, setAnswer] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    const question = q.trim();
    if (!question || busy) return;
    setBusy(true);
    setErr(null);
    setAnswer(null);
    // Yield so the UI can paint loading state before the Tauri/Ollama round-trip (avoids a “frozen” frame).
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    try {
      const text = await invoke<string>("help_assistant_ask", { question });
      setAnswer(text);
    } catch (e) {
      setErr(e != null ? String(e) : "Request failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <h3 className="help-detail-title">Ask Athena (beta)</h3>
      <p>
        A short, <strong>in‑app only</strong> assistant powered by your local <strong>Ollama</strong> model. It is prompted to answer{" "}
        <em>only</em> how to use Athena (UI, maps, mesh, library, settings)—not general homework or unrelated topics.
      </p>
      <ul>
        <li>Requires <strong>Ollama running</strong> and at least one model installed (same as chat).</li>
        <li>Does not use your document library (no RAG)—it&apos;s for navigation and “how do I…?” questions.</li>
        <li>For long-form help in chat, you can still use <code>/athena</code> with the project reference.</li>
      </ul>
      <div className="help-ask-athena">
        <label htmlFor="help-ask-input" className="help-ask-label">
          Your question
        </label>
        <textarea
          id="help-ask-input"
          className="help-ask-textarea map-chat-input"
          rows={3}
          placeholder='e.g. "Where do I enable offline basemaps?"'
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void submit();
            }
          }}
          disabled={busy}
        />
        <div className="help-ask-actions">
          <span className="text-muted small help-ask-hint">
            <kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>Enter</kbd> to send
          </span>
          <button
            type="button"
            className={`send-btn${q.trim() && !busy ? " send-btn-active" : ""}`}
            onClick={() => void submit()}
            disabled={busy || !q.trim()}
            title="Ask (⌘/Ctrl+Enter)"
          >
            Ask
          </button>
        </div>
        {busy ? (
          <div className="help-ask-thinking" role="status" aria-live="polite" aria-busy="true">
            <span className="message-loading-dots" aria-hidden>
              <span className="message-loading-dot" />
              <span className="message-loading-dot" />
              <span className="message-loading-dot" />
            </span>
            <span className="help-ask-thinking-label">Thinking…</span>
          </div>
        ) : null}
        {err ? (
          <p className="help-ask-error" role="alert">
            {err}
          </p>
        ) : null}
        {answer ? (
          <div className="help-ask-answer">
            <h4 className="help-ask-answer-title">Reply</h4>
            <div className="help-ask-answer-body">{answer}</div>
          </div>
        ) : null}
      </div>
    </>
  );
}
