//! Meshtastic inbound text → Ollama → reply chunks for DM back over mesh.
//! One persisted chat per remote node id (`mesh_<node_num>.json`).

use std::path::Path;

use tauri::Emitter;

use crate::chats::{get_chat, save_chat, SavedMessage};
use crate::config::load_config;
use crate::inference::{self, load_models_for_inference};
use crate::mesh_calendar;
use crate::mesh_logbook;
use crate::ollama::ChatMessage;
use crate::user_profile;

/// Emitted after a mesh-origin chat is created or updated so the UI can refresh the sidebar.
pub const MESH_CHATS_UPDATED_EVENT: &str = "athena:mesh-chats-updated";

/// Meshtastic broadcast `to` value for flood / everyone.
pub const MESHTASTIC_BROADCAST_TO: u32 = 0xFFFF_FFFF;

pub fn mesh_chat_id(node_num: u32) -> String {
    format!("mesh_{}", node_num)
}

/// Primary mesh channel index (Meshtastic “Primary”; broadcast visible to all nodes on that channel).
pub const PRIMARY_MESH_CHANNEL: u32 = 0;

/// Whether we should run auto-reply: primary channel only; not from self; to us or broadcast.
/// Replies are sent as DMs so each conversation stays a private chat with Athena.
pub fn should_run_mesh_ai(from: u32, to: u32, my_node: u32, channel: u32) -> bool {
    if channel != PRIMARY_MESH_CHANNEL {
        return false;
    }
    if my_node == 0 || from == my_node {
        return false;
    }
    if to == my_node || to == MESHTASTIC_BROADCAST_TO {
        return true;
    }
    false
}

/// Split reply into UTF-8 safe chunks under max bytes (Meshtastic short-text limit).
pub fn chunk_mesh_utf8(text: &str, max_bytes: usize) -> Vec<String> {
    if max_bytes == 0 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        let clen = ch.len_utf8();
        if clen > max_bytes {
            continue;
        }
        if !current.is_empty() && current.len() + clen > max_bytes {
            out.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn mesh_system_prompt(cfg: &crate::config::AppConfig) -> String {
    let mut s = format!(
        "You are Athena, the assistant for {}. You are a local offline survival and repair assistant. You are replying over a Meshtastic LoRa mesh: keep answers concise, practical, and plain text (no markdown tables). If you are unsure, say so briefly.",
        crate::ATHENA_FULL_NAME
    );
    s.push_str(&user_profile::line_display_name_for_prompt(
        cfg.user_display_name.as_deref(),
    ));
    s
}

fn build_ollama_messages(
    system: &str,
    history_before_user: &[SavedMessage],
    new_user: &str,
    max_messages: usize,
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system.to_string(),
    }];
    let max = max_messages.max(1);
    let start = history_before_user.len().saturating_sub(max);
    for m in &history_before_user[start..] {
        messages.push(ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: new_user.to_string(),
    });
    messages
}

/// Runs Ollama, saves/updates the per-node chat file, returns text chunks to transmit.
pub fn process_mesh_message_and_reply(
    app_data_dir: &Path,
    app: Option<&tauri::AppHandle>,
    sender_id: u32,
    user_text: &str,
    display_name: &str,
) -> Result<Vec<String>, String> {
    let cfg = load_config(app_data_dir);
    if !cfg.mesh_ai_auto_reply.unwrap_or(true) {
        return Err("Mesh AI auto-reply is disabled.".to_string());
    }
    let trimmed = user_text.trim();
    if trimmed.is_empty() {
        return Err("Empty mesh message.".to_string());
    }
    if mesh_logbook::is_logbook_mesh_text(trimmed) {
        return Err("Logbook sync packet (not a chat prompt).".to_string());
    }
    if mesh_calendar::is_calendar_mesh_text(trimmed) {
        return Err("Calendar sync packet (not a chat prompt).".to_string());
    }

    let models = load_models_for_inference(&cfg);
    inference::ensure_inference_ready(&cfg, &models)?;

    let chat_id = mesh_chat_id(sender_id);
    let existing = get_chat(app_data_dir, chat_id.clone())?;

    let prev: Vec<SavedMessage> = existing
        .as_ref()
        .map(|c| c.messages.clone())
        .unwrap_or_default();

    let max_memory = cfg.memory_messages.unwrap_or(20) as usize;
    let system = mesh_system_prompt(&cfg);
    let ollama_msgs = build_ollama_messages(&system, &prev, trimmed, max_memory);

    let reply =
        inference::chat_unified(&cfg, &models, ollama_msgs, None).map_err(|e| e.to_string())?;
    let reply_trimmed = reply.trim();
    if reply_trimmed.is_empty() {
        return Err("Model returned an empty reply.".to_string());
    }

    let mut history = prev;
    history.push(SavedMessage {
        role: "user".to_string(),
        content: trimmed.to_string(),
    });
    history.push(SavedMessage {
        role: "assistant".to_string(),
        content: reply_trimmed.to_string(),
    });

    let title = existing
        .as_ref()
        .filter(|c| !c.title.is_empty() && c.title != "New chat")
        .map(|c| c.title.clone())
        .unwrap_or_else(|| {
            if display_name.is_empty() {
                format!("Athena (private) · node {}", sender_id)
            } else {
                format!("Athena (private) · {}", display_name)
            }
        });

    save_chat(
        app_data_dir,
        Some(chat_id.clone()),
        title,
        history,
        Some("Meshtastic mesh radio".to_string()),
    )?;

    if let Some(h) = app {
        let _ = h.emit(
            MESH_CHATS_UPDATED_EVENT,
            serde_json::json!({ "chatId": chat_id, "senderId": sender_id }),
        );
    }

    Ok(chunk_mesh_utf8(reply_trimmed, 200))
}
