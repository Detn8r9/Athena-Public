use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::{load_config, save_config};

/// Default Ollama base URL (override with `ATHENA_OLLAMA_BASE_URL` for integration tests).
const OLLAMA_HOST_DEFAULT: &str = "http://127.0.0.1:11434";

fn ollama_base_url() -> String {
    std::env::var("ATHENA_OLLAMA_BASE_URL").unwrap_or_else(|_| OLLAMA_HOST_DEFAULT.to_string())
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

pub fn get_available_models() -> Result<Vec<ModelInfo>, String> {
    let url = format!("{}/api/tags", ollama_base_url());
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

fn try_spawn_ollama_serve(program: impl AsRef<std::ffi::OsStr>) -> bool {
    Command::new(program.as_ref())
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
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
pub fn try_launch_ollama_server_with_preferences(extra_paths_first: &[std::path::PathBuf]) {
    if is_ollama_running() {
        return;
    }
    for p in extra_paths_first {
        if p.as_os_str().is_empty() {
            continue;
        }
        if try_spawn_ollama_serve(p) {
            return;
        }
    }
    for bin in ollama_binary_paths() {
        if try_spawn_ollama_serve(bin) {
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

/// Small instruct model used when Ollama is running but the library is empty (first-run assist).
const DEFAULT_OLLAMA_INSTRUCT_MODEL: &str = "llama3.2:3b";

/// Best-effort: if Ollama responds on HTTP but `/api/tags` is empty, pull a default model and pin it in config.
pub fn maybe_ensure_default_ollama_model(app_data_dir: &Path) {
    if !is_ollama_running() {
        return;
    }
    let Ok(models) = get_available_models() else {
        return;
    };
    if !models.is_empty() {
        return;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(900))
        .build()
    else {
        return;
    };
    let url = format!("{}/api/pull", ollama_base_url());
    let body = serde_json::json!({ "name": DEFAULT_OLLAMA_INSTRUCT_MODEL, "stream": false });
    let Ok(resp) = client.post(url).json(&body).send() else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let _ = resp.bytes();
    let Ok(after) = get_available_models() else {
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
        c.explicit_ollama_model = Some(DEFAULT_OLLAMA_INSTRUCT_MODEL.to_string());
        let _ = save_config(app_data_dir, &c);
    }
}


pub fn chat(
    model: &str,
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
) -> Result<String, String> {
    let client = blocking_client_chat()?;
    let url = format!("{}/api/chat", ollama_base_url());
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

    let response = client
        .post(format!("{}/api/chat", ollama_base_url()))
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
