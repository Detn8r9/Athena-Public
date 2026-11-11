use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::{load_config, save_config, AppConfig};

/// Default Ollama base URL (override with `ATHENA_OLLAMA_BASE_URL` for integration tests).
pub const OLLAMA_HOST_DEFAULT: &str = "http://127.0.0.1:11434";

/// Strict timeout for Hive Mind `/api/tags` health checks before routing chat traffic.
const OLLAMA_HIVE_HEALTH_TIMEOUT: Duration = Duration::from_secs(1);

fn ollama_base_url() -> String {
    std::env::var("ATHENA_OLLAMA_BASE_URL").unwrap_or_else(|_| OLLAMA_HOST_DEFAULT.to_string())
}

fn blocking_client_hive_health() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(OLLAMA_HIVE_HEALTH_TIMEOUT)
        .timeout(OLLAMA_HIVE_HEALTH_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

/// Returns `true` when `GET {base}/api/tags` completes with HTTP success within [`OLLAMA_HIVE_HEALTH_TIMEOUT`].
pub fn ollama_hive_tags_reachable(base: &str) -> bool {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return false;
    }
    let Ok(client) = blocking_client_hive_health() else {
        return false;
    };
    let url = format!("{base}/api/tags");
    client
        .get(url)
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Normalize user input in Settings (`hive_mind_seed_url`) to an Ollama HTTP origin (adds `http://` and `:11434` when omitted).
fn normalize_user_ollama_base(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let with_scheme = if t.contains("://") {
        t.to_string()
    } else {
        format!("http://{t}")
    };
    let lower = with_scheme.to_lowercase();
    let mut out = with_scheme.trim_end_matches('/').to_string();
    for prefix in ["http://", "https://"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let host_part = rest.split('/').next().unwrap_or(rest);
            // If there is no `:` in the host:port segment, assume default Ollama port 11434.
            if !host_part.contains(':') {
                let insert_at = prefix.len() + host_part.len();
                out.insert_str(insert_at, ":11434");
            }
            break;
        }
    }
    Some(out)
}

fn hive_seed_ollama_base(config: &AppConfig) -> Option<String> {
    config
        .hive_mind_seed_url
        .as_deref()
        .and_then(normalize_user_ollama_base)
}

/// Resolved Ollama HTTP base for chat and `/api/tags` model listing.
///
/// - Honors `ATHENA_OLLAMA_BASE_URL` when set (integration tests / overrides).
/// - When `config` is `Some`, probes `hive_mind_seed_url` with a 1s `/api/tags` check; on success uses it.
/// - Otherwise uses the local sidecar at [`OLLAMA_HOST_DEFAULT`]. Hive probe failures are silent (no user error).
pub fn effective_ollama_base_url(config: Option<&AppConfig>) -> String {
    if let Ok(v) = std::env::var("ATHENA_OLLAMA_BASE_URL") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    match config {
        None => OLLAMA_HOST_DEFAULT.to_string(),
        Some(cfg) => {
            if let Some(ref hive) = hive_seed_ollama_base(cfg) {
                if ollama_hive_tags_reachable(hive) {
                    return hive.clone();
                }
            }
            OLLAMA_HOST_DEFAULT.to_string()
        }
    }
}

/// True when the resolved base is the default local Ollama sidecar (used for header route labeling).
pub fn ollama_base_is_local_node(base: &str) -> bool {
    let u = base.trim().trim_end_matches('/').to_lowercase();
    u == "http://127.0.0.1:11434" || u == "http://localhost:11434"
}

/// Upper bound for a single non-streaming `/api/chat` call (initial response).
const OLLAMA_CHAT_TIMEOUT: Duration = Duration::from_secs(120);
/// Time allowed for `/api/tags` and availability probes.
const OLLAMA_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Idle timeout between chunks on the streaming HTTP body (no bytes received).
const OLLAMA_STREAM_READ_IDLE: Duration = Duration::from_secs(30);
/// Wall-clock cap for the entire streaming response (first byte to completion).
const OLLAMA_STREAM_MAX_DURATION: Duration = Duration::from_secs(120);

fn ollama_stream_read_idle() -> Duration {
    std::env::var("ATHENA_OLLAMA_STREAM_IDLE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(OLLAMA_STREAM_READ_IDLE)
}

fn ollama_stream_max_duration() -> Duration {
    std::env::var("ATHENA_OLLAMA_STREAM_MAX_DURATION_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(OLLAMA_STREAM_MAX_DURATION)
}

fn blocking_client_chat() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .timeout(OLLAMA_CHAT_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

fn blocking_client_probe() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(OLLAMA_PROBE_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemSpecs {
    pub ram_gb: u64,
    pub cpu_cores: usize,
    pub recommended_tier: String,
    /// Max "previous messages" the UI should allow; scales with installed RAM.
    pub max_memory_messages: u32,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Option<Vec<OllamaModel>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub tier: String,
    pub size_estimate: u32,
}

/// Inference options forwarded to Ollama's JSON `options` object (`/api/chat`, `/api/generate`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ModelOptions {
    pub temperature: f32,
    pub num_ctx: u32,
    /// Cap generated tokens (Ollama `num_predict`). Shorter = faster Conversation turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ModelOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Ollama vision: raw base64 image payloads (no `data:` prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            images: None,
        }
    }

    pub fn with_images(mut self, images: Vec<String>) -> Self {
        if !images.is_empty() {
            self.images = Some(images);
        }
        self
    }
}

#[derive(Debug, Deserialize)]
pub struct OllamaChatResponse {
    pub message: Option<OllamaMessage>,
}

#[derive(Debug, Deserialize)]
pub struct OllamaMessage {
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    message: Option<OllamaStreamMessage>,
    done: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamMessage {
    content: Option<String>,
}

pub fn get_system_specs() -> SystemSpecs {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    sys.refresh_all();
    let ram_gb = sys.total_memory() / (1024 * 1024 * 1024);
    let ram_gb = if ram_gb == 0 { 4 } else { ram_gb };
    let cpu_cores = sys.cpus().len().max(1);
    let recommended_tier = recommended_tier_for_specs(ram_gb, cpu_cores);
    let max_memory_messages = max_memory_messages_for_ram(ram_gb);
    SystemSpecs {
        ram_gb,
        cpu_cores,
        recommended_tier: recommended_tier.to_string(),
        max_memory_messages,
    }
}

/// Upper bound for chat context length (message count), by total system RAM.
/// Conservative so we do not invite OOM on the rest of the app + OS + Ollama.
pub fn max_memory_messages_for_ram(ram_gb: u64) -> u32 {
    match ram_gb {
        0..=8 => 80,
        9..=16 => 120,
        17..=24 => 160,
        25..=32 => 200,
        33..=48 => 260,
        49..=64 => 320,
        65..=96 => 380,
        _ => 450,
    }
}

fn recommended_tier_for_specs(ram_gb: u64, _cpu_cores: usize) -> &'static str {
    if ram_gb < 8 {
        "small"
    } else if ram_gb < 16 {
        "medium"
    } else if ram_gb < 32 {
        "large"
    } else {
        "xlarge"
    }
}

fn parse_size_from_model_name(name: &str) -> u32 {
    let name_lower = name.to_lowercase();
    if name_lower.contains("1b") || name_lower.contains(":1b") {
        return 1;
    }
    if name_lower.contains("3b") || name_lower.contains(":3b") {
        return 3;
    }
    if name_lower.contains("7b") || name_lower.contains(":7b") || name_lower.contains("8b") {
        return 7;
    }
    if name_lower.contains("13b") || name_lower.contains(":13b") {
        return 13;
    }
    if name_lower.contains("34b") || name_lower.contains(":34b") {
        return 34;
    }
    if name_lower.contains("70b") || name_lower.contains(":70b") {
        return 70;
    }
    5
}

fn tier_for_size(size: u32) -> &'static str {
    if size <= 3 {
        "small"
    } else if size <= 10 {
        "medium"
    } else if size <= 20 {
        "large"
    } else {
        "xlarge"
    }
}

pub fn get_available_models(config: Option<&AppConfig>) -> Result<Vec<ModelInfo>, String> {
    let base = effective_ollama_base_url(config);
    let url = format!("{}/api/tags", base.trim_end_matches('/'));
    let client = blocking_client_probe()?;
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    let body: TagsResponse = resp.json().map_err(|e| e.to_string())?;
    let models = body.models.unwrap_or_default();
    let mut out: Vec<ModelInfo> = models
        .into_iter()
        .map(|m| {
            let size = parse_size_from_model_name(&m.name);
            let tier = tier_for_size(size).to_string();
            ModelInfo {
                name: m.name,
                tier,
                size_estimate: size,
            }
        })
        .collect();
    out.sort_by_key(|m| m.size_estimate);
    Ok(out)
}

pub fn is_ollama_running() -> bool {
    let url = format!("{}/api/tags", ollama_base_url());
    if let Ok(client) = blocking_client_probe() {
        return client.get(&url).send().is_ok();
    }
    false
}

/// Polls the configured Ollama HTTP endpoint after the bundled sidecar starts.
/// Performs up to `max_attempts` GET requests to `/` with `delay_ms` between retries (no delay before the first try).
/// Returns `true` when any request completes (including non-2xx responses that still mean the server accepted TCP/HTTP).
#[allow(dead_code)]
pub async fn ollama_http_ready_after_sidecar(max_attempts: u32, delay_ms: u64) -> bool {
    let root_url = format!("{}/", ollama_base_url().trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    for attempt in 0..max_attempts {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        if client.get(&root_url).send().await.is_ok() {
            return true;
        }
    }
    false
}

fn try_spawn_ollama_serve(
    program: impl AsRef<std::ffi::OsStr>,
    models_dir: Option<&Path>,
) -> bool {
    let mut cmd = Command::new(program.as_ref());
    cmd.arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = models_dir {
        cmd.env("OLLAMA_MODELS", dir);
    }
    cmd.spawn().is_ok()
}

fn ollama_binary_paths() -> Vec<&'static str> {
    #[cfg(target_os = "macos")]
    return [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
        "ollama",
    ]
    .to_vec();
    #[cfg(target_os = "windows")]
    return ["ollama"].to_vec();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return ["/usr/bin/ollama", "/usr/local/bin/ollama", "ollama"].to_vec();
}

/// Prefer `extra_paths_first` (configured or managed CLI), then well-known locations / PATH, then macOS app bundle.
/// When `models_dir` is set, Ollama writes/reads models there (`OLLAMA_MODELS` = AthenaData/user/models/ollama).
pub fn try_launch_ollama_server_with_preferences(
    extra_paths_first: &[std::path::PathBuf],
    models_dir: Option<&Path>,
) {
    if is_ollama_running() {
        return;
    }
    for p in extra_paths_first {
        if p.as_os_str().is_empty() {
            continue;
        }
        if try_spawn_ollama_serve(p, models_dir) {
            return;
        }
    }
    for bin in ollama_binary_paths() {
        if try_spawn_ollama_serve(bin, models_dir) {
            return;
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .args(["-a", "Ollama"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Used by setup helpers to validate a path before persisting it in config.
pub fn path_looks_like_ollama_cli(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.eq_ignore_ascii_case("ollama") || n.eq_ignore_ascii_case("ollama.exe"))
            .unwrap_or(false)
}

pub fn select_model_for_tier(available: &[ModelInfo], tier: &str) -> Option<String> {
    let tier_lower = tier.to_lowercase();
    if tier_lower == "xlarge" {
        return available
            .iter()
            .rev()
            .find(|m| m.tier.to_lowercase() == "xlarge")
            .map(|m| m.name.clone())
            .or_else(|| {
                available
                    .iter()
                    .rev()
                    .find(|m| m.tier == "large")
                    .map(|m| m.name.clone())
            })
            .or_else(|| available.last().map(|m| m.name.clone()));
    }
    for m in available {
        if m.tier.to_lowercase() == tier_lower {
            return Some(m.name.clone());
        }
    }
    match tier_lower.as_str() {
        "small" => available.first().map(|m| m.name.clone()),
        "large" => available.last().map(|m| m.name.clone()),
        _ => available
            .iter()
            .find(|m| m.tier == "medium")
            .map(|m| m.name.clone())
            .or_else(|| available.first().map(|m| m.name.clone())),
    }
}

/// Prefer the bundled app seed tag; network pull is last resort when `app/models/ollama` has no blobs.
pub fn maybe_ensure_default_ollama_model(app_data_dir: &Path) {
    let paths = crate::athena_paths::AthenaPaths::new(app_data_dir);
    let _ = crate::athena_paths::ensure_layout(app_data_dir);
    let tag = crate::athena_paths::bundled_default_ollama_tag(&paths);

    // Offline-first: copy bundled Ollama blobs into the writable user store.
    if let Ok(true) = crate::athena_paths::seed_user_ollama_from_app(&paths) {
        let mut c = load_config(app_data_dir);
        let empty_explicit = c
            .explicit_ollama_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none();
        if empty_explicit {
            c.explicit_ollama_model = Some(tag.clone());
            let _ = save_config(app_data_dir, &c);
        }
    }

    if !is_ollama_running() {
        return;
    }
    let Ok(models) = get_available_models(None) else {
        return;
    };
    if !models.is_empty() {
        return;
    }

    // No local seed and empty library — last-resort network pull (setup with connectivity).
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(900))
        .build()
    else {
        return;
    };
    let url = format!("{}/api/pull", ollama_base_url());
    let body = serde_json::json!({ "name": tag, "stream": false });
    let Ok(resp) = client.post(url).json(&body).send() else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let _ = resp.bytes();
    let Ok(after) = get_available_models(None) else {
        return;
    };
    if after.is_empty() {
        return;
    }
    let mut c = load_config(app_data_dir);
    let empty_explicit = c
        .explicit_ollama_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none();
    if empty_explicit {
        c.explicit_ollama_model = Some(tag);
        let _ = save_config(app_data_dir, &c);
    }
}


pub fn chat(
    config: Option<&AppConfig>,
    model: &str,
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
) -> Result<String, String> {
    let base = effective_ollama_base_url(config);
    let client = blocking_client_chat()?;
    let url = format!("{}/api/chat", base.trim_end_matches('/'));
    let body = OllamaChatRequest {
        model: model.to_string(),
        messages,
        stream: false,
        options,
    };
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let parsed: OllamaChatResponse = resp.json().map_err(|e| e.to_string())?;
    Ok(parsed.message.map(|m| m.content).unwrap_or_default())
}

async fn ollama_stream_read_lines(
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
    tx: SyncSender<Result<Option<String>, String>>,
    cancel: Option<Arc<AtomicBool>>,
    ollama_base: String,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let body = OllamaChatRequest {
        model,
        messages,
        stream: true,
        options,
    };

    let chat_url = format!("{}/api/chat", ollama_base.trim_end_matches('/'));
    let response = client
        .post(chat_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let stream_start = Instant::now();
    let mut saw_done = false;
    let mut user_cancelled = false;

    loop {
        if cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::SeqCst))
        {
            user_cancelled = true;
            break;
        }
        if stream_start.elapsed() > ollama_stream_max_duration() {
            return Err("Ollama stream exceeded maximum duration (120s).".to_string());
        }

        let next = tokio::time::timeout(ollama_stream_read_idle(), stream.next()).await;
        let stream_item = match next {
            Err(_) => {
                return Err("Ollama stream stalled: no data for 30 seconds.".to_string());
            }
            Ok(item) => item,
        };

        match stream_item {
            None => {
                if saw_done {
                    break;
                }
                return Err("Ollama stream ended unexpectedly (connection closed).".to_string());
            }
            Some(Err(e)) => {
                return Err(e.to_string());
            }
            Some(Ok(bytes)) => {
                buf.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim().to_string();
                    buf.drain(..pos + 1);
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(&line) {
                        if let Some(ref msg) = chunk.message {
                            if let Some(ref content) = msg.content {
                                if !content.is_empty() {
                                    tx.send(Ok(Some(content.clone())))
                                        .map_err(|_| "Stream channel closed.".to_string())?;
                                }
                            }
                        }
                        if chunk.done == Some(true) {
                            saw_done = true;
                            break;
                        }
                    }
                }
            }
        }

        if saw_done {
            break;
        }
    }

    if user_cancelled {
        let _ = tx.send(Ok(None));
        return Ok(());
    }

    if !buf.trim().is_empty() {
        for rest in buf.lines() {
            let line = rest.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(line) {
                if let Some(ref msg) = chunk.message {
                    if let Some(ref content) = msg.content {
                        if !content.is_empty() {
                            tx.send(Ok(Some(content.clone())))
                                .map_err(|_| "Stream channel closed.".to_string())?;
                        }
                    }
                }
                if chunk.done == Some(true) {
                    saw_done = true;
                }
            }
        }
    }

    if !saw_done {
        return Err("Ollama stream ended without completion marker (done: true).".to_string());
    }

    tx.send(Ok(None))
        .map_err(|_| "Stream channel closed.".to_string())?;
    Ok(())
}

pub fn chat_stream<F, G>(
    config: Option<&AppConfig>,
    model: &str,
    messages: Vec<ChatMessage>,
    on_chunk: &mut F,
    on_done: G,
    options: Option<ModelOptions>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<(), String>
where
    F: FnMut(&str),
    G: FnOnce(),
{
    let ollama_base = effective_ollama_base_url(config);
    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Option<String>, String>>(64);
    let model = model.to_string();
    let cancel_c = cancel.clone();
    let handle = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        let result = rt.block_on(ollama_stream_read_lines(
            model,
            messages,
            options,
            tx.clone(),
            cancel_c,
            ollama_base,
        ));
        if let Err(e) = result {
            let _ = tx.send(Err(e));
        }
    });

    loop {
        match rx.recv() {
            Ok(Ok(Some(s))) => on_chunk(&s),
            Ok(Ok(None)) => break,
            Ok(Err(e)) => {
                let _ = handle.join();
                return Err(e);
            }
            Err(_) => {
                let _ = handle.join();
                return Err("Ollama stream channel closed unexpectedly.".to_string());
            }
        }
    }
    let _ = handle.join();
    on_done();
    Ok(())
}
