/**
 * Dexter-faithful push-to-talk for Athena Conversation.
 * Native cpal only — never hold getUserMedia while recording (macOS mic exclusivity).
 */

import { athenaInvoke as invoke, isAthenaDesktopRuntime } from "../../shared/ipc/athenaIpc";
import { voiceDebug, voiceDebugPreview } from "./voiceDebug";

export type PushToTalkSession = {
  start: () => Promise<void>;
  /** Stop + Whisper in Rust. Returns transcript text (may be empty). */
  stopAndTranscribe: (whisperBaseUrl: string, model?: string | null) => Promise<PttTranscript>;
  cancel: () => void;
  isRecording: () => boolean;
};

export type PttTranscript = {
  text: string;
  durationMs: number;
  sampleRate: number;
  peak: number;
  samples: number;
};

export function createNativePushToTalk(): PushToTalkSession {
  if (!isAthenaDesktopRuntime()) {
    throw new Error("Native push-to-talk requires the Athena desktop app.");
  }

  let recording = false;

  return {
    async start() {
      if (recording) return;
      await invoke("start_ptt_recording");
      recording = true;
      voiceDebug("ptt.start", { via: "native_cpal" });
    },

    async stopAndTranscribe(whisperBaseUrl: string, model?: string | null) {
      if (!recording) {
        return { text: "", durationMs: 0, sampleRate: 16000, peak: 0, samples: 0 };
      }
      recording = false;
      const result = await invoke<PttTranscript>("stop_ptt_and_transcribe", {
        whisperBaseUrl,
        model: model ?? null,
      });
      voiceDebug("ptt.native_transcript", {
        durationMs: result.durationMs,
        peak: Math.round(result.peak * 10000) / 10000,
        samples: result.samples,
        preview: voiceDebugPreview(result.text || "(empty)"),
      });
      return result;
    },

    cancel() {
      recording = false;
      void invoke("cancel_ptt_recording").catch(() => {});
    },

    isRecording: () => recording,
  };
}
