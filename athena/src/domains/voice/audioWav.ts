/**
 * Convert arbitrary MediaRecorder blobs to 16 kHz mono WAV for Whisper.
 * WKWebView / Chromium webm-opus sometimes yields empty STT; WAV is reliable.
 */

function writeString(view: DataView, offset: number, str: string) {
  for (let i = 0; i < str.length; i++) view.setUint8(offset + i, str.charCodeAt(i));
}

function encodeWavPcm16(samples: Float32Array, sampleRate: number): Blob {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  writeString(view, 0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeString(view, 8, "WAVE");
  writeString(view, 12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeString(view, 36, "data");
  view.setUint32(40, samples.length * 2, true);
  let offset = 44;
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    offset += 2;
  }
  return new Blob([buffer], { type: "audio/wav" });
}

function mixToMono(audio: AudioBuffer): Float32Array {
  const len = audio.length;
  const out = new Float32Array(len);
  const channels = audio.numberOfChannels;
  if (channels === 1) {
    out.set(audio.getChannelData(0));
    return out;
  }
  for (let c = 0; c < channels; c++) {
    const ch = audio.getChannelData(c);
    for (let i = 0; i < len; i++) out[i] += ch[i] / channels;
  }
  return out;
}

function resampleLinear(input: Float32Array, fromRate: number, toRate: number): Float32Array {
  if (fromRate === toRate) return input;
  const ratio = fromRate / toRate;
  const outLen = Math.max(1, Math.round(input.length / ratio));
  const out = new Float32Array(outLen);
  for (let i = 0; i < outLen; i++) {
    const src = i * ratio;
    const i0 = Math.floor(src);
    const i1 = Math.min(input.length - 1, i0 + 1);
    const t = src - i0;
    out[i] = input[i0] * (1 - t) + input[i1] * t;
  }
  return out;
}

/**
 * Pad 16 kHz mono WAV with trailing silence so Whisper gets ≥ ~1s of audio.
 * Short clips (e.g. "hey") otherwise fail with "input is too short".
 */
export async function padWav16kMonoToMinMs(wavBlob: Blob, minMs = 1000): Promise<Blob> {
  const minSamples = Math.ceil((16_000 * minMs) / 1000);
  const raw = await wavBlob.arrayBuffer();
  if (raw.byteLength < 44) return wavBlob;
  const pcmBytes = raw.byteLength - 44;
  const sampleCount = Math.floor(pcmBytes / 2);
  if (sampleCount >= minSamples) return wavBlob;

  const AC =
    window.AudioContext ||
    (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
  const ctx = new AC();
  try {
    const decoded = await ctx.decodeAudioData(raw.slice(0));
    const mono = mixToMono(decoded);
    const pcm = resampleLinear(mono, decoded.sampleRate, 16_000);
    const padded = new Float32Array(minSamples);
    padded.set(pcm.subarray(0, Math.min(pcm.length, minSamples)));
    return encodeWavPcm16(padded, 16_000);
  } finally {
    try {
      await ctx.close();
    } catch {
      /* ignore */
    }
  }
}

/** Encode raw float PCM (any sample rate) as 16 kHz mono WAV for Whisper. */
export function float32PcmToWav16kMono(
  pcm: Float32Array,
  sampleRate: number,
  opts?: { minPeak?: number },
): Blob {
  if (pcm.length < 64) throw new Error("Audio clip too short to transcribe.");
  const pcm16 = resampleLinear(pcm, sampleRate, 16_000);
  let peak = 0;
  for (let i = 0; i < pcm16.length; i++) {
    const a = Math.abs(pcm16[i]);
    if (a > peak) peak = a;
  }
  const minPeak = opts?.minPeak ?? 0.004;
  if (peak < minPeak) {
    throw new Error("Audio too quiet — speak closer to the mic.");
  }
  return encodeWavPcm16(pcm16, 16_000);
}

/** Decode a recorded blob and re-encode as 16 kHz mono PCM WAV. */
export async function blobToWav16kMono(blob: Blob): Promise<Blob> {
  if (blob.size < 64) throw new Error("Audio clip too short to transcribe.");
  const AC =
    window.AudioContext ||
    (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
  const ctx = new AC();
  try {
    const raw = await blob.arrayBuffer();
    const decoded = await ctx.decodeAudioData(raw.slice(0));
    const mono = mixToMono(decoded);
    const pcm = resampleLinear(mono, decoded.sampleRate, 16_000);
    // Reject near-silent clips so we don't burn a Whisper round-trip on noise.
    let peak = 0;
    for (let i = 0; i < pcm.length; i++) {
      const a = Math.abs(pcm[i]);
      if (a > peak) peak = a;
    }
    if (peak < 0.008) {
      throw new Error("Audio too quiet — speak closer to the mic.");
    }
    return encodeWavPcm16(pcm, 16_000);
  } finally {
    try {
      await ctx.close();
    } catch {
      /* ignore */
    }
  }
}
