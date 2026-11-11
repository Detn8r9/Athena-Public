// Persist chat sessions to app data so users can return after app restart.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedChat {
    pub id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub messages: Vec<SavedMessage>,
    #[serde(default)]
    /// Short summary of conversation context (topics, entities) for retrieval and resolving "it"/"that".
    pub context_summary: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub updated_at: i64,
}

fn chats_dir(app_data_dir: &Path) -> std::path::PathBuf {
    crate::athena_paths::AthenaPaths::new(app_data_dir).user_chats_dir()
}

fn chat_path(app_data_dir: &Path, id: &str) -> std::path::PathBuf {
    chats_dir(app_data_dir).join(format!("{}.json", sanitize_id(id)))
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect()
}

#[allow(dead_code)] // Legacy file-backed listing; local UI uses SQLite `local_chats`.
pub fn list_chats(app_data_dir: &Path) -> Result<Vec<ChatSummary>, String> {
    let dir = chats_dir(app_data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut summaries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(chat) = serde_json::from_str::<SavedChat>(&data) {
                summaries.push(ChatSummary {
                    id: chat.id,
                    title: chat.title,
                    description: chat.description,
                    updated_at: chat.updated_at,
                });
            }
        }
    }
    summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(summaries)
}

pub fn get_chat(app_data_dir: &Path, id: String) -> Result<Option<SavedChat>, String> {
    let path = chat_path(app_data_dir, &id);
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let chat = serde_json::from_str::<SavedChat>(&data).map_err(|e| e.to_string())?;
    Ok(Some(chat))
}

pub fn save_chat(
    app_data_dir: &Path,
    id: Option<String>,
    title: String,
    messages: Vec<SavedMessage>,
    description: Option<String>,
) -> Result<String, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let dir = chats_dir(app_data_dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // If no id was provided, try to detect an existing chat with the same
    // message history so we update it instead of creating duplicates.
    let mut resolved_id = id;
    if resolved_id.is_none() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(existing) = serde_json::from_str::<SavedChat>(&data) {
                        if existing.messages.len() == messages.len()
                            && existing
                                .messages
                                .iter()
                                .zip(messages.iter())
                                .all(|(a, b)| a.role == b.role && a.content == b.content)
                        {
                            // Reuse this chat id.
                            resolved_id = Some(existing.id);
                            break;
                        }
                    }
                }
            }
        }
    }

    let (id, created_at, existing_summary) = if let Some(existing_id) = resolved_id {
        let path = chat_path(app_data_dir, &existing_id);
        let (created, summary) = if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                match serde_json::from_str::<SavedChat>(&data).ok() {
                    Some(chat) => (chat.created_at, chat.context_summary),
                    None => (now, None),
                }
            } else {
                (now, None)
            }
        } else {
            (now, None)
        };
        (existing_id, created, summary)
    } else {
        let new_id = format!("chat_{}", now);
        (new_id, now, None)
    };

    let chat = SavedChat {
        id: id.clone(),
        user_id: None,
        title: if title.is_empty() {
            "New chat".to_string()
        } else {
            title
        },
        description,
        messages,
        context_summary: existing_summary,
        created_at,
        updated_at: now,
    };
    let path = chat_path(app_data_dir, &id);
    let data = serde_json::to_string_pretty(&chat).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Update the context summary from the last exchange. Appends a short retrieval-friendly line so
/// follow-up questions (e.g. "what brand is the doorbell?") can resolve "doorbell" from context.
#[allow(dead_code)] // Legacy file-backed path; mesh JSON chats may still use file helpers elsewhere.
pub fn update_chat_context_summary(
    app_data_dir: &Path,
    id: String,
    last_user_msg: &str,
    last_assistant_msg: &str,
) -> Result<(), String> {
    let path = chat_path(app_data_dir, &id);
    if !path.exists() {
        return Err("Chat not found".to_string());
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut chat: SavedChat = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    // Build a short line: key topics/entities from the exchange (for retrieval and "it"/"that" resolution).
    let combined = format!("{} {}", last_user_msg, last_assistant_msg);
    let words: Vec<&str> = combined
        .split_whitespace()
        .filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '\''))
        .take(50)
        .collect();
    let new_line = words.join(" ");
    let updated = if let Some(ref existing) = chat.context_summary {
        let trimmed = existing.trim();
        if trimmed.is_empty() {
            new_line
        } else {
            format!("{} {}", trimmed, new_line)
        }
    } else {
        new_line
    };
    // Keep last ~600 chars so follow-up retrieval has enough context (e.g. address, doc names).
    chat.context_summary = Some(if updated.len() > 600 {
        updated
            .chars()
            .skip(updated.len() - 600)
            .collect::<String>()
    } else {
        updated
    });

    chat.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let out = serde_json::to_string_pretty(&chat).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(())
}

/// Update only title and/or description of an existing chat. Preserves messages and created_at.
#[allow(dead_code)]
pub fn update_chat_metadata(
    app_data_dir: &Path,
    id: String,
    title: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    let path = chat_path(app_data_dir, &id);
    if !path.exists() {
        return Err("Chat not found".to_string());
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut chat: SavedChat = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    if let Some(t) = title {
        chat.title = t;
    }
    if let Some(d) = description {
        chat.description = Some(d);
    }
    chat.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let out = serde_json::to_string_pretty(&chat).map_err(|e| e.to_string())?;
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub fn delete_chat(app_data_dir: &Path, id: String) -> Result<(), String> {
    let path = chat_path(app_data_dir, &id);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
