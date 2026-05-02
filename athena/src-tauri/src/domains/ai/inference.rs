//! Resolve inference backend and model; dispatch to Ollama or OpenAI-compatible.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::ollama::{
    self, chat, chat_stream, get_available_models, is_ollama_running, select_model_for_tier,
    ChatMessage, ModelInfo, ModelOptions,
};
use crate::model_manager;
use crate::openai_chat::{
    chat_completions_blocking, chat_completions_stream, is_allowed_openai_base_url,
    probe_openai_compatible,
};

fn effective_openai_base_url(config: &AppConfig) -> Option<String> {
    let from_cfg = config
        .openai_compatible_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    from_cfg.or_else(|| model_manager::managed_llama_openai_base_url())
}

pub fn is_openai_compatible_backend(config: &AppConfig) -> bool {
    matches!(
        config.inference_backend.as_deref().map(|s| s.to_lowercase()),
        Some(ref s) if s == "openai_compatible" || s == "openai"
    )
}

/// Returns error if OpenAI-compatible is selected but URL is invalid or unreachable.
pub fn ensure_inference_ready(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
) -> Result<(), String> {
    if is_openai_compatible_backend(config) {
        let base = effective_openai_base_url(config).ok_or_else(|| {
            "Set an OpenAI-compatible base URL (e.g. http://127.0.0.1:1234/v1) in Settings, or start the managed local llama server."
                .to_string()
        })?;
        if !is_allowed_openai_base_url(&base) {
            return Err("OpenAI-compatible base URL must use 127.0.0.1 or localhost.".to_string());
        }
        if !probe_openai_compatible(&base) {
            return Err(
                "Cannot reach OpenAI-compatible server. Start LM Studio (or similar) and ensure the API is on."
                    .to_string(),
            );
        }
        let _ = resolve_model_name(config, ollama_models)?;
        Ok(())
    } else {
        if !is_ollama_running() {
            return Err(
                "Ollama is not running. Start Ollama and ensure a model is installed.".to_string(),
            );
        }
        if ollama_models.is_empty() {
            return Err("No models found in Ollama.".to_string());
        }
        let _ = resolve_model_name(config, ollama_models)?;
        Ok(())
    }
}

pub fn resolve_model_name(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
) -> Result<String, String> {
    if is_openai_compatible_backend(config) {
        return config
            .openai_compatible_model
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Set a model name for OpenAI-compatible mode in Settings.".to_string());
    }
    if let Some(ref name) = config.explicit_ollama_model {
        let t = name.trim();
        if !t.is_empty() && ollama_models.iter().any(|m| m.name == t) {
            return Ok(t.to_string());
        }
    }
    let tier = config
        .model_override
        .clone()
        .unwrap_or_else(|| ollama::get_system_specs().recommended_tier);
    select_model_for_tier(ollama_models, &tier)
        .ok_or_else(|| "No suitable model found for the selected tier.".to_string())
}

pub fn chat_unified(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
) -> Result<String, String> {
    let model = resolve_model_name(config, ollama_models)?;
    if is_openai_compatible_backend(config) {
        let base = effective_openai_base_url(config).ok_or_else(|| "Missing base URL.".to_string())?;
        let key = config.openai_compatible_api_key.as_deref();
        chat_completions_blocking(&base, key, &model, messages, options)
    } else {
        chat(&model, messages, options)
    }
}

/// Ollama model list is skipped when using OpenAI-compatible backend (not needed for resolution).
pub fn load_models_for_inference(config: &AppConfig) -> Vec<ModelInfo> {
    if is_openai_compatible_backend(config) {
        Vec::new()
    } else {
        get_available_models().unwrap_or_default()
    }
}

/// Title generation using the active inference backend (same trimming rules as legacy Ollama-only helpers).
pub fn generate_chat_title_unified(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    first_message: &str,
) -> Result<String, String> {
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
            content: format!(
                "Generate a short title for this question or topic:\n\n{}",
                prompt
            ),
        },
    ];
    let out = chat_unified(config, ollama_models, messages, None)?;
    let title = out
        .trim()
        .trim_matches('"')
        .chars()
        .take(80)
        .collect::<String>()
        .trim()
        .to_string();
    Ok(if title.is_empty() {
        "New chat".to_string()
    } else {
        title
    })
}

/// One-line description using the active inference backend.
pub fn generate_chat_description_unified(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    messages: &[ChatMessage],
) -> Result<String, String> {
    if messages.is_empty() {
        return Ok(String::new());
    }
    let summary: String = messages
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                m.role,
                m.content.chars().take(300).collect::<String>()
            )
        })
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
            content: format!(
                "Summarize this conversation in one short sentence:\n\n{}",
                combined
            ),
        },
    ];
    let out = chat_unified(config, ollama_models, msgs, None)?;
    let desc = out
        .trim()
        .trim_matches('"')
        .chars()
        .take(200)
        .collect::<String>()
        .trim()
        .to_string();
    Ok(desc)
}

pub fn chat_stream_unified<F, G>(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    messages: Vec<ChatMessage>,
    on_chunk: &mut F,
    on_done: G,
    options: Option<ModelOptions>,
    stream_cancel: Option<Arc<AtomicBool>>,
) -> Result<(), String>
where
    F: FnMut(&str),
    G: FnOnce(),
{
    let model = resolve_model_name(config, ollama_models)?;
    if is_openai_compatible_backend(config) {
        let base = effective_openai_base_url(config).ok_or_else(|| "Missing base URL.".to_string())?;
        let key = config.openai_compatible_api_key.as_deref();
        chat_completions_stream(
            &base,
            key,
            &model,
            messages,
            on_chunk,
            on_done,
            options,
            stream_cancel,
        )
    } else {
        chat_stream(
            &model,
            messages,
            on_chunk,
            on_done,
            options,
            stream_cancel,
        )
    }
}
