import { useEffect, useState } from "react";

export type HelpSectionId =
  | "overview"
  | "chat"
  | "tools"
  | "sidebar"
  | "header"
  | "library"
  | "meshtastic"
  | "settings"
  | "replies";

const SECTIONS: { id: HelpSectionId; label: string; blurb: string }[] = [
  { id: "overview", label: "Overview", blurb: "What Athena is and how it works" },
  { id: "chat", label: "Chat & composer", blurb: "Sending messages and shortcuts" },
  { id: "tools", label: "Tools", blurb: "Attachments and the Tools menu" },
  { id: "sidebar", label: "Sidebar", blurb: "Chats list and organization" },
  { id: "header", label: "Top bar", blurb: "Icons across the header" },
  { id: "library", label: "Library & Kiwix", blurb: "Documents, indexing, offline Wikipedia" },
  { id: "meshtastic", label: "Meshtastic", blurb: "USB LoRa mesh radio" },
  { id: "settings", label: "Settings", blurb: "General, AI, location, mesh, appearance, troubleshooting" },
  { id: "replies", label: "Replies & playback", blurb: "TTS, copy, and citations" },
];

interface HelpModalProps {
  open: boolean;
  onClose: () => void;
}

export function HelpModal({ open, onClose }: HelpModalProps) {
  const [active, setActive] = useState<HelpSectionId>("overview");

  useEffect(() => {
    if (open) setActive("overview");
  }, [open]);

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

        <div className="help-modal-shell">
          <nav className="help-modal-nav" aria-labelledby="help-modal-title">
            <p className="help-modal-nav-intro">Choose a topic:</p>
            <ul className="help-modal-nav-list" role="tablist" aria-label="Help sections">
              {SECTIONS.map((s) => (
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
          </nav>

          <div
            className="help-modal-detail"
            role="tabpanel"
            id={`help-panel-${active}`}
            aria-labelledby={`help-tab-${active}`}
          >
            {active === "overview" && <OverviewPanel />}
            {active === "chat" && <ChatPanel />}
            {active === "tools" && <ToolsPanel />}
            {active === "sidebar" && <SidebarPanel />}
            {active === "header" && <HeaderPanel />}
            {active === "library" && <LibraryPanel />}
            {active === "meshtastic" && <MeshtasticPanel />}
            {active === "settings" && <SettingsPanel />}
            {active === "replies" && <RepliesPanel />}
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
          <strong>Ollama</strong> installed and running, with at least one model pulled (e.g. <code>ollama run llama3.2:3b</code>
          ). If Ollama isn&apos;t running, Athena shows a banner at the top of the app.
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
          <strong>Ollama</strong> — Status and a &quot;Try to start Ollama&quot; action when the server isn&apos;t running.
        </li>
        <li>
          <strong>Model size &amp; conversation memory</strong> — Tier selection and how many past messages to include; hardware
          estimate when available.
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
