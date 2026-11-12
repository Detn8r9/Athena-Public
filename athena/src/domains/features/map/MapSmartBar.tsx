import { useState } from "react";
import { Wand2 } from "lucide-react";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";

type MapToolResult = { tool: string; ok: boolean; error?: string; data?: unknown };

/**
 * Map assistant: sends to `map_assistant_send` (Ollama + tool extraction in Rust).
 */
export function MapSmartBar({
  onSend,
  onResult,
  placeholder = "Ask the map assistant… (find markers or places, directions, route)",
}: {
  onSend: (msg: string) => Promise<{ reply: string; toolResults: MapToolResult[] }>;
  onResult?: (r: { reply: string; toolResults: MapToolResult[] }) => void;
  placeholder?: string;
}) {
  const [text, setText] = useState("");
  const [pending, setPending] = useState(false);

  return (
    <form
      className="map-smart-bar"
      onSubmit={async (e) => {
        e.preventDefault();
        const m = text.trim();
        if (!m || pending) return;
        setPending(true);
        try {
          const r = await onSend(m);
          onResult?.({
            reply: String((r as any)?.reply ?? ""),
            toolResults: Array.isArray((r as any)?.toolResults) ? ((r as any).toolResults as MapToolResult[]) : [],
          });
          setText("");
        } catch (err) {
          onResult?.({ reply: String(err), toolResults: [] });
        } finally {
          setPending(false);
        }
      }}
    >
      <span className="map-smart-icon" aria-hidden>
        <Wand2 size={18} />
      </span>
      <input
        type="text"
        className="map-smart-input"
        placeholder={placeholder}
        value={text}
        onChange={(e) => setText(e.target.value)}
        disabled={pending}
      />
      <button type="submit" className="map-smart-submit" disabled={pending}>
        {pending ? "…" : "Run"}
      </button>
    </form>
  );
}

export type MapAssistantContextJson = {
  center: [number, number];
  zoom: number;
};

/**
 * @param contextJson - When set, backend merges territory spatial context (e.g. which named zones contain the map center).
 */
export async function mapAssistantSendOnInvoke(
  msg: string,
  contextJson?: MapAssistantContextJson | null,
) {
  return await invoke<{ reply: string; toolResults: MapToolResult[] }>("map_assistant_send", {
    // Backend command expects `userMessage` (camelCase).
    // Keep `user_message` as a compatibility fallback if older builds still read snake_case.
    userMessage: msg,
    user_message: msg,
    context_json: contextJson != null ? contextJson : null,
  });
}
