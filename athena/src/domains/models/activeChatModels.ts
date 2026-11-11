/**
 * Unified chat-model options for Settings / routing: Ollama tags first, then on-disk GGUFs.
 */

export type ChatModelOption = {
  /** Value stored / passed to `set_explicit_ollama_model` (tag or `.gguf` basename). */
  value: string;
  label: string;
  source: "ollama" | "gguf";
};

function trimName(raw: unknown): string {
  return typeof raw === "string" ? raw.trim() : "";
}

/** Build selectable chat models: prefer Ollama tags; append GGUF files not already covered by a tag. */
export function buildChatModelOptions(
  ollamaTags: readonly string[],
  ggufFilenames: readonly string[],
): ChatModelOption[] {
  const seen = new Set<string>();
  const out: ChatModelOption[] = [];

  for (const raw of ollamaTags) {
    const t = trimName(raw);
    if (!t || seen.has(t)) continue;
    seen.add(t);
    out.push({ value: t, label: t, source: "ollama" });
  }

  for (const raw of ggufFilenames) {
    const t = trimName(raw);
    if (!t) continue;
    const stem = t.toLowerCase().endsWith(".gguf") ? t.slice(0, -5) : t;
    if (seen.has(t) || (stem.length > 0 && seen.has(stem))) continue;
    seen.add(t);
    if (stem) seen.add(stem);
    out.push({ value: t, label: `${t} (local file)`, source: "gguf" });
  }

  return out;
}

/** Values for routing selects: Auto first, then every installed chat model id. */
export function chatModelIdsForRouting(options: readonly ChatModelOption[]): string[] {
  return options.map((o) => o.value);
}

/** Strip `.gguf` when persisting an explicit Ollama / engine model id. */
export function explicitModelValueFromSelection(value: string): string | null {
  const v = value.trim();
  if (!v) return null;
  if (v.toLowerCase().endsWith(".gguf")) {
    const stem = v.slice(0, -5).trim();
    return stem.length > 0 ? stem : null;
  }
  return v;
}

/** Whether a routing / selection value refers to an installed model (or Auto). */
export function isInstalledChatModelId(
  value: string,
  options: readonly ChatModelOption[],
  autoValue = "auto",
): boolean {
  const v = value.trim();
  if (!v || v === autoValue) return true;
  if (options.some((o) => o.value === v)) return true;
  const lower = v.toLowerCase();
  return options.some((o) => {
    if (o.value.toLowerCase() === lower) return true;
    if (o.source === "gguf") {
      const stem = o.value.toLowerCase().endsWith(".gguf") ? o.value.slice(0, -5) : o.value;
      return stem.toLowerCase() === lower;
    }
    return false;
  });
}
