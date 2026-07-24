import { FormEvent, useRef, useState } from "react";
import { api } from "../lib/api";

type Msg = { role: "user" | "assistant"; content: string; meta?: string };

export function ChatWidget() {
  const [messages, setMessages] = useState<Msg[]>([
    {
      role: "assistant",
      content: "Athena online. Cloud-first routing with local Ollama failover. Hold Push-to-talk to speak.",
    },
  ]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);
  const mediaRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<BlobPart[]>([]);

  const send = async (e?: FormEvent) => {
    e?.preventDefault();
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    const next = [...messages, { role: "user" as const, content: text }];
    setMessages(next);
    setBusy(true);
    try {
      const res = await api.chat(next.map((m) => ({ role: m.role, content: m.content })));
      setMessages((m) => [
        ...m,
        {
          role: "assistant",
          content: res.content,
          meta: `${res.provider} · ${res.model}${res.detail ? ` · ${res.detail}` : ""}`,
        },
      ]);
    } catch (err) {
      setMessages((m) => [
        ...m,
        { role: "assistant", content: `Error: ${String(err)}` },
      ]);
    } finally {
      setBusy(false);
    }
  };

  const startPtt = async () => {
    if (recording || busy) return;
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const rec = new MediaRecorder(stream);
    chunksRef.current = [];
    rec.ondataavailable = (ev) => {
      if (ev.data.size > 0) chunksRef.current.push(ev.data);
    };
    rec.onstop = async () => {
      stream.getTracks().forEach((t) => t.stop());
      const blob = new Blob(chunksRef.current, { type: "audio/webm" });
      setBusy(true);
      try {
        const fd = new FormData();
        fd.append("audio", blob, "ptt.webm");
        const res = await fetch("/api/voice/ptt", { method: "POST", body: fd });
        if (!res.ok) throw new Error(await res.text());
        const transcript = res.headers.get("X-Athena-Transcript") || "";
        const reply = res.headers.get("X-Athena-Reply") || "";
        const provider = res.headers.get("X-Athena-Provider") || "";
        const model = res.headers.get("X-Athena-Model") || "";
        setMessages((m) => [
          ...m,
          { role: "user", content: transcript || "(no speech detected)" },
          { role: "assistant", content: reply, meta: `${provider} · ${model} · voice` },
        ]);
        const wav = await res.blob();
        const url = URL.createObjectURL(wav);
        const audio = new Audio(url);
        await audio.play();
      } catch (err) {
        setMessages((m) => [...m, { role: "assistant", content: `Voice error: ${String(err)}` }]);
      } finally {
        setBusy(false);
        setRecording(false);
      }
    };
    mediaRef.current = rec;
    rec.start();
    setRecording(true);
  };

  const stopPtt = () => {
    mediaRef.current?.stop();
  };

  return (
    <div style={{ display: "grid", gridTemplateRows: "1fr auto", height: "100%", gap: "0.5rem" }}>
      <div className="chat-log">
        {messages.map((m, i) => (
          <div key={i} className={`bubble ${m.role}`}>
            <div>{m.content}</div>
            {m.meta ? (
              <div className="stub-copy" style={{ marginTop: "0.25rem" }}>
                {m.meta}
              </div>
            ) : null}
          </div>
        ))}
      </div>
      <form className="chat-compose" onSubmit={send}>
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Message Athena…"
          disabled={busy}
        />
        <div className="row-actions">
          <button type="submit" className="btn primary" disabled={busy}>
            Send
          </button>
          <button
            type="button"
            className={`btn ${recording ? "ptt-active" : ""}`}
            disabled={busy && !recording}
            onMouseDown={() => void startPtt()}
            onMouseUp={stopPtt}
            onMouseLeave={() => recording && stopPtt()}
            onTouchStart={(e) => {
              e.preventDefault();
              void startPtt();
            }}
            onTouchEnd={(e) => {
              e.preventDefault();
              stopPtt();
            }}
          >
            {recording ? "Release to send" : "Push to talk"}
          </button>
        </div>
      </form>
    </div>
  );
}
