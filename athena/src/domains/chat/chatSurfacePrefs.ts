const STORAGE_KEY = "athena-chat-surface-default";
const THINK_KEY = "athena-conversation-think-mode";

/** Default surface when opening Chat view. */
export type ChatSurfaceDefault = "chat" | "conversation";

/** Quick reply vs longer technical reasoning. */
export type ConversationThinkMode = "quick" | "deep";

export type AthenaEmotion =
  | "neutral"
  | "happy"
  | "listening"
  | "thinking"
  | "speaking"
  | "concerned"
  | "surprised"
  | "curious";

export type UserEmotionHint = "calm" | "engaged" | "stressed" | "unknown";

export function readChatSurfaceDefault(): ChatSurfaceDefault {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "conversation" || v === "chat") return v;
  } catch {
    /* ignore */
  }
  return "chat";
}

export function saveChatSurfaceDefault(mode: ChatSurfaceDefault) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    /* ignore */
  }
}

export function readThinkMode(): ConversationThinkMode {
  try {
    const v = localStorage.getItem(THINK_KEY);
    if (v === "deep" || v === "quick") return v;
  } catch {
    /* ignore */
  }
  return "quick";
}

export function saveThinkMode(mode: ConversationThinkMode) {
  try {
    localStorage.setItem(THINK_KEY, mode);
  } catch {
    /* ignore */
  }
}

/** Map assistant text + phase into a Yes Man expression. */
export function emotionFromAssistant(
  phase: "idle" | "listening" | "thinking" | "speaking",
  text: string,
  userHint: UserEmotionHint,
): AthenaEmotion {
  if (phase === "listening") return "listening";
  if (phase === "thinking") return "thinking";
  if (phase === "speaking") {
    const t = text.toLowerCase();
    if (/sorry|careful|danger|warning|risk|injury|critical/.test(t)) return "concerned";
    if (/great|excellent|perfect|wonderful|yes[!]|absolutely/.test(t)) return "happy";
    if (/wait|really|wow|unexpected/.test(t)) return "surprised";
    if (userHint === "stressed") return "concerned";
    if (userHint === "engaged") return "curious";
    return "speaking";
  }
  if (userHint === "stressed") return "concerned";
  if (userHint === "engaged") return "curious";
  return "neutral";
}

export function thinkModeModelOptions(
  mode: ConversationThinkMode,
  baseTemp: number,
  baseCtx: number,
  surface: "chat" | "conversation" = "chat",
): { temperature: number; num_ctx: number; numPredict: number | null; promptPrefix: string } {
  const spoken = surface === "conversation";
  if (mode === "deep") {
    return {
      temperature: Math.min(baseTemp, 0.4),
      num_ctx: Math.min(Math.max(baseCtx, 6144), 8192),
      numPredict: spoken ? 720 : null,
      promptPrefix: spoken
        ? "[Deep voice mode] You are speaking with someone. Be thorough and technical when the question needs it: steps, caveats, edge cases. Stay organized and speakable (short paragraphs, not a wall of markdown). If a camera frame is attached, identify objects or the scene and use that to answer.\n\n"
        : "[Deep technical mode] Prefer thorough, precise, step-by-step answers with caveats when uncertain. Cover edge cases when relevant. Still stay organized and readable. If an image is attached, identify what is shown and use it.\n\n",
    };
  }
  return {
    temperature: Math.min(Math.max(baseTemp, 0.5), 0.65),
    num_ctx: Math.min(baseCtx, spoken ? 3072 : 4096),
    numPredict: spoken ? 220 : null,
    promptPrefix: spoken
      ? "[Quick voice mode] Live conversation. Be fast, natural, and still accurate — usually 2–4 spoken sentences with the useful facts, not a one-liner and not an essay. No markdown, no 'Sure' preamble. If a camera frame is attached, name what you see (person or object) only as it helps the question, then answer.\n\n"
      : "[Quick mode] Be concise and useful. Lead with the answer, then the key facts. Use lists only when they help. If an image is attached, identify it and answer from what you see.\n\n",
  };
}

/** Lightweight sentiment from user transcript. */
export function userEmotionFromTranscript(text: string, rms: number): UserEmotionHint {
  const t = text.toLowerCase();
  if (/help|scared|hurt|panic|emergency|urgent|afraid|stressed/.test(t) || rms > 0.12) {
    return "stressed";
  }
  if (/thanks|cool|nice|good|great|curious|how|why|tell me/.test(t)) return "engaged";
  if (text.trim().length > 0) return "calm";
  return "unknown";
}
