use crate::config::AppConfig;
use crate::db::Database;
use crate::inference;
use crate::ollama::{ChatMessage, ModelInfo};
use serde_json::json;
use std::time::Duration;

const LEARNED_SUFFIX: &str = "::learned";

fn learned_path_for(source_path: &str) -> String {
    format!("{}{}", source_path, LEARNED_SUFFIX)
}

fn truncate_for_model(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    format!("{}...", &s[..max_chars])
}

pub fn ensure_learned_note_for_text(
    db: &Database,
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    source_path: &str,
    source_name: &str,
    source_text: &str,
) -> Result<(), String> {
    let learned_path = learned_path_for(source_path);
    if db.get_by_path(&learned_path)?.is_some() {
        return Ok(());
    }

    // Keep this small and highly searchable; we want stable tokens the retriever can grab.
    let source_preview = truncate_for_model(source_text, 18_000);
    let system = r#"You are building a compact "knowledge note" from a user's document for later retrieval.

Rules:
- Only use facts that are explicitly present in the document text provided. Do not guess.
- If a detail is not stated, write "unknown".
- Prefer short, literal phrases that are easy to search.
- Output must be plain text in the following format:

TITLE: <short title>
SOURCE_PATH: <repeat exactly>
KEY_FACTS:
- <fact 1>
- <fact 2>
SEARCH_TERMS: <comma-separated keywords/phrases>
Q_AND_A:
Q: <question someone might ask>
A: <answer grounded in the text OR 'unknown'>

Keep it under ~1200 words."#;

    let user = format!(
        "SOURCE_PATH: {}\nSOURCE_NAME: {}\n\nDOCUMENT_TEXT:\n{}",
        source_path, source_name, source_preview
    );

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user,
        },
    ];

    // Light retry: local inference may be starting up or busy.
    let mut last_err: Option<String> = None;
    for _ in 0..2 {
        match inference::chat_unified(config, ollama_models, messages.clone(), None) {
            Ok(note) => {
                let meta = json!({
                    "source_path": source_path,
                    "source_name": source_name,
                    "kind": "learned_note"
                })
                .to_string();
                db.upsert_file(
                    &learned_path,
                    "learned",
                    &format!("Learned: {}", source_name),
                    None,
                    None,
                    Some(note.trim()),
                    Some(&meta),
                )?;
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(600));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "Failed to build learned note.".to_string()))
}

pub fn build_learned_notes_for_all_text(
    db: &Database,
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    limit: usize,
) -> Result<usize, String> {
    let all = db.get_all()?;
    let mut made = 0usize;
    for f in all {
        if made >= limit {
            break;
        }
        if f.file_type != "text" {
            continue;
        }
        let Some(text) = f.text_content.clone() else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        // Skip very large files; they can be processed manually later.
        if text.len() > 2_000_000 {
            continue;
        }
        if ensure_learned_note_for_text(db, config, ollama_models, &f.path, &f.name, &text).is_ok() {
            made += 1;
        }
    }
    Ok(made)
}
