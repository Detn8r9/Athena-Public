/**
 * Main chat stream watchdog (mirrors App.tsx thresholds):
 * - 200s max wait for the first chunk (longer than the server's own 180s first-token limit, so the
 *   server's more specific error wins; a busy CPU-only station can take minutes to read a long prompt)
 * - 60s max idle after the first chunk
 */

export const FIRST_CHUNK_TIMEOUT_MS = 200_000;
export const IDLE_TIMEOUT_MS = 60_000;
export const FIRST_CHUNK_TIMEOUT_MESSAGE =
  "The AI took too long to start answering. The station may be busy; try again in a minute, or choose a smaller model in Settings › AI & Models.";
export const IDLE_TIMEOUT_MESSAGE = "The AI stopped partway through its answer. Try asking again.";

export type MainChatWatchdogResult =
  | { action: "none" }
  | { action: "stall_first_timeout"; message: string }
  | { action: "stall_idle"; message: string };

export function evaluateMainChatWatchdog(input: {
  nowMs: number;
  sending: boolean;
  sendStartMs: number;
  lastChunkAtMs: number | null;
  gotFirstChunk: boolean;
}): MainChatWatchdogResult {
  const { nowMs, sending, sendStartMs, lastChunkAtMs, gotFirstChunk } = input;
  if (!sending) {
    return { action: "none" };
  }
  if (!gotFirstChunk) {
    if (nowMs - sendStartMs > FIRST_CHUNK_TIMEOUT_MS) {
      return {
        action: "stall_first_timeout",
        message: FIRST_CHUNK_TIMEOUT_MESSAGE,
      };
    }
  } else if (lastChunkAtMs != null && nowMs - lastChunkAtMs > IDLE_TIMEOUT_MS) {
    return {
      action: "stall_idle",
      message: IDLE_TIMEOUT_MESSAGE,
    };
  }
  return { action: "none" };
}
