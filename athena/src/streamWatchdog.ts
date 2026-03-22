/**
 * Main chat stream watchdog (mirrors App.tsx thresholds):
 * - 120s max wait for the first chunk
 * - 30s max idle after the first chunk
 */

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
    if (nowMs - sendStartMs > 120_000) {
      return {
        action: "stall_first_timeout",
        message: "The local AI did not respond in time (120s).",
      };
    }
  } else if (lastChunkAtMs != null && nowMs - lastChunkAtMs > 30_000) {
    return {
      action: "stall_idle",
      message: "The local AI stalled (no output for 30s).",
    };
  }
  return { action: "none" };
}
