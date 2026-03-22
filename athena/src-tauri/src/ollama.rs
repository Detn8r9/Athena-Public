use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const OLLAMA_HOST: &str = "http://127.0.0.1:11434";

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

#[derive(Debug, Serialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
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
    let url = format!("{}/api/tags", OLLAMA_HOST);
    let resp = ureq::get(&url).call().map_err(|e| e.to_string())?;
    let body: TagsResponse = resp.into_json().map_err(|e| e.to_string())?;
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
    ureq::get(&format!("{}/api/tags", OLLAMA_HOST))
        .call()
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

pub fn try_launch_ollama_server() {
    if is_ollama_running() {
        return;
    }
    for bin in ollama_binary_paths() {
        let result = Command::new(bin)
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if result.is_ok() {
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

pub fn chat(model: &str, messages: Vec<ChatMessage>) -> Result<String, String> {
    let url = format!("{}/api/chat", OLLAMA_HOST);
    let body = OllamaChatRequest {
        model: model.to_string(),
        messages,
        stream: false,
    };
    let resp = ureq::post(&url)
        .send_json(serde_json::to_value(body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let parsed: OllamaChatResponse = resp.into_json().map_err(|e| e.to_string())?;
    Ok(parsed.message.map(|m| m.content).unwrap_or_default())
}

pub fn chat_stream<F, G>(model: &str, messages: Vec<ChatMessage>, on_chunk: &mut F, on_done: G) -> Result<(), String>
where
    F: FnMut(&str),
    G: FnOnce(),
{
    let url = format!("{}/api/chat", OLLAMA_HOST);
    let body = OllamaChatRequest {
        model: model.to_string(),
        messages,
        stream: true,
    };
    let resp = ureq::post(&url)
        .send_json(serde_json::to_value(body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let reader = BufReader::new(resp.into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(line) {
            if let Some(ref msg) = chunk.message {
                if let Some(ref content) = msg.content {
                    on_chunk(content);
                }
            }
            if chunk.done == Some(true) {
                break;
            }
        }
    }
    on_done();
    Ok(())
}

/// Generate a short chat title (3–6 words) from the first user message. Uses non-streaming chat.
pub fn generate_chat_title(model: &str, first_message: &str) -> Result<String, String> {
    let truncated = first_message.trim();
    let prompt = if truncated.len() > 400 {
        format!("{}…", truncated.chars().take(400).collect::<String>())
    } else {
        truncated.to_string()
    };
    let system = "You generate very short chat titles. Reply with only the title: 3 to 6 words, no quotes, no punctuation at the end.";
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Generate a short title for this question or topic:\n\n{}", prompt),
        },
    ];
    let out = chat(model, messages)?;
    let title = out.trim().trim_matches('"').chars().take(80).collect::<String>().trim().to_string();
    Ok(if title.is_empty() { "New chat".to_string() } else { title })
}

/// Generate a one-line description of what the conversation is about. Uses non-streaming chat.
pub fn generate_chat_description(model: &str, messages: &[ChatMessage]) -> Result<String, String> {
    if messages.is_empty() {
        return Ok(String::new());
    }
    let summary: String = messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content.chars().take(300).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n\n");
    let combined = if summary.len() > 1500 {
        format!("{}…", summary.chars().take(1500).collect::<String>())
    } else {
        summary
    };
    let system = "You summarize conversations in one short sentence. Reply with only that sentence: what the conversation is about, no quotes.";
    let msgs = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Summarize this conversation in one short sentence:\n\n{}", combined),
        },
    ];
    let out = chat(model, msgs)?;
    let desc = out.trim().trim_matches('"').chars().take(200).collect::<String>().trim().to_string();
    Ok(desc)
}
