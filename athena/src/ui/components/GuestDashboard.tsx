import { useEffect, useRef, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { type UnlistenFn } from "@tauri-apps/api/event";
import { useAuth } from "../../domains/auth/AuthContext";

type UiMsg = {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  ts: number;
};

function nowId() {
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function GuestDashboard() {
  const { logout } = useAuth();
  const [tab, setTab] = useState<"radio" | "assistant">("radio");

  // Emergency radio (broadcast-only)
  const [radioDraft, setRadioDraft] = useState("");
  const [radioMsgs, setRadioMsgs] = useState<UiMsg[]>(() => [
    {
      id: nowId(),
      role: "system",
      text: "Guest mode emergency radio. Broadcast-only on Open/Distress (channel 0).",
      ts: Date.now(),
    },
  ]);

  // Basic assistant (already air-gapped server-side for clearance 0)
  const [aiDraft, setAiDraft] = useState("");
  const [aiMsgs, setAiMsgs] = useState<UiMsg[]>(() => [
    {
      id: nowId(),
      role: "system",
      text: "Basic Assistant: general safety advice only. No ATHENA operational knowledge.",
      ts: Date.now(),
    },
  ]);
  const aiActiveReplyIdRef = useRef<string | null>(null);

  useEffect(() => {
    let un: UnlistenFn | undefined;
    void listen<{ sender_id: number; text: string; channel?: number }>("athena:mesh-text", (ev) => {
      const p = ev.payload;
      if (!p || typeof p.sender_id !== "number" || typeof p.text !== "string") return;
      const ch = typeof p.channel === "number" ? p.channel : 0;
      if (ch !== 0) return; // guest sees open distress only
      const t = p.text.trim();
      if (!t) return;
      setRadioMsgs((prev) => [...prev, { id: nowId(), role: "assistant", text: `Node ${p.sender_id}: ${t}`, ts: Date.now() }]);
    }).then((fn) => {
      un = fn;
    });
    return () => {
      void un?.();
    };
  }, []);

  useEffect(() => {
    let unChunk: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    let unErr: UnlistenFn | undefined;

    // Stream events are already used elsewhere in ATHENA; we reuse them here for the simplified UI.
    void listen<{ chunk?: string }>("athena:stream-chunk", (ev) => {
      const activeId = aiActiveReplyIdRef.current;
      if (!activeId) return;
      const chunk = ev.payload?.chunk ?? "";
      if (!chunk) return;
      setAiMsgs((prev) =>
        prev.map((m) => (m.id === activeId ? { ...m, text: (m.text ?? "") + chunk } : m)),
      );
    }).then((fn) => (unChunk = fn));

    void listen("athena:stream-done", () => {
      aiActiveReplyIdRef.current = null;
    }).then((fn) => (unDone = fn));

    void listen<{ message?: string }>("athena:stream-error", (ev) => {
      aiActiveReplyIdRef.current = null;
      const msg = ev.payload?.message ?? "Assistant error.";
      setAiMsgs((prev) => [...prev, { id: nowId(), role: "system", text: msg, ts: Date.now() }]);
    }).then((fn) => (unErr = fn));

    return () => {
      void unChunk?.();
      void unDone?.();
      void unErr?.();
    };
  }, []);

  async function sendRadio() {
    const text = radioDraft.trim();
    if (!text) return;
    setRadioDraft("");
    setRadioMsgs((prev) => [...prev, { id: nowId(), role: "user", text, ts: Date.now() }]);
    try {
      await invoke("send_mesh_message", {
        text,
        channel: 0,
        destinationNodeId: null,
        wantAck: false,
      });
      setRadioMsgs((prev) => [
        ...prev,
        { id: nowId(), role: "system", text: "Broadcast sent.", ts: Date.now() },
      ]);
    } catch (e) {
      setRadioMsgs((prev) => [
        ...prev,
        { id: nowId(), role: "system", text: `Radio error: ${String(e)}`, ts: Date.now() },
      ]);
    }
  }

  async function sendAi() {
    const text = aiDraft.trim();
    if (!text) return;
    setAiDraft("");
    setAiMsgs((prev) => [...prev, { id: nowId(), role: "user", text, ts: Date.now() }]);

    const replyId = nowId();
    aiActiveReplyIdRef.current = replyId;
    setAiMsgs((prev) => [...prev, { id: replyId, role: "assistant", text: "", ts: Date.now() }]);

    try {
      // Backend enforces guest air-gap + restrictive system prompt.
      await invoke("send_message_stream", {
        message: text,
        // Keep minimal context for guest; backend bypasses RAG anyway.
        messages: [],
      });
    } catch (e) {
      aiActiveReplyIdRef.current = null;
      setAiMsgs((prev) => [
        ...prev,
        { id: nowId(), role: "system", text: `Assistant error: ${String(e)}`, ts: Date.now() },
      ]);
    }
  }

  return (
    <div className="guest-sandbox">
      <header className="guest-sandbox__header">
        <div className="guest-sandbox__head-row">
          <div>
            <div className="guest-sandbox__title">GUEST SANDBOX</div>
            <div className="guest-sandbox__subtitle">
              Level 0 access. Emergency comms + basic survival assistant only.
            </div>
          </div>
          <button
            type="button"
            className="secondary"
            onClick={() => void logout()}
            title="Return to login"
          >
            Exit
          </button>
        </div>
      </header>

      <nav className="guest-sandbox__nav" aria-label="Guest sections">
        <button
          type="button"
          className={tab === "radio" ? "glass-action-btn" : "secondary"}
          onClick={() => setTab("radio")}
        >
          Emergency Radio
        </button>
        <button
          type="button"
          className={tab === "assistant" ? "glass-action-btn" : "secondary"}
          onClick={() => setTab("assistant")}
        >
          Basic Assistant
        </button>
      </nav>

      <main className="guest-sandbox__main">
        {tab === "radio" ? (
          <div>
            <div className="guest-sandbox__h2">Open/Distress Channel (0)</div>
            <div className="guest-sandbox__lead">Broadcast-only. No channel controls. No encrypted/internal comms access.</div>
            <div className="guest-sandbox__msgs">
              {radioMsgs.map((m) => (
                <div
                  key={m.id}
                  className={"guest-sandbox__msg" + (m.role === "user" ? " guest-sandbox__msg--user" : "")}
                >
                  <div className="guest-sandbox__msg-meta">
                    {m.role === "user" ? "You" : m.role === "system" ? "System" : "Remote"}
                  </div>
                  <div className="guest-sandbox__msg-text">{m.text}</div>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div>
            <div className="guest-sandbox__h2">Basic Assistant</div>
            <div className="guest-sandbox__lead">General, safe advice only. This guest instance is air-gapped from ATHENA systems.</div>
            <div className="guest-sandbox__msgs">
              {aiMsgs.map((m) => (
                <div
                  key={m.id}
                  className={"guest-sandbox__msg" + (m.role === "user" ? " guest-sandbox__msg--user" : "")}
                >
                  <div className="guest-sandbox__msg-meta">
                    {m.role === "user" ? "You" : m.role === "assistant" ? "Assistant" : "System"}
                  </div>
                  <div className="guest-sandbox__msg-text">{m.text || (m.role === "assistant" ? "…" : "")}</div>
                </div>
              ))}
            </div>
          </div>
        )}
      </main>

      <footer className="guest-sandbox__footer">
        {tab === "radio" ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void sendRadio();
            }}
            className="guest-sandbox__form"
          >
            <input
              value={radioDraft}
              onChange={(e) => setRadioDraft(e.target.value)}
              placeholder="Broadcast distress message…"
              className="guest-sandbox__input"
            />
            <button type="submit" className="glass-action-btn">
              Send
            </button>
          </form>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void sendAi();
            }}
            className="guest-sandbox__form"
          >
            <input
              value={aiDraft}
              onChange={(e) => setAiDraft(e.target.value)}
              placeholder="Ask a survival question…"
              className="guest-sandbox__input"
            />
            <button type="submit" className="glass-action-btn">
              Send
            </button>
          </form>
        )}
      </footer>
    </div>
  );
}

