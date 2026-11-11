import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { disposeSharedMicPipeline, getSharedMicPipeline } from "./micPipeline";
import { adaptiveSilenceOffMs, isJunkTranscript } from "./conversationSpeech";
import { blobToWav16kMono, padWav16kMonoToMinMs } from "./audioWav";
import { voiceDebug, voiceDebugPreview } from "./voiceDebug";

export const WHISPER_MODULE_ID = "whisper-server";
export const KOKORO_MODULE_ID = "kokoro-tts";

/**
 * Offline duplex SOTA (2026) for Athena Conversation:
 * - STT: Distil-Whisper Large V3 via faster-whisper (near Large-V3 WER, ~6× faster; English-first)
 * - TTS: Kokoro-82M (voice selectable in Settings; default af_heart)
 * - LLM: local Ollama (already Athena’s brain)
 *
 * Absolute ASR leaderboard winners (Canary-Qwen, Granite 8B) need heavier GPU stacks and
 * are a poor fit for always-on laptop duplex. Parakeet is faster but weaker on conversational WER.
 */
export const ATHENA_OFFLINE_STT_MODEL = "Systran/faster-distil-whisper-large-v3";
/** OpenAI-compatible alias used by older Whisper containers / fallback. */
export const ATHENA_OFFLINE_STT_FALLBACK_MODEL = "whisper-1";
/** Prefer this when Distil is not yet listed — already common on Athena installs. */
export const ATHENA_OFFLINE_STT_FAST_MODEL = "Systran/faster-whisper-small";
export const ATHENA_OFFLINE_TTS_VOICE = "af_heart";
export const KOKORO_VOICE_STORAGE_KEY = "athena-kokoro-voice";

let cachedWhisperModel: { baseUrl: string; model: string; at: number } | null = null;

export function clearWhisperModelCache() {
  cachedWhisperModel = null;
}

/** Pick a model the running Whisper server already has (avoids multi-second cold loads). */
export async function resolveWhisperModel(baseUrl: string): Promise<string> {
  const root = baseUrl.replace(/\/$/, "");
  const now = Date.now();
  if (cachedWhisperModel && cachedWhisperModel.baseUrl === root && now - cachedWhisperModel.at < 60_000) {
    return cachedWhisperModel.model;
  }
  try {
    const res = await fetch(`${root}/v1/models`);
    if (res.ok) {
      const json = (await res.json()) as { data?: Array<{ id?: string }> };
      const ids = (json.data ?? []).map((d) => String(d.id ?? "").trim()).filter(Boolean);
      const preferred = [
        "Systran/faster-whisper-tiny.en",
        "Systran/faster-whisper-base.en",
        "Systran/faster-whisper-base",
        "Systran/faster-whisper-small.en",
        ATHENA_OFFLINE_STT_FAST_MODEL,
        ATHENA_OFFLINE_STT_MODEL,
      ];
      const pick = preferred.find((p) => ids.includes(p)) ?? ids[0] ?? ATHENA_OFFLINE_STT_MODEL;
      cachedWhisperModel = { baseUrl: root, model: pick, at: now };
      voiceDebug("stt.model_resolved", { model: pick, available: ids.slice(0, 6) });
      return pick;
    }
  } catch (e) {
    voiceDebug("stt.model_resolve_fail", {
      error: e instanceof Error ? e.message : String(e),
    });
  }
  return ATHENA_OFFLINE_STT_MODEL;
}

/**
 * Preload Whisper weights so the first spoken turn is not a 10–20s stall.
 * Safe to call when entering Conversation.
 */
export async function warmWhisper(baseUrl: string): Promise<void> {
  const root = baseUrl.replace(/\/$/, "");
  const model = await resolveWhisperModel(root);
  // 1.05s of near-silence at 16 kHz mono PCM WAV (44-byte header + samples).
  const sr = 16_000;
  const n = Math.ceil(sr * 1.05);
  const buf = new ArrayBuffer(44 + n * 2);
  const view = new DataView(buf);
  const writeStr = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(o + i, s.charCodeAt(i));
  };
  writeStr(0, "RIFF");
  view.setUint32(4, 36 + n * 2, true);
  writeStr(8, "WAVE");
  writeStr(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sr, true);
  view.setUint32(28, sr * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeStr(36, "data");
  view.setUint32(40, n * 2, true);
  // Tiny non-zero samples so servers that reject pure digital silence still accept the warm-up.
  for (let i = 0; i < n; i++) view.setInt16(44 + i * 2, i % 64 === 0 ? 40 : 0, true);
  const blob = new Blob([buf], { type: "audio/wav" });
  const t0 = performance.now();
  try {
    await withTimeout(transcribeWithWhisper(root, blob, "en", model), 45_000, "Whisper warm timed out");
    voiceDebug("stt.warm_ok", { model, ms: Math.round(performance.now() - t0) });
  } catch (e) {
    voiceDebug("stt.warm_fail", {
      model,
      ms: Math.round(performance.now() - t0),
      error: e instanceof Error ? e.message : String(e),
    });
  }
}

export type KokoroVoiceInfo = { id: string; name: string };

export function getPreferredKokoroVoice(): string {
  try {
    const v = window.localStorage.getItem(KOKORO_VOICE_STORAGE_KEY)?.trim();
    if (v) return v;
  } catch {
    /* ignore */
  }
  return ATHENA_OFFLINE_TTS_VOICE;
}

export function setPreferredKokoroVoice(voiceId: string): void {
  const id = voiceId.trim() || ATHENA_OFFLINE_TTS_VOICE;
  try {
    window.localStorage.setItem(KOKORO_VOICE_STORAGE_KEY, id);
  } catch {
    /* ignore */
  }
}

/** List voices from a healthy Kokoro OpenAI-compatible server. */
export async function listKokoroVoices(baseUrl: string): Promise<KokoroVoiceInfo[]> {
  const root = baseUrl.replace(/\/$/, "");
  const res = await fetch(`${root}/v1/audio/voices`);
  if (!res.ok) {
    throw new Error(`Kokoro voices failed (${res.status})`);
  }
  const json = (await res.json()) as {
    voices?: Array<{ id?: string; name?: string } | string>;
  };
  const raw = json.voices ?? [];
  const out: KokoroVoiceInfo[] = [];
  const seen = new Set<string>();
  for (const v of raw) {
    const id = typeof v === "string" ? v : String(v.id ?? v.name ?? "").trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    const name = typeof v === "string" ? v : String(v.name ?? v.id ?? id);
    out.push({ id, name });
  }
  out.sort((a, b) => a.name.localeCompare(b.name));
  return out;
}

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
      return "Hold to talk · Distil-Whisper + Kokoro";
    case "stt_only":
      return "Hold to talk · Distil-Whisper + browser TTS";
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
  model?: string,
): Promise<string> {
  const root = baseUrl.replace(/\/$/, "");
  const modelId = model?.trim() || (await resolveWhisperModel(root));

  // Normalize to WAV so Distil-Whisper never gets a half-flushed webm/mp4 container.
  let upload = blob;
  let filename = "utterance.webm";
  const t0 = performance.now();
  try {
    upload = await blobToWav16kMono(blob);
    upload = await padWav16kMonoToMinMs(upload, 1000);
    filename = "utterance.wav";
    voiceDebug("stt.wav_ready", {
      srcBytes: blob.size,
      wavBytes: upload.size,
      srcType: blob.type || "(none)",
      ms: Math.round(performance.now() - t0),
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    voiceDebug("stt.wav_failed", { srcBytes: blob.size, srcType: blob.type || "(none)", error: msg });
    if (/too quiet|too short/i.test(msg)) throw e;
    // Fall through with original blob if decode fails (rare).
    const ext = blob.type.includes("ogg") ? "ogg" : blob.type.includes("mp4") ? "m4a" : "webm";
    filename = `utterance.${ext}`;
  }

  const attempt = async (id: string) => {
    voiceDebug("stt.request", { model: id, file: filename, bytes: upload.size });
    const reqStart = performance.now();
    const form = new FormData();
    form.append("file", upload, filename);
    form.append("model", id);
    form.append("language", language);
    form.append("response_format", "json");
    form.append("temperature", "0");
    // Speaches / faster-whisper-server: avoid SSE streaming bodies in Tauri fetch.
    form.append("stream", "false");

    const res = await fetch(`${root}/v1/audio/transcriptions`, {
      method: "POST",
      body: form,
    });
    if (!res.ok) {
      const body = await res.text().catch(() => "");
      voiceDebug("stt.http_error", {
        model: id,
        status: res.status,
        ms: Math.round(performance.now() - reqStart),
        body: body.slice(0, 160),
      });
      throw new Error(`Whisper STT failed (${res.status}): ${body.slice(0, 200)}`);
    }
    const ct = res.headers.get("content-type") || "";
    let text = "";
    if (ct.includes("application/json") || ct.includes("+json")) {
      const json = (await res.json()) as { text?: string; transcription?: string };
      text = String(json.text ?? json.transcription ?? "").trim();
    } else {
      const raw = (await res.text()).trim();
      // Some builds return bare text even when json was requested.
      if (raw.startsWith("{")) {
        try {
          const json = JSON.parse(raw) as { text?: string };
          text = String(json.text ?? "").trim();
        } catch {
          text = raw;
        }
      } else {
        text = raw;
      }
    }
    voiceDebug("stt.response", {
      model: id,
      ms: Math.round(performance.now() - reqStart),
      chars: text.length,
      preview: text ? voiceDebugPreview(text) : "(empty)",
    });
    return text;
  };

  try {
    return await attempt(modelId);
  } catch (e) {
    if (modelId !== ATHENA_OFFLINE_STT_FALLBACK_MODEL) {
      try {
        return await attempt(ATHENA_OFFLINE_STT_FALLBACK_MODEL);
      } catch {
        /* surface original */
      }
    }
    throw e;
  }
}

export async function synthesizeWithKokoro(
  baseUrl: string,
  text: string,
  voice = ATHENA_OFFLINE_TTS_VOICE,
  speed = 1.0,
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
      speed: Math.min(1.25, Math.max(0.8, speed)),
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
 * Warms AudioContext + MediaRecorder (prime chunk) before accepting speech
 * so the first spoken phrase is captured without a second take.
 * Prefer pause/resume across turns — tearing down MediaRecorder every turn
 * is unreliable on WebKit and adds hundreds of ms of prime delay.
 */
export function startWhisperListen(opts: {
  stream: MediaStream;
  whisperBaseUrl: string;
  onInterim?: (hint: string) => void;
  onTranscript: (text: string) => void;
  onError?: (message: string) => void;
  onEnergy?: (rms: number) => void;
  isPaused?: () => boolean;
  /** Skip the long first-prime wait when the mic pipeline is already warm. */
  fastPrime?: boolean;
  /** Quicker endpointing for live conversation turn-taking. */
  snappy?: boolean;
}): WhisperListenSession {
  const serial = ++globalListenSerial;
  let recorder: MediaRecorder | null = null;
  let chunks: Blob[] = [];
  let recording = false;
  let stopped = false;
  let busy = false;
  let pauseRequested = false;
  let resumeWhenIdle = false;
  let hadSpeech = false;
  let primed = false;
  let rollingTimer: number | null = null;
  let speechStartedAt = 0;
  let currentSilenceOffMs = opts.snappy ? 1000 : 900;
  /** STT in flight — recorder may still be armed to catch the next turn. */
  let sttInFlight = false;
  const pendingSttBlobs: Blob[] = [];
  const mime = pickRecorderMime();
  const pipeline = getSharedMicPipeline(opts.stream);
  const speechOnMs = opts.snappy ? 28 : 45;
  const primeCapMs = opts.fastPrime ? 70 : 700;
  const timesliceMs = opts.snappy ? 60 : 120;
  const forcePrimeMs = opts.fastPrime ? 40 : 280;

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
    rollingTimer = window.setTimeout(() => {
      rollingTimer = null;
      if (stopped || serial !== globalListenSerial) return;
      if (busy || pauseRequested || opts.isPaused?.()) return;
      if (!recording || hadSpeech) return;
      // Soft-rotate silence buffer without dropping the primed recorder state.
      safeStopRecorder();
    }, 18_000);
  };

  const armRecorder = (onPrimed?: () => void) => {
    if (stopped || serial !== globalListenSerial) return;
    // Allow arming while STT runs so repeats during "Transcribing…" are not lost.
    if (recording || pauseRequested || opts.isPaused?.()) return;
    if (!opts.stream.active || opts.stream.getAudioTracks().every((t) => t.readyState !== "live")) {
      opts.onError?.("Microphone stream ended. Re-enable Conversation.");
      return;
    }
    try {
      chunks = [];
      hadSpeech = false;
      const wasPrimed = primed;
      const rec = mime
        ? new MediaRecorder(opts.stream, { mimeType: mime })
        : new MediaRecorder(opts.stream);
      recorder = rec;
      rec.ondataavailable = (e) => {
        if (e.data.size > 0) {
          chunks.push(e.data);
          if (!primed) {
            primed = true;
            onPrimed?.();
          }
        }
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
          const speechMs = speechStartedAt > 0 ? performance.now() - speechStartedAt : 0;
          hadSpeech = false;
          speechStartedAt = 0;

          const rearm = () => {
            if (stopped || pauseRequested || opts.isPaused?.() || recording) return;
            armRecorder();
          };

          // While Athena thinks/speaks we pause the mic — never STT that stop (echo / half-clip).
          if (pauseRequested) {
            voiceDebug("utterance.discard", {
              reason: "paused",
              blobBytes: blob.size,
              speechMs: Math.round(speechMs),
            });
            return;
          }

          if (!uttered || blob.size < 400 || speechMs < 100) {
            voiceDebug("utterance.skip", {
              uttered,
              blobBytes: blob.size,
              speechMs: Math.round(speechMs),
              reason: !uttered ? "no_speech" : blob.size < 400 ? "blob_tiny" : "speech_too_short",
            });
            if (!stopped && !pauseRequested && !opts.isPaused?.()) {
              rearm();
            }
            return;
          }

          // Capture next speech while this clip is transcribed (fixes lost repeats).
          rearm();
          pendingSttBlobs.push(blob);
          voiceDebug("utterance.end", {
            blobBytes: blob.size,
            mime: blob.type || mime || "(unknown)",
            speechMs: Math.round(speechMs),
            queued: pendingSttBlobs.length,
            sttBusy: sttInFlight,
          });
          if (sttInFlight) return;

          sttInFlight = true;
          opts.onInterim?.("Transcribing…");
          try {
            while (pendingSttBlobs.length > 0) {
              if (stopped || serial !== globalListenSerial || pauseRequested) {
                pendingSttBlobs.length = 0;
                break;
              }
              const nextBlob = pendingSttBlobs.shift()!;
              const sttStart = performance.now();
              let text = "";
              try {
                text = await withTimeout(
                  transcribeWithWhisper(opts.whisperBaseUrl, nextBlob),
                  opts.snappy ? 18_000 : 45_000,
                  "Whisper STT timed out",
                );
              } catch (e) {
                if (!stopped && serial === globalListenSerial) {
                  const msg = e instanceof Error ? e.message : String(e);
                  voiceDebug("stt.error", { error: msg });
                  if (/too quiet|too short/i.test(msg)) {
                    opts.onInterim?.("Too quiet — speak closer");
                    window.setTimeout(() => {
                      if (!stopped && serial === globalListenSerial && !pauseRequested) {
                        opts.onInterim?.("Listening…");
                      }
                    }, 1100);
                  } else {
                    opts.onError?.(msg);
                  }
                }
                continue;
              }
              if (stopped || serial !== globalListenSerial || pauseRequested) break;
              if (text && !isJunkTranscript(text)) {
                voiceDebug("transcript.ok", {
                  ms: Math.round(performance.now() - sttStart),
                  chars: text.length,
                  preview: voiceDebugPreview(text),
                });
                // Hand off — App pauses listen for think/speak.
                pendingSttBlobs.length = 0;
                opts.onTranscript(text);
                break;
              }
              voiceDebug("transcript.rejected", {
                ms: Math.round(performance.now() - sttStart),
                reason: text ? "junk" : "empty",
                preview: text ? voiceDebugPreview(text) : "(empty)",
              });
              opts.onInterim?.("Didn't catch that — try again");
              window.setTimeout(() => {
                if (!stopped && serial === globalListenSerial && !pauseRequested) {
                  opts.onInterim?.("Listening…");
                }
              }, 900);
            }
          } finally {
            sttInFlight = false;
            busy = false;
            if (resumeWhenIdle && !stopped && !pauseRequested && !opts.isPaused?.() && !recording) {
              resumeWhenIdle = false;
              rearm();
            } else if (!stopped && !pauseRequested && !opts.isPaused?.() && !recording) {
              rearm();
            }
          }
        })();
      };
      rec.start(timesliceMs);
      recording = true;
      // Keep prior primed flag across soft restarts so we don't re-wait every rotate.
      if (wasPrimed) primed = true;
      scheduleRollingRestart();

      // Force an early timeslice so the container header is written before speech.
      window.setTimeout(() => {
        try {
          if (rec.state === "recording") rec.requestData();
        } catch {
          /* ignore */
        }
        if (!primed) {
          primed = true;
          onPrimed?.();
        }
      }, forcePrimeMs);
    } catch (e) {
      recording = false;
      recorder = null;
      opts.onError?.(e instanceof Error ? e.message : String(e));
    }
  };

  const endUtterance = () => {
    if (!recording || !hadSpeech) return;
    clearRolling();
    // Flush the last timeslice, then hang so trailing phonemes land in the blob.
    try {
      if (recorder?.state === "recording") recorder.requestData();
    } catch {
      /* ignore */
    }
    window.setTimeout(() => {
      if (stopped || serial !== globalListenSerial) return;
      if (!recording || !hadSpeech) return;
      safeStopRecorder();
    }, opts.snappy ? 320 : 280);
  };

  let sub: {
    unsubscribe: () => void;
    setPaused: (u: boolean, b: boolean) => void;
    setSilenceOffMs: (ms: number) => void;
  } | null = null;

  void (async () => {
    opts.onInterim?.("Preparing mic…");
    try {
      await pipeline.ensureGraph();
    } catch {
      /* still try to arm */
    }
    if (stopped || serial !== globalListenSerial) return;

    await new Promise<void>((resolve) => {
      let done = false;
      const finish = () => {
        if (done) return;
        done = true;
        resolve();
      };
      armRecorder(finish);
      window.setTimeout(finish, primeCapMs);
    });

    if (stopped || serial !== globalListenSerial) return;

    sub = pipeline.subscribe({
      speechThreshold: opts.snappy ? 0.01 : 0.014,
      speechOnMs,
      // Base value; duplexVoice updates silenceOffMs as speech duration grows.
      silenceOffMs: currentSilenceOffMs,
      bargeInMs: 999999,
      onSample: (s) => {
        opts.onEnergy?.(s.rms);
        // Stretch endpointing while the user is mid-thought.
        if (hadSpeech && speechStartedAt > 0 && sub) {
          const dur = performance.now() - speechStartedAt;
          const next = adaptiveSilenceOffMs(dur, Boolean(opts.snappy));
          if (next !== currentSilenceOffMs) {
            currentSilenceOffMs = next;
            sub.setSilenceOffMs(next);
          }
        }
      },
      onSpeechStart: () => {
        // Do not require primed — missing that droped the first words of many turns.
        if (!recording && !pauseRequested && !opts.isPaused?.()) {
          armRecorder();
        }
        hadSpeech = true;
        if (!speechStartedAt) speechStartedAt = performance.now();
        currentSilenceOffMs = adaptiveSilenceOffMs(0, Boolean(opts.snappy));
        sub?.setSilenceOffMs(currentSilenceOffMs);
        clearRolling();
        voiceDebug("speech.start", { silenceOffMs: currentSilenceOffMs, primed });
        opts.onInterim?.("Listening…");
      },
      onSpeechEnd: () => {
        const dur = speechStartedAt > 0 ? Math.round(performance.now() - speechStartedAt) : 0;
        voiceDebug("speech.end", { speechMs: dur });
        endUtterance();
      },
    });
    opts.onInterim?.("Listening…");
    voiceDebug("listen.armed", {
      snappy: Boolean(opts.snappy),
      fastPrime: Boolean(opts.fastPrime),
      mime: mime || "(default)",
      whisperBaseUrl: opts.whisperBaseUrl,
    });
  })();

  return {
    setPaused: (paused: boolean) => {
      pauseRequested = paused;
      voiceDebug(paused ? "listen.pause" : "listen.resume", {
        busy: sttInFlight,
        recording,
        pending: pendingSttBlobs.length,
      });
      sub?.setPaused(paused, true);
      if (paused) {
        resumeWhenIdle = false;
        // Clear speech flags before stop so onstop does not launch a junk STT job.
        hadSpeech = false;
        speechStartedAt = 0;
        pendingSttBlobs.length = 0;
        clearRolling();
        safeStopRecorder();
      } else if (!stopped) {
        busy = false;
        resumeWhenIdle = false;
        if (!recording) {
          // Brief yield so WebKit finishes the previous MediaRecorder stop before re-arm.
          window.setTimeout(() => {
            if (stopped || pauseRequested || recording || serial !== globalListenSerial) return;
            armRecorder();
          }, opts.snappy ? 28 : 40);
        }
      }
    },
    stop: () => {
      stopped = true;
      resumeWhenIdle = false;
      speechStartedAt = 0;
      pendingSttBlobs.length = 0;
      clearRolling();
      sub?.unsubscribe();
      sub = null;
      safeStopRecorder();
      recorder = null;
      recording = false;
    },
  };
}

/**
 * Barge-in while Athena is speaking.
 * Ported from voice2 InterruptDetectorWorker: track a slow RMS baseline of speaker
 * bleed, and only interrupt when mic energy exceeds max(2×baseline, absolute floor)
 * for several consecutive frames — so laptop speakers don't false-trigger.
 */
export function startBargeInMonitor(
  stream: MediaStream,
  onBargeIn: () => void,
): { stop: () => void } {
  const pipeline = getSharedMicPipeline(stream);
  let fired = false;
  let baselineRms = 0.001;
  let consecutive = 0;
  let lastFireAt = 0;
  const ENERGY_MULT = 2.0;
  const ABS_FLOOR = 0.06;
  const FRAMES_REQUIRED = 8; // ~128ms at ~60fps analyser ticks
  const REFRACTORY_MS = 800;

  const sub = pipeline.subscribe({
    speechThreshold: 99,
    speechOnMs: 999999,
    silenceOffMs: 999999,
    bargeInMs: 999999,
    pauseUtterance: true,
    onSample: (s) => {
      if (fired) return;
      const rms = s.rms;
      // Slow baseline = speaker bleed / ambient while Athena talks
      baselineRms = 0.995 * baselineRms + 0.005 * rms;
      const threshold = Math.max(baselineRms * ENERGY_MULT, ABS_FLOOR);
      if (rms > threshold) consecutive += 1;
      else consecutive = 0;
      if (consecutive < FRAMES_REQUIRED) return;
      const now = performance.now();
      if (now - lastFireAt < REFRACTORY_MS) {
        consecutive = 0;
        return;
      }
      lastFireAt = now;
      consecutive = 0;
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

/** Warm shared analyser early so Conversation's first listen is instant. */
export async function warmMicPipeline(stream: MediaStream): Promise<void> {
  try {
    await getSharedMicPipeline(stream).ensureGraph();
  } catch {
    /* ignore */
  }
}
