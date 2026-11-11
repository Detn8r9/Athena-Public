/**
 * Voice / Conversation debug logging for `tauri dev` terminal troubleshooting.
 *
 * Enable any of:
 * - localStorage `athena_voice_debug` = `1` | `true` | `on`
 * - Vite env `VITE_ATHENA_VOICE_DEBUG=1`
 * - Settings → AI → Conversation → Voice debug logs
 */

export const VOICE_DEBUG_STORAGE_KEY = "athena_voice_debug";

function readStoredFlag(): boolean | null {
  try {
    const v = localStorage.getItem(VOICE_DEBUG_STORAGE_KEY);
    if (v == null) return null;
    const t = v.trim().toLowerCase();
    if (t === "1" || t === "true" || t === "on" || t === "yes") return true;
    if (t === "0" || t === "false" || t === "off" || t === "no") return false;
  } catch {
    /* private mode */
  }
  return null;
}

function readEnvFlag(): boolean {
  try {
    const env = (import.meta as ImportMeta & { env?: Record<string, string | boolean | undefined> }).env;
    const v = env?.VITE_ATHENA_VOICE_DEBUG;
    if (v == null) return false;
    const t = String(v).trim().toLowerCase();
    return t === "1" || t === "true" || t === "on" || t === "yes";
  } catch {
    return false;
  }
}

/** True when voice pipeline should print diagnostic lines to the terminal/console. */
export function isVoiceDebugEnabled(): boolean {
  const stored = readStoredFlag();
  if (stored != null) return stored;
  return readEnvFlag();
}

export function setVoiceDebugEnabled(on: boolean): void {
  try {
    localStorage.setItem(VOICE_DEBUG_STORAGE_KEY, on ? "1" : "0");
  } catch {
    /* ignore */
  }
}

type VoiceDebugDetail = Record<string, unknown> | undefined;

/** Prefix makes logs easy to filter in the `tauri dev` terminal. */
export function voiceDebug(event: string, detail?: VoiceDebugDetail): void {
  if (!isVoiceDebugEnabled()) return;
  const ts = new Date().toISOString().slice(11, 23);
  if (detail && Object.keys(detail).length > 0) {
    console.info(`[athena:voice ${ts}] ${event}`, detail);
  } else {
    console.info(`[athena:voice ${ts}] ${event}`);
  }
}

/** Short preview for transcripts — never dump huge bodies. */
export function voiceDebugPreview(text: string, max = 80): string {
  const t = text.replace(/\s+/g, " ").trim();
  if (t.length <= max) return t;
  return `${t.slice(0, max)}…`;
}
