/**
 * Conversation speech helpers — keep spoken turns short, clean, and natural.
 * Sentence buffering ported from nix-vox `SentenceBuffer` (streaming LLM→TTS).
 */

/** Strip markdown / noise so TTS doesn't read asterisks, links, or code. */
export function cleanTextForSpeech(raw: string): string {
  let t = raw.replace(/\r\n/g, "\n").trim();
  if (!t) return "";

  // Drop fenced code blocks entirely (don't speak source).
  t = t.replace(/```[\s\S]*?```/g, " ");
  // Inline code → plain
  t = t.replace(/`([^`]+)`/g, "$1");
  // Links: keep label
  t = t.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
  // Images
  t = t.replace(/!\[([^\]]*)\]\([^)]+\)/g, "$1");
  // Headings / bold / italic / strike
  t = t.replace(/^#{1,6}\s+/gm, "");
  t = t.replace(/(\*\*|__)(.*?)\1/g, "$2");
  t = t.replace(/(\*|_)(.*?)\1/g, "$2");
  t = t.replace(/~~(.*?)~~/g, "$1");
  // List markers
  t = t.replace(/^\s*[-*+]\s+/gm, "");
  t = t.replace(/^\s*\d+\.\s+/gm, "");
  // Collapse whitespace
  t = t.replace(/\n{2,}/g, ". ").replace(/\n/g, " ").replace(/\s{2,}/g, " ").trim();
  // Soften ellipsis / dashes for speech
  t = t.replace(/\s*[—–]\s*/g, ", ");
  return t;
}

/**
 * Pick a short spoken turn from assistant text (1–2 sentences).
 * Full reply can remain longer on screen.
 */
export function pickConversationalSpeakText(raw: string, maxChars = 200): string {
  const cleaned = cleanTextForSpeech(raw);
  if (!cleaned) return "";

  const sentences = cleaned.match(/[^.!?…]+[.!?…]+|[^.!?…]+$/g);
  if (!sentences || sentences.length === 0) {
    return cleaned.length > maxChars ? `${cleaned.slice(0, maxChars).trim()}…` : cleaned;
  }

  let out = sentences[0].trim();
  if (out.length < 36 && sentences[1]) {
    const two = `${out} ${sentences[1].trim()}`.trim();
    if (two.length <= maxChars) out = two;
  }
  if (out.length > maxChars) {
    out = `${out.slice(0, maxChars).trim()}…`;
  }
  return out;
}

/**
 * Whisper often hallucinates short fillers on silence / noise.
 * Return true when the transcript should be ignored (keep listening).
 * Keep real short answers ("yes", "okay", "no") — those are valid turns.
 */
export function isJunkTranscript(text: string): boolean {
  const s = text.trim();
  if (s.length < 1) return true;
  const lower = s
    .toLowerCase()
    .replace(/["']/g, "")
    .replace(/\s+/g, " ")
    .trim();
  // Common Whisper ghosts on silence / YouTube-trained leftovers
  if (
    /^(thank you for watching\.?|thanks for watching\.?|字幕|subtitle[s]?\.?|music\.?|\[\s*(music|silence|blank|applause)\s*\]|\.+|…+|♪+|�+|the end\.?|please subscribe\.?|thanks for listening\.?)$/i.test(
      lower,
    )
  ) {
    return true;
  }
  // Lone courtesy ghosts (very common on quiet clips with Distil-Whisper)
  const bare = lower.replace(/\.+$/, "");
  if (/^(thank you|thanks|you|bye|goodbye)$/i.test(bare)) return true;
  // Single filler sounds with no real words
  if (/^(um+|uh+|hmm+|mm+|ah+|oh+)$/i.test(bare)) return true;
  // Mostly non-letters
  const letters = (s.match(/[a-zA-Z]/g) ?? []).length;
  if (letters < 1) return true;
  return false;
}

/** Adaptive end-of-utterance silence: short replies cut faster; longer thoughts get a beat. */
export function adaptiveSilenceOffMs(speechDurationMs: number, snappy: boolean): number {
  if (!snappy) return 900;
  // Patient endpointing — mid-phrase pauses ("can you… hear me") must not split the turn.
  // Big-tech assistants wait ~0.8–1.2s; we stay in that band.
  if (speechDurationMs < 900) return 950;
  if (speechDurationMs < 2500) return 1050;
  if (speechDurationMs < 5500) return 1150;
  return 1250;
}

/**
 * First clause/sentence ready for early TTS while the model is still streaming.
 * Prefers terminal punctuation; falls back to a natural clause once long enough
 * so Conversation does not wait for a full period on slow models.
 *
 * Pass `hardOnly: true` for early duplex TTS so we never start speaking a soft
 * mid-thought clause that then "freezes" before the real sentence finishes.
 */
export function extractFirstSpeakableSentence(
  streamed: string,
  minChars = 6,
  maxChars = 200,
  opts?: { hardOnly?: boolean },
): string | null {
  const cleaned = cleanTextForSpeech(streamed);
  if (cleaned.length < minChars) return null;

  const hard = cleaned.match(/^(.+?[.!?…])(?:\s|$)/);
  if (hard) {
    const s = hard[1].trim();
    if (s.length >= minChars) {
      return s.length > maxChars ? `${s.slice(0, maxChars).trim()}…` : s;
    }
  }

  if (opts?.hardOnly) return null;

  // Soft clause once we have enough of a spoken beat (not just "Sure,").
  if (cleaned.length >= Math.max(minChars + 8, 28)) {
    const soft = cleaned.match(/^(.{4,160}?)([,;:—–])\s+/);
    if (soft) {
      const before = soft[1].trim();
      const punct = soft[2] === "," || soft[2] === ";" ? soft[2] : ".";
      // Substantial phrase before the break → speak that clause.
      if (before.length >= 12) {
        const s = `${before}${punct}`.trim();
        if (s.length >= minChars) {
          return s.length > maxChars ? `${s.slice(0, maxChars).trim()}…` : s;
        }
      }
      // Short opener ("Sure,") — extend into the following words until a pause or budget.
      const rest = cleaned.slice(soft[0].length);
      const extended = rest.match(/^(.{10,140}?)(?:[.!?…,;:—–]\s+|$)/);
      if (extended) {
        const s = `${before}${punct} ${extended[1].trim()}`.trim();
        if (s.length >= 24) {
          return s.length > maxChars ? `${s.slice(0, maxChars).trim()}…` : s;
        }
      }
    }
  }

  // Last resort for terse replies with no usable break yet.
  if (cleaned.length >= 48) {
    const slice = cleaned.slice(0, Math.min(maxChars, 120)).trim();
    const lastSpace = slice.lastIndexOf(" ");
    if (lastSpace > 24) return `${slice.slice(0, lastSpace)}…`;
  }

  return null;
}

/**
 * Streaming sentence splitter — ported from nix-vox `SentenceBuffer`.
 * Push LLM chunks; get complete sentences as soon as punctuation + whitespace
 * confirms a boundary (avoids cutting "Dr." / "Mrs.").
 */
export class SpokenSentenceBuffer {
  private buffer = "";
  private readonly wordFlushAt: number;

  constructor(wordFlushAt = 12) {
    this.wordFlushAt = Math.max(6, wordFlushAt);
  }

  /** Add a streamed token/chunk; returns newly completed sentences. */
  push(token: string): string[] {
    if (!token) return [];
    const completed: string[] = [];

    const startsWithWhitespace = /^\s/.test(token);
    if (startsWithWhitespace && this.buffer.length > 0) {
      const trimmed = this.buffer.trimEnd();
      const last = trimmed.slice(-1);
      if (last === "." || last === "!" || last === "?" || last === ";" || last === "…") {
        completed.push(trimmed);
        this.buffer = "";
      }
    }

    this.buffer += token;

    // Mid-buffer boundaries: "Hello. World"
    const chars = [...this.buffer];
    let lastSplit = 0;
    const midCompleted: string[] = [];
    for (let i = 0; i < chars.length; i++) {
      const ch = chars[i];
      if ((ch === "." || ch === "!" || ch === "?" || ch === ";" || ch === "…") && i + 1 < chars.length) {
        if (/\s/.test(chars[i + 1]!)) {
          const sentence = chars.slice(lastSplit, i + 1).join("").trim();
          if (sentence) midCompleted.push(sentence);
          lastSplit = i + 1;
        }
      }
    }
    if (lastSplit > 0) {
      completed.push(...midCompleted);
      this.buffer = chars.slice(lastSplit).join("").trimStart();
    }

    // Word-count fallback so long run-ons still start TTS.
    const words = this.buffer.trim().split(/\s+/).filter(Boolean);
    if (words.length >= this.wordFlushAt) {
      const flushed = this.buffer.trim();
      if (flushed) {
        completed.push(flushed);
        this.buffer = "";
      }
    }

    return completed.map((s) => s.trim()).filter(Boolean);
  }

  /** Leftover text after the stream ends (not already emitted). */
  flush(): string | null {
    const trimmed = this.buffer.trim();
    this.buffer = "";
    return trimmed.length > 0 ? trimmed : null;
  }

  reset() {
    this.buffer = "";
  }
}
