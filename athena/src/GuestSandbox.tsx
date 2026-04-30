import { useState } from "react";
import { Radio, MessageSquare } from "lucide-react";

export function GuestSandbox({ onOpenRadio, onOpenAssistant }: { onOpenRadio: () => void; onOpenAssistant: () => void }) {
  const [tab, setTab] = useState<"home" | "radio" | "assistant">("home");

  if (tab === "radio") {
    return (
      <div className="home-view-scroll">
        <button type="button" className="secondary" onClick={() => setTab("home")}>
          Back
        </button>
        <h2 style={{ marginTop: 12 }}>Emergency Radio</h2>
        <p className="text-muted">Guest mode: Open/Distress channel only.</p>
        <button type="button" className="glass-action-btn" onClick={onOpenRadio}>
          Open Emergency Radio
        </button>
      </div>
    );
  }

  if (tab === "assistant") {
    return (
      <div className="home-view-scroll">
        <button type="button" className="secondary" onClick={() => setTab("home")}>
          Back
        </button>
        <h2 style={{ marginTop: 12 }}>Basic Assistant</h2>
        <p className="text-muted">Guest mode: no library access; general safety advice only.</p>
        <button type="button" className="glass-action-btn" onClick={onOpenAssistant}>
          Open Basic Assistant
        </button>
      </div>
    );
  }

  return (
    <div className="home-view-scroll">
      <div className="glass-panel" style={{ padding: "1rem", borderRadius: 14 }}>
        <h2 style={{ margin: 0 }}>Guest sandbox</h2>
        <p className="text-muted" style={{ marginTop: 8 }}>
          Level 0 access. You can use Emergency Radio and a Basic Assistant. All ATHENA operational features are locked.
        </p>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: 12, marginTop: 14 }}>
          <button type="button" className="glass-action-btn" onClick={() => setTab("radio")}>
            <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
              <Radio size={18} /> Emergency Radio
            </span>
          </button>
          <button type="button" className="glass-action-btn" onClick={() => setTab("assistant")}>
            <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
              <MessageSquare size={18} /> Basic Assistant
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}

