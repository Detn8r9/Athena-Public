//! Local TTS via Piper for character voices (default, fallout, ultron).
//! Voice models: app_data_dir/voices/<voice_id>/model.onnx + model.onnx.json

use piper_rs::synth::PiperSpeechSynthesizer;
use riff_wave::WaveWriter;
use std::io::Cursor;
use std::path::Path;

const VOICES_SUBDIR: &str = "voices";

fn voice_config_path(app_data_dir: &Path, voice_id: &str) -> std::path::PathBuf {
    app_data_dir.join(VOICES_SUBDIR).join(voice_id).join("model.onnx.json")
}

fn add_static_noise(samples: &mut [f32], amount: f32) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    for (i, s) in samples.iter_mut().enumerate() {
        i.hash(&mut h);
        let noise = ((h.finish() % 1000) as f32 / 1000.0 - 0.5) * 2.0 * amount;
        *s = (*s + noise).clamp(-1.0, 1.0);
    }
}

pub fn synthesize_speech(
    app_data_dir: &Path,
    text: &str,
    voice_id: &str,
) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Empty text".to_string());
    }

    let config_path = voice_config_path(app_data_dir, voice_id);
    if !config_path.exists() {
        return Err(format!(
            "Voice model not found. Place Piper model files in: {}",
            config_path.parent().unwrap().display()
        ));
    }

    let model = piper_rs::from_config_path(config_path.as_path()).map_err(|e| e.to_string())?;
    let synth = PiperSpeechSynthesizer::new(model).map_err(|e| e.to_string())?;
    let info = synth.audio_output_info().map_err(|e| e.to_string())?;

    let mut all_samples: Vec<f32> = Vec::new();
    for result in synth
        .synthesize_parallel(text.to_string(), None)
        .map_err(|e| e.to_string())?
    {
        let audio = result.map_err(|e| e.to_string())?;
        all_samples.extend(audio.into_vec());
    }

    if all_samples.is_empty() {
        return Err("No speech generated".to_string());
    }

    if voice_id == "fallout" {
        add_static_noise(&mut all_samples, 0.015);
    }

    let samples_i16: Vec<i16> = all_samples
        .iter()
        .map(|&f| (f.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();

    let mut buf = Cursor::new(Vec::<u8>::new());
    let mut wave = WaveWriter::new(
        info.num_channels as u16,
        info.sample_rate as u32,
        (info.sample_width * 8) as u16,
        &mut buf,
    )
    .map_err(|e| e.to_string())?;

    for &s in &samples_i16 {
        wave.write_sample_i16(s).map_err(|e| e.to_string())?;
    }
    wave.sync_header().map_err(|e| e.to_string())?;

    Ok(buf.into_inner())
}
