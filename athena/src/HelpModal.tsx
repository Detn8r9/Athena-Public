import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type HelpSectionId =
  | "overview"
  | "home"
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
  | "replies"
  | "help_assistant";

type SectionDef = {
  id: HelpSectionId;
  label: string;
  blurb: string;
  /** Extra words for search (lowercase phrases). */
  keywords?: string[];
};

const SECTIONS: SectionDef[] = [
  { id: "overview", label: "Overview", blurb: "What Athena is and how it works", keywords: ["ollama", "local", "offline"] },
  { id: "home", label: "Home", blurb: "Dashboard, CPU/RAM, quick launch", keywords: ["dashboard", "launcher", "system"] },
  {
    id: "tactical_map",
    label: "Tactical map",
    blurb: "Pins, layers, mesh, right‑click",
    keywords: ["map", "pin", "tactical", "poi", "mesh", "leaflet"],
  },
  {
    id: "maps_offline",
    label: "Maps & offline data",
    blurb: "Kiwix ZIMs vs MBTiles / archives",
    keywords: ["kiwix", "zim", "mbtiles", "pmtiles", "download", "basemap", "archive"],
  },
  { id: "chat", label: "Chat & composer", blurb: "Sending messages and shortcuts", keywords: ["message", "keyboard"] },
  { id: "tools", label: "Tools", blurb: "Attachments and the Tools menu", keywords: ["attach", "file", "image"] },
  { id: "sidebar", label: "Sidebar", blurb: "Chats list and organization", keywords: ["chat list", "pin"] },
  { id: "header", label: "Top bar", blurb: "Icons across the header", keywords: ["export", "pdf", "triage"] },
  { id: "library", label: "Library & Kiwix", blurb: "Documents, indexing, offline Wikipedia", keywords: ["index", "rag", "zim"] },
  { id: "meshtastic", label: "Meshtastic", blurb: "USB LoRa mesh radio", keywords: ["mesh", "lora", "usb"] },
  {
    id: "radio_comms",
    label: "Comms (radio)",
    blurb: "Serial radio terminal, channels",
    keywords: ["radio", "serial", "comms", "mesh", "packet"],
  },
  { id: "settings", label: "Settings", blurb: "General, AI, location, mesh, appearance", keywords: ["preferences", "theme"] },
  { id: "replies", label: "Replies & playback", blurb: "TTS, copy, and citations", keywords: ["speech", "copy"] },
  {
    id: "help_assistant",
    label: "Ask Athena",
    blurb: "AI help for using the app (beta)",
    keywords: ["ai", "question", "search", "how do i"],
  },
];

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
  const [active, setActive] = useState<HelpSectionId>("overview");
  const [search, setSearch] = useState("");

  const visibleSections = useMemo(() => SECTIONS.filter((s) => sectionMatches(search, s)), [search]);

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
          <h2 id="help-modal-title">Help &amp; features</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close help">
            ×
          </button>
        </div>

        <div className="help-modal-search-wrap">
          <label htmlFor="help-modal-search" className="help-modal-search-label">
            Search topics
          </label>
          <input
            id="help-modal-search"
            type="search"
            className="help-modal-search-input map-chat-input"
            placeholder="Filter topics (e.g. map, mesh, library)…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            autoComplete="off"
          />
        </div>

        <div className="help-modal-shell">
          <nav className="help-modal-nav" aria-labelledby="help-modal-title">
            <p className="help-modal-nav-intro">{search.trim() ? "Matching topics:" : "Choose a topic:"}</p>
            {visibleSections.length === 0 ? (
              <p className="text-muted small help-modal-no-match">No topics match. Clear the search or try Ask Athena.</p>
            ) : (
              <ul className="help-modal-nav-list" role="tablist" aria-label="Help sections">
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
              <p className="help-modal-empty-detail text-muted">
                No topics match this filter. Clear the search box above, or open <strong>Ask Athena</strong> from the sidebar once
                it appears again.
              </p>
            ) : (
              <>
                {active === "overview" && <OverviewPanel />}
                {active === "home" && <HomePanel />}
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

function OverviewPanel() {
  return (
    <>
      <h3 className="help-detail-title">Overview</h3>
      <p>
        <strong>Athena</strong> is a local, offline-first survival and repair assistant. It runs a large language model on
        your machine through <strong>Ollama</strong> and grounds answers in documents you add to your <strong>Library</strong>{" "}
        (files in a folder you choose). Nothing is sent to the cloud for inference—everything stays on your device.
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
          A <strong>Library folder</strong> (optional but recommended): PDFs, text, and Kiwix <code>.zim</code> files are indexed
          so answers can cite your material.
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
        (then your question) so the model uses the built-in <strong>Athena project reference</strong> instead of relying on
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
        The main area shows your messages and Athena&apos;s replies. Type in the box at the bottom and press{" "}
        <kbd>Enter</kbd> to send, or click <strong>Send</strong>. Use <kbd>Shift</kbd>+<kbd>Enter</kbd> for a new line without
        sending.
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
          <strong>Pin</strong> (<span aria-hidden>📌</span>) — Keep important chats at the top of the list.
        </li>
        <li>
          <strong>Delete</strong> — Remove a chat permanently.
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
          <strong>Library</strong> — Choose your library folder, see indexed files, download more data (opens Kiwix Library in the
          browser), scan for local ZIM files, and rebuild the vector index when you add many documents.
        </li>
        <li>
          <strong>Map</strong> (when enabled) — Tactical map with pins and layers. See <strong>Tactical map</strong> in Help.
        </li>
        <li>
          <strong>Meshtastic</strong> — Opens the USB mesh radio panel: Primary-channel broadcast send/receive. See the{" "}
          <strong>Meshtastic</strong> topic in this help for detail.
        </li>
        <li>
          <strong>Settings</strong> — Organized sections: General, AI &amp; models, Location, Mesh &amp; voice, Appearance,
          Troubleshooting.
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
      <h4>Library folder</h4>
      <p>
        Open <strong>Library</strong> from the header, then <strong>Choose folder</strong>. Athena watches that folder and indexes
        supported documents so semantic search can find relevant chunks. <strong>Clear</strong> removes the path (not the files on
        disk).
      </p>
      <h4>Kiwix / Wikipedia offline</h4>
      <p>
        <strong>Download more data</strong> (in the Files section) opens the <strong>Offline ZIM (Kiwix)</strong> dialog: link to
        the Kiwix Library site, then place <code>.zim</code> files inside your library folder and use{" "}
        <strong>Scan for Local Files</strong> to discover them.
      </p>
      <h4>Indexing</h4>
      <p>
        The <strong>Files in library</strong> list shows detected files; use <strong>View more</strong> when you have many.
        Under <strong>Vector index</strong>, run <strong>Rebuild vector index</strong> after adding or changing lots of documents
        so semantic search embeddings stay accurate (can take a while on large libraries).
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
          In <strong>Settings → Features</strong>, <strong>Auto-reply to mesh</strong> uses Ollama to answer other nodes&apos; Primary
          traffic; replies go as <strong>direct messages</strong> to each sender, and a <strong>private chat</strong> per node is
          saved in the sidebar—separate from the public mesh log.
        </li>
        <li>
          The mesh panel&apos;s <strong>?</strong> button has extra tips (e.g. USB vs BLE with a phone).
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
        Open <strong>Settings</strong> from the gear icon. The first screen lists categories grouped as <strong>Basics</strong>,{" "}
        <strong>Context &amp; devices</strong>, and <strong>Look &amp; support</strong>:
      </p>

      <h4>General</h4>
      <ul>
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
          <strong>Library citations</strong> — Toggle &quot;From your library&quot; under assistant messages.
        </li>
      </ul>

      <h4>Location &amp; maps</h4>
      <ul>
        <li>
          <strong>Location-aware answers</strong> and your <strong>address or area</strong> (stored locally; not live cloud GPS).
        </li>
      </ul>

      <h4>Mesh &amp; voice</h4>
      <ul>
        <li>
          <strong>Meshtastic auto-reply</strong> — AI replies over USB mesh when Ollama is running.
        </li>
        <li>
          <strong>Text-to-speech</strong> — Read replies aloud; voice presets when available.
        </li>
      </ul>

      <h4>Appearance &amp; theme</h4>
      <ul>
        <li>
          <strong>Fullscreen</strong>, <strong>night vision</strong>, theme presets, <strong>personality</strong>, custom themes, and
          color tuning.
        </li>
      </ul>

      <h4>Troubleshooting</h4>
      <ul>
        <li>
          <strong>Error log</strong> — Copy for support; shortcuts to Library and Help; how to open <strong>Developer</strong> tools.
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
        <strong>Knowledge catalog</strong>, and more.
      </p>
      <ul>
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
      <h4>Sidebar &amp; layers</h4>
      <p>
        Use the tactical sidebar to toggle visibility, focus layers, and manage POIs. Mesh overlays (when connected) show node
        positions on the map where supported by your build.
      </p>
    </>
  );
}

function MapsOfflinePanel() {
  return (
    <>
      <h3 className="help-detail-title">Maps &amp; offline data</h3>
      <p>Athena mixes two different kinds of “offline map” data—each has its own job:</p>
      <ul>
        <li>
          <strong>Kiwix / ZIM maps</strong> — Wikipedia‑style <code>.zim</code> files in your Library. Great for place names and
          reference. Add them via Library → offline data, then index as usual.
        </li>
        <li>
          <strong>Basemap tiles (MBTiles / PMTiles / archives)</strong> — Raster or vector tiles for the <em>moving map</em>. Pick
          regions or import files under <strong>Settings → Offline maps</strong> and (when enabled) the Library <strong>Maps</strong>{" "}
          tab. These are what you pan/zoom on the tactical map—not the ZIM files.
        </li>
      </ul>
      <p>
        If something looks empty, check that the right tile pack is <strong>enabled</strong> for the main map and that files finished
        downloading. Satellite or API keys (when used) live under Settings → API keys.
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
