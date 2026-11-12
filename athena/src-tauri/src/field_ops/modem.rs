//! Data over any radio (S3): turn text into audio tones and back.
//!
//! Plug a voice radio's microphone and speaker jacks into the computer (or hold them to the
//! speaker and microphone) and this sends text over it. Any radio that carries a voice carries
//! this: handhelds, CB, marine, HF. No special hardware on either end.
//!
//! The scheme is deliberately plain so it survives poor audio paths:
//!
//! * two tones, 1200 Hz for a 1 and 2200 Hz for a 0 (the classic Bell 202 pair), at 300 bits
//!   per second, which every voice channel passes;
//! * a run of alternating bits first, so the radio's transmitter and the receiver's volume
//!   control have settled before data starts;
//! * a sync word, then length, text and a checksum, all protected by a Hamming(7,4) code that
//!   repairs one wrong bit in every seven;
//! * a CRC over the text, so a message that could not be repaired is rejected rather than shown
//!   garbled.
//!
//! Transmitting on amateur or licensed bands needs the right licence in normal times.

use serde::Serialize;
use tauri::State;

use crate::auth::AuthSession;

pub const MARK_HZ: f64 = 1200.0;
pub const SPACE_HZ: f64 = 2200.0;
pub const BAUD: f64 = 300.0;
/// Sample rate used for generated audio.
pub const TX_SAMPLE_RATE: u32 = 48_000;
/// Longest text carried in one transmission, in bytes.
pub const MAX_PAYLOAD_BYTES: usize = 512;
const SYNC: [u8; 2] = [0x2D, 0xD4];
/// Bytes of alternating bits sent before the sync word (about 0.75 s at 300 baud).
const PREAMBLE_BYTES: usize = 28;
/// Silence either side of a transmission.
const PAD_SECONDS: f64 = 0.15;
const AMPLITUDE: f32 = 0.6;

fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for b in data {
        crc ^= u16::from(*b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn push_byte_bits(bits: &mut Vec<bool>, byte: u8) {
    for i in (0..8).rev() {
        bits.push(byte >> i & 1 == 1);
    }
}

/// Hamming(7,4): four data bits become seven, laid out p1 p2 d1 p3 d2 d3 d4.
fn hamming_encode_nibble(bits: &mut Vec<bool>, nibble: u8) {
    let d = [nibble >> 3 & 1 == 1, nibble >> 2 & 1 == 1, nibble >> 1 & 1 == 1, nibble & 1 == 1];
    let p1 = d[0] ^ d[1] ^ d[3];
    let p2 = d[0] ^ d[2] ^ d[3];
    let p3 = d[1] ^ d[2] ^ d[3];
    bits.extend_from_slice(&[p1, p2, d[0], p3, d[1], d[2], d[3]]);
}

/// Decode seven bits to a nibble, repairing a single wrong bit.
fn hamming_decode_nibble(code: &[bool]) -> u8 {
    let mut c = [code[0], code[1], code[2], code[3], code[4], code[5], code[6]];
    let s1 = c[0] ^ c[2] ^ c[4] ^ c[6];
    let s2 = c[1] ^ c[2] ^ c[5] ^ c[6];
    let s3 = c[3] ^ c[4] ^ c[5] ^ c[6];
    let syndrome = usize::from(s1) | usize::from(s2) << 1 | usize::from(s3) << 2;
    if syndrome != 0 {
        c[syndrome - 1] = !c[syndrome - 1];
    }
    u8::from(c[2]) << 3 | u8::from(c[4]) << 2 | u8::from(c[5]) << 1 | u8::from(c[6])
}

fn hamming_encode_bytes(bits: &mut Vec<bool>, bytes: &[u8]) {
    for b in bytes {
        hamming_encode_nibble(bits, b >> 4);
        hamming_encode_nibble(bits, b & 0x0F);
    }
}

/// Decode `count` bytes from `bits` starting at `start`. `None` when there are not enough bits.
fn hamming_decode_bytes(bits: &[bool], start: usize, count: usize) -> Option<Vec<u8>> {
    let needed = count * 14;
    let slice = bits.get(start..start + needed)?;
    Some(
        slice
            .chunks(14)
            .map(|c| hamming_decode_nibble(&c[..7]) << 4 | hamming_decode_nibble(&c[7..]))
            .collect(),
    )
}

/// The bits of one transmission: preamble, sync word, then length, payload and CRC (coded).
pub fn frame_bits(payload: &[u8]) -> Result<Vec<bool>, String> {
    if payload.is_empty() {
        return Err("There is nothing to send.".to_string());
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(format!(
            "That message is {} bytes; the most one transmission carries is {MAX_PAYLOAD_BYTES}. Split it up.",
            payload.len()
        ));
    }
    let mut bits = Vec::new();
    for _ in 0..PREAMBLE_BYTES {
        push_byte_bits(&mut bits, 0x55);
    }
    for b in SYNC {
        push_byte_bits(&mut bits, b);
    }
    let len = payload.len() as u16;
    hamming_encode_bytes(&mut bits, &len.to_be_bytes());
    hamming_encode_bytes(&mut bits, payload);
    hamming_encode_bytes(&mut bits, &crc16_ccitt(payload).to_be_bytes());
    Ok(bits)
}

/// Audio for a message, as samples between -1 and 1.
pub fn modulate(payload: &[u8], sample_rate: u32) -> Result<Vec<f32>, String> {
    let bits = frame_bits(payload)?;
    let rate = f64::from(sample_rate);
    let samples_per_bit = rate / BAUD;
    let pad = (PAD_SECONDS * rate) as usize;
    let total = (bits.len() as f64 * samples_per_bit).ceil() as usize;
    let mut out = vec![0.0f32; pad];
    out.reserve(total + pad);
    // One running phase across bits, so the tone changes without a click.
    let mut phase = 0.0f64;
    for n in 0..total {
        let bit = bits[((n as f64 / samples_per_bit) as usize).min(bits.len() - 1)];
        let freq = if bit { MARK_HZ } else { SPACE_HZ };
        phase += 2.0 * std::f64::consts::PI * freq / rate;
        out.push(AMPLITUDE * phase.sin() as f32);
    }
    out.extend(std::iter::repeat(0.0).take(pad));
    Ok(out)
}

/// Energy of one tone in a block of samples (Goertzel).
fn tone_energy(samples: &[f32], freq: f64, sample_rate: f64) -> f64 {
    let coeff = 2.0 * (2.0 * std::f64::consts::PI * freq / sample_rate).cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for x in samples {
        let s0 = f64::from(*x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

/// Slice the audio into bits starting `offset` samples in.
fn slice_bits(samples: &[f32], sample_rate: f64, offset: f64) -> Vec<bool> {
    let samples_per_bit = sample_rate / BAUD;
    let mut bits = Vec::new();
    let mut k = 0usize;
    loop {
        let start = (offset + k as f64 * samples_per_bit).round() as usize;
        let end = (offset + (k + 1) as f64 * samples_per_bit).round() as usize;
        if end > samples.len() {
            break;
        }
        let block = &samples[start..end];
        bits.push(tone_energy(block, MARK_HZ, sample_rate) > tone_energy(block, SPACE_HZ, sample_rate));
        k += 1;
    }
    bits
}

fn sync_pattern() -> Vec<bool> {
    let mut bits = Vec::new();
    for b in SYNC {
        push_byte_bits(&mut bits, b);
    }
    bits
}

/// Try to read a frame from a bit stream. Returns the first payload whose checksum is good.
fn decode_bits(bits: &[bool]) -> Option<Vec<u8>> {
    let sync = sync_pattern();
    if bits.len() < sync.len() {
        return None;
    }
    for i in 0..=bits.len() - sync.len() {
        // Allow one wrong bit in the sync word itself.
        let mismatches = bits[i..i + sync.len()].iter().zip(&sync).filter(|(a, b)| a != b).count();
        if mismatches > 1 {
            continue;
        }
        let body = i + sync.len();
        let Some(len_bytes) = hamming_decode_bytes(bits, body, 2) else {
            continue;
        };
        let len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
        if len == 0 || len > MAX_PAYLOAD_BYTES {
            continue;
        }
        let Some(payload) = hamming_decode_bytes(bits, body + 28, len) else {
            continue;
        };
        let Some(crc_bytes) = hamming_decode_bytes(bits, body + 28 + len * 14, 2) else {
            continue;
        };
        if crc16_ccitt(&payload) == u16::from_be_bytes([crc_bytes[0], crc_bytes[1]]) {
            return Some(payload);
        }
    }
    None
}

/// Recover a message from recorded audio. `None` when no intact message is found.
pub fn demodulate(samples: &[f32], sample_rate: u32) -> Option<Vec<u8>> {
    let rate = f64::from(sample_rate);
    let samples_per_bit = rate / BAUD;
    if samples_per_bit < 8.0 {
        return None;
    }
    // The receiver does not know where a bit starts, so try eight alignments across one bit.
    const ALIGNMENTS: usize = 8;
    for a in 0..ALIGNMENTS {
        let offset = samples_per_bit * a as f64 / ALIGNMENTS as f64;
        if let Some(payload) = decode_bits(&slice_bits(samples, rate, offset)) {
            return Some(payload);
        }
    }
    None
}

/// 16-bit mono WAV file bytes.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Read a PCM WAV file (8, 16 or 32-bit integer, or 32-bit float; any channel count) as mono.
pub fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let bad = || "That file is not a WAV recording this can read.".to_string();
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad());
    }
    let mut pos = 12;
    let mut format: Option<(u16, u16, u32, u16)> = None; // (tag, channels, rate, bits)
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
        let body_start = pos + 8;
        let body_end = (body_start + size).min(bytes.len());
        let body = &bytes[body_start..body_end];
        if id == b"fmt " && body.len() >= 16 {
            format = Some((
                u16::from_le_bytes([body[0], body[1]]),
                u16::from_le_bytes([body[2], body[3]]),
                u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                u16::from_le_bytes([body[14], body[15]]),
            ));
        } else if id == b"data" {
            data = Some(body);
        }
        pos = body_start + size + (size & 1);
    }
    let (tag, channels, rate, bits) = format.ok_or_else(bad)?;
    let data = data.ok_or_else(bad)?;
    let channels = usize::from(channels.max(1));
    let bytes_per = usize::from(bits / 8);
    if rate == 0 || bytes_per == 0 {
        return Err(bad());
    }
    let sample = |b: &[u8]| -> Option<f32> {
        Some(match (tag, bits) {
            (1, 8) => (f32::from(b[0]) - 128.0) / 128.0,
            (1, 16) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            (1, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            // WAVE_FORMAT_EXTENSIBLE with 16-bit samples is common from recorders.
            (0xFFFE, 16) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            _ => return None,
        })
    };
    let frame = bytes_per * channels;
    let mut out = Vec::with_capacity(data.len() / frame.max(1));
    for f in data.chunks_exact(frame) {
        let mut sum = 0.0;
        for c in 0..channels {
            sum += sample(&f[c * bytes_per..(c + 1) * bytes_per]).ok_or_else(bad)?;
        }
        out.push(sum / channels as f32);
    }
    Ok((out, rate))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModemAudioInfo {
    pub path: String,
    pub seconds: f64,
    pub bytes: usize,
}

/// Seconds of air time a message of this many bytes takes.
pub fn air_time_seconds(payload_len: usize) -> f64 {
    let bits = (PREAMBLE_BYTES + SYNC.len()) * 8 + (2 + payload_len + 2) * 14;
    bits as f64 / BAUD + 2.0 * PAD_SECONDS
}

/// How long a message will take to send, in seconds, so the operator knows before keying up.
#[tauri::command]
pub fn modem_air_time(text: String) -> f64 {
    air_time_seconds(text.trim().len())
}

/// Write a message as a WAV file that can be played into any radio.
#[tauri::command]
pub fn modem_write_audio(
    text: String,
    path: String,
    session: State<'_, AuthSession>,
) -> Result<ModemAudioInfo, String> {
    super::require_member(&session)?;
    let text = text.trim();
    let samples = modulate(text.as_bytes(), TX_SAMPLE_RATE)?;
    std::fs::write(&path, encode_wav(&samples, TX_SAMPLE_RATE)).map_err(|e| format!("Could not write the audio file: {e}"))?;
    Ok(ModemAudioInfo {
        path,
        seconds: samples.len() as f64 / f64::from(TX_SAMPLE_RATE),
        bytes: text.len(),
    })
}

/// Read a message out of a recording of a transmission.
#[tauri::command]
pub fn modem_read_audio(path: String, session: State<'_, AuthSession>) -> Result<String, String> {
    super::require_member(&session)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the recording: {e}"))?;
    let (samples, rate) = decode_wav(&bytes)?;
    let payload = demodulate(&samples, rate).ok_or(
        "No intact message was found in that recording. Check the volume is not distorting, and that the whole transmission was recorded.",
    )?;
    Ok(String::from_utf8_lossy(&payload).into_owned())
}

/// Play a message out of the computer's sound output, for a radio held to the speaker or wired
/// to the headphone jack. Returns when the audio has finished.
#[tauri::command]
pub async fn modem_transmit(text: String, session: crate::auth::CallerSession) -> Result<f64, String> {
    super::require_member(&session)?;
    let payload = text.trim().as_bytes().to_vec();
    tokio::task::spawn_blocking(move || audio::play(&payload))
        .await
        .map_err(|e| e.to_string())?
}

/// Listen on the computer's sound input for up to `seconds` and return the first message heard.
#[tauri::command]
pub async fn modem_listen(seconds: u32, session: crate::auth::CallerSession) -> Result<String, String> {
    super::require_member(&session)?;
    let seconds = seconds.clamp(2, 120);
    tokio::task::spawn_blocking(move || audio::listen(seconds))
        .await
        .map_err(|e| e.to_string())?
}

/// Sound card input and output.
mod audio {
    use super::{demodulate, modulate};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub fn play(payload: &[u8]) -> Result<f64, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("No sound output device was found.")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate().0;
        let channels = usize::from(config.channels());
        let samples = Arc::new(modulate(payload, rate)?);
        let seconds = samples.len() as f64 / f64::from(rate);
        let position = Arc::new(Mutex::new(0usize));
        let err_fn = |e| eprintln!("[athena:modem] output stream error: {e}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let (s, p) = (Arc::clone(&samples), Arc::clone(&position));
                device.build_output_stream(
                    &config.into(),
                    move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut pos = p.lock().map(|g| *g).unwrap_or(usize::MAX);
                        for frame in out.chunks_mut(channels) {
                            let v = s.get(pos).copied().unwrap_or(0.0);
                            frame.iter_mut().for_each(|x| *x = v);
                            pos = pos.saturating_add(1);
                        }
                        if let Ok(mut g) = p.lock() {
                            *g = pos;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let (s, p) = (Arc::clone(&samples), Arc::clone(&position));
                device.build_output_stream(
                    &config.into(),
                    move |out: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        let mut pos = p.lock().map(|g| *g).unwrap_or(usize::MAX);
                        for frame in out.chunks_mut(channels) {
                            let v = (s.get(pos).copied().unwrap_or(0.0) * 32767.0) as i16;
                            frame.iter_mut().for_each(|x| *x = v);
                            pos = pos.saturating_add(1);
                        }
                        if let Ok(mut g) = p.lock() {
                            *g = pos;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            other => return Err(format!("The sound output uses a sample format this cannot play ({other:?}).")),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs_f64(seconds + 3.0);
        while position.lock().map(|g| *g).unwrap_or(usize::MAX) < samples.len() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(40));
        }
        // Let the last buffer leave the sound card before the stream is dropped.
        std::thread::sleep(Duration::from_millis(200));
        Ok(seconds)
    }

    pub fn listen(seconds: u32) -> Result<String, String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("No microphone or line input was found.")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate().0;
        let channels = usize::from(config.channels()).max(1);
        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let err_fn = |e| eprintln!("[athena:modem] input stream error: {e}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let b = Arc::clone(&buffer);
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if let Ok(mut g) = b.lock() {
                            g.extend(data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let b = Arc::clone(&buffer);
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if let Ok(mut g) = b.lock() {
                            g.extend(
                                data.chunks(channels)
                                    .map(|f| f.iter().map(|x| f32::from(*x) / 32768.0).sum::<f32>() / channels as f32),
                            );
                        }
                    },
                    err_fn,
                    None,
                )
            }
            other => return Err(format!("The sound input uses a sample format this cannot record ({other:?}).")),
        }
        .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(u64::from(seconds));
        let mut checked = 0usize;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(750));
            let snapshot = buffer.lock().map(|g| g.clone()).unwrap_or_default();
            // Only re-check once another second of audio has arrived.
            if snapshot.len() < checked + rate as usize {
                continue;
            }
            checked = snapshot.len();
            if let Some(payload) = demodulate(&snapshot, rate) {
                return Ok(String::from_utf8_lossy(&payload).into_owned());
            }
        }
        drop(stream);
        let all = buffer.lock().map(|g| g.clone()).unwrap_or_default();
        demodulate(&all, rate)
            .map(|p| String::from_utf8_lossy(&p).into_owned())
            .ok_or_else(|| "Nothing was received. Check the radio is connected to the sound input and turned up.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise in [-1, 1] with roughly Gaussian shape.
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f32 {
            let mut sum = 0.0f32;
            for _ in 0..6 {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                sum += ((self.0 >> 33) as f32 / (1u64 << 31) as f32) - 1.0;
            }
            sum / 6.0f32.sqrt() * 1.732 / 3.0
        }
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn add_noise(signal: &[f32], snr_db: f32, seed: u64) -> Vec<f32> {
        let mut noise = Noise(seed);
        let raw: Vec<f32> = signal.iter().map(|_| noise.next()).collect();
        let scale = rms(signal) / rms(&raw) / 10f32.powf(snr_db / 20.0);
        signal.iter().zip(&raw).map(|(s, n)| s + n * scale).collect()
    }

    const MESSAGE: &str = "Ridge to base: bridge out at the ford, use the north track. 3 people, all well.";

    #[test]
    fn a_clean_transmission_round_trips() {
        let audio = modulate(MESSAGE.as_bytes(), 48_000).unwrap();
        assert_eq!(demodulate(&audio, 48_000).as_deref(), Some(MESSAGE.as_bytes()));
        assert!(audio.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn it_works_at_common_sound_card_rates() {
        for rate in [8_000u32, 16_000, 22_050, 44_100, 48_000] {
            let audio = modulate(b"rate check", rate).unwrap();
            assert_eq!(demodulate(&audio, rate).as_deref(), Some(&b"rate check"[..]), "{rate} Hz");
        }
    }

    #[test]
    fn it_survives_noise_as_loud_as_the_signal() {
        let audio = modulate(MESSAGE.as_bytes(), 48_000).unwrap();
        for seed in [1u64, 2, 3] {
            let noisy = add_noise(&audio, 0.0, seed);
            assert_eq!(demodulate(&noisy, 48_000).as_deref(), Some(MESSAGE.as_bytes()), "seed {seed}");
        }
    }

    #[test]
    fn it_survives_a_quiet_recording_and_unknown_start() {
        let audio = modulate(MESSAGE.as_bytes(), 48_000).unwrap();
        // Recording starts 1.37 s early at an awkward sample offset, at 2% volume, with noise.
        let mut recording = vec![0.0f32; 65_777];
        recording.extend(audio.iter().map(|s| s * 0.02));
        recording.extend(vec![0.0f32; 20_000]);
        let noisy = add_noise(&recording, 6.0, 9);
        assert_eq!(demodulate(&noisy, 48_000).as_deref(), Some(MESSAGE.as_bytes()));
    }

    #[test]
    fn it_survives_clipping() {
        let audio: Vec<f32> = modulate(MESSAGE.as_bytes(), 48_000)
            .unwrap()
            .iter()
            .map(|s| (s * 6.0).clamp(-1.0, 1.0))
            .collect();
        assert_eq!(demodulate(&audio, 48_000).as_deref(), Some(MESSAGE.as_bytes()));
    }

    #[test]
    fn a_wrong_bit_in_every_codeword_is_repaired() {
        let mut bits = frame_bits(b"hamming").unwrap();
        let body = (PREAMBLE_BYTES + SYNC.len()) * 8;
        let mut i = body + 3;
        while i < bits.len() {
            bits[i] = !bits[i];
            i += 7;
        }
        assert_eq!(decode_bits(&bits).as_deref(), Some(&b"hamming"[..]));
    }

    #[test]
    fn damage_beyond_repair_is_rejected_not_shown_garbled() {
        let mut bits = frame_bits(b"do not garble me").unwrap();
        let body = (PREAMBLE_BYTES + SYNC.len()) * 8 + 28;
        for i in (body..body + 60).step_by(2) {
            bits[i] = !bits[i];
        }
        assert_eq!(decode_bits(&bits), None);
        assert_eq!(demodulate(&vec![0.0; 48_000], 48_000), None, "silence holds no message");
        let mut noise = Noise(4);
        let hiss: Vec<f32> = (0..96_000).map(|_| noise.next()).collect();
        assert_eq!(demodulate(&hiss, 48_000), None, "noise holds no message");
    }

    #[test]
    fn empty_and_oversized_messages_are_refused() {
        assert!(frame_bits(b"").is_err());
        assert!(frame_bits(&vec![b'x'; MAX_PAYLOAD_BYTES]).is_ok());
        let err = frame_bits(&vec![b'x'; MAX_PAYLOAD_BYTES + 1]).unwrap_err();
        assert!(err.contains("513 bytes"), "{err}");
    }

    #[test]
    fn non_ascii_text_survives() {
        let text = "Très bien — 水 OK ✓";
        let audio = modulate(text.as_bytes(), 16_000).unwrap();
        assert_eq!(String::from_utf8(demodulate(&audio, 16_000).unwrap()).unwrap(), text);
    }

    #[test]
    fn wav_files_round_trip_and_stereo_is_mixed_down() {
        let audio = modulate(b"via wav", 48_000).unwrap();
        let (samples, rate) = decode_wav(&encode_wav(&audio, 48_000)).unwrap();
        assert_eq!(rate, 48_000);
        assert_eq!(demodulate(&samples, rate).as_deref(), Some(&b"via wav"[..]));

        // Hand-built 16-bit stereo file: left carries the signal, right is silent.
        let mono = encode_wav(&audio, 48_000);
        let mut stereo = mono[..44].to_vec();
        stereo[22] = 2; // channels
        let byte_rate = 48_000u32 * 4;
        stereo[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        stereo[32] = 4; // block align
        let data_len = ((mono.len() - 44) * 2) as u32;
        stereo[40..44].copy_from_slice(&data_len.to_le_bytes());
        for s in mono[44..].chunks(2) {
            stereo.extend_from_slice(s);
            stereo.extend_from_slice(&[0, 0]);
        }
        let riff_len = (stereo.len() - 8) as u32;
        stereo[4..8].copy_from_slice(&riff_len.to_le_bytes());
        let (samples, rate) = decode_wav(&stereo).unwrap();
        assert_eq!(demodulate(&samples, rate).as_deref(), Some(&b"via wav"[..]));

        assert!(decode_wav(b"not a wav file").is_err());
    }

    #[test]
    fn air_time_matches_the_generated_audio() {
        let audio = modulate(MESSAGE.as_bytes(), 48_000).unwrap();
        let actual = audio.len() as f64 / 48_000.0;
        assert!((air_time_seconds(MESSAGE.len()) - actual).abs() < 0.05);
        assert!(actual < 6.0, "an 80-byte message takes about 5 seconds: {actual}");
    }
}
