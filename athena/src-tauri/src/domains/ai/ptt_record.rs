//! Dexter-faithful push-to-talk: cpal record → WAV → Whisper HTTP → transcript.
//!
//! Critical: do NOT hold a browser getUserMedia stream open while recording.
//! On macOS, WebKit exclusive mic access makes cpal capture silence, and Whisper
//! then returns junk ghosts like "Thank you."

use base64::{engine::general_purpose::STANDARD, Engine};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::State;

#[derive(Default)]
pub struct PttRecordState {
    is_recording: Mutex<bool>,
    samples: Mutex<Vec<f32>>,
    sample_rate: Mutex<u32>,
    channels: Mutex<usize>,
}

impl PttRecordState {
    pub fn request_stop(&self) {
        if let Ok(mut flag) = self.is_recording.lock() {
            *flag = false;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PttTranscriptResult {
    pub text: String,
    pub duration_ms: u32,
    pub sample_rate: u32,
    pub peak: f32,
    pub samples: u32,
    /// Present when debugging raw audio; omit large payloads from normal path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wav_base64: Option<String>,
}

fn downmix_interleaved(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    let frames = data.len() / channels;
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut sum = 0.0f32;
        for ch in 0..channels {
            sum += data[frame * channels + ch];
        }
        out.push(sum / channels as f32);
    }
    out
}

fn encode_wav_pcm16_mono(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let data_bytes = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_bytes);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i = if clamped < 0.0 {
            (clamped * 32768.0) as i16
        } else {
            (clamped * 32767.0) as i16
        };
        out.extend_from_slice(&i.to_le_bytes());
    }
    Ok(out)
}

fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == 0 || to_rate == 0 || input.is_empty() {
        return Vec::new();
    }
    if from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).round().max(1.0) as usize;
    let mut out = vec![0.0f32; out_len];
    for (i, sample) in out.iter_mut().enumerate() {
        let src = i as f64 * ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(input.len() - 1);
        let t = (src - i0 as f64) as f32;
        *sample = input[i0] * (1.0 - t) + input[i1] * t;
    }
    out
}

/// Mirror Dexter `voice::record_audio` — dedicated thread, cpal default input, until flag clears.
fn record_loop(state: Arc<PttRecordState>) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No microphone found.".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("Mic config failed: {e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = (config.channels() as usize).max(1);
    let sample_format = config.sample_format();

    eprintln!(
        "[athena:ptt] device ready rate={sample_rate} channels={channels} format={sample_format:?}"
    );

    *state
        .sample_rate
        .lock()
        .map_err(|_| "ptt lock".to_string())? = sample_rate;
    *state.channels.lock().map_err(|_| "ptt lock".to_string())? = channels;

    let err_fn = |err| eprintln!("[athena:ptt] stream error: {err}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let buf_state = Arc::clone(&state);
            let ch = channels;
            device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        // Downmix to mono (Dexter often sees mono devices; stereo must be mixed for Whisper).
                        let mono = downmix_interleaved(data, ch);
                        if let Ok(mut buf) = buf_state.samples.lock() {
                            buf.extend_from_slice(&mono);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Mic stream failed: {e}"))?
        }
        cpal::SampleFormat::I16 => {
            let buf_state = Arc::clone(&state);
            let ch = channels;
            device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        let mono = downmix_interleaved(&floats, ch);
                        if let Ok(mut buf) = buf_state.samples.lock() {
                            buf.extend_from_slice(&mono);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Mic stream failed: {e}"))?
        }
        other => return Err(format!("Unsupported mic sample format: {other:?}")),
    };

    stream.play().map_err(|e| format!("Mic play failed: {e}"))?;

    loop {
        let recording = state.is_recording.lock().map(|g| *g).unwrap_or(false);
        if !recording {
            break;
        }
        thread::sleep(Duration::from_millis(40));
    }

    drop(stream);
    Ok(())
}

fn take_recording(state: &PttRecordState) -> Result<(Vec<f32>, u32), String> {
    {
        let mut flag = state.is_recording.lock().map_err(|_| "ptt lock".to_string())?;
        *flag = false;
    }
    // Brief settle so the capture thread finishes the last buffer (Dexter ~100ms; we keep it tight).
    thread::sleep(Duration::from_millis(50));

    let sample_rate = *state.sample_rate.lock().map_err(|_| "ptt lock".to_string())?;
    let samples = {
        let mut guard = state.samples.lock().map_err(|_| "ptt lock".to_string())?;
        std::mem::take(&mut *guard)
    };
    Ok((samples, sample_rate))
}

fn samples_to_wav_16k(samples: &[f32], sample_rate: u32) -> Result<(Vec<u8>, f32, u32), String> {
    if samples.len() < 800 {
        return Err("Hold longer — almost no audio was captured.".to_string());
    }
    let rate = if sample_rate > 0 { sample_rate } else { 48_000 };
    let pcm16k = resample_linear(samples, rate, 16_000);
    let mut peak = 0.0f32;
    for &s in &pcm16k {
        let a = s.abs();
        if a > peak {
            peak = a;
        }
    }
    if peak < 0.002 {
        return Err(format!(
            "Audio too quiet (peak={peak:.4}) — speak closer, or check System Settings → Microphone for Athena."
        ));
    }
    // Pad to ~0.5s — enough for Whisper, half the wait of a full 1s pad.
    let min_samples = 8_000usize;
    let mut padded = pcm16k;
    if padded.len() < min_samples {
        padded.resize(min_samples, 0.0);
    }
    let wav = encode_wav_pcm16_mono(&padded, 16_000)?;
    let duration_ms = ((samples.len() as f64 / rate as f64) * 1000.0).round() as u32;
    Ok((wav, peak, duration_ms))
}

fn whisper_http_transcribe(base_url: &str, model: &str, wav: Vec<u8>) -> Result<String, String> {
    let root = base_url.trim_end_matches('/');
    let url = format!("{root}/v1/audio/transcriptions");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let part = reqwest::blocking::multipart::Part::bytes(wav)
        .file_name("utterance.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("multipart: {e}"))?;
    let form = reqwest::blocking::multipart::Form::new()
        .part("file", part)
        .text("model", model.to_string())
        .text("language", "en")
        .text("response_format", "json")
        .text("temperature", "0")
        .text("stream", "false");

    let res = client
        .post(&url)
        .multipart(form)
        .send()
        .map_err(|e| format!("Whisper request failed: {e}"))?;
    let status = res.status();
    let body = res.text().map_err(|e| format!("Whisper body: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Whisper STT failed ({status}): {}",
            body.chars().take(200).collect::<String>()
        ));
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        let text = json
            .get("text")
            .or_else(|| json.get("transcription"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        return Ok(text);
    }
    Ok(body.trim().to_string())
}

fn resolve_model(base_url: &str, preferred: Option<&str>) -> String {
    if let Some(p) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        return p.to_string();
    }
    static CACHED: Mutex<Option<(String, String, std::time::Instant)>> = Mutex::new(None);
    if let Ok(guard) = CACHED.lock() {
        if let Some((url, model, at)) = guard.as_ref() {
            if url == base_url && at.elapsed() < Duration::from_secs(120) {
                return model.clone();
            }
        }
    }
    let root = base_url.trim_end_matches('/');
    let url = format!("{root}/v1/models");
    // Prefer the fastest English models first — Conversation latency > absolute WER.
    let preferred_order = [
        "Systran/faster-whisper-tiny.en",
        "Systran/faster-whisper-base.en",
        "Systran/faster-whisper-base",
        "Systran/faster-whisper-small.en",
        "Systran/faster-whisper-small",
        "Systran/faster-distil-whisper-large-v3",
        "whisper-1",
    ];
    let mut picked = preferred_order[0].to_string();
    if let Ok(res) = ureq::get(&url).set("Accept", "application/json").call() {
        if let Ok(json) = res.into_json::<serde_json::Value>() {
            let ids: Vec<String> = json
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            picked = preferred_order
                .iter()
                .find(|p| ids.iter().any(|id| id == **p))
                .map(|s| (*s).to_string())
                .or_else(|| ids.first().cloned())
                .unwrap_or_else(|| preferred_order[0].to_string());
        }
    }
    if let Ok(mut guard) = CACHED.lock() {
        *guard = Some((base_url.to_string(), picked.clone(), std::time::Instant::now()));
    }
    picked
}

/// Transcribe already-recorded audio (any sample rate, mono) with the local Whisper service.
/// Used for radio intercepts recorded to a file.
pub fn transcribe_samples(
    whisper_base_url: &str,
    model: Option<&str>,
    samples: &[f32],
    sample_rate: u32,
) -> Result<String, String> {
    let (wav, _peak, _duration_ms) = samples_to_wav_16k(samples, sample_rate)?;
    let model_id = resolve_model(whisper_base_url, model);
    whisper_http_transcribe(whisper_base_url, &model_id, wav)
}

#[tauri::command]
pub fn start_ptt_recording(state: State<'_, Arc<PttRecordState>>) -> Result<(), String> {
    {
        let mut flag = state.is_recording.lock().map_err(|_| "ptt lock".to_string())?;
        if *flag {
            return Ok(());
        }
        *flag = true;
    }
    {
        let mut samples = state.samples.lock().map_err(|_| "ptt lock".to_string())?;
        samples.clear();
    }

    let shared = Arc::clone(&*state);
    let shared_err = Arc::clone(&*state);
    thread::Builder::new()
        .name("athena-ptt".into())
        .spawn(move || {
            if let Err(e) = record_loop(shared) {
                eprintln!("[athena:ptt] record_loop: {e}");
                if let Ok(mut flag) = shared_err.is_recording.lock() {
                    *flag = false;
                }
            }
        })
        .map_err(|e| format!("Could not start mic thread: {e}"))?;

    // Brief settle so CoreAudio starts delivering buffers before the user speaks.
    thread::sleep(Duration::from_millis(35));
    Ok(())
}

#[tauri::command]
pub async fn stop_ptt_and_transcribe(
    state: State<'_, Arc<PttRecordState>>,
    whisper_base_url: String,
    model: Option<String>,
) -> Result<PttTranscriptResult, String> {
    let shared = Arc::clone(&*state);
    tokio::task::spawn_blocking(move || {
        let (samples, sample_rate) = take_recording(&shared)?;
        let n = samples.len() as u32;
        let (wav, peak, duration_ms) = samples_to_wav_16k(&samples, sample_rate)?;
        eprintln!(
            "[athena:ptt] captured samples={n} duration_ms={duration_ms} peak={peak:.4} wav_bytes={}",
            wav.len()
        );

        let model_id = resolve_model(&whisper_base_url, model.as_deref());
        let text = whisper_http_transcribe(&whisper_base_url, &model_id, wav)?;
        eprintln!(
            "[athena:ptt] transcript model={model_id} text={:?}",
            text.chars().take(80).collect::<String>()
        );

        Ok(PttTranscriptResult {
            text,
            duration_ms,
            sample_rate: 16_000,
            peak,
            samples: n,
            wav_base64: None,
        })
    })
    .await
    .map_err(|e| format!("PTT task failed: {e}"))?
}

/// Stop without STT — returns WAV for callers that still want the blob.
#[tauri::command]
pub fn stop_ptt_recording(state: State<'_, Arc<PttRecordState>>) -> Result<PttTranscriptResult, String> {
    let (samples, sample_rate) = take_recording(&state)?;
    let n = samples.len() as u32;
    let (wav, peak, duration_ms) = samples_to_wav_16k(&samples, sample_rate)?;
    Ok(PttTranscriptResult {
        text: String::new(),
        duration_ms,
        sample_rate: 16_000,
        peak,
        samples: n,
        wav_base64: Some(STANDARD.encode(&wav)),
    })
}

#[tauri::command]
pub fn cancel_ptt_recording(state: State<'_, Arc<PttRecordState>>) -> Result<(), String> {
    state.request_stop();
    if let Ok(mut samples) = state.samples.lock() {
        samples.clear();
    }
    Ok(())
}

pub fn managed_state() -> Arc<PttRecordState> {
    Arc::new(PttRecordState::default())
}