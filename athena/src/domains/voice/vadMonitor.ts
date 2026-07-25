/** Energy-based VAD for duplex conversation (no external deps). */

export type VadMonitorOptions = {
  /** RMS above this counts as speech (0–1 scale after normalisation). */
  speechThreshold?: number;
  /** ms of speech before we consider utterance started. */
  speechOnMs?: number;
  /** ms of silence after speech before utterance ends. */
  silenceOffMs?: number;
  onSpeechStart?: () => void;
  onSpeechEnd?: () => void;
  /** Continuous speech while monitoring (for barge-in). */
  onBargeIn?: () => void;
  /** ms of sustained speech required to fire barge-in. */
  bargeInMs?: number;
};

export class VadMonitor {
  private ctx: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private analyser: AnalyserNode | null = null;
  private raf = 0;
  private speechAccum = 0;
  private silenceAccum = 0;
  private inSpeech = false;
  private bargeAccum = 0;
  private lastTs = 0;
  private opts: Required<
    Pick<VadMonitorOptions, "speechThreshold" | "speechOnMs" | "silenceOffMs" | "bargeInMs">
  > &
    Pick<VadMonitorOptions, "onSpeechStart" | "onSpeechEnd" | "onBargeIn">;

  constructor(opts: VadMonitorOptions = {}) {
    this.opts = {
      speechThreshold: opts.speechThreshold ?? 0.028,
      speechOnMs: opts.speechOnMs ?? 220,
      silenceOffMs: opts.silenceOffMs ?? 1100,
      bargeInMs: opts.bargeInMs ?? 280,
      onSpeechStart: opts.onSpeechStart,
      onSpeechEnd: opts.onSpeechEnd,
      onBargeIn: opts.onBargeIn,
    };
  }

  start(stream: MediaStream) {
    this.stop();
    const AC =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    this.ctx = new AC();
    this.source = this.ctx.createMediaStreamSource(stream);
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = 2048;
    this.analyser.smoothingTimeConstant = 0.4;
    this.source.connect(this.analyser);
    this.speechAccum = 0;
    this.silenceAccum = 0;
    this.inSpeech = false;
    this.bargeAccum = 0;
    this.lastTs = performance.now();
    const tick = () => {
      this.raf = requestAnimationFrame(tick);
      this.sample();
    };
    this.raf = requestAnimationFrame(tick);
  }

  /** Switch barge-in callback without restarting the graph. */
  setBargeInHandler(handler: (() => void) | undefined) {
    this.opts.onBargeIn = handler;
  }

  setUtteranceHandlers(onStart?: () => void, onEnd?: () => void) {
    this.opts.onSpeechStart = onStart;
    this.opts.onSpeechEnd = onEnd;
  }

  private rms(): number {
    if (!this.analyser) return 0;
    const buf = new Float32Array(this.analyser.fftSize);
    this.analyser.getFloatTimeDomainData(buf);
    let sum = 0;
    for (let i = 0; i < buf.length; i++) sum += buf[i] * buf[i];
    return Math.sqrt(sum / buf.length);
  }

  private sample() {
    const now = performance.now();
    const dt = Math.min(80, Math.max(0, now - this.lastTs));
    this.lastTs = now;
    const level = this.rms();
    const speaking = level >= this.opts.speechThreshold;

    if (this.opts.onBargeIn) {
      if (speaking) {
        this.bargeAccum += dt;
        if (this.bargeAccum >= this.opts.bargeInMs) {
          this.bargeAccum = 0;
          this.opts.onBargeIn();
        }
      } else {
        this.bargeAccum = 0;
      }
    }

    if (speaking) {
      this.speechAccum += dt;
      this.silenceAccum = 0;
      if (!this.inSpeech && this.speechAccum >= this.opts.speechOnMs) {
        this.inSpeech = true;
        this.opts.onSpeechStart?.();
      }
    } else {
      this.speechAccum = 0;
      if (this.inSpeech) {
        this.silenceAccum += dt;
        if (this.silenceAccum >= this.opts.silenceOffMs) {
          this.inSpeech = false;
          this.silenceAccum = 0;
          this.opts.onSpeechEnd?.();
        }
      }
    }
  }

  stop() {
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
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
    this.inSpeech = false;
  }
}
