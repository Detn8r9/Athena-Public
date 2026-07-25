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
): { temperature: number; num_ctx: number; promptPrefix: string } {
  if (mode === "deep") {
    return {
      temperature: Math.min(baseTemp, 0.45),
      num_ctx: Math.max(baseCtx, 8192),
      promptPrefix:
        "[Deep analysis mode] Take your time. Prefer thorough, technical, step-by-step answers with caveats when uncertain.\n\n",
    };
  }
  return {
    temperature: Math.max(baseTemp, 0.55),
    num_ctx: Math.min(baseCtx, 4096),
    promptPrefix:
      "[Quick response mode] Answer briefly and clearly. Prefer actionable steps over long essays.\n\n",
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
