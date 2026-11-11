/** Shared mic graph — one AudioContext per stream to avoid crash on re-listen. */

export type MicEnergySample = {
  rms: number;
  speaking: boolean;
};

type Subscriber = {
  id: number;
  onSample?: (s: MicEnergySample) => void;
  speechThreshold: number;
  speechOnMs: number;
  silenceOffMs: number;
  bargeInMs: number;
  onSpeechStart?: () => void;
  onSpeechEnd?: () => void;
  onBargeIn?: () => void;
  /** When true, utterance detection is ignored (still samples for barge-in if set). */
  pauseUtterance: boolean;
  /** When true, barge-in ignored. */
  pauseBargeIn: boolean;
};

let nextSubId = 1;

/**
 * Single analyser pipeline for a MediaStream. Multiple subscribers can listen
 * without creating extra AudioContexts (which crashed Conversation on 2nd turn).
 */
export class SharedMicPipeline {
  private stream: MediaStream;
  private ctx: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private analyser: AnalyserNode | null = null;
  private raf = 0;
  private lastTs = 0;
  private subs = new Map<number, Subscriber>();
  private speechAccum = new Map<number, number>();
  private silenceAccum = new Map<number, number>();
  private inSpeech = new Map<number, boolean>();
  private bargeAccum = new Map<number, number>();
  /** Push-to-talk PCM capture (ScriptProcessor — continuous samples, not analyser snapshots). */
  private captureProc: ScriptProcessorNode | null = null;
  private captureMute: GainNode | null = null;
  private captureChunks: Float32Array[] = [];
  private capturing = false;

  constructor(stream: MediaStream) {
    this.stream = stream;
  }

  async ensureGraph(): Promise<void> {
    if (this.analyser && this.ctx) {
      if (this.ctx.state === "suspended") {
        try {
          await this.ctx.resume();
        } catch {
          /* ignore */
        }
      }
      return;
    }
    const AC =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    this.ctx = new AC();
    this.source = this.ctx.createMediaStreamSource(this.stream);
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = 2048;
    this.analyser.smoothingTimeConstant = 0.45;
    this.source.connect(this.analyser);
    this.lastTs = performance.now();
    const tick = () => {
      this.raf = requestAnimationFrame(tick);
      this.sample();
    };
    this.raf = requestAnimationFrame(tick);
    try {
      await this.ctx.resume();
    } catch {
      /* ignore */
    }
  }

  /**
   * Begin capturing continuous mono PCM for push-to-talk.
   * Safer than MediaRecorder in WKWebView (which often yields silent/junk Whisper ghosts).
   */
  async startPcmCapture(): Promise<void> {
    await this.ensureGraph();
    if (!this.ctx || !this.source) {
      throw new Error("Microphone audio graph failed to start.");
    }
    this.captureChunks = [];
    this.capturing = true;
    if (!this.captureProc) {
      // 4096 ≈ 85ms @ 48kHz — good balance of latency vs callback overhead.
      this.captureProc = this.ctx.createScriptProcessor(4096, 1, 1);
      this.captureMute = this.ctx.createGain();
      this.captureMute.gain.value = 0;
      this.captureProc.onaudioprocess = (e) => {
        if (!this.capturing) return;
        const input = e.inputBuffer.getChannelData(0);
        this.captureChunks.push(new Float32Array(input));
      };
      this.source.connect(this.captureProc);
      this.captureProc.connect(this.captureMute);
      // Destination connection required on some engines for onaudioprocess to fire.
      this.captureMute.connect(this.ctx.destination);
    }
    if (this.ctx.state === "suspended") {
      try {
        await this.ctx.resume();
      } catch {
        /* ignore */
      }
    }
  }

  /** Stop PCM capture and return merged samples at the AudioContext sample rate. */
  stopPcmCapture(): { pcm: Float32Array; sampleRate: number } | null {
    this.capturing = false;
    const chunks = this.captureChunks;
    this.captureChunks = [];
    const rate = this.ctx?.sampleRate ?? 48_000;
    if (!chunks.length) return null;
    let total = 0;
    for (const c of chunks) total += c.length;
    if (total < 32) return null;
    const pcm = new Float32Array(total);
    let offset = 0;
    for (const c of chunks) {
      pcm.set(c, offset);
      offset += c.length;
    }
    return { pcm, sampleRate: rate };
  }

  /** Drop capture nodes when Conversation exits (keeps analyser for energy if still subscribed). */
  stopPcmCaptureHardware(): void {
    this.capturing = false;
    this.captureChunks = [];
    try {
      this.captureProc?.disconnect();
    } catch {
      /* ignore */
    }
    try {
      this.captureMute?.disconnect();
    } catch {
      /* ignore */
    }
    this.captureProc = null;
    this.captureMute = null;
  }

  subscribe(
    opts: Omit<Subscriber, "id" | "pauseUtterance" | "pauseBargeIn"> & {
      pauseUtterance?: boolean;
      pauseBargeIn?: boolean;
    },
  ): {
    id: number;
    unsubscribe: () => void;
    setPaused: (u: boolean, b: boolean) => void;
    setSilenceOffMs: (ms: number) => void;
  } {
    const id = nextSubId++;
    this.subs.set(id, {
      id,
      onSample: opts.onSample,
      speechThreshold: opts.speechThreshold,
      speechOnMs: opts.speechOnMs,
      silenceOffMs: opts.silenceOffMs,
      bargeInMs: opts.bargeInMs,
      onSpeechStart: opts.onSpeechStart,
      onSpeechEnd: opts.onSpeechEnd,
      onBargeIn: opts.onBargeIn,
      pauseUtterance: opts.pauseUtterance ?? false,
      pauseBargeIn: opts.pauseBargeIn ?? false,
    });
    this.speechAccum.set(id, 0);
    this.silenceAccum.set(id, 0);
    this.inSpeech.set(id, false);
    this.bargeAccum.set(id, 0);
    void this.ensureGraph();
    return {
      id,
      unsubscribe: () => {
        this.subs.delete(id);
        this.speechAccum.delete(id);
        this.silenceAccum.delete(id);
        this.inSpeech.delete(id);
        this.bargeAccum.delete(id);
        if (this.subs.size === 0) this.teardownGraph();
      },
      setPaused: (pauseUtterance, pauseBargeIn) => {
        const s = this.subs.get(id);
        if (!s) return;
        s.pauseUtterance = pauseUtterance;
        s.pauseBargeIn = pauseBargeIn;
      },
      setSilenceOffMs: (ms: number) => {
        const s = this.subs.get(id);
        if (!s) return;
        s.silenceOffMs = Math.max(200, Math.min(2000, ms));
      },
    };
  }

  private rms(): number {
    if (!this.analyser) return 0;
    const buf = new Float32Array(this.analyser.fftSize);
    try {
      this.analyser.getFloatTimeDomainData(buf);
    } catch {
      return 0;
    }
    let sum = 0;
    for (let i = 0; i < buf.length; i++) sum += buf[i] * buf[i];
    return Math.sqrt(sum / buf.length);
  }

  private sample() {
    const now = performance.now();
    const dt = Math.min(80, Math.max(0, now - this.lastTs));
    this.lastTs = now;
    const level = this.rms();

    for (const sub of this.subs.values()) {
      const speaking = level >= sub.speechThreshold;
      sub.onSample?.({ rms: level, speaking });

      if (!sub.pauseBargeIn && sub.onBargeIn) {
        if (speaking) {
          const b = (this.bargeAccum.get(sub.id) ?? 0) + dt;
          this.bargeAccum.set(sub.id, b);
          if (b >= sub.bargeInMs) {
            this.bargeAccum.set(sub.id, 0);
            try {
              sub.onBargeIn();
            } catch {
              /* never crash UI from VAD */
            }
          }
        } else {
          this.bargeAccum.set(sub.id, 0);
        }
      }

      if (sub.pauseUtterance) continue;

      if (speaking) {
        const sp = (this.speechAccum.get(sub.id) ?? 0) + dt;
        this.speechAccum.set(sub.id, sp);
        this.silenceAccum.set(sub.id, 0);
        if (!this.inSpeech.get(sub.id) && sp >= sub.speechOnMs) {
          this.inSpeech.set(sub.id, true);
          try {
            sub.onSpeechStart?.();
          } catch {
            /* ignore */
          }
        }
      } else {
        this.speechAccum.set(sub.id, 0);
        if (this.inSpeech.get(sub.id)) {
          const sil = (this.silenceAccum.get(sub.id) ?? 0) + dt;
          this.silenceAccum.set(sub.id, sil);
          if (sil >= sub.silenceOffMs) {
            this.inSpeech.set(sub.id, false);
            this.silenceAccum.set(sub.id, 0);
            try {
              sub.onSpeechEnd?.();
            } catch {
              /* ignore */
            }
          }
        }
      }
    }
  }

  private teardownGraph() {
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.stopPcmCaptureHardware();
    try {
      this.source?.disconnect();
    } catch {
      /* ignore */
    }
    try {
      void this.ctx?.close();
    } catch {
      /* ignore */
    }
    this.source = null;
    this.analyser = null;
    this.ctx = null;
  }

  dispose() {
    this.subs.clear();
    this.teardownGraph();
  }
}

const pipelines = new WeakMap<MediaStream, SharedMicPipeline>();

export function getSharedMicPipeline(stream: MediaStream): SharedMicPipeline {
  let p = pipelines.get(stream);
  if (!p) {
    p = new SharedMicPipeline(stream);
    pipelines.set(stream, p);
  }
  return p;
}

export function disposeSharedMicPipeline(stream: MediaStream) {
  const p = pipelines.get(stream);
  if (p) {
    p.dispose();
    pipelines.delete(stream);
  }
}
