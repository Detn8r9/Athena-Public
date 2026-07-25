import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { disposeSharedMicPipeline, getSharedMicPipeline } from "./micPipeline";

export const WHISPER_MODULE_ID = "whisper-server";
export const KOKORO_MODULE_ID = "kokoro-tts";

export type DuplexMode = "duplex" | "stt_only" | "tts_only" | "browser";

export type DuplexCapability = {
  whisper: boolean;
  kokoro: boolean;
  whisperBaseUrl: string | null;
  kokoroBaseUrl: string | null;
  mode: DuplexMode;
  label: string;
};

export function duplexModeLabel(mode: DuplexMode): string {
  switch (mode) {
    case "duplex":
      return "Local duplex (Whisper + Kokoro)";
    case "stt_only":
      return "Local STT (Whisper) + browser TTS";
    case "tts_only":
      return "Browser STT + local TTS (Kokoro)";
    default:
      return "Browser speech";
  }
}

export async function probeDuplexCapability(): Promise<DuplexCapability> {
  let whisper = false;
  let kokoro = false;
  let whisperBaseUrl: string | null = null;
  let kokoroBaseUrl: string | null = null;

  try {
    whisper = await invoke<boolean>("marketplace_health", { id: WHISPER_MODULE_ID });
    if (whisper) {
      whisperBaseUrl = await invoke<string>("marketplace_open_url", { id: WHISPER_MODULE_ID });
    }
  } catch {
    whisper = false;
  }

  try {
    kokoro = await invoke<boolean>("marketplace_health", { id: KOKORO_MODULE_ID });
    if (kokoro) {
      kokoroBaseUrl = await invoke<string>("marketplace_open_url", { id: KOKORO_MODULE_ID });
    }
  } catch {
    kokoro = false;
  }

  let mode: DuplexMode = "browser";
  if (whisper && kokoro) mode = "duplex";
  else if (whisper) mode = "stt_only";
  else if (kokoro) mode = "tts_only";

  return {
    whisper,
    kokoro,
    whisperBaseUrl,
    kokoroBaseUrl,
    mode,
    label: duplexModeLabel(mode),
  };
}

function pickRecorderMime(): string | undefined {
  const candidates = [
    "audio/webm;codecs=opus",
    "audio/webm",
    "audio/ogg;codecs=opus",
    "audio/mp4",
  ];
  for (const c of candidates) {
    if (typeof MediaRecorder !== "undefined" && MediaRecorder.isTypeSupported(c)) return c;
  }
  return undefined;
}

export async function transcribeWithWhisper(
  baseUrl: string,
  blob: Blob,
  language = "en",
): Promise<string> {
  const root = baseUrl.replace(/\/$/, "");
  const form = new FormData();
  const ext = blob.type.includes("ogg") ? "ogg" : blob.type.includes("mp4") ? "m4a" : "webm";
  form.append("file", blob, `utterance.${ext}`);
  form.append("model", "whisper-1");
  form.append("language", language);
  form.append("response_format", "json");

  const res = await fetch(`${root}/v1/audio/transcriptions`, {
    method: "POST",
    body: form,
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`Whisper STT failed (${res.status}): ${body.slice(0, 200)}`);
  }
  const ct = res.headers.get("content-type") || "";
  if (ct.includes("application/json")) {
    const json = (await res.json()) as { text?: string };
    return (json.text ?? "").trim();
  }
  return (await res.text()).trim();
}

export async function synthesizeWithKokoro(
  baseUrl: string,
  text: string,
  voice = "af_bella",
): Promise<Blob> {
  const root = baseUrl.replace(/\/$/, "");
  const res = await fetch(`${root}/v1/audio/speech`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model: "kokoro",
      input: text,
      voice,
      response_format: "wav",
      stream: false,
      speed: 1.0,
    }),
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`Kokoro TTS failed (${res.status}): ${body.slice(0, 200)}`);
  }
  return await res.blob();
}

/** Fail fast so Conversation never hangs forever on a dead TTS module. */
export function withTimeout<T>(promise: Promise<T>, ms: number, label = "timeout"): Promise<T> {
  return new Promise((resolve, reject) => {
    const t = window.setTimeout(() => reject(new Error(label)), ms);
    promise.then(
      (v) => {
        window.clearTimeout(t);
        resolve(v);
      },
      (e) => {
        window.clearTimeout(t);
        reject(e);
      },
    );
  });
}

export type WhisperListenSession = {
  stop: () => void;
  setPaused: (paused: boolean) => void;
};

let globalListenSerial = 0;

/**
 * Continuous mic listen with VAD endpointing → Whisper.
 *
 * Pre-arms MediaRecorder as soon as the analyser graph is ready so the first
 * utterance is not clipped (previous path only started recording after speech
 * was already detected, which missed short first turns).
 */
export function startWhisperListen(opts: {
  stream: MediaStream;
  whisperBaseUrl: string;
  onInterim?: (hint: string) => void;
  onTranscript: (text: string) => void;
  onError?: (message: string) => void;
  onEnergy?: (rms: number) => void;
  isPaused?: () => boolean;
}): WhisperListenSession {
  const serial = ++globalListenSerial;
  let recorder: MediaRecorder | null = null;
  let chunks: Blob[] = [];
  let recording = false;
  let stopped = false;
  let busy = false;
  let pauseRequested = false;
  let hadSpeech = false;
  let rollingTimer: number | null = null;
  const mime = pickRecorderMime();
  const pipeline = getSharedMicPipeline(opts.stream);

  const clearRolling = () => {
    if (rollingTimer != null) {
      window.clearTimeout(rollingTimer);
      rollingTimer = null;
    }
  };

  const safeStopRecorder = () => {
    const rec = recorder;
    if (!rec) return;
    try {
      if (rec.state === "recording") {
        try {
          rec.requestData();
        } catch {
          /* older engines */
        }
      }
      if (rec.state === "recording" || rec.state === "paused") {
        rec.stop();
      }
    } catch {
      recording = false;
    }
  };

  const scheduleRollingRestart = () => {
    clearRolling();
    // Avoid unbounded silence buffers while waiting for the user to speak.
    rollingTimer = window.setTimeout(() => {
      rollingTimer = null;
      if (stopped || serial !== globalListenSerial) return;
      if (busy || pauseRequested || opts.isPaused?.()) return;
      if (!recording || hadSpeech) return;
      safeStopRecorder();
    }, 18_000);
  };

  const armRecorder = () => {
    if (stopped || serial !== globalListenSerial) return;
    if (recording || busy || pauseRequested || opts.isPaused?.()) return;
    if (!opts.stream.active || opts.stream.getAudioTracks().every((t) => t.readyState !== "live")) {
      opts.onError?.("Microphone stream ended. Re-enable Conversation.");
      return;
    }
    try {
      chunks = [];
      hadSpeech = false;
      const rec = mime
        ? new MediaRecorder(opts.stream, { mimeType: mime })
        : new MediaRecorder(opts.stream);
      recorder = rec;
      rec.ondataavailable = (e) => {
        if (e.data.size > 0) chunks.push(e.data);
      };
      rec.onerror = () => {
        recording = false;
        opts.onError?.("Microphone recording failed.");
      };
      rec.onstop = () => {
        void (async () => {
          if (stopped || serial !== globalListenSerial) return;
          const blob = new Blob(chunks, { type: rec.mimeType || mime || "audio/webm" });
          chunks = [];
          recording = false;
          recorder = null;
          clearRolling();

          const uttered = hadSpeech;
          hadSpeech = false;

          // Rolling silence discard — re-arm without Whisper.
          if (!uttered || blob.size < 600) {
            if (!stopped && !pauseRequested && !opts.isPaused?.() && !busy) {
              armRecorder();
            }
            return;
          }

          busy = true;
          opts.onInterim?.("Transcribing…");
          try {
            const text = await withTimeout(
              transcribeWithWhisper(opts.whisperBaseUrl, blob),
              45_000,
              "Whisper STT timed out",
            );
            if (stopped || serial !== globalListenSerial) return;
            if (text) {
              opts.onTranscript(text);
              return;
            }
            // Empty transcript — keep listening.
            opts.onInterim?.("Listening…");
            if (!pauseRequested && !opts.isPaused?.()) armRecorder();
          } catch (e) {
            if (!stopped && serial === globalListenSerial) {
              opts.onError?.(e instanceof Error ? e.message : String(e));
            }
          } finally {
            busy = false;
            if (!stopped && serial === globalListenSerial) {
              /* transcript path usually stops the session from App */
            }
          }
        })();
      };
      rec.start(200);
      recording = true;
      opts.onInterim?.("Listening…");
      scheduleRollingRestart();
    } catch (e) {
      recording = false;
      recorder = null;
      opts.onError?.(e instanceof Error ? e.message : String(e));
    }
  };

  const endUtterance = () => {
    if (!recording || !hadSpeech) return;
    clearRolling();
    safeStopRecorder();
  };

  let sub: { unsubscribe: () => void; setPaused: (u: boolean, b: boolean) => void } | null = null;

  void (async () => {
    opts.onInterim?.("Preparing mic…");
    try {
      await pipeline.ensureGraph();
    } catch {
      /* still try to arm */
    }
    if (stopped || serial !== globalListenSerial) return;
    sub = pipeline.subscribe({
      speechThreshold: 0.022,
      speechOnMs: 100,
      silenceOffMs: 850,
      bargeInMs: 999999,
      onSample: (s) => opts.onEnergy?.(s.rms),
      onSpeechStart: () => {
        hadSpeech = true;
        clearRolling();
        opts.onInterim?.("Listening…");
        if (!recording && !busy) armRecorder();
      },
      onSpeechEnd: () => endUtterance(),
    });
    armRecorder();
  })();

  return {
    setPaused: (paused: boolean) => {
      pauseRequested = paused;
      sub?.setPaused(paused, true);
      if (paused) {
        clearRolling();
        safeStopRecorder();
      } else if (!stopped && !busy && !recording) {
        armRecorder();
      }
    },
    stop: () => {
      stopped = true;
      clearRolling();
      sub?.unsubscribe();
      sub = null;
      safeStopRecorder();
      recorder = null;
      recording = false;
    },
  };
}

export function startBargeInMonitor(
  stream: MediaStream,
  onBargeIn: () => void,
): { stop: () => void } {
  const pipeline = getSharedMicPipeline(stream);
  let fired = false;
  const sub = pipeline.subscribe({
    speechThreshold: 0.04,
    speechOnMs: 999999,
    silenceOffMs: 999999,
    bargeInMs: 320,
    pauseUtterance: true,
    onBargeIn: () => {
      if (fired) return;
      fired = true;
      try {
        onBargeIn();
      } catch {
        /* ignore */
      }
    },
  });
  return {
    stop: () => sub.unsubscribe(),
  };
}

export function releaseMicPipeline(stream: MediaStream | null) {
  if (stream) disposeSharedMicPipeline(stream);
}
