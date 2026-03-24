//! OpenAI-compatible HTTP API (`/v1/chat/completions`) for local servers (LM Studio, etc.).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use std::sync::mpsc::SyncSender;

use crate::ollama::{ChatMessage, ModelOptions};

const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const CHAT_TIMEOUT: Duration = Duration::from_secs(120);
const STREAM_IDLE: Duration = Duration::from_secs(30);
const STREAM_MAX: Duration = Duration::from_secs(120);

/// Allow only loopback-ish URLs to reduce SSRF risk.
pub fn is_allowed_openai_base_url(url: &str) -> bool {
    let u = url.trim().to_lowercase();
    u.starts_with("http://127.0.0.1")
        || u.starts_with("http://localhost")
        || u.starts_with("https://127.0.0.1")
        || u.starts_with("https://localhost")
}

fn normalize_base(base: &str) -> String {
    let t = base.trim().trim_end_matches('/');
    if t.ends_with("/v1") {
        t.to_string()
    } else {
        format!("{}/v1", t)
    }
}

pub fn probe_openai_compatible(base_url: &str) -> bool {
    if !is_allowed_openai_base_url(base_url) {
        return false;
    }
    let base = normalize_base(base_url);
    let url = format!("{}/models", base);
    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(PROBE_TIMEOUT)
        .timeout(PROBE_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.get(&url).send().is_ok()
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMsg>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct OpenAiMsg {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Option<Vec<OpenAiChoice>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: Option<OpenAiMsgBody>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMsgBody {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Option<Vec<OpenAiStreamChoice>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}

pub fn chat_completions_blocking(
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
) -> Result<String, String> {
    if !is_allowed_openai_base_url(base_url) {
        return Err("Base URL must use http://127.0.0.1 or http://localhost (HTTPS allowed on loopback).".to_string());
    }
    let base = normalize_base(base_url);
    let url = format!("{}/chat/completions", base);
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(CHAT_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let msgs: Vec<OpenAiMsg> = messages
        .into_iter()
        .map(|m| OpenAiMsg {
            role: m.role,
            content: m.content,
        })
        .collect();
    let mut body = OpenAiChatRequest {
        model: model.to_string(),
        messages: msgs,
        stream: false,
        temperature: options.map(|o| o.temperature),
    };
    if body.temperature.is_none() {
        body.temperature = Some(0.7);
    }
    let mut req = client.post(&url).json(&body);
    if let Some(k) = api_key.filter(|s| !s.trim().is_empty()) {
        req = req.header("Authorization", format!("Bearer {}", k.trim()));
    }
    let resp = req.send().map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    let parsed: OpenAiChatResponse = resp.json().map_err(|e| e.to_string())?;
    Ok(parsed
        .choices
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.message)
        .and_then(|m| m.content)
        .unwrap_or_default())
}

async fn openai_stream_read(
    base_url: String,
    api_key: Option<String>,
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
    tx: SyncSender<Result<Option<String>, String>>,
) -> Result<(), String> {
    if !is_allowed_openai_base_url(&base_url) {
        return Err("Invalid base URL.".to_string());
    }
    let base = normalize_base(&base_url);
    let url = format!("{}/chat/completions", base);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let msgs: Vec<OpenAiMsg> = messages
        .into_iter()
        .map(|m| OpenAiMsg {
            role: m.role,
            content: m.content,
        })
        .collect();
    let mut body = OpenAiChatRequest {
        model,
        messages: msgs,
        stream: true,
        temperature: options.map(|o| o.temperature).or(Some(0.7)),
    };
    if body.temperature.is_none() {
        body.temperature = Some(0.7);
    }
    let mut req = client.post(&url).json(&body);
    if let Some(ref k) = api_key {
        if !k.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", k.trim()));
        }
    }
    let response = req
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let stream_start = Instant::now();
    let mut saw_done = false;

    loop {
        if stream_start.elapsed() > STREAM_MAX {
            return Err("OpenAI-compatible stream exceeded maximum duration.".to_string());
        }
        let next = tokio::time::timeout(STREAM_IDLE, stream.next()).await;
        let stream_item = match next {
            Err(_) => return Err("Stream stalled (no data).".to_string()),
            Ok(item) => item,
        };
        match stream_item {
            None => break,
            Some(Err(e)) => return Err(e.to_string()),
            Some(Ok(bytes)) => {
                buf.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim().to_string();
                    buf.drain(..pos + 1);
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let payload = line.strip_prefix("data: ").unwrap_or(&line).trim();
                    if payload == "[DONE]" {
                        saw_done = true;
                        break;
                    }
                    if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(payload) {
                        if let Some(choices) = chunk.choices {
                            for c in choices {
                                if let Some(d) = c.delta {
                                    if let Some(content) = d.content {
                                        if !content.is_empty() {
                                            tx.send(Ok(Some(content))).map_err(|_| "channel closed")?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if saw_done {
            break;
        }
    }
    tx.send(Ok(None)).map_err(|_| "channel closed")?;
    Ok(())
}

pub fn chat_completions_stream<F, G>(
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    messages: Vec<ChatMessage>,
    on_chunk: &mut F,
    on_done: G,
    options: Option<ModelOptions>,
) -> Result<(), String>
where
    F: FnMut(&str),
    G: FnOnce(),
{
    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Option<String>, String>>(64);
    let base = base_url.to_string();
    let key = api_key.map(|s| s.to_string());
    let model = model.to_string();
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
        let result = rt.block_on(openai_stream_read(
            base,
            key,
            model,
            messages,
            options,
            tx.clone(),
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
                return Err("Stream channel closed.".to_string());
            }
        }
    }
    let _ = handle.join();
    on_done();
    Ok(())
}
