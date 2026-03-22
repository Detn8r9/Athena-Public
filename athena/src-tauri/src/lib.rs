mod chats;
mod chunking;
mod config;
mod db;
mod embeddings;
mod indexer;
mod kiwix;
mod learner;
mod maps;
mod athena_map_protocol;
mod athena_mbtiles_tile_protocol;
mod map_manager_catalog;
mod map_manager_download;
mod pmtiles_region_extract;
mod mbtiles_io;
mod pmtiles_io;
mod tile_pack;
mod mesh_radio;
mod mesh_listener;
mod mesh_ai;
mod ocr;
mod ollama;
mod quarantine;
mod theme;
mod system;
mod vector_index;
mod watcher;
#[cfg(feature = "piper-tts")]
mod tts;

/// Re-exported for integration tests (`tests/`) and external callers embedding `athena_lib`.
pub use db::Database;
pub use ollama::{chat_stream, ChatMessage};

use crossbeam_channel::Sender;
use std::collections::HashSet;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

use config::{load_config, save_config, AppConfig, CustomTheme};
use db::{extract_keywords, ChunkEmbedding, IndexedFile, IndexStats, RagCounts};
use chats::{get_chat as get_saved_chat, list_chats as list_saved_chats, save_chat as save_saved_chat, update_chat_context_summary as update_saved_chat_context_summary, update_chat_metadata as update_saved_chat_metadata, delete_chat as delete_saved_chat, ChatSummary, SavedChat, SavedMessage};
use kiwix::{scan_kiwix_zim_files as kiwix_scan_zim_files, DiscoveredZim};
use ollama::{
    get_system_specs as get_specs, get_available_models, is_ollama_running, max_memory_messages_for_ram,
    select_model_for_tier, try_launch_ollama_server, chat,
    generate_chat_title as ollama_generate_chat_title, generate_chat_description as ollama_generate_chat_description,
    ModelInfo, SystemSpecs,
};
use watcher::{run_watcher_loop, WatchCommand};
use learner::build_learned_notes_for_all_text;

pub struct AppState {
    pub db: Arc<Database>,
    pub watcher_tx: Sender<WatchCommand>,
    pub active_offline_map_downloads: Arc<Mutex<HashSet<String>>>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Send a plain text packet on a Meshtastic channel (default 0). Broadcast if `destination_node_id` is omitted; otherwise DM that node.
#[tauri::command]
async fn send_mesh_message(
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
    text: String,
    want_ack: Option<bool>,
    channel: Option<u32>,
    destination_node_id: Option<u32>,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Message text is empty.".to_string());
    }
    if text.as_bytes().len() > 233 {
        // Meshtastic payload limit is byte-oriented; 233 is a common safe bound for short text.
        return Err("Message is too long (max 233 bytes for typical Meshtastic text).".to_string());
    }
    control
        .send_text_message(
            text,
            want_ack.unwrap_or(false),
            channel.unwrap_or(0),
            destination_node_id,
        )
        .await
}

#[tauri::command]
fn get_mesh_ai_auto_reply(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(load_config(&app_data_dir).mesh_ai_auto_reply.unwrap_or(true))
}

#[tauri::command]
fn is_mesh_radio_connected() -> bool {
    mesh_listener::mesh_radio_is_connected()
}

#[tauri::command]
fn set_mesh_ai_auto_reply(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut cfg = load_config(&app_data_dir);
    cfg.mesh_ai_auto_reply = Some(value);
    save_config(&app_data_dir, &cfg)
}

#[tauri::command]
fn get_watch_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(load_config(&app_data_dir).watch_folder)
}

#[tauri::command]
fn get_theme_config(app: tauri::AppHandle) -> Result<(Option<String>, Option<String>, Vec<CustomTheme>), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let cfg = load_config(&app_data_dir);
    Ok((cfg.theme_id, cfg.personality_id, cfg.custom_themes))
}

#[tauri::command]
fn set_theme_config(
    app: tauri::AppHandle,
    theme_id: Option<String>,
    personality_id: Option<String>,
    custom_themes: Vec<CustomTheme>,
) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut cfg = load_config(&app_data_dir);
    cfg.theme_id = theme_id;
    cfg.personality_id = personality_id;
    cfg.custom_themes = custom_themes;
    save_config(&app_data_dir, &cfg)
}

#[tauri::command]
fn set_fullscreen(app: tauri::AppHandle, fullscreen: bool) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Main window not found")?;
    window.set_fullscreen(fullscreen).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    let window = app.get_webview_window("main").ok_or("Main window not found")?;
    window.is_fullscreen().map_err(|e| e.to_string())
}

fn kiwix_downloads_dir(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join("kiwix_downloads")
}

#[tauri::command]
fn set_watch_folder(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut config = load_config(&app_data_dir);
    config.watch_folder = path.clone();
    save_config(&app_data_dir, &config)?;
    let kiwix_dir = kiwix_downloads_dir(&app_data_dir).to_string_lossy().to_string();
    std::fs::create_dir_all(&kiwix_dir).map_err(|e| e.to_string())?;
    let folders: Vec<String> = match path {
        Some(p) => vec![p, kiwix_dir],
        None => vec![kiwix_dir],
    };
    let state = app.state::<AppState>();
    state.watcher_tx.send(WatchCommand::SetFolders(folders)).map_err(|e| e.to_string())?;

    // Kick off "learned notes" generation in the background so the assistant can retrieve distilled facts.
    let db = Arc::clone(&state.db);
    std::thread::spawn(move || {
        if !is_ollama_running() {
            return;
        }
        let app_data_dir = match app.path().app_data_dir() {
            Ok(d) => d,
            Err(_) => return,
        };
        let config = load_config(&app_data_dir);
        let models = match get_available_models() {
            Ok(m) => m,
            Err(_) => return,
        };
        if models.is_empty() {
            return;
        }
        let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
        let model_name = match select_model_for_tier(&models, &tier) {
            Some(m) => m,
            None => return,
        };
        let _ = build_learned_notes_for_all_text(db.as_ref(), &model_name, 50);
    });

    Ok(())
}

#[tauri::command]
fn build_library_knowledge(app: tauri::AppHandle, limit: Option<u32>) -> Result<u32, String> {
    if !is_ollama_running() {
        return Err("Ollama is not running. Start Ollama first to build knowledge notes.".to_string());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let models = get_available_models().map_err(|e| e.to_string())?;
    if models.is_empty() {
        return Err("No models found in Ollama. Install one first (e.g. ollama run llama3.2:3b).".to_string());
    }
    let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
    let model_name = select_model_for_tier(&models, &tier).ok_or("No suitable model found for the selected tier.")?;
    let state = app.state::<AppState>();
    let made = build_learned_notes_for_all_text(state.db.as_ref(), &model_name, limit.unwrap_or(200) as usize)?;
    Ok(made as u32)
}

#[tauri::command]
fn get_indexed_files(app: tauri::AppHandle) -> Result<Vec<IndexedFile>, String> {
    let state = app.state::<AppState>();
    state.db.get_all()
}

/// Manually ingest one or more files into the index. Corrupted or unreadable files
/// are moved to quarantine and a notification is emitted; the app does not crash.
#[tauri::command]
fn ingest_files(app: tauri::AppHandle, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let db = &state.db;
    for p in paths {
        let path_buf = std::path::PathBuf::from(&p);
        let path = path_buf.as_path();
        if !path.is_file() {
            continue;
        }
        if !indexer::is_supported_path(path) {
            continue;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| indexer::index_file(path)));
        match result {
            Ok(Some(data)) => {
                let _ = db.upsert_file(
                    &data.path,
                    &data.file_type,
                    &data.name,
                    Some(data.size),
                    data.modified_at,
                    data.text_content.as_deref(),
                    data.metadata_json.as_deref(),
                );
                if let Some(ref text) = data.text_content {
                    if !text.trim().is_empty() {
                        let _ = vector_index::index_document(db, &p, text);
                    }
                }
            }
            Ok(None) => {
                if let Ok(qpath) = quarantine::move_to_quarantine(path, &app_data_dir) {
                    let _ = app.emit(
                        watcher::FILE_QUARANTINED_EVENT,
                        serde_json::json!({
                            "path": p,
                            "reason": "File could not be read or format is corrupted.",
                            "quarantine_path": qpath.to_string_lossy(),
                        }),
                    );
                }
            }
            Err(_) => {
                if let Ok(qpath) = quarantine::move_to_quarantine(path, &app_data_dir) {
                    let _ = app.emit(
                        watcher::FILE_QUARANTINED_EVENT,
                        serde_json::json!({
                            "path": p,
                            "reason": "File caused an error while indexing.",
                            "quarantine_path": qpath.to_string_lossy(),
                        }),
                    );
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn get_index_stats(app: tauri::AppHandle) -> Result<IndexStats, String> {
    let state = app.state::<AppState>();
    state.db.get_stats()
}

#[derive(serde::Serialize)]
struct RagStats {
    document_count: i64,
    chunk_count: i64,
}

#[tauri::command]
fn get_rag_stats(app: tauri::AppHandle) -> Result<RagStats, String> {
    let state = app.state::<AppState>();
    let RagCounts {
        document_count,
        chunk_count,
    } = state.db.get_rag_counts()?;
    Ok(RagStats {
        document_count,
        chunk_count,
    })
}

#[derive(serde::Serialize)]
struct RebuildFailedFile {
    path: String,
    name: String,
    error: String,
}

#[derive(serde::Serialize)]
struct RebuildVectorIndexResult {
    processed: u32,
    failed: u32,
    failed_files: Vec<RebuildFailedFile>,
}

#[derive(Clone, serde::Serialize)]
struct RebuildProgress {
    processed: u32,
    total: usize,
    failed: u32,
}

#[tauri::command]
async fn rebuild_vector_index(app: tauri::AppHandle) -> Result<RebuildVectorIndexResult, String> {
    let state = app.state::<AppState>();
    let all = state.db.get_all()?;
    let files_with_text: Vec<IndexedFile> = all
        .into_iter()
        .filter(|r| {
            r.text_content
                .as_deref()
                .map_or(false, |t| !t.trim().is_empty())
        })
        .collect();
    let total = files_with_text.len();
    let app_handle = app.clone();
    let db = Arc::clone(&state.db);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut processed = 0u32;
        let mut failed = 0u32;
        let mut failed_files: Vec<RebuildFailedFile> = Vec::new();
        for row in &files_with_text {
            let text = row.text_content.as_deref().expect("filter guarantees non-null text_content");
            match vector_index::index_document(db.as_ref(), &row.path, text) {
                Ok(()) => processed += 1,
                Err(e) => {
                    failed += 1;
                    failed_files.push(RebuildFailedFile {
                        path: row.path.clone(),
                        name: row.name.clone(),
                        error: e.clone(),
                    });
                }
            }
            let _ = app_handle.emit(
                "athena:rebuild-progress",
                RebuildProgress {
                    processed,
                    total,
                    failed,
                },
            );
        }
        Ok::<RebuildVectorIndexResult, String>(RebuildVectorIndexResult {
            processed,
            failed,
            failed_files,
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(result)
}

#[tauri::command]
fn get_system_specs() -> Result<SystemSpecs, String> {
    Ok(get_specs())
}

#[tauri::command]
fn get_battery_state() -> Result<system::BatteryState, String> {
    system::get_battery_state()
}

#[tauri::command]
fn get_ollama_available_models() -> Result<Vec<ModelInfo>, String> {
    get_available_models()
}

#[tauri::command]
fn is_ollama_available() -> bool {
    is_ollama_running()
}

/// Tries to start the Ollama server (e.g. `ollama serve` or open Ollama app on macOS).
/// Call this when the user explicitly asks to start Ollama; the app may need permission to launch processes.
#[tauri::command]
fn try_launch_ollama() {
    try_launch_ollama_server();
}

#[tauri::command]
fn get_recommended_model() -> Result<Option<String>, String> {
    let specs = get_specs();
    let models = get_available_models()?;
    let model = select_model_for_tier(&models, &specs.recommended_tier);
    Ok(model)
}

#[tauri::command]
fn get_model_override(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(load_config(&app_data_dir).model_override)
}

#[tauri::command]
fn set_model_override(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut config = load_config(&app_data_dir);
    let normalized = value.and_then(|v| {
        let v = v.to_lowercase();
        if v.is_empty() || v == "auto" {
            None
        } else if v == "small" || v == "medium" || v == "large" || v == "xlarge" {
            Some(v)
        } else {
            None
        }
    });
    config.model_override = normalized;
    save_config(&app_data_dir, &config)?;
    Ok(())
}

const DEFAULT_MEMORY_MESSAGES: u32 = 20;
const MIN_MEMORY_MESSAGES: u32 = 5;

#[tauri::command]
fn get_memory_messages(app: tauri::AppHandle) -> Result<u32, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let max = max_memory_messages_for_ram(get_specs().ram_gb);
    let v = load_config(&app_data_dir).memory_messages.unwrap_or(DEFAULT_MEMORY_MESSAGES);
    Ok(v.clamp(MIN_MEMORY_MESSAGES, max))
}

#[tauri::command]
fn set_memory_messages(app: tauri::AppHandle, value: u32) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut config = load_config(&app_data_dir);
    let max = max_memory_messages_for_ram(get_specs().ram_gb);
    let clamped = value.clamp(MIN_MEMORY_MESSAGES, max);
    config.memory_messages = Some(clamped);
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn get_feature_map_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(load_config(&app_data_dir).feature_map_enabled.unwrap_or(false))
}

#[tauri::command]
fn set_feature_map_enabled(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut config = load_config(&app_data_dir);
    config.feature_map_enabled = Some(value);
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn get_show_sources_in_chat(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(load_config(&app_data_dir).show_sources_in_chat.unwrap_or(true))
}

#[tauri::command]
fn set_show_sources_in_chat(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut config = load_config(&app_data_dir);
    config.show_sources_in_chat = Some(value);
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn get_user_location(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(load_config(&app_data_dir).user_location_address.clone())
}

#[tauri::command]
fn set_user_location(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut config = load_config(&app_data_dir);
    config.user_location_address = value.filter(|s| !s.trim().is_empty());
    save_config(&app_data_dir, &config)?;
    Ok(())
}

/// Rough hardware estimate for the current model + conversation memory setting.
#[derive(serde::Serialize)]
pub struct MemoryEstimate {
    pub memory_messages: u32,
    pub model_name: String,
    pub model_size_b: u32,
    pub estimated_model_ram_gb: f64,
    pub estimated_context_tokens: u32,
    pub estimated_context_ram_mb: u32,
    pub total_estimated_ram_gb: f64,
}

#[tauri::command]
fn get_memory_estimate(app: tauri::AppHandle) -> Result<MemoryEstimate, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let memory_messages = config.memory_messages.unwrap_or(DEFAULT_MEMORY_MESSAGES);

    let models = get_available_models().map_err(|e| e.to_string())?;
    let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
    let model_name = select_model_for_tier(&models, &tier).unwrap_or_else(|| "(no model)".to_string());
    let model_size_b = models
        .iter()
        .find(|m| m.name == model_name)
        .map(|m| m.size_estimate)
        .unwrap_or(0);

    // Rough: ~0.8 GB RAM per 1B params for inference
    let estimated_model_ram_gb = (model_size_b as f64) * 0.8;
    // ~250 tokens per message on average; ~50 KB per token for KV cache (small model)
    let estimated_context_tokens = memory_messages * 250;
    let estimated_context_ram_mb = (estimated_context_tokens as f64 * 0.05) as u32;
    let total_estimated_ram_gb = estimated_model_ram_gb + (estimated_context_ram_mb as f64 / 1024.0);

    Ok(MemoryEstimate {
        memory_messages,
        model_name,
        model_size_b,
        estimated_model_ram_gb,
        estimated_context_tokens,
        estimated_context_ram_mb,
        total_estimated_ram_gb,
    })
}

#[tauri::command]
fn list_chats(app: tauri::AppHandle) -> Result<Vec<ChatSummary>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    list_saved_chats(&app_data_dir)
}

#[tauri::command]
fn get_chat(app: tauri::AppHandle, id: String) -> Result<Option<SavedChat>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    get_saved_chat(&app_data_dir, id)
}

#[tauri::command]
fn save_chat(app: tauri::AppHandle, id: Option<String>, title: String, messages: Vec<SavedMessage>, description: Option<String>) -> Result<String, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    save_saved_chat(&app_data_dir, id, title, messages, description)
}

#[tauri::command]
fn update_chat_metadata(app: tauri::AppHandle, id: String, title: Option<String>, description: Option<String>) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    update_saved_chat_metadata(&app_data_dir, id, title, description)
}

#[tauri::command]
fn update_chat_context_summary(
    app: tauri::AppHandle,
    id: String,
    last_user_msg: String,
    last_assistant_msg: String,
) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    update_saved_chat_context_summary(&app_data_dir, id, &last_user_msg, &last_assistant_msg)
}

/// Synthesize speech for character voices (fallout, ultron). Returns WAV as base64 or error.
#[tauri::command]
fn synthesize_speech(app: tauri::AppHandle, text: String, voice_id: String) -> Result<String, String> {
    #[cfg(feature = "piper-tts")]
    {
        use base64::Engine;
        let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let bytes = tts::synthesize_speech(&app_data_dir, &text, &voice_id)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
    }
    #[cfg(not(feature = "piper-tts"))]
    {
        let _ = (app, text, voice_id);
        Err("Character voice synthesis (Piper) is not enabled in this build. Use Default voice or build with --features piper-tts and add voice models to app data/voices/.".to_string())
    }
}

#[tauri::command]
fn generate_chat_title(app: tauri::AppHandle, first_message: String) -> Result<String, String> {
    if !is_ollama_running() {
        return Err("Ollama is not running.".to_string());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let models = get_available_models().map_err(|e| e.to_string())?;
    if models.is_empty() {
        return Err("No models found in Ollama.".to_string());
    }
    let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
    let model_name = select_model_for_tier(&models, &tier).ok_or("No suitable model found.")?;
    ollama_generate_chat_title(&model_name, &first_message)
}

#[tauri::command]
fn generate_chat_description(app: tauri::AppHandle, messages: Vec<SavedMessage>) -> Result<String, String> {
    if !is_ollama_running() {
        return Err("Ollama is not running.".to_string());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let models = get_available_models().map_err(|e| e.to_string())?;
    if models.is_empty() {
        return Err("No models found in Ollama.".to_string());
    }
    let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
    let model_name = select_model_for_tier(&models, &tier).ok_or("No suitable model found.")?;
    let chat_messages: Vec<ChatMessage> = messages.into_iter().map(|m| ChatMessage { role: m.role, content: m.content }).collect();
    ollama_generate_chat_description(&model_name, &chat_messages)
}

#[tauri::command]
fn delete_chat(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    delete_saved_chat(&app_data_dir, id)
}

const ERROR_LOG_FILENAME: &str = "athena_error_log.txt";
const ERROR_LOG_MAX_BYTES: usize = 100_000;

fn error_log_path(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join(ERROR_LOG_FILENAME)
}

#[tauri::command]
fn append_error_log(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
    let path = error_log_path(&app_data_dir);
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let line = format!("[{}] {}\n", timestamp, message.replace('\n', " "));
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?
        .write_all(line.as_bytes())
        .map_err(|e| e.to_string())?;
    if path.metadata().map(|m| m.len() as usize > ERROR_LOG_MAX_BYTES).unwrap_or(false) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let target_keep = content.len().saturating_sub(ERROR_LOG_MAX_BYTES / 2);
            let start = content
                .char_indices()
                .find(|(i, _)| *i >= target_keep)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let start = content[start..].find('\n').map(|nl| start + nl + 1).unwrap_or(start);
            let _ = std::fs::write(&path, &content[start..]);
        }
    }
    Ok(())
}

#[tauri::command]
fn get_error_log(app: tauri::AppHandle) -> Result<String, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = error_log_path(&app_data_dir);
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct DeviceStats {
    pub cpu_usage_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_temp_c: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_usage_pct: Option<f32>,
}

fn get_cpu_temp_c() -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let out = Command::new("powermetrics")
            .args(["--samplers", "smc", "-i", "1", "-n", "1"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("CPU die temperature") {
                if let Some(rest) = line.split("CPU die temperature").nth(1) {
                    let num: String = rest.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
                    if let Ok(t) = num.trim().parse::<f32>() {
                        return Some(t);
                    }
                }
            }
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let paths = [
            "/sys/class/thermal/thermal_zone0/temp",
            "/sys/class/hwmon/hwmon0/temp1_input",
            "/sys/class/hwmon/hwmon1/temp1_input",
        ];
        for path in paths {
            if let Ok(s) = fs::read_to_string(path) {
                if let Ok(millideg) = s.trim().parse::<u32>() {
                    return Some(millideg as f32 / 1000.0);
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = ();
        None
    }
}

fn get_gpu_usage_pct() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let out = Command::new("nvidia-smi")
            .args(["--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout);
        let s = s.trim().trim_end_matches('%').trim();
        s.parse::<f32>().ok()
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let out = Command::new("powermetrics")
            .args(["--samplers", "gpu_power", "-i", "1", "-n", "1"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.to_lowercase().contains("gpu") && line.contains('%') {
                let num: String = line.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
                if let Ok(pct) = num.trim().parse::<f32>() {
                    return Some(pct.min(100.0));
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = ();
        None
    }
}

#[tauri::command]
fn get_device_stats() -> Result<DeviceStats, String> {
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_memory();
    // CPU usage is a delta: refresh twice so the second read is meaningful.
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    let raw_cpu = sys.global_cpu_info().cpu_usage();
    // Global CPU is the *sum* of all cores (each 0–100%), so average = sum / core_count, already in %.
    let num_cpus = sys.cpus().len().max(1) as f32;
    let cpu_usage_pct = (raw_cpu / num_cpus).min(100.0);
    let ram_used_mb = sys.used_memory() / (1024 * 1024);
    let ram_total_mb = sys.total_memory() / (1024 * 1024);
    Ok(DeviceStats {
        cpu_usage_pct,
        ram_used_mb,
        ram_total_mb,
        disk_used_gb: 0.0,
        disk_total_gb: 0.0,
        net_rx_bytes: 0,
        net_tx_bytes: 0,
        cpu_temp_c: get_cpu_temp_c(),
        gpu_usage_pct: get_gpu_usage_pct(),
    })
}

/// When we have library excerpts, enforce strict grounding: no invented brands or names.
const LIBRARY_RULE_WITH_CONTEXT: &str = "The excerpts below are the user's own Library documents. Use them to answer.\n\nWhen the answer (e.g. address, house location, property details, names, brands) IS in the excerpts, say so clearly: e.g. \"According to your library, your house is at [address]\" or \"Your documents say…\" Do NOT say \"I don't have information about your house\" or \"the library doesn't contain\" when that information appears in the excerpts below.\n\nDo NOT say you cannot access their surroundings, see their house, or that you cannot see something—use the documents below. Only if the answer is truly not in the excerpts, say you don't have that information and suggest adding a document to the Library.\n\nCRITICAL (survival/repair safety): For brands, model names, part numbers, and any specific fact about the user's property or equipment, state ONLY what is explicitly written in the excerpts. Never substitute a different brand or product (e.g. do not say Nest, Ring, or Honeywell unless that exact name appears in the excerpts). If the document says Aqara, say Aqara—not another brand. Wrong specifics in repair or survival can be dangerous; when in doubt, say you don't see that in the documents.";

/// Shipped reference: user prefix `/athena` injects this (see `ATHENA_PROJECT_REFERENCE.md`).
const ATHENA_PROJECT_REFERENCE: &str = include_str!("../../ATHENA_PROJECT_REFERENCE.md");

const LIBRARY_RULE_ATHENA_REF_MODE: &str = "The user is asking about the Athena desktop application itself (features, settings, UI, Meshtastic, Library, shortcuts, limitations). Answer using the \"Athena application reference\" appendix in this system prompt. Do not invent features not described there. If something is not in the appendix, say you are not sure and suggest the in-app Help (?) menu.";

/// Strips leading `/athena` (case-insensitive). Returns `(message_for_model, is_athena_ref_query)`.
fn parse_athena_ref_message(message: &str) -> (String, bool) {
    let t = message.trim_start();
    if t.len() >= 7 && t[..7].eq_ignore_ascii_case("/athena") {
        let after = t.get(7..).unwrap_or("").trim_start();
        let user_part = if after.is_empty() {
            "Briefly describe what the Athena application is and summarize its main capabilities, using the Athena application reference in your system instructions."
        } else {
            after
        };
        (user_part.to_string(), true)
    } else {
        (message.to_string(), false)
    }
}

/// Sort so learned notes (compressed Q&A) come first, then the rest.
fn sort_docs_learned_first(docs: &mut [IndexedFile]) {
    docs.sort_by(|a, b| {
        let a_learned = a.file_type == "learned";
        let b_learned = b.file_type == "learned";
        match (a_learned, b_learned) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}

/// Max chars for a single-doc snippet when we use a keyword-centered window (keeps brands/specs near keywords in context).
const SNIPPET_WINDOW_BEFORE: usize = 1500;
const SNIPPET_WINDOW_AFTER: usize = 3500;
/// If a doc is under this size and matches the query, include it in full so we never miss a brand/spec.
const FULL_DOC_THRESHOLD: usize = 7000;

fn build_context_from_docs(docs: &[IndexedFile], query: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut total = 0;
    let keywords = extract_keywords(query, 6);
    for d in docs {
        if let Some(ref t) = d.text_content {
            let snippet = if keywords.is_empty() {
                if t.len() > 4000 {
                    format!("{}...", &t[..4000])
                } else {
                    t.clone()
                }
            } else if t.len() <= FULL_DOC_THRESHOLD && keywords.iter().any(|k| t.to_lowercase().contains(k)) {
                // Small doc that matches: include full text so brands/specs are never cut off.
                t.clone()
            } else {
                let lower = t.to_lowercase();
                // Find the span that includes every keyword so the snippet contains all relevant parts.
                let mut min_pos = 0usize;
                let mut max_pos = 0usize;
                let mut any = false;
                for k in &keywords {
                    if let Some(p) = lower.find(k) {
                        let end = p + k.len();
                        if !any {
                            min_pos = p;
                            max_pos = end;
                            any = true;
                        } else {
                            min_pos = min_pos.min(p);
                            max_pos = max_pos.max(end);
                        }
                    }
                }
                let (start, end) = if any {
                    let start = min_pos.saturating_sub(SNIPPET_WINDOW_BEFORE);
                    let end = (max_pos + SNIPPET_WINDOW_AFTER).min(t.len());
                    (start, end)
                } else {
                    (0, t.len().min(4000))
                };
                let mut s = t[start..end].to_string();
                if start > 0 {
                    s = format!("...{}", s);
                }
                if end < t.len() {
                    s.push_str("...");
                }
                s
            };
            if total + snippet.len() + 2 > max_chars {
                break;
            }
            out.push_str(&format!("\n--- From: {} ---\n{}\n", d.name, snippet));
            total += snippet.len() + 2;
        }
    }
    out
}

/// Short snippet from each doc (for fallback when keyword/semantic return nothing).
fn build_recent_docs_context(docs: &[IndexedFile], max_chars_per_doc: usize, max_total: usize) -> String {
    let mut out = String::new();
    let mut total = 0;
    for d in docs {
        let Some(ref t) = d.text_content else { continue };
        let snippet = if t.len() <= max_chars_per_doc {
            t.as_str()
        } else {
            &t[..max_chars_per_doc]
        };
        let block = format!("\n--- From: {} ---\n{}...\n", d.name, snippet);
        if total + block.len() > max_total {
            break;
        }
        out.push_str(&block);
        total += block.len();
    }
    out
}

/// Build context string from semantically similar chunks (vector search results).
fn build_semantic_context(chunks: &[ChunkEmbedding], max_chars: usize) -> String {
    let mut out = String::new();
    let mut total = 0;
    for c in chunks {
        let name = std::path::Path::new(&c.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&c.file_path);
        let block = format!("\n--- From: {} ---\n{}\n", name, c.chunk_text);
        if total + block.len() > max_chars {
            break;
        }
        out.push_str(&block);
        total += block.len();
    }
    out
}

const RRF_K: f64 = 60.0;
const RRF_KEYWORD_LIMIT: i64 = 20;
const RRF_SEMANTIC_LIMIT: usize = 25;
const RRF_TOP_N: usize = 15;
const RRF_COMBINED_MAX_CHARS: usize = 12_000;
/// When we have conversation context (follow-up), use more keyword docs and fewer semantic so topic docs win.
const RRF_KEYWORD_LIMIT_WITH_CONTEXT: i64 = 30;
const RRF_SEMANTIC_LIMIT_WITH_CONTEXT: usize = 12;

/// Reciprocal Rank Fusion: merge keyword docs and semantic chunks by path, then build one context string.
/// Prefers paths that have a keyword hit (topic match) over semantic-only so follow-ups stay on-topic.
/// When keyword_only_for_followup is true, only paths that appeared in keyword search are used (no semantic-only),
/// so follow-up context stays strictly on the conversation topic.
/// Returns (context_string, source_file_names for citations).
fn build_context_rrf(
    docs: &[IndexedFile],
    chunks: &[ChunkEmbedding],
    query: &str,
    keyword_only_for_followup: bool,
) -> (String, Vec<String>) {
    use std::collections::{HashMap, HashSet};
    #[derive(Default)]
    struct Entry {
        score: f64,
        doc: Option<IndexedFile>,
        chunks: Vec<ChunkEmbedding>,
    }
    let keyword_paths: HashSet<String> = docs.iter().map(|d| d.path.clone()).collect();
    let mut by_path: HashMap<String, Entry> = HashMap::new();
    for (r, d) in docs.iter().enumerate() {
        let rank = (r + 1) as f64;
        let score = 1.0 / (RRF_K + rank);
        let e = by_path.entry(d.path.clone()).or_default();
        e.score += score;
        if e.doc.is_none() {
            e.doc = Some(d.clone());
        }
    }
    for (r, c) in chunks.iter().enumerate() {
        let rank = (r + 1) as f64;
        let score = 1.0 / (RRF_K + rank);
        let e = by_path.entry(c.file_path.clone()).or_default();
        e.score += score;
        e.chunks.push(c.clone());
    }
    let mut paths: Vec<(f64, bool, String)> = by_path
        .iter()
        .filter(|(path, _)| !keyword_only_for_followup || keyword_paths.contains(*path))
        .map(|(path, e)| (e.score, keyword_paths.contains(path), path.clone()))
        .collect();
    paths.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal))
    });
    let top_paths: Vec<String> = paths
        .into_iter()
        .take(RRF_TOP_N)
        .map(|(_, _, p)| p)
        .collect();

    let mut out = String::new();
    let mut seen_names = std::collections::HashSet::new();
    let mut source_names: Vec<String> = Vec::new();
    let mut total = 0;
    let per_path_budget = RRF_COMBINED_MAX_CHARS / top_paths.len().max(1);

    for path in &top_paths {
        let Some(entry) = by_path.get(path) else { continue };
        let name = entry
            .doc
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_else(|| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path)
                    .to_string()
            });
        if seen_names.insert(name.clone()) {
            source_names.push(name);
        }
        let remaining = RRF_COMBINED_MAX_CHARS.saturating_sub(total);
        let budget = per_path_budget.min(remaining);
        if budget == 0 {
            break;
        }
        if let Some(ref doc) = entry.doc {
            let ctx = build_context_from_docs(std::slice::from_ref(doc), query, budget);
            if !ctx.is_empty() {
                out.push_str(&ctx);
                total += ctx.len();
            }
        } else if !entry.chunks.is_empty() {
            let ctx = build_semantic_context(&entry.chunks, budget);
            out.push_str(&ctx);
            total += ctx.len();
        }
    }
    (out, source_names)
}

/// Previous conversation turns (user + assistant) for context. Omit the current user message.
#[derive(Clone, serde::Deserialize)]
struct HistoryMessage {
    role: String,
    content: String,
}

fn build_location_section(config: &config::AppConfig) -> String {
    if !config.feature_map_enabled.unwrap_or(false) {
        return String::new();
    }
    if let Some(ref addr) = config.user_location_address {
        if !addr.trim().is_empty() {
            return format!(
                "\n\nThe user's current location is: {}. Use this to give location-specific advice when relevant (e.g. where to find spare parts, types of places nearby like auto parts stores or large parking lots). You cannot look up real-time data; suggest categories of places and reasoning based on their location. In scenarios like scarcity or emergency, consider places that might have relevant resources (e.g. auto parts stores, junkyards, parking lots with similar vehicles).",
                addr.trim()
            );
        }
    }
    "\n\nLocation feature is on but the user has not set their address. When the question would benefit from location-specific advice (e.g. finding places, spare parts, resources), ask the user for their current address or general area so you can tailor suggestions.".to_string()
}

fn build_personality_section(config: &AppConfig) -> String {
    let theme = theme::get_active_theme(config);
    let personality_id = theme::get_active_personality_id(config);
    let style = match personality_id.as_str() {
        "fallout" => "You speak like a retro-futuristic terminal in a Fallout-style universe: concise, matter-of-fact, with dry, sometimes dark humor. You keep answers practical and terse, like a pre-war maintenance terminal trying to be helpful despite everything falling apart.",
        "fieldguide" => "You speak like an old survival manual or vintage field guide: terse, instructional, no-nonsense. Use short sentences, numbered steps where appropriate, and the occasional dated turn of phrase. Sound like printed text from a well-thumbed booklet—authoritative, sparse, and to the point.",
        _ => "You speak like a calm, practical survival instructor. Be encouraging but honest, avoid drama, and focus on clear, step-by-step guidance that a stressed person can follow.",
    };
    format!(
        "\n\nCurrent theme preset: \"{}\". Personality: {}.\n{}",
        theme.name, personality_id, style
    )
}

const KNOWLEDGE_BLOCK_FILENAME: &str = "knowledge_block.txt";

fn knowledge_block_path(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join(KNOWLEDGE_BLOCK_FILENAME)
}

fn read_persistent_knowledge_block(app_data_dir: &std::path::Path) -> String {
    let path = knowledge_block_path(app_data_dir);
    if path.exists() {
        std::fs::read_to_string(&path)
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        let default_content = "# Add critical survival facts and user preferences below.\n# One per line or in paragraphs. This content is injected into every AI query.\n";
        let _ = std::fs::write(&path, default_content);
        String::new()
    }
}

fn build_persistent_knowledge_section(app_data_dir: &std::path::Path) -> String {
    let content = read_persistent_knowledge_block(app_data_dir);
    let trimmed = content.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("\n\nPersistent knowledge (always apply):\n{}\n", trimmed)
    }
}

fn build_user_gps_section(user_lat: Option<f64>, user_lon: Option<f64>) -> String {
    match (user_lat, user_lon) {
        (Some(lat), Some(lon)) => format!(
            "\n\nThe user's current GPS coordinates are ({:.5}, {:.5}). They may ask to mark this location on the map or refer to \"my location\"; use these coordinates when relevant.",
            lat, lon
        ),
        _ => String::new(),
    }
}

fn build_poi_and_maps_section(app_data_dir: &std::path::Path) -> String {
    let pois = maps::get_pois(app_data_dir);
    let downloaded = maps::get_downloaded_maps(app_data_dir);
    if pois.is_empty() && downloaded.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    if !pois.is_empty() {
        let list: Vec<String> = pois
            .iter()
            .map(|p| format!("{}: {} at ({:.5}, {:.5}){}", p.name, p.id, p.lat, p.lon, if p.notes.is_empty() { String::new() } else { format!(" — {}", p.notes) }))
            .collect();
        parts.push(format!(
            "The user has saved points of interest (POIs) on their map. Use these when they refer to places by name (e.g. \"Homebase\"). You can suggest adding new POIs; the user can add them by clicking the map in the Map tab or by naming the place.\n{}",
            list.join("\n")
        ));
    }
    if !downloaded.is_empty() {
        let list: Vec<String> = downloaded.iter().map(|m| format!("{} ({}, {})", m.name, m.region, m.style)).collect();
        parts.push(format!(
            "Downloaded offline maps (user can view these in the Map tab): {}.",
            list.join("; ")
        ));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("\n\n{}", parts.join("\n\n"))
}

#[tauri::command]
fn get_local_time() -> Result<system::LocalTimeInfo, String> {
    Ok(system::get_local_time())
}

#[tauri::command]
fn get_sun_times(lat: f64, lon: f64) -> Result<system::SunTimes, String> {
    system::get_sun_times(lat, lon)
}

/// Build retrieval query and conversation prompt from recent messages (history or saved chat) + current message.
/// Used when context_summary is missing/empty so follow-up questions still search the right docs.
fn build_retrieval_from_history(history: &[HistoryMessage], message: &str) -> (String, String) {
    if history.len() < 2 {
        return (message.to_string(), String::new());
    }
    let start = history.len().saturating_sub(4);
    let combined: String = history[start..]
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    build_retrieval_from_combined_text(&combined, message)
}

/// Same as above but from saved chat messages (source of truth when we have a loaded chat).
fn build_retrieval_from_saved_messages(messages: &[SavedMessage], message: &str) -> (String, String) {
    if messages.len() < 2 {
        return (message.to_string(), String::new());
    }
    let start = messages.len().saturating_sub(4);
    let combined: String = messages[start..]
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    build_retrieval_from_combined_text(&combined, message)
}

fn build_retrieval_from_combined_text(combined: &str, message: &str) -> (String, String) {
    let words: Vec<&str> = combined
        .split_whitespace()
        .filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '\''))
        .take(80)
        .collect();
    let context = words.join(" ");
    if context.is_empty() {
        (message.to_string(), String::new())
    } else {
        let retrieval_query = format!("{} {}", context, message);
        let conversation_context_prompt = format!(
            "\n\nConversation context (use to resolve \"it\"/\"that\" and what the user is referring to): {}",
            context
        );
        (retrieval_query, conversation_context_prompt)
    }
}

fn build_chat_messages(
    system_content: String,
    history: &[HistoryMessage],
    new_user_message: &str,
    max_history_messages: usize,
) -> Vec<ChatMessage> {
    let max = max_history_messages.max(1);
    let mut messages = vec![ChatMessage { role: "system".to_string(), content: system_content }];
    let start = history.len().saturating_sub(max);
    for h in &history[start..] {
        if !h.content.is_empty() || h.role == "user" {
            messages.push(ChatMessage { role: h.role.clone(), content: h.content.clone() });
        }
    }
    messages.push(ChatMessage { role: "user".to_string(), content: new_user_message.to_string() });
    messages
}

#[tauri::command]
fn send_message(app: tauri::AppHandle, message: String, history: Option<Vec<HistoryMessage>>) -> Result<String, String> {
    if !is_ollama_running() {
        return Err("Ollama is not running. Start Ollama and ensure a model is installed (e.g. ollama run llama3.2:3b).".to_string());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let models = get_available_models().map_err(|e| e.to_string())?;
    if models.is_empty() {
        return Err("No models found in Ollama. Install one with: ollama run llama3.2:3b".to_string());
    }
    let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
    let model_name = select_model_for_tier(&models, &tier).ok_or("No suitable model found for the selected tier.")?;

    let state = app.state::<AppState>();
    let db = &state.db;
    let (message_for_model, athena_ref_mode) = parse_athena_ref_message(&message);

    let (context_section, library_rule) = if athena_ref_mode {
        (String::new(), LIBRARY_RULE_ATHENA_REF_MODE)
    } else {
        let retrieval_query = message_for_model.as_str();
        let mentioned = db.get_indexed_files_by_name_mentioned(retrieval_query).unwrap_or_default();
        let mentioned_section = if mentioned.is_empty() {
            String::new()
        } else {
            let ctx = build_context_from_docs(&mentioned, retrieval_query, 4000);
            format!(
                "\n\nThe user referred to or asked about the following document(s); use them to answer:\n{}\n\n",
                ctx
            )
        };

        let mut docs = db.search_text_content(retrieval_query, RRF_KEYWORD_LIMIT).unwrap_or_default();
        sort_docs_learned_first(&mut docs);
        let query_vec = embeddings::embed_text(retrieval_query);
        let semantic_chunks = db.search_similar_chunks(&query_vec, RRF_SEMANTIC_LIMIT).unwrap_or_default();
        let (rrf_context, _source_names) = build_context_rrf(&docs, &semantic_chunks, retrieval_query, false);

        let keyword_semantic_section = if rrf_context.is_empty() {
            let recent = db.get_recent_text_files(3).unwrap_or_default();
            let fallback = build_recent_docs_context(&recent, 600, 2000);
            if fallback.is_empty() {
                String::new()
            } else {
                format!(
                    "\n\nRecent library documents (use if the user's question might be answered here):\n{}",
                    fallback
                )
            }
        } else {
            format!(
                "\n\nRelevant excerpts from the user's library (use these to answer questions about the user's documents, property, or notes when the answer appears below):\n{}",
                rrf_context
            )
        };

        let context_section = format!("{}{}", mentioned_section, keyword_semantic_section);
        let library_rule = if context_section.trim().is_empty() {
            "When the user asks about something that might be in their Library (e.g. their house, notes), say you don't have that information and suggest they add or place a relevant document in their Library folder, then try again."
        } else {
            LIBRARY_RULE_WITH_CONTEXT
        };
        (context_section, library_rule)
    };

    let conversation_context_prompt = "";
    let athena_ref_section = if athena_ref_mode {
        format!(
            "\n\n---\nAthena application reference (authoritative for questions about the Athena app):\n{}\n---\n",
            ATHENA_PROJECT_REFERENCE
        )
    } else {
        String::new()
    };
    let persistent_knowledge_section = build_persistent_knowledge_section(&app_data_dir);
    let location_section = build_location_section(&config);
    let poi_maps_section = build_poi_and_maps_section(&app_data_dir);
    let personality_section = build_personality_section(&config);
    let system_content = format!(
        "You are Athena, a local offline survival and repair assistant. You help with practical skills, first aid, repair, and survival knowledge. Answer concisely and clearly.\n\nCritical rule: Do NOT guess or make up facts. {}{}{}{}{}{}{}{}",
        library_rule,
        conversation_context_prompt,
        persistent_knowledge_section,
        context_section,
        athena_ref_section,
        location_section,
        poi_maps_section,
        personality_section
    );

    let history = history.as_deref().unwrap_or(&[]);
    let max_memory = config.memory_messages.unwrap_or(20) as usize;
    let messages = build_chat_messages(system_content, history, &message_for_model, max_memory);

    chat(&model_name, messages).map_err(|e| e.to_string())
}

const STREAM_CHUNK_EVENT: &str = "athena:stream-chunk";
const STREAM_DONE_EVENT: &str = "athena:stream-done";
const STREAM_ERROR_EVENT: &str = "athena:stream-error";
const STREAM_SOURCES_EVENT: &str = "athena:stream-sources";

const KIWIX_PROGRESS_EVENT: &str = "athena:kiwix-progress";
const KIWIX_DONE_EVENT: &str = "athena:kiwix-done";
const KIWIX_ERROR_EVENT: &str = "athena:kiwix-error";

#[tauri::command]
fn scan_kiwix_zim_files(app: tauri::AppHandle) -> Result<Vec<DiscoveredZim>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let root: std::path::PathBuf = config
        .watch_folder
        .as_deref()
        .filter(|p| std::path::Path::new(p).is_dir())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| kiwix_downloads_dir(&app_data_dir));
    kiwix_scan_zim_files(&root)
}

#[derive(serde::Serialize)]
pub struct LibraryFile {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
}

#[tauri::command]
fn list_library_contents(app: tauri::AppHandle) -> Result<Vec<LibraryFile>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let kiwix_dir = kiwix_downloads_dir(&app_data_dir);
    let mut out = Vec::new();
    let mut add_dir = |dir: &std::path::Path| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.is_file() {
                    let path_str = path.to_string_lossy().to_string();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    out.push(LibraryFile { path: path_str, name, size_bytes });
                }
            }
        }
    };
    if let Some(ref folder) = config.watch_folder {
        add_dir(std::path::Path::new(folder));
    }
    let kiwix_str = kiwix_dir.to_string_lossy();
    if kiwix_dir.exists() && config.watch_folder.as_deref() != Some(kiwix_str.as_ref()) {
        add_dir(&kiwix_dir);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[tauri::command]
fn remove_library_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _ = state.db.delete_by_path_prefix(&path);
    let _ = state.db.delete_by_path(&path);
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(())
}

// --- Maps & POIs ---

#[tauri::command]
fn get_map_catalog(app: tauri::AppHandle) -> Result<Vec<maps::MapCatalogEntry>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let location_enabled = config.feature_map_enabled.unwrap_or(false);
    let address = config.user_location_address.as_deref();
    Ok(maps::get_map_catalog(&app_data_dir, location_enabled, address))
}

#[tauri::command]
fn get_downloaded_maps(app: tauri::AppHandle) -> Result<Vec<maps::DownloadedMap>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(maps::get_downloaded_maps(&app_data_dir))
}

/// Returns the offline map manager catalog (currently embedded mock JSON; later a remote fetch).
#[tauri::command]
fn get_offline_map_manager_catalog() -> Result<Vec<map_manager_catalog::OfflineMapCatalogEntry>, String> {
    map_manager_catalog::catalog_from_embedded_mock_json()
}

#[tauri::command]
fn list_downloaded_offline_map_ids(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    map_manager_download::list_downloaded_offline_map_ids(&app_data_dir)
}

#[tauri::command]
async fn download_offline_map(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    map_id: String,
    download_url: String,
) -> Result<(), String> {
    let map_id = map_id.trim().to_string();
    let download_url = download_url.trim().to_string();
    if map_id.is_empty() {
        return Err("Map id is required.".to_string());
    }
    if download_url.is_empty() {
        return Err("Download URL is required.".to_string());
    }
    let parsed = reqwest::Url::parse(&download_url).map_err(|e| format!("Invalid download URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Download URL must use http or https.".to_string());
    }

    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    map_manager_download::ensure_offline_maps_directory(&app_data_dir)?;

    let (final_path, part_path) = map_manager_download::destination_paths(&app_data_dir, &map_id, &download_url)?;

    if final_path.is_file() {
        return Err("This map archive is already downloaded.".to_string());
    }

    {
        let mut active_guard = state
            .active_offline_map_downloads
            .lock()
            .map_err(|_| "Offline map download state lock was poisoned.".to_string())?;
        if !active_guard.insert(map_id.clone()) {
            return Err("A download for this map is already in progress.".to_string());
        }
    }

    let app_handle = app.clone();
    let active_offline_map_downloads = Arc::clone(&state.active_offline_map_downloads);
    let map_id_task = map_id.clone();

    tauri::async_runtime::spawn(async move {
        let outcome = map_manager_download::stream_download_to_file(
            app_handle.clone(),
            map_id_task.clone(),
            download_url,
            final_path.clone(),
            part_path.clone(),
        )
        .await;

        if let Ok(mut guard) = active_offline_map_downloads.lock() {
            guard.remove(&map_id_task);
        }

        match outcome {
            Ok(()) => {
                let path_string = final_path.to_string_lossy().to_string();
                let _ = app_handle.emit(
                    map_manager_download::MAP_DOWNLOAD_COMPLETE_EVENT,
                    map_manager_download::MapDownloadCompletePayload {
                        map_id: map_id_task,
                        path: path_string,
                    },
                );
            }
            Err(message) => {
                let _ = std::fs::remove_file(&part_path);
                let _ = app_handle.emit(
                    map_manager_download::MAP_DOWNLOAD_ERROR_EVENT,
                    map_manager_download::MapDownloadErrorPayload {
                        map_id: map_id_task,
                        message,
                    },
                );
            }
        }
    });

    Ok(())
}

fn network_probe_ok_gstatic(status: u16) -> bool {
    status == 204
}
fn network_probe_ok_apple(status: u16) -> bool {
    status == 200
}
fn network_probe_ok_msft(status: u16) -> bool {
    status == 200 || status == 204
}

/// Best-effort internet check: HTTP probes used by major OS captive-portal detection (no auth).
#[tauri::command]
fn check_network_reachable() -> bool {
    let probes: [(&str, fn(u16) -> bool); 3] = [
        (
            "http://connectivitycheck.gstatic.com/generate_204",
            network_probe_ok_gstatic,
        ),
        (
            "http://captive.apple.com/hotspot-detect.html",
            network_probe_ok_apple,
        ),
        (
            "http://www.msftconnecttest.com/connecttest.txt",
            network_probe_ok_msft,
        ),
    ];
    for (url, ok) in probes {
        if let Ok(resp) = ureq::get(url)
            .timeout(std::time::Duration::from_secs(4))
            .call()
        {
            if ok(resp.status()) {
                return true;
            }
        }
    }
    false
}

#[tauri::command]
fn get_smallest_kiwix_map_suggestion(app: tauri::AppHandle) -> Result<Option<maps::MapCatalogEntry>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let Some(addr) = config
        .user_location_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    Ok(maps::smallest_kiwix_map_for_address(addr))
}

#[tauri::command]
fn geocode_place_query(query: String) -> Result<(f64, f64), String> {
    tile_pack::geocode_nominatim(query.trim())
}

#[tauri::command]
fn list_tile_packs_cmd(app: tauri::AppHandle) -> Result<Vec<tile_pack::TilePackInfo>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(tile_pack::list_tile_packs(&app_data_dir))
}

#[tauri::command]
fn offline_basemap_paths_cmd(app: tauri::AppHandle) -> Result<tile_pack::OfflineBasemapPaths, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(tile_pack::offline_basemap_paths(&app_data_dir))
}

#[tauri::command]
fn estimate_tile_pack_tiles(
    center_lat: f64,
    center_lon: f64,
    radius_miles: f64,
    zoom_min: u8,
    zoom_max: u8,
) -> Result<tile_pack::TilePackEstimate, String> {
    Ok(tile_pack::estimate_tile_download(
        center_lat,
        center_lon,
        radius_miles,
        zoom_min,
        zoom_max,
    ))
}

#[tauri::command]
fn export_tile_pack_to_mbtiles_file(
    app: tauri::AppHandle,
    pack_id: String,
    dest_path: String,
) -> Result<(), String> {
    if dest_path.trim().is_empty() {
        return Err("Destination path is required.".to_string());
    }
    tile_pack::export_pack_to_mbtiles_background(
        app,
        KIWIX_PROGRESS_EVENT,
        KIWIX_DONE_EVENT,
        KIWIX_ERROR_EVENT,
        pack_id,
        dest_path,
    );
    Ok(())
}

#[tauri::command]
fn start_mbtiles_import(app: tauri::AppHandle, source_path: String, display_name: String) -> Result<String, String> {
    if source_path.trim().is_empty() {
        return Err("MBTiles path is required.".to_string());
    }
    let job_id = format!(
        "mbtiles_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    tile_pack::import_mbtiles_background(
        app,
        KIWIX_PROGRESS_EVENT,
        KIWIX_DONE_EVENT,
        KIWIX_ERROR_EVENT,
        job_id.clone(),
        source_path,
        display_name,
    );
    Ok(job_id)
}

#[tauri::command]
fn start_pmtiles_import(app: tauri::AppHandle, source_path: String, display_name: String) -> Result<String, String> {
    if source_path.trim().is_empty() {
        return Err("PMTiles path is required.".to_string());
    }
    let job_id = format!(
        "pmtiles_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    tile_pack::import_pmtiles_background(
        app,
        KIWIX_PROGRESS_EVENT,
        KIWIX_DONE_EVENT,
        KIWIX_ERROR_EVENT,
        job_id.clone(),
        source_path,
        display_name,
    );
    Ok(job_id)
}

#[tauri::command]
fn remove_tile_pack_cmd(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tile_pack::remove_tile_pack(&app_data_dir, &id)
}

#[tauri::command]
fn start_tile_pack_download(
    app: tauri::AppHandle,
    source: String,
    center_lat: f64,
    center_lon: f64,
    radius_miles: f64,
    zoom_min: u8,
    zoom_max: u8,
    name: String,
) -> Result<String, String> {
    let pack_id = format!(
        "{}_{}",
        source.trim().to_lowercase().replace(' ', "_"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    tile_pack::download_tile_pack_background(
        app.clone(),
        KIWIX_PROGRESS_EVENT,
        KIWIX_DONE_EVENT,
        KIWIX_ERROR_EVENT,
        pack_id.clone(),
        name,
        source,
        center_lat,
        center_lon,
        radius_miles,
        zoom_min,
        zoom_max,
    );
    Ok(pack_id)
}

#[tauri::command]
fn download_map(
    app: tauri::AppHandle,
    url: String,
    name: String,
    region: String,
    style: String,
) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL is required".to_string());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dest_dir = maps::map_downloads_dir(&app_data_dir);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let filename = url.split('/').last().unwrap_or("download.zim");
    let dest_path = dest_dir.join(filename);

    let app_emit = app.clone();
    let url_clone = url.clone();
    let name_clone = name.clone();
    let region_clone = region.clone();
    let style_clone = style.clone();
    std::thread::spawn(move || {
        let mut on_progress = |downloaded: u64, total: Option<u64>| {
            let payload = serde_json::json!({ "id": url_clone, "downloaded_bytes": downloaded, "total_bytes": total });
            let _ = app_emit.emit(KIWIX_PROGRESS_EVENT, payload);
        };
        match kiwix::download_zim(&url_clone, &dest_path, &mut on_progress) {
            Ok(()) => {
                let path_str = dest_path.to_string_lossy().to_string();
                if let Ok(app_data) = app_emit.path().app_data_dir() {
                    let _ = maps::add_downloaded_map(&app_data, path_str.clone(), name_clone, region_clone, style_clone);
                }
                let _ = app_emit.emit(KIWIX_DONE_EVENT, serde_json::json!({ "id": url_clone, "path": path_str }));
            }
            Err(e) => {
                let _ = app_emit.emit(KIWIX_ERROR_EVENT, serde_json::json!({ "id": url_clone, "error": e }));
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn remove_downloaded_map(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    maps::remove_downloaded_map(&app_data_dir, &path)?;
    if std::path::Path::new(&path).exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_pois(app: tauri::AppHandle) -> Result<Vec<maps::Poi>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(maps::get_pois(&app_data_dir))
}

#[tauri::command]
fn add_poi(app: tauri::AppHandle, name: String, lat: f64, lon: f64, notes: String) -> Result<maps::Poi, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    maps::add_poi(&app_data_dir, name, lat, lon, notes)
}

#[tauri::command]
fn remove_poi(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    maps::remove_poi(&app_data_dir, &id)
}

#[tauri::command]
fn update_poi(
    app: tauri::AppHandle,
    id: String,
    name: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    notes: Option<String>,
) -> Result<maps::Poi, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    maps::update_poi(&app_data_dir, &id, name, lat, lon, notes)
}


#[tauri::command]
fn send_message_stream(
    app: tauri::AppHandle,
    message: String,
    history: Option<Vec<HistoryMessage>>,
    user_lat: Option<f64>,
    user_lon: Option<f64>,
    chat_id: Option<String>,
) -> Result<(), String> {
    if !is_ollama_running() {
        return Err("Ollama is not running. Start Ollama and ensure a model is installed (e.g. ollama run llama3.2:3b).".to_string());
    }
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let models = get_available_models().map_err(|e| e.to_string())?;
    if models.is_empty() {
        return Err("No models found in Ollama. Install one with: ollama run llama3.2:3b".to_string());
    }
    let tier: String = config.model_override.clone().unwrap_or_else(|| get_specs().recommended_tier);
    let model_name = select_model_for_tier(&models, &tier).ok_or("No suitable model found for the selected tier.")?;

    let db_path = app_data_dir.join("athena_index.db");
    let app_emit = app.clone();
    let history_clone = history;
    let app_data_dir_clone = app_data_dir.clone();
    let chat_id_clone = chat_id.clone();
    std::thread::spawn(move || {
        let app_data_dir = app_data_dir_clone;
        let db = match Database::new(&db_path) {
            Ok(d) => d,
            Err(e) => {
                let _ = app_emit.emit(STREAM_ERROR_EVENT, e);
                return;
            }
        };

        let config = load_config(&app_data_dir);
        let history_ref = history_clone.as_deref().unwrap_or(&[]);

        let (message_for_model, athena_ref_mode) = parse_athena_ref_message(&message);

        // Build retrieval query and conversation prompt. Prefer full saved messages over context_summary
        // so we get maximum keyword signal (exact address, names) for follow-ups.
        let (retrieval_query, conversation_context_prompt) = if let Some(ref cid) = chat_id_clone {
            if let Ok(Some(chat)) = get_saved_chat(&app_data_dir, cid.clone()) {
                if chat.messages.len() >= 2 {
                    build_retrieval_from_saved_messages(&chat.messages, &message_for_model)
                } else {
                    let summary = chat.context_summary.unwrap_or_default();
                    if !summary.trim().is_empty() {
                        let retrieval_query = format!("{} {}", summary, message_for_model);
                        let conversation_context_prompt = format!(
                            "\n\nConversation context (use to resolve \"it\"/\"that\" and what the user is referring to): {}",
                            summary
                        );
                        (retrieval_query, conversation_context_prompt)
                    } else {
                        (message_for_model.clone(), String::new())
                    }
                }
            } else {
                build_retrieval_from_history(history_ref, &message_for_model)
            }
        } else {
            build_retrieval_from_history(history_ref, &message_for_model)
        };

        let (
            context_section,
            library_rule,
            all_sources,
        ) = if athena_ref_mode {
            (String::new(), LIBRARY_RULE_ATHENA_REF_MODE, Vec::new())
        } else {
            let mentioned = db.get_indexed_files_by_name_mentioned(&retrieval_query).unwrap_or_default();
            let mentioned_section = if mentioned.is_empty() {
                String::new()
            } else {
                let ctx = build_context_from_docs(&mentioned, &retrieval_query, 4000);
                format!(
                    "\n\nThe user referred to or asked about the following document(s); use them to answer:\n{}\n\n",
                    ctx
                )
            };

            let has_conversation_context = retrieval_query != message_for_model;
            let keyword_limit = if has_conversation_context {
                RRF_KEYWORD_LIMIT_WITH_CONTEXT
            } else {
                RRF_KEYWORD_LIMIT
            };
            let semantic_limit = if has_conversation_context {
                RRF_SEMANTIC_LIMIT_WITH_CONTEXT
            } else {
                RRF_SEMANTIC_LIMIT
            };
            let mut docs = db.search_text_content(&retrieval_query, keyword_limit).unwrap_or_default();
            sort_docs_learned_first(&mut docs);
            let query_vec = embeddings::embed_text(&retrieval_query);
            let semantic_chunks = db.search_similar_chunks(&query_vec, semantic_limit).unwrap_or_default();

            let (rrf_context, source_names) = build_context_rrf(
                &docs,
                &semantic_chunks,
                &retrieval_query,
                has_conversation_context,
            );

            let keyword_semantic_section = if rrf_context.is_empty() {
                let recent = db.get_recent_text_files(3).unwrap_or_default();
                let fallback = build_recent_docs_context(&recent, 600, 2000);
                if fallback.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n\nRecent library documents (use if the user's question might be answered here):\n{}",
                        fallback
                    )
                }
            } else {
                format!(
                    "\n\nRelevant excerpts from the user's library (use these to answer questions about the user's documents, property, or notes when the answer appears below). Addresses and property details here are from the user's Library—cite them as such. The \"current location\" line elsewhere is from Settings for general area only.\n{}",
                    rrf_context
                )
            };

            let context_section = format!("{}{}", mentioned_section, keyword_semantic_section);

            let mut all_sources: Vec<String> = mentioned.iter().map(|d| d.name.clone()).collect();
            for n in &source_names {
                if !all_sources.contains(n) {
                    all_sources.push(n.clone());
                }
            }
            const MAX_SOURCES_EMIT: usize = 12;
            if all_sources.len() > MAX_SOURCES_EMIT {
                all_sources.truncate(MAX_SOURCES_EMIT);
            }

            let library_rule = if context_section.trim().is_empty() {
                "When the user asks about something that might be in their Library (e.g. their house, notes), say you don't have that information and suggest they add or place a relevant document in their Library folder, then try again."
            } else {
                LIBRARY_RULE_WITH_CONTEXT
            };

            (context_section, library_rule, all_sources)
        };

        if !all_sources.is_empty() {
            let _ = app_emit.emit(STREAM_SOURCES_EVENT, &all_sources);
        }

        let athena_ref_section = if athena_ref_mode {
            format!(
                "\n\n---\nAthena application reference (authoritative for questions about the Athena app):\n{}\n---\n",
                ATHENA_PROJECT_REFERENCE
            )
        } else {
            String::new()
        };

        let persistent_knowledge_section = build_persistent_knowledge_section(&app_data_dir);
        let location_section = build_location_section(&config);
        let poi_maps_section = build_poi_and_maps_section(&app_data_dir);
        let user_gps_section = build_user_gps_section(user_lat, user_lon);
        let personality_section = build_personality_section(&config);
        let system_content = format!(
            "You are Athena, a local offline survival and repair assistant. You help with practical skills, first aid, repair, and survival knowledge. Answer concisely and clearly.\n\nCritical rule: Do NOT guess or make up facts. {}{}{}{}{}{}{}{}{}",
            library_rule,
            conversation_context_prompt,
            persistent_knowledge_section,
            context_section,
            athena_ref_section,
            location_section,
            poi_maps_section,
            user_gps_section,
            personality_section
        );

        let max_memory = config.memory_messages.unwrap_or(20) as usize;
        let messages = build_chat_messages(system_content, history_ref, &message_for_model, max_memory);

        let mut on_chunk = |chunk: &str| {
            let _ = app_emit.emit(STREAM_CHUNK_EVENT, chunk);
        };
        let on_done = || {
            let _ = app_emit.emit(STREAM_DONE_EVENT, ());
        };
        if let Err(e) = chat_stream(&model_name, messages, &mut on_chunk, on_done) {
            let _ = app_emit.emit(STREAM_ERROR_EVENT, e);
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = athena_mbtiles_tile_protocol::register_with_builder(athena_map_protocol::register_with_builder(
        tauri::Builder::default(),
    ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_plugin_macos_permissions::init());
    builder.setup(|app| {
            // Ensure the native window always has a non-empty title (avoids macOS fullscreen crash when title is null).
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title("Athena");
            }
            let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
            let _ = map_manager_download::ensure_offline_maps_directory(&app_data_dir);
            let _ = map_manager_download::cleanup_stale_part_files(&app_data_dir);
            let mesh_control =
                mesh_listener::spawn_mesh_radio_listener(app.handle().clone(), app_data_dir.clone());
            app.manage(mesh_control);
            let db_path = app_data_dir.join("athena_index.db");
            let db = Database::new(&db_path)?;
            let db = Arc::new(db);
            let (watcher_tx, watcher_rx) = crossbeam_channel::unbounded();
            run_watcher_loop(Arc::clone(&db), watcher_rx, app_data_dir.clone(), app.handle().clone());
            app.manage(AppState {
                db,
                watcher_tx: watcher_tx.clone(),
                active_offline_map_downloads: Arc::new(Mutex::new(HashSet::new())),
            });
            let config = load_config(&app_data_dir);
            let kiwix_dir = app_data_dir.join("kiwix_downloads");
            std::fs::create_dir_all(&kiwix_dir).map_err(|e| e.to_string())?;
            let kiwix_dir_str = kiwix_dir.to_string_lossy().to_string();
            let folders: Vec<String> = match &config.watch_folder {
                Some(folder) => {
                    let norm = folder.trim();
                    if norm.is_empty() {
                        vec![kiwix_dir_str]
                    } else if let Ok(canon) = std::path::Path::new(norm).canonicalize() {
                        if canon == app_data_dir {
                            vec![kiwix_dir_str]
                        } else {
                            vec![folder.clone(), kiwix_dir_str]
                        }
                    } else {
                        vec![folder.clone(), kiwix_dir_str]
                    }
                }
                None => vec![kiwix_dir_str],
            };
            let _ = watcher_tx.send(WatchCommand::SetFolders(folders));
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(800));
                if !is_ollama_running() {
                    try_launch_ollama_server();
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_watch_folder,
            set_watch_folder,
            get_theme_config,
            set_theme_config,
            get_indexed_files,
            ingest_files,
            get_index_stats,
            get_rag_stats,
            rebuild_vector_index,
            list_library_contents,
            remove_library_file,
            get_system_specs,
            get_battery_state,
            get_ollama_available_models,
            is_ollama_available,
            try_launch_ollama,
            get_recommended_model,
            get_model_override,
            set_model_override,
            get_memory_messages,
            set_memory_messages,
            get_feature_map_enabled,
            set_feature_map_enabled,
            get_show_sources_in_chat,
            set_show_sources_in_chat,
            get_user_location,
            set_user_location,
            get_memory_estimate,
            get_local_time,
            get_sun_times,
            send_message,
            send_message_stream,
            scan_kiwix_zim_files,
            get_map_catalog,
            get_offline_map_manager_catalog,
            list_downloaded_offline_map_ids,
            pmtiles_region_extract::extract_pmtiles_region,
            download_offline_map,
            get_downloaded_maps,
            check_network_reachable,
            get_smallest_kiwix_map_suggestion,
            geocode_place_query,
            list_tile_packs_cmd,
            offline_basemap_paths_cmd,
            estimate_tile_pack_tiles,
            export_tile_pack_to_mbtiles_file,
            start_mbtiles_import,
            start_pmtiles_import,
            remove_tile_pack_cmd,
            start_tile_pack_download,
            download_map,
            remove_downloaded_map,
            get_pois,
            add_poi,
            remove_poi,
            update_poi,
            list_chats,
            get_chat,
            save_chat,
            update_chat_metadata,
            update_chat_context_summary,
            generate_chat_title,
            generate_chat_description,
            delete_chat,
            synthesize_speech,
            append_error_log,
            get_error_log,
            get_device_stats,
            build_library_knowledge,
            set_fullscreen,
            is_fullscreen,
            send_mesh_message,
            get_mesh_ai_auto_reply,
            set_mesh_ai_auto_reply,
            is_mesh_radio_connected,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
