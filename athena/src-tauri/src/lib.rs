mod academy;
mod auth;
mod athena_map_protocol;
mod athena_mbtiles_tile_protocol;
mod chats;
mod chunking;
mod commands;
mod config;
mod db;
mod downloader;
mod neural_embed;
mod rerank;
mod embeddings;
mod hardware_map;
mod indexer;
mod inference;
mod inventory;
#[cfg(target_os = "linux")]
mod isoc23_shim;
mod kiwix;
mod kiwix_tools;
mod lan_cluster;
mod lan_identity;
mod lan_packets;
mod lan_sync;
mod learner;
mod library_installer_location;
#[cfg(target_os = "macos")]
mod macos_window;
mod map_manager;
mod map_manager_catalog;
mod map_manager_download;
mod maps;
mod mbtiles_io;
mod mesh_ai;
mod mesh_calendar;
mod mesh_router;
mod mesh_listener;
mod mesh_logbook;
mod mesh_sos;
mod mesh_radio;
mod moondream;
mod ocr;
mod offline_routing;
mod ollama;
mod ollama_setup;
mod openai_chat;
mod package_manager;
mod portable;
mod diagnostics;
mod pmtiles_io;
mod pmtiles_region_extract;
mod quarantine;
mod radio_manager;
mod rag_indexer;
mod remote_queue;
mod satellite_region_download;
mod sdr_manager;
mod sdr_radio;
mod spatial;
mod system;
mod telemetry;
mod theme;
mod tile_pack;
#[cfg(feature = "piper-tts")]
mod tts;
mod user_profile;
mod usb;
mod vector_index;
mod watcher;

/// Full product name (locked in). The UI and assistant persona may still use the short name **Athena**.
pub const ATHENA_FULL_NAME: &str =
    "Autonomous Tactical Heuristic Emergency Network Archive (ATHENA)";

/// Re-exported for integration tests (`tests/`) and external callers embedding `athena_lib`.
pub use db::Database;
pub use portable::init_portable_app_data_override;
pub use ollama::{chat_stream, ChatMessage, ModelOptions};

use crossbeam_channel::Sender;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;

use chats::{ChatSummary, SavedChat, SavedMessage};
use config::{
    effective_embedding_folder, effective_reranker_folder, ensure_model_directories, load_config,
    save_config, AppConfig, CustomTheme, ModelDirectoryPaths,
};
use db::{
    extract_keywords, AcademyProgressRow, ArchiveWatchRootRow, CalendarEvent, ChunkEmbedding,
    HybridSearchHit, IndexStats, IndexedFile, LogbookEntry, RagCounts, TacticalMeshNodeLocation,
    TacticalPointOfInterest,
};
use inference::{
    chat_stream_unified, chat_unified, ensure_inference_ready, generate_chat_description_unified,
    generate_chat_title_unified, load_models_for_inference,
};
use kiwix::{scan_kiwix_zim_files_multi as kiwix_scan_zim_files_multi, DiscoveredZim};
use learner::build_learned_notes_for_all_text;
use ollama::{
    get_available_models, get_system_specs as get_specs, is_ollama_running,
    max_memory_messages_for_ram, maybe_ensure_default_ollama_model, path_looks_like_ollama_cli,
    select_model_for_tier, try_launch_ollama_server_with_preferences, ModelInfo, SystemSpecs,
};
use radio_manager::RadioState;
use walkdir::WalkDir;
use watcher::{run_watcher_loop, WatchCommand};

pub struct AppState {
    /// Shared SQLite handle; must be exclusive during restore/replace of `athena_index.db`.
    pub db: Arc<Mutex<Database>>,
    /// AppData `models/...` tree for ONNX and chat assets.
    pub model_paths: ModelDirectoryPaths,
    pub watcher_tx: Sender<WatchCommand>,
    pub active_offline_map_downloads: Arc<Mutex<HashSet<String>>>,
    pub active_package_downloads: Arc<Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
    /// Serialize reads/writes of `athena_config.json`.
    pub config_lock: Arc<Mutex<()>>,
}


/// Chat models: only `.gguf` files; skip hidden names (e.g. `.DS_Store`).
fn list_sorted_gguf_files(dir: &std::path::Path) -> Result<Vec<String>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !name.starts_with('.') && name.to_lowercase().ends_with(".gguf")
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

fn validate_model_subfolder_name(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("Folder name is empty.".to_string());
    }
    if s.contains('/') || s.contains('\\') || s.contains("..") {
        return Err("Invalid folder name.".to_string());
    }
    if s.chars()
        .any(|c| matches!(c, '\0' | ':' | '<' | '>' | '|' | '*' | '?'))
    {
        return Err("Invalid folder name.".to_string());
    }
    Ok(s.to_string())
}

fn list_sorted_subdirs(dir: &std::path::Path) -> Result<Vec<String>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    Ok(names)
}

#[tauri::command]
fn scan_chat_models(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    list_sorted_gguf_files(&state.model_paths.chat_models)
}

#[tauri::command]
fn scan_embedding_models(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    list_sorted_subdirs(&state.model_paths.embeddings_root)
}

#[tauri::command]
fn scan_reranker_models(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    list_sorted_subdirs(&state.model_paths.rerankers_root)
}

/// Bundled `ollama serve` sidecar process; stdout/stderr are drained asynchronously; killed on [`RunEvent::Exit`].
struct OllamaState(Mutex<Option<CommandChild>>);

fn kill_ollama_sidecar_if_running(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<OllamaState>() else {
        return;
    };
    let Ok(mut guard) = state.0.lock() else {
        return;
    };
    if let Some(child) = guard.take() {
        if let Err(e) = child.kill() {
            eprintln!("[athena] failed to stop Ollama sidecar: {e}");
        }
    }
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
    if text.len() > 233 {
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
fn add_mesh_profile(
    state: tauri::State<'_, AppState>,
    node_id: i64,
    username: String,
    counter_sign: String,
) -> Result<db::MeshProfile, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .mesh_profile_upsert(node_id, &username, &counter_sign, 1)
}

#[tauri::command]
fn mesh_emergency_list_active(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<db::MeshProfile>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_mesh_profiles_sos_non_normal()
}

#[tauri::command]
fn mesh_emergency_operator_resolve(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    node_id: i64,
    accept_clear: bool,
) -> Result<(), String> {
    let u = auth::authenticated_user(&session)?;
    if u.rank_level < 4 {
        return Err("Centurion rank (4+) required.".to_string());
    }
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .mesh_emergency_operator_resolve(node_id, accept_clear)?;
    let n = u32::try_from(node_id).map_err(|_| "Invalid node id.".to_string())?;
    crate::mesh_router::emit_mesh_emergency(&app, &state.db, n);
    Ok(())
}

#[tauri::command]
fn remove_mesh_profile(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .mesh_profile_delete_by_id(&id)?;
    Ok(())
}

#[tauri::command]
fn list_mesh_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<db::MeshProfile>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .mesh_profile_list()
}

#[tauri::command]
fn list_mesh_network_nodes_cmd(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<db::MeshNetworkNodeSummary>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_mesh_network_nodes()
}

#[tauri::command]
fn list_mesh_chats_for_node_cmd(
    state: tauri::State<'_, AppState>,
    node_id: i64,
    limit: Option<usize>,
) -> Result<Vec<db::MeshChatRow>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_mesh_chats_for_node(node_id, limit.unwrap_or(200))
}

#[tauri::command]
async fn send_mesh_operator_chat_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
    node_id: i64,
    text: String,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Message text is empty.".to_string());
    }
    if text.len() > 233 {
        return Err("Message is too long (max 233 bytes for typical Meshtastic text).".to_string());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let id = uuid::Uuid::new_v4().to_string();
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .mesh_chat_insert(&id, node_id, "human", "chat", &text, now)?;
    let dest = u32::try_from(node_id)
        .map_err(|_| "Node id is out of range for Meshtastic.".to_string())?;
    control
        .send_text_message(
            text,
            false,
            mesh_ai::PRIMARY_MESH_CHANNEL,
            Some(dest),
        )
        .await?;
    let _ = app.emit(
        mesh_router::MESH_NETWORK_CHAT_UPDATED,
        serde_json::json!({ "nodeId": dest }),
    );
    Ok(())
}

static LOGBOOK_ENTRY_SEQ: AtomicU64 = AtomicU64::new(1);
static CALENDAR_ENTRY_SEQ: AtomicU64 = AtomicU64::new(1);

/// Logbook profile occupation (Settings) — substring match unlocks tactical notes for ranks below 4.
fn tactical_occupation_matches(occ: &str) -> bool {
    let o = occ.to_lowercase();
    [
        "soldier",
        "marine",
        "infantry",
        "sergeant",
        "officer",
        "tactical",
        "operator",
        "security",
        "military",
        "army",
        "navy",
        "airman",
        "air force",
        "national guard",
        "guard",
        "mp",
        "mps",
        "corpsman",
        "sailor",
        "coast guard",
        "ranger",
        "artillery",
        "cavalry",
        "commando",
        "recon",
        "scout",
    ]
    .iter()
    .any(|k| o.contains(k))
}

fn can_access_tactical_logbook(session: &auth::AuthSession, occupation_cfg: &str) -> bool {
    if auth::authenticated_user(session)
        .map(|u| u.rank_level >= 4)
        .unwrap_or(false)
    {
        return true;
    }
    tactical_occupation_matches(occupation_cfg)
}

#[tauri::command]
fn logbook_list_entries(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
) -> Result<Vec<LogbookEntry>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    let occ = cfg
        .logbook_author_occupation
        .as_deref()
        .unwrap_or("")
        .trim();
    let include_tactical = can_access_tactical_logbook(&session, occ);
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_logbook_entries(include_tactical)
}

#[tauri::command]
async fn logbook_add_entry(
    app: tauri::AppHandle,
    body: String,
    parent_entry_id: Option<String>,
    notebook: Option<String>,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
) -> Result<LogbookEntry, String> {
    let body = body.trim().to_string();
    if body.is_empty() {
        return Err("Note is empty.".to_string());
    }
    if body.len() > 32_000 {
        return Err("Note is too long (32k character limit).".to_string());
    }
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    let author_name = cfg
        .logbook_author_name
        .as_deref()
        .unwrap_or("")
        .trim();
    let author_occupation = cfg
        .logbook_author_occupation
        .as_deref()
        .unwrap_or("")
        .trim();
    let nb_raw = notebook
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("general");
    let nb_norm = if nb_raw.eq_ignore_ascii_case("tactical") {
        "tactical"
    } else {
        "general"
    };
    if nb_norm == "tactical" && !can_access_tactical_logbook(&session, author_occupation) {
        return Err(
            "Tactical notes require Centurion rank (4+) or a military-style job title in your logbook profile."
                .to_string(),
        );
    }
    let parent = parent_entry_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(pid) = parent {
        let prow = state
            .db
            .lock()
            .map_err(|e| e.to_string())?
            .get_logbook_entry(pid)?;
        let Some(p) = prow else {
            return Err("Parent log entry was not found.".to_string());
        };
        let p_tac = p.notebook.eq_ignore_ascii_case("tactical");
        if nb_norm == "tactical" && !p_tac {
            return Err("Tactical notes must reply under another tactical note.".to_string());
        }
        if nb_norm == "general" && p_tac {
            return Err("Field notes cannot be threaded under tactical notes.".to_string());
        }
    }
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let id = format!(
        "lb-{}-{}",
        created_at,
        LOGBOOK_ENTRY_SEQ.fetch_add(1, Ordering::Relaxed)
    );
    {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        g.insert_logbook_local(
            &id,
            &body,
            created_at,
            author_name,
            author_occupation,
            parent,
            nb_norm,
        )?;
    }
    let entry = state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .get_logbook_entry(&id)?
        .ok_or_else(|| "Logbook insert failed.".to_string())?;
    if mesh_listener::mesh_radio_is_connected() && nb_norm == "general" {
        let wire =
            mesh_logbook::wire_from_logbook_entry(&entry, mesh_listener::mesh_local_node_id());
        if let Ok(packet) = mesh_logbook::encode_logbook_packet(&wire) {
            let _ = control
                .send_text_message(packet, false, mesh_ai::PRIMARY_MESH_CHANNEL, None)
                .await;
        }
    }
    Ok(entry)
}

#[tauri::command]
fn academy_get_curriculum() -> Result<academy::AcademyCurriculumRoot, String> {
    academy::get_curriculum()
}

#[tauri::command]
fn academy_list_completed(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AcademyProgressRow>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_academy_progress()
}

#[tauri::command]
fn academy_mark_lesson_complete(
    state: tauri::State<'_, AppState>,
    args: academy::AcademyLessonIdArg,
) -> Result<(), String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .mark_academy_lesson_complete(&args.lesson_id)
}

#[tauri::command]
fn academy_get_lesson_html(
    app: tauri::AppHandle,
    args: academy::AcademyLessonIdArg,
) -> Result<academy::AcademyLessonHtml, String> {
    academy::fetch_lesson_html(&app, &args.lesson_id)
}

#[tauri::command]
fn logbook_delete_entry(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .delete_logbook_entry(id.trim())
}

#[tauri::command]
async fn logbook_broadcast_entry(
    id: String,
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let id = id.trim().to_string();
    let entry = state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .get_logbook_entry(&id)?
        .ok_or_else(|| "Log entry not found.".to_string())?;
    if entry.notebook.eq_ignore_ascii_case("tactical") {
        return Err("Tactical notes are not sent over the mesh.".to_string());
    }
    let wire = mesh_logbook::wire_from_logbook_entry(&entry, mesh_listener::mesh_local_node_id());
    let packet = mesh_logbook::encode_logbook_packet(&wire)?;
    control
        .send_text_message(packet, false, mesh_ai::PRIMARY_MESH_CHANNEL, None)
        .await
}


#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogbookAuthorProfilePayload {
    name: String,
    occupation: String,
}

#[tauri::command]
fn get_logbook_author_profile(app: tauri::AppHandle) -> Result<LogbookAuthorProfilePayload, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let c = load_config(&app_data_dir);
    Ok(LogbookAuthorProfilePayload {
        name: c.logbook_author_name.unwrap_or_default().trim().to_string(),
        occupation: c
            .logbook_author_occupation
            .unwrap_or_default()
            .trim()
            .to_string(),
    })
}

#[tauri::command]
fn set_logbook_author_profile(
    app: tauri::AppHandle,
    name: String,
    occupation: String,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut c = load_config(&app_data_dir);
    c.logbook_author_name = Some(name.trim().to_string());
    c.logbook_author_occupation = Some(occupation.trim().to_string());
    save_config(&app_data_dir, &c)
}

#[tauri::command]
fn logbook_mesh_max_body_bytes() -> usize {
    mesh_logbook::max_body_utf8_bytes_for_mesh()
}

#[tauri::command]
async fn logbook_request_mesh_sync(
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
) -> Result<(), String> {
    let packet = mesh_logbook::encode_logbook_sync_request()?;
    control
        .send_text_message(packet, false, mesh_ai::PRIMARY_MESH_CHANNEL, None)
        .await
}

#[tauri::command]
fn calendar_list_events(
    state: tauri::State<'_, AppState>,
    range_start: Option<i64>,
    range_end: Option<i64>,
    include_deleted: Option<bool>,
) -> Result<Vec<CalendarEvent>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_calendar_events(range_start, range_end, include_deleted.unwrap_or(false))
}

#[tauri::command]
async fn calendar_upsert_event(
    mut event: CalendarEvent,
    state: tauri::State<'_, AppState>,
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
) -> Result<CalendarEvent, String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if event.id.trim().is_empty() {
        event.id = format!(
            "cal-{}-{}",
            now_ms,
            CALENDAR_ENTRY_SEQ.fetch_add(1, Ordering::Relaxed)
        );
    } else {
        event.id = event.id.trim().to_string();
    }
    event.title = event.title.trim().to_string();
    if event.title.is_empty() {
        return Err("Event title is required.".to_string());
    }
    if event.start_ts > event.end_ts {
        std::mem::swap(&mut event.start_ts, &mut event.end_ts);
    }
    event.source = if event.source.trim().is_empty() {
        "user".to_string()
    } else {
        event.source.trim().to_string()
    };
    if event.updated_at <= 0 {
        event.updated_at = now_ms;
    }
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .upsert_calendar_event(&event)?;

    if mesh_listener::mesh_radio_is_connected() {
        let wire =
            mesh_calendar::wire_from_calendar_event(&event, mesh_listener::mesh_local_node_id());
        if let Ok(packet) = mesh_calendar::encode_calendar_packet(&wire) {
            let _ = control
                .send_text_message(packet, false, mesh_ai::PRIMARY_MESH_CHANNEL, None)
                .await;
        }
    }
    Ok(event)
}

#[tauri::command]
async fn calendar_delete_event(
    id: String,
    state: tauri::State<'_, AppState>,
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
) -> Result<(), String> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("Event id is required.".to_string());
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .tombstone_calendar_event(&id, now_ms)?;
    let tombstone = CalendarEvent {
        id,
        title: "(deleted)".to_string(),
        notes: None,
        start_ts: now_ms,
        end_ts: now_ms,
        all_day: true,
        location_label: None,
        linked_marker_id: None,
        participant_names: Vec::new(),
        reminder_minutes_before: None,
        recurrence_rule: None,
        category: None,
        source: "user".to_string(),
        updated_at: now_ms,
        origin_node_id: None,
        deleted: true,
    };
    if mesh_listener::mesh_radio_is_connected() {
        let wire = mesh_calendar::wire_from_calendar_event(
            &tombstone,
            mesh_listener::mesh_local_node_id(),
        );
        if let Ok(packet) = mesh_calendar::encode_calendar_packet(&wire) {
            let _ = control
                .send_text_message(packet, false, mesh_ai::PRIMARY_MESH_CHANNEL, None)
                .await;
        }
    }
    Ok(())
}

#[tauri::command]
async fn calendar_request_mesh_sync(
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
) -> Result<(), String> {
    let packet = mesh_calendar::encode_calendar_sync_request()?;
    control
        .send_text_message(packet, false, mesh_ai::PRIMARY_MESH_CHANNEL, None)
        .await
}

#[tauri::command]
fn get_mesh_ai_auto_reply(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir)
        .mesh_ai_auto_reply
        .unwrap_or(true))
}

#[tauri::command]
fn is_mesh_radio_connected() -> bool {
    mesh_listener::mesh_radio_is_connected()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshRadioIdentity {
    connected: bool,
    node_id: u32,
    display_name: String,
}

#[tauri::command]
fn get_mesh_radio_identity() -> MeshRadioIdentity {
    MeshRadioIdentity {
        connected: mesh_listener::mesh_radio_is_connected(),
        node_id: mesh_listener::mesh_local_node_id(),
        display_name: mesh_listener::mesh_local_display_name(),
    }
}

#[tauri::command]
fn is_mesh_remote_registration_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    Ok(cfg
        .mesh_registration_admin_password
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false))
}

#[tauri::command]
fn set_mesh_registration_admin_password(
    app: tauri::AppHandle,
    password: Option<String>,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.mesh_registration_admin_password = password.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    save_config(&app_data_dir, &cfg)
}

#[tauri::command]
fn set_mesh_ai_auto_reply(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.mesh_ai_auto_reply = Some(value);
    save_config(&app_data_dir, &cfg)
}

#[tauri::command]
fn get_ai_inference_sleep(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir).ai_inference_sleep.unwrap_or(false))
}

#[tauri::command]
fn set_ai_inference_sleep(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.ai_inference_sleep = Some(value);
    save_config(&app_data_dir, &cfg)?;
    let _ = app.emit(
        AI_INFERENCE_SLEEP_EVENT,
        serde_json::json!({ "asleep": value }),
    );
    Ok(())
}

/// Power menu: pause local AI, turn off mesh auto-reply, stop Athena-managed Ollama sidecar. Caller should log out next.
#[tauri::command]
fn enter_station_minimal_mode(app: tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.ai_inference_sleep = Some(true);
    cfg.mesh_ai_auto_reply = Some(false);
    save_config(&app_data_dir, &cfg)?;
    let _ = app.emit(
        AI_INFERENCE_SLEEP_EVENT,
        serde_json::json!({ "asleep": true }),
    );
    kill_ollama_sidecar_if_running(&app);
    Ok(())
}

#[tauri::command]
fn get_watch_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let state = app.state::<AppState>();
    let g = state.db.lock().map_err(|e| e.to_string())?;
    if let Some(p) = g.first_archive_watch_root_path()? {
        return Ok(Some(p));
    }
    drop(g);
    Ok(load_config(&app_data_dir).watch_folder)
}

#[tauri::command]
#[allow(clippy::type_complexity)]
fn get_theme_config(
    app: tauri::AppHandle,
) -> Result<(Option<String>, Option<String>, Vec<CustomTheme>), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.theme_id = theme_id;
    cfg.personality_id = personality_id;
    cfg.custom_themes = custom_themes;
    save_config(&app_data_dir, &cfg)
}

/// Toggle fullscreen. On macOS we use **simple fullscreen** (not native Spaces fullscreen) so it
/// stays compatible with stripping `FullScreenPrimary` (which avoids a green-button AppKit crash).
#[tauri::command]
async fn set_fullscreen(app: tauri::AppHandle, fullscreen: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
    #[cfg(not(target_os = "macos"))]
    {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }

    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    let _ = window.set_title("ATHENA");

    #[cfg(target_os = "macos")]
    {
        window
            .set_simple_fullscreen(fullscreen)
            .map_err(|e| e.to_string())?;
        if let Some(state) = app.try_state::<macos_window::MacosSimpleFullscreenState>() {
            state.0.store(fullscreen, Ordering::SeqCst);
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.set_fullscreen(fullscreen).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn is_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    if let Some(state) = app.try_state::<macos_window::MacosSimpleFullscreenState>() {
        return Ok(state.0.load(Ordering::SeqCst));
    }
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.is_fullscreen().map_err(|e| e.to_string())
}

pub(crate) fn kiwix_downloads_dir(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join("kiwix_downloads")
}

pub(crate) fn compute_watcher_folders(
    db: &Database,
    app_data_dir: &std::path::Path,
    config: &AppConfig,
) -> Result<Vec<String>, String> {
    let kiwix_dir = kiwix_downloads_dir(app_data_dir);
    std::fs::create_dir_all(&kiwix_dir).map_err(|e| e.to_string())?;
    let kiwix_str = kiwix_dir.to_string_lossy().to_string();
    let mut folders: Vec<String> = Vec::new();
    for folder in db.list_archive_watch_root_paths_ordered()? {
        let norm = folder.trim();
        if norm.is_empty() {
            continue;
        }
        let resolved = crate::portable::resolve_library_watch_root_path(norm, app_data_dir);
        let resolved = resolved.canonicalize().unwrap_or(resolved);
        if resolved.as_path() == app_data_dir {
            continue;
        }
        let push_str = resolved.to_string_lossy().to_string();
        if !folders.iter().any(|f| f == &push_str) {
            folders.push(push_str);
        }
    }
    if folders.is_empty() {
        if let Some(ref wf) = config.watch_folder {
            let norm = wf.trim();
            if !norm.is_empty() {
                let resolved = crate::portable::resolve_library_watch_root_path(norm, app_data_dir);
                let resolved = resolved.canonicalize().unwrap_or(resolved);
                if resolved.as_path() != app_data_dir {
                    let push_str = resolved.to_string_lossy().to_string();
                    if !folders.iter().any(|f| f == &push_str) {
                        folders.push(push_str);
                    }
                }
            }
        }
    }
    if !folders.iter().any(|f| f == &kiwix_str) {
        folders.push(kiwix_str);
    }
    Ok(folders)
}

fn refresh_watcher_folders_from_app(app: &tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let state = app.state::<AppState>();
    let folders = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, &app_data_dir, &config)?
    };
    state
        .watcher_tx
        .send(WatchCommand::SetFolders(folders))
        .map_err(|e| e.to_string())?;
    Ok(())
}


#[tauri::command]
fn set_watch_folder(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let state = app.state::<AppState>();
    {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        g.clear_archive_watch_roots()?;
        if let Some(ref p) = path {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                g.add_archive_watch_root(trimmed, None)?;
            }
        }
    }
    let mut config = load_config(&app_data_dir);
    config.watch_folder = path.clone();
    save_config(&app_data_dir, &config)?;
    refresh_watcher_folders_from_app(&app)?;

    // Kick off "learned notes" generation in the background so the assistant can retrieve distilled facts.
    let db = Arc::clone(&state.db);
    std::thread::spawn(move || {
        let app_data_dir = match crate::portable::resolved_app_data_dir(&app) {
            Ok(d) => d,
            Err(_) => return,
        };
        let config = load_config(&app_data_dir);
        let models = load_models_for_inference(&config);
        if ensure_inference_ready(&config, &models).is_err() {
            return;
        }
        let _ = match db.lock() {
            Ok(guard) => build_learned_notes_for_all_text(&guard, &config, &models, 50),
            Err(_) => return,
        };
    });

    Ok(())
}


#[tauri::command]
fn list_archive_watch_roots(state: tauri::State<'_, AppState>) -> Result<Vec<ArchiveWatchRootRow>, String> {
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.list_archive_watch_roots()
}

#[tauri::command]
fn add_archive_watch_root(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
    label: Option<String>,
) -> Result<i64, String> {
    let id = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        let lbl = label.as_deref().filter(|s| !s.trim().is_empty());
        g.add_archive_watch_root(&path, lbl)
            .map_err(|e| if e.contains("UNIQUE") { "That folder is already registered.".to_string() } else { e })?
    };
    refresh_watcher_folders_from_app(&app)?;
    Ok(id)
}

#[tauri::command]
fn remove_archive_watch_root(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        g.remove_archive_watch_root(id)?;
    }
    refresh_watcher_folders_from_app(&app)?;
    Ok(())
}


#[tauri::command]
fn build_library_knowledge(app: tauri::AppHandle, limit: Option<u32>) -> Result<u32, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;
    let state = app.state::<AppState>();
    let made = build_learned_notes_for_all_text(
        &*state.db.lock().map_err(|e| e.to_string())?,
        &config,
        &models,
        limit.unwrap_or(200) as usize,
    )?;
    Ok(made as u32)
}

#[tauri::command]
fn get_indexed_files(app: tauri::AppHandle) -> Result<Vec<IndexedFile>, String> {
    let state = app.state::<AppState>();
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.get_all()
}

/// Manually ingest one or more files into the index. Corrupted or unreadable files
/// are moved to quarantine and a notification is emitted; the app does not crash.
#[tauri::command]
fn ingest_files(app: tauri::AppHandle, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let state = app.state::<AppState>();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    for p in paths {
        let path_buf = std::path::PathBuf::from(&p);
        let path = path_buf.as_path();
        if !path.is_file() {
            continue;
        }
        if !indexer::is_supported_path(path) {
            continue;
        }
        let ctx = watcher::resolve_ingest_context(&*db, &app_data_dir, path);
        let ctx_tuple = ctx.as_ref().map(|(k, r)| (k.as_str(), r.as_path()));
        if indexer::skip_if_up_to_date(path, &*db, ctx_tuple).is_some() {
            continue;
        }
        let ingest_display_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty());
        let _ingest_guard =
            watcher::IngestionActivityGuard::begin(&app, ingest_display_name);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| indexer::index_file(path)));
        match result {
            Ok(Some(mut data)) => {
                indexer::fill_relative_path_key(&mut data, ctx_tuple, path);
                println!("[athena] Checkpoint M1: ingest_files got Some; upsert_file");
                if let Err(e) = db.upsert_file(
                    &data.path,
                    &data.file_type,
                    &data.name,
                    Some(data.size),
                    data.modified_at,
                    data.text_content.as_deref(),
                    data.metadata_json.as_deref(),
                    Some(data.display_title.as_str()),
                    data.page_count.map(|n| n as i64),
                    data.relative_path.as_deref(),
                ) {
                    eprintln!("[athena] Fatal Ingestion Error: ingest upsert_file failed: {:?}", e);
                    continue;
                }
                println!("[athena] Checkpoint M2: upsert ok; maybe index_document");
                if let Some(ref text) = data.text_content {
                    if !text.trim().is_empty() {
                        println!("[athena] Checkpoint M3: calling vector_index::index_document");
                        match vector_index::index_document(
                            &db,
                            &p,
                            &data.display_title,
                            data.index_pages.as_deref(),
                            text,
                        ) {
                            Ok(()) => {}
                            Err(e) => {
                                eprintln!("[athena] Fatal Ingestion Error: {:?}", e);
                            }
                        }
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
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.get_stats()
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
    } = state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .get_rag_counts()?;
    Ok(RagStats {
        document_count,
        chunk_count,
    })
}

#[tauri::command]
fn test_rag_search(query: String, app: tauri::AppHandle) -> Result<(), String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("query is required".to_string());
    }
    let state = app.state::<AppState>();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let hits = db.search_similar_chunks(q, 3)?;
    eprintln!("[athena] test_rag_search query={}", q);
    for (i, h) in hits.iter().enumerate() {
        let preview: String = h.chunk_text.chars().take(400).collect();
        eprintln!(
            "[athena] hit {} file={} chunk_index={} preview={}",
            i + 1,
            h.file_path,
            h.chunk_index,
            preview.replace('\n', " ")
        );
    }
    Ok(())
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
    let all = state.db.lock().map_err(|e| e.to_string())?.get_all()?;
    let files_with_text: Vec<IndexedFile> = all
        .into_iter()
        .filter(|r| {
            r.text_content
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty())
        })
        .collect();
    let total = files_with_text.len();
    let app_handle = app.clone();
    let db = Arc::clone(&state.db);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let guard = db.lock().map_err(|e| e.to_string())?;
        let db_ref = &*guard;
        let mut processed = 0u32;
        let mut failed = 0u32;
        let mut failed_files: Vec<RebuildFailedFile> = Vec::new();
        for row in &files_with_text {
            let text = row
                .text_content
                .as_deref()
                .expect("filter guarantees non-null text_content");
            let doc_title = row
                .display_title
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or(row.name.as_str());
            match vector_index::index_document(db_ref, &row.path, doc_title, None, text) {
                Ok(()) => processed += 1,
                Err(e) => {
                    eprintln!("[athena] Fatal Ingestion Error: {:?}", e);
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
        let _ = db_ref.rebuild_chunk_embeddings_vec();
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

#[derive(Debug, serde::Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, serde::Deserialize)]
struct OllamaTagModel {
    name: String,
}

#[tauri::command]
async fn get_local_models() -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let res = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("Failed to reach Ollama /api/tags: {e}"))?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!(
            "Ollama /api/tags failed with status {status}: {body}"
        ));
    }
    let parsed: OllamaTagsResponse = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama tags response: {e}"))?;
    Ok(parsed.models.into_iter().map(|m| m.name).collect())
}

#[tauri::command]
async fn pull_new_model(model_name: String) -> Result<bool, String> {
    let model = model_name.trim();
    if model.is_empty() {
        return Err("Model name is required.".to_string());
    }
    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:11434/api/pull")
        .json(&serde_json::json!({ "name": model }))
        .send()
        .await
        .map_err(|e| format!("Failed to call Ollama /api/pull: {e}"))?;
    let status_ok = res.status().is_success();
    let _body = res.text().await.unwrap_or_default();
    Ok(status_ok)
}

#[tauri::command]
fn is_ollama_available() -> bool {
    is_ollama_running()
}

/// `true` when the app is managing a live bundled `ollama serve` sidecar (`OllamaState` holds a child process).
#[tauri::command]
fn get_sidecar_status(state: tauri::State<'_, OllamaState>) -> bool {
    state.0.lock().map(|guard| guard.is_some()).unwrap_or(false)
}

#[tauri::command]
fn get_ollama_setup_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, OllamaState>,
) -> Result<ollama_setup::OllamaSetupStatus, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let sidecar = state.0.lock().map(|guard| guard.is_some()).unwrap_or(false);
    Ok(ollama_setup::build_setup_status(
        &app_data_dir,
        &config,
        sidecar,
    ))
}

#[tauri::command]
fn install_ollama_cli_for_athena(app: tauri::AppHandle) -> Result<String, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let p = ollama_setup::install_ollama_cli_to_app_data(&app_data_dir)?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
fn copy_ollama_into_athena_managed(
    app: tauri::AppHandle,
    source_path: String,
) -> Result<String, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let src = std::path::PathBuf::from(source_path.trim());
    let p = ollama_setup::copy_cli_into_managed_dir(&src, &app_data_dir)?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
fn set_ollama_athena_cli_path(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    let normalized = path.and_then(|p| {
        let t = p.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    if let Some(ref p) = normalized {
        let pb = std::path::Path::new(p);
        if !path_looks_like_ollama_cli(pb) {
            return Err("That path is not an ollama or ollama.exe file.".to_string());
        }
    }
    config.ollama_athena_cli_path = normalized;
    save_config(&app_data_dir, &config)
}

/// Tries to start the Ollama server (e.g. `ollama serve` or open Ollama app on macOS).
/// Call this when the user explicitly asks to start Ollama; the app may need permission to launch processes.
#[tauri::command]
fn try_launch_ollama(app: tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let prefs = ollama_setup::resolve_launch_paths(&app_data_dir, &config);
    try_launch_ollama_server_with_preferences(&prefs);
    Ok(())
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir).model_override)
}

#[tauri::command]
fn set_model_override(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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

#[derive(serde::Serialize)]
struct UserAndInferenceSettings {
    user_display_name: Option<String>,
    user_birthdate_iso: Option<String>,
    allow_nsfw_unrestricted_models: bool,
    is_at_least_18: bool,
    is_birthday_today: bool,
    time_format: String,
    calendar_region: Option<String>,
    calendar_quiet_hours_start: u8,
    calendar_quiet_hours_end: u8,
    inference_backend: Option<String>,
    explicit_ollama_model: Option<String>,
    openai_compatible_base_url: Option<String>,
    openai_compatible_model: Option<String>,
    openai_compatible_api_key_set: bool,
    /// Effective ONNX embedding folder under `models/embeddings`.
    active_embedding_model_folder: String,
    /// Effective ONNX reranker folder under `models/rerankers`.
    active_reranker_model_folder: String,
    ai_inference_sleep: bool,
}

#[tauri::command]
fn get_user_and_inference_settings(
    app: tauri::AppHandle,
) -> Result<UserAndInferenceSettings, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let c = load_config(&app_data_dir);
    let bd = c.user_birthdate_iso.as_deref();
    Ok(UserAndInferenceSettings {
        user_display_name: c.user_display_name.clone(),
        user_birthdate_iso: c.user_birthdate_iso.clone(),
        allow_nsfw_unrestricted_models: c.allow_nsfw_unrestricted_models.unwrap_or(false),
        is_at_least_18: user_profile::is_at_least_18(bd),
        is_birthday_today: user_profile::is_birthday_today(bd),
        time_format: match c.time_format.as_deref().map(str::trim) {
            Some("24") => "24".to_string(),
            _ => "12".to_string(),
        },
        calendar_region: c.calendar_region.clone(),
        calendar_quiet_hours_start: c.calendar_quiet_hours_start.unwrap_or(22).min(23),
        calendar_quiet_hours_end: c.calendar_quiet_hours_end.unwrap_or(7).min(23),
        inference_backend: c.inference_backend.clone(),
        explicit_ollama_model: c.explicit_ollama_model.clone(),
        openai_compatible_base_url: c.openai_compatible_base_url.clone(),
        openai_compatible_model: c.openai_compatible_model.clone(),
        openai_compatible_api_key_set: c
            .openai_compatible_api_key
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false),
        active_embedding_model_folder: effective_embedding_folder(&c),
        active_reranker_model_folder: effective_reranker_folder(&c),
        ai_inference_sleep: c.ai_inference_sleep.unwrap_or(false),
    })
}

#[tauri::command]
fn set_user_display_name(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.user_display_name = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_user_birthdate_iso(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.user_birthdate_iso = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        if chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").is_ok() {
            Some(t.to_string())
        } else {
            None
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_time_format(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.time_format = match value.as_deref().map(|s| s.trim()) {
        Some("24") => Some("24".to_string()),
        Some("12") => Some("12".to_string()),
        _ => None,
    };
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_calendar_region(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.calendar_region = value.and_then(|s| {
        let t = s.trim().to_uppercase();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_calendar_quiet_hours(
    app: tauri::AppHandle,
    start_hour: u8,
    end_hour: u8,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.calendar_quiet_hours_start = Some(start_hour.min(23));
    config.calendar_quiet_hours_end = Some(end_hour.min(23));
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_inference_backend(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    let normalized = match value.as_deref().map(|s| s.trim().to_lowercase()) {
        Some(ref s) if s == "openai_compatible" || s == "openai" => {
            Some("openai_compatible".to_string())
        }
        _ => None,
    };
    config.inference_backend = normalized;
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_explicit_ollama_model(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.explicit_ollama_model = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
async fn set_active_embedding_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    folder_name: String,
) -> Result<(), String> {
    let safe = validate_model_subfolder_name(&folder_name)?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let dir = state.model_paths.embeddings_root.join(&safe);
    if !dir.is_dir() {
        return Err(format!(
            "Embedding folder not found under models/embeddings: {safe}"
        ));
    }
    let mut config = load_config(&app_data_dir);
    config.active_embedding_model_folder = Some(safe);
    save_config(&app_data_dir, &config)?;
    let bundle_emb = app
        .path()
        .resolve("embedding", tauri::path::BaseDirectory::Resource)
        .ok()
        .map(|p| p.to_path_buf());
    let dir_blocking = dir;
    tokio::task::spawn_blocking(move || {
        let emb = Arc::new(neural_embed::NeuralEmbedder::load(
            Some(dir_blocking.as_path()),
            bundle_emb.as_deref(),
        ));
        neural_embed::replace_global(emb);
    })
    .await
    .map_err(|e| format!("embedding model swap task: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn set_active_reranker_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    folder_name: String,
) -> Result<(), String> {
    let safe = validate_model_subfolder_name(&folder_name)?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let dir = state.model_paths.rerankers_root.join(&safe);
    if !dir.is_dir() {
        return Err(format!(
            "Reranker folder not found under models/rerankers: {safe}"
        ));
    }
    let mut config = load_config(&app_data_dir);
    config.active_reranker_model_folder = Some(safe);
    save_config(&app_data_dir, &config)?;
    let bundle_rr = app
        .path()
        .resolve("reranker", tauri::path::BaseDirectory::Resource)
        .ok()
        .map(|p| p.to_path_buf());
    let dir_blocking = dir;
    tokio::task::spawn_blocking(move || {
        let rr = Arc::new(rerank::Reranker::load(
            Some(dir_blocking.as_path()),
            bundle_rr.as_deref(),
        ));
        rerank::replace_global(rr);
    })
    .await
    .map_err(|e| format!("reranker model swap task: {e}"))?;
    Ok(())
}

#[tauri::command]
fn set_openai_compatible_base_url(
    app: tauri::AppHandle,
    value: Option<String>,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.openai_compatible_base_url = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_openai_compatible_api_key(
    app: tauri::AppHandle,
    value: Option<String>,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.openai_compatible_api_key = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_openai_compatible_model(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.openai_compatible_model = value.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn set_allow_nsfw_unrestricted_models(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    if !user_profile::is_at_least_18(config.user_birthdate_iso.as_deref()) {
        config.allow_nsfw_unrestricted_models = Some(false);
    } else {
        config.allow_nsfw_unrestricted_models = Some(value);
    }
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn probe_openai_compatible_connection(base_url: String) -> Result<bool, String> {
    Ok(crate::openai_chat::probe_openai_compatible(&base_url))
}

#[tauri::command]
fn is_inference_ready(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let models = load_models_for_inference(&config);
    Ok(ensure_inference_ready(&config, &models).is_ok())
}

#[tauri::command]
fn moondream_describe_image(_app: tauri::AppHandle, image_path: String) -> Result<String, String> {
    moondream::describe_image_stub(std::path::Path::new(&image_path))
}

#[tauri::command]
fn kiwix_search_with_tool(
    app: tauri::AppHandle,
    zim_path: String,
    query: String,
) -> Result<Vec<String>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    let bin = cfg
        .kiwix_search_binary_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "Set kiwix_search_binary_path in athena_config.json to your kiwix-search binary (GPL-3)."
                .to_string()
        })?;
    kiwix_tools::kiwix_search_lines(bin, &zim_path, &query)
}

#[tauri::command]
fn get_offline_route_geojson(
    lat1: f64,
    lon1: f64,
    lat2: f64,
    lon2: f64,
) -> Result<offline_routing::RouteGeoJson, String> {
    if !(lat1.is_finite() && lon1.is_finite() && lat2.is_finite() && lon2.is_finite()) {
        return Err("Coordinates must be finite.".to_string());
    }
    Ok(offline_routing::straight_line_route_geojson(
        lat1, lon1, lat2, lon2,
    ))
}

const DEFAULT_MEMORY_MESSAGES: u32 = 20;
const MIN_MEMORY_MESSAGES: u32 = 5;

#[tauri::command]
fn get_memory_messages(app: tauri::AppHandle) -> Result<u32, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let max = max_memory_messages_for_ram(get_specs().ram_gb);
    let v = load_config(&app_data_dir)
        .memory_messages
        .unwrap_or(DEFAULT_MEMORY_MESSAGES);
    Ok(v.clamp(MIN_MEMORY_MESSAGES, max))
}

#[tauri::command]
fn set_memory_messages(app: tauri::AppHandle, value: u32) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    let max = max_memory_messages_for_ram(get_specs().ram_gb);
    let clamped = value.clamp(MIN_MEMORY_MESSAGES, max);
    config.memory_messages = Some(clamped);
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn get_feature_map_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir)
        .feature_map_enabled
        .unwrap_or(false))
}

#[tauri::command]
fn set_feature_map_enabled(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.feature_map_enabled = Some(value);
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn get_show_sources_in_chat(app: tauri::AppHandle) -> Result<bool, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir)
        .show_sources_in_chat
        .unwrap_or(true))
}

#[tauri::command]
fn set_show_sources_in_chat(app: tauri::AppHandle, value: bool) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.show_sources_in_chat = Some(value);
    save_config(&app_data_dir, &config)?;
    Ok(())
}

#[tauri::command]
fn get_user_location(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir).user_location_address.clone())
}

#[tauri::command]
fn set_user_location(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.user_location_address = value.filter(|s| !s.trim().is_empty());
    save_config(&app_data_dir, &config)?;
    Ok(())
}

fn normalize_mbtiles_file_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return None;
    }
    if !trimmed.to_ascii_lowercase().ends_with(".mbtiles") {
        return None;
    }
    Some(trimmed.to_string())
}

#[tauri::command]
fn get_enabled_main_map_raster_archive_file_names(
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    let mut values: Vec<String> = cfg
        .enabled_main_map_raster_archive_file_names
        .iter()
        .filter_map(|name| normalize_mbtiles_file_name(name))
        .collect();
    values.sort();
    values.dedup();
    Ok(values)
}

#[tauri::command]
fn set_enabled_main_map_raster_archive_file_names(
    app: tauri::AppHandle,
    archive_file_names: Vec<String>,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut normalized: Vec<String> = archive_file_names
        .iter()
        .filter_map(|name| normalize_mbtiles_file_name(name))
        .collect();
    normalized.sort();
    normalized.dedup();
    let mut cfg = load_config(&app_data_dir);
    cfg.enabled_main_map_raster_archive_file_names = normalized;
    save_config(&app_data_dir, &cfg)?;
    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TacticalDataFile {
    exported_at: i64,
    points_of_interest: Vec<TacticalPointOfInterest>,
    mesh_node_locations: Vec<TacticalMeshNodeLocation>,
}

#[derive(Debug, serde::Serialize)]
struct TacticalImportResult {
    imported_points_of_interest: usize,
    imported_mesh_node_locations: usize,
    source_path: String,
}

fn tactical_export_default_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let base_dir = if let Ok(d) = app.path().download_dir() {
        d
    } else {
        crate::portable::resolved_app_data_dir(app)?
    };
    std::fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;
    Ok(base_dir.join("athena_tactical.athena-tactical"))
}

#[tauri::command]
fn export_tactical_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
) -> Result<String, String> {
    let u = auth::authenticated_user(&session)?;
    if u.rank_level < 4 {
        return Err("Insufficient rank for tactical export.".to_string());
    }
    let export_path = tactical_export_default_path(&app)?;
    let (points, nodes) = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        (
            g.list_tactical_points_of_interest()?,
            g.list_tactical_mesh_node_locations()?,
        )
    };
    let payload = TacticalDataFile {
        exported_at: chrono::Utc::now().timestamp(),
        points_of_interest: points,
        mesh_node_locations: nodes,
    };
    let encoded = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&export_path, encoded).map_err(|e| e.to_string())?;
    Ok(export_path.to_string_lossy().to_string())
}

#[tauri::command]
fn import_tactical_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    source_path: Option<String>,
) -> Result<TacticalImportResult, String> {
    let u = auth::authenticated_user(&session)?;
    if u.rank_level < 4 {
        return Err("Insufficient rank for tactical import.".to_string());
    }
    let path = if let Some(raw) = source_path {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            tactical_export_default_path(&app)?
        } else {
            std::path::PathBuf::from(trimmed)
        }
    } else {
        tactical_export_default_path(&app)?
    };
    if !path.is_file() {
        return Err("Tactical data file not found.".to_string());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let payload: TacticalDataFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .import_tactical_data(&payload.points_of_interest, &payload.mesh_node_locations)?;
    Ok(TacticalImportResult {
        imported_points_of_interest: payload.points_of_interest.len(),
        imported_mesh_node_locations: payload.mesh_node_locations.len(),
        source_path: path.to_string_lossy().to_string(),
    })
}

#[derive(Debug, serde::Serialize)]
struct TacticalMapLayers {
    points_of_interest: Vec<TacticalPointOfInterest>,
    mesh_node_locations: Vec<TacticalMeshNodeLocation>,
}

#[tauri::command]
fn get_tactical_map_layers(state: tauri::State<'_, AppState>) -> Result<TacticalMapLayers, String> {
    let g = state.db.lock().map_err(|e| e.to_string())?;
    Ok(TacticalMapLayers {
        points_of_interest: g.list_tactical_points_of_interest()?,
        mesh_node_locations: g.list_tactical_mesh_node_locations()?,
    })
}

#[tauri::command]
fn add_tactical_poi(
    state: tauri::State<'_, AppState>,
    name: String,
    lat: f64,
    lon: f64,
    lucide_icon_name: Option<String>,
    description: Option<String>,
    pin_layer: Option<String>,
) -> Result<TacticalPointOfInterest, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Pin name is required.".to_string());
    }
    let raw_icon = lucide_icon_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("MapPin");
    if !raw_icon.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(
            "Icon name must be a Lucide export name (letters and digits only).".to_string(),
        );
    }
    let layer = pin_layer
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("resource");
    if layer != "resource" && layer != "network" {
        return Err("pin_layer must be \"resource\" or \"network\".".to_string());
    }
    let desc = description
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();

    let id = format!(
        "tpoi-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let mut notes = String::new();
    if layer == "network" {
        notes.push_str("@network\n");
    }
    notes.push_str(&format!("@icon:{raw_icon}"));
    if !desc.is_empty() {
        notes.push('\n');
        notes.push_str(&desc);
    }
    let updated_at = chrono::Utc::now().timestamp();
    let point = TacticalPointOfInterest {
        id,
        name: name.to_string(),
        lat,
        lon,
        notes,
        updated_at,
    };
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .upsert_tactical_point_of_interest(&point)?;
    Ok(point)
}

#[tauri::command]
fn delete_tactical_poi(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .delete_tactical_point_of_interest(&id)
}

#[tauri::command]
fn broadcast_tactical_poi(
    id: String,
    state: tauri::State<'_, AppState>,
    radio_state: tauri::State<'_, RadioState>,
) -> Result<(), String> {
    let point_id = id.trim().to_string();
    if point_id.is_empty() {
        return Err("Tactical point id is required.".to_string());
    }
    let point = state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .get_tactical_point_of_interest_by_id(&point_id)?
        .ok_or_else(|| "Tactical point not found.".to_string())?;
    let encoded_json = serde_json::to_string(&point).map_err(|e| e.to_string())?;
    let encoded = format!("ATN:{encoded_json}");

    let mut guard = radio_state
        .tx_port
        .lock()
        .map_err(|e| format!("Radio TX state lock failed: {e}"))?;
    let Some(port) = guard.as_mut() else {
        return Err("No radio port connected.".to_string());
    };
    port.write_all(encoded.as_bytes())
        .map_err(|e| format!("Failed to write tactical POI payload: {e}"))?;
    port.write_all(b"\n")
        .map_err(|e| format!("Failed to write tactical POI newline: {e}"))?;
    port.flush()
        .map_err(|e| format!("Failed to flush tactical POI payload: {e}"))?;
    Ok(())
}

#[tauri::command]
fn get_offline_mbtiles_zoom_range(
    app: tauri::AppHandle,
    archive_file_name: String,
) -> Result<Option<(u8, u8)>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let maps_dir = map_manager_download::offline_maps_directory(&app_data_dir);
    let name = archive_file_name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("Archive file name must be a single file name.".to_string());
    }
    if !name.to_ascii_lowercase().ends_with(".mbtiles") {
        return Err("Archive file name must end with .mbtiles.".to_string());
    }
    let path = maps_dir.join(name);
    if !path.is_file() {
        return Ok(None);
    }
    mbtiles_io::read_mbtiles_zoom_range(&path)
}

#[tauri::command]
fn get_offline_mbtiles_bounds(
    app: tauri::AppHandle,
    archive_file_name: String,
) -> Result<Option<(f64, f64, f64, f64)>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let maps_dir = map_manager_download::offline_maps_directory(&app_data_dir);
    let name = archive_file_name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("Archive file name must be a single file name.".to_string());
    }
    if !name.to_ascii_lowercase().ends_with(".mbtiles") {
        return Err("Archive file name must end with .mbtiles.".to_string());
    }
    let path = maps_dir.join(name);
    if !path.is_file() {
        return Ok(None);
    }
    mbtiles_io::read_mbtiles_bounds(&path)
}

#[tauri::command]
fn get_satellite_api_key(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(load_config(&app_data_dir).satellite_api_key.clone())
}

#[tauri::command]
fn set_satellite_api_key(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.satellite_api_key = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let memory_messages = config.memory_messages.unwrap_or(DEFAULT_MEMORY_MESSAGES);

    let models = get_available_models().map_err(|e| e.to_string())?;
    let tier: String = config
        .model_override
        .clone()
        .unwrap_or_else(|| get_specs().recommended_tier);
    let model_name =
        select_model_for_tier(&models, &tier).unwrap_or_else(|| "(no model)".to_string());
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
    let total_estimated_ram_gb =
        estimated_model_ram_gb + (estimated_context_ram_mb as f64 / 1024.0);

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

fn resolve_local_chat_target_user_id(
    actor: &auth::AuthenticatedPersonnel,
    target_user_id: Option<&str>,
    db: &Database,
) -> Result<String, String> {
    if actor.rank_level < 4 {
        return Ok(actor.id.clone());
    }
    let tid = target_user_id.map(str::trim).filter(|s| !s.is_empty());
    match tid {
        None => Ok(actor.id.clone()),
        Some(t) if t == actor.id.as_str() => Ok(actor.id.clone()),
        Some(t) => {
            if db.personnel_fetch_by_id(t)?.is_none() {
                return Err("Unknown personnel id.".to_string());
            }
            Ok(t.to_string())
        }
    }
}

#[tauri::command]
fn list_chats(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    target_user_id: Option<String>,
) -> Result<Vec<ChatSummary>, String> {
    let u = auth::authenticated_user(&session)?;
    let g = state.db.lock().map_err(|e| e.to_string())?;
    let tid = resolve_local_chat_target_user_id(&u, target_user_id.as_deref(), &*g)?;
    g.local_chat_list_summaries(&tid)
}

#[tauri::command]
fn get_chat(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    id: String,
    target_user_id: Option<String>,
) -> Result<Option<SavedChat>, String> {
    let u = auth::authenticated_user(&session)?;
    let g = state.db.lock().map_err(|e| e.to_string())?;
    let tid = resolve_local_chat_target_user_id(&u, target_user_id.as_deref(), &*g)?;
    g.local_chat_get(&id, &tid)
}

#[tauri::command]
fn save_chat(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    id: Option<String>,
    title: String,
    messages: Vec<SavedMessage>,
    description: Option<String>,
) -> Result<String, String> {
    let u = auth::authenticated_user(&session)?;
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.local_chat_save(&u.id, id, title, messages, description)
}

#[tauri::command]
fn update_chat_metadata(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    id: String,
    title: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    let u = auth::authenticated_user(&session)?;
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.local_chat_update_metadata(&u.id, &id, title, description)
}

#[tauri::command]
fn update_chat_context_summary(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    id: String,
    last_user_msg: String,
    last_assistant_msg: String,
) -> Result<(), String> {
    let u = auth::authenticated_user(&session)?;
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.local_chat_update_context_summary(&u.id, &id, &last_user_msg, &last_assistant_msg)
}

/// Synthesize speech for character voices (fallout, ultron). Returns WAV as base64 or error.
#[tauri::command]
fn synthesize_speech(
    app: tauri::AppHandle,
    text: String,
    voice_id: String,
) -> Result<String, String> {
    #[cfg(feature = "piper-tts")]
    {
        use base64::Engine;
        let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        return Ok("New chat".to_string());
    }
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;
    generate_chat_title_unified(&config, &models, &first_message)
}

#[tauri::command]
fn generate_chat_description(
    app: tauri::AppHandle,
    messages: Vec<SavedMessage>,
) -> Result<String, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        return Ok(String::new());
    }
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;
    let chat_messages: Vec<ChatMessage> = messages
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    generate_chat_description_unified(&config, &models, &chat_messages)
}

#[tauri::command]
fn delete_chat(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    id: String,
) -> Result<(), String> {
    let u = auth::authenticated_user(&session)?;
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.local_chat_delete(&u.id, &id)
}

const ERROR_LOG_FILENAME: &str = "athena_error_log.txt";
const ERROR_LOG_MAX_BYTES: usize = 100_000;

fn error_log_path(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join(ERROR_LOG_FILENAME)
}

#[tauri::command]
fn append_error_log(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    if path
        .metadata()
        .map(|m| m.len() as usize > ERROR_LOG_MAX_BYTES)
        .unwrap_or(false)
    {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let target_keep = content.len().saturating_sub(ERROR_LOG_MAX_BYTES / 2);
            let start = content
                .char_indices()
                .find(|(i, _)| *i >= target_keep)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let start = content[start..]
                .find('\n')
                .map(|nl| start + nl + 1)
                .unwrap_or(start);
            let _ = std::fs::write(&path, &content[start..]);
        }
    }
    Ok(())
}

#[tauri::command]
fn get_error_log(app: tauri::AppHandle) -> Result<String, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    pub swap_used_mb: u64,
    pub swap_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    /// Cumulative bytes read from block devices (Linux: /sys/block/*/stat). Used for throughput deltas.
    pub disk_read_bytes_total: u64,
    pub disk_write_bytes_total: u64,
    pub logical_cpus: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_cores: Option<u32>,
    pub uptime_secs: u64,
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_temp_c: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_usage_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_charging: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_time_to_empty_minutes: Option<u32>,
}

fn pick_main_disk_gb(disks: &sysinfo::Disks) -> (f64, f64) {
    use std::path::Path;
    #[cfg(windows)]
    let root = Path::new("C:\\");
    #[cfg(not(windows))]
    let root = Path::new("/");

    for disk in disks.list() {
        if disk.mount_point() == root {
            let total = disk.total_space();
            if total == 0 {
                continue;
            }
            let used = total.saturating_sub(disk.available_space());
            return (
                used as f64 / 1_073_741_824.0,
                total as f64 / 1_073_741_824.0,
            );
        }
    }
    let mut best: Option<(f64, f64)> = None;
    for disk in disks.list() {
        let total = disk.total_space();
        if total == 0 {
            continue;
        }
        let used_gb = (total.saturating_sub(disk.available_space())) as f64 / 1_073_741_824.0;
        let total_gb = total as f64 / 1_073_741_824.0;
        match best {
            None => best = Some((used_gb, total_gb)),
            Some((_, best_total)) if total_gb > best_total => best = Some((used_gb, total_gb)),
            _ => {}
        }
    }
    best.unwrap_or((0.0, 0.0))
}

fn network_io_totals() -> (u64, u64) {
    use sysinfo::Networks;
    let mut networks = Networks::new_with_refreshed_list();
    networks.refresh();
    let mut rx = 0u64;
    let mut tx = 0u64;
    for (name, data) in networks.iter() {
        if name == "lo" || name.starts_with("lo") {
            continue;
        }
        rx += data.total_received();
        tx += data.total_transmitted();
    }
    (rx, tx)
}

#[cfg(target_os = "linux")]
fn linux_block_io_bytes() -> (u64, u64) {
    use std::fs;
    use std::path::Path;
    let mut r = 0u64;
    let mut w = 0u64;
    let dir = Path::new("/sys/block");
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, 0);
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("loop") || name.starts_with("ram") {
            continue;
        }
        let path = format!("/sys/block/{}/stat", name);
        if let Ok(line) = fs::read_to_string(&path) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 7 {
                r += parts[2].parse::<u64>().unwrap_or(0);
                w += parts[6].parse::<u64>().unwrap_or(0);
            }
        }
    }
    (r.saturating_mul(512), w.saturating_mul(512))
}

fn disk_io_bytes_totals() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        linux_block_io_bytes()
    }
    #[cfg(not(target_os = "linux"))]
    {
        (0, 0)
    }
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
                    let num: String = rest
                        .chars()
                        .filter(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
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
            .args([
                "--query-gpu=utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
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
                let num: String = line
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
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
    use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    sys.refresh_memory();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    let raw_cpu = sys.global_cpu_info().cpu_usage();
    let num_cpus = sys.cpus().len().max(1) as f32;
    let cpu_usage_pct = (raw_cpu / num_cpus).min(100.0);
    let ram_used_mb = sys.used_memory() / (1024 * 1024);
    let ram_total_mb = sys.total_memory() / (1024 * 1024);
    let swap_used_mb = sys.used_swap() / (1024 * 1024);
    let swap_total_mb = sys.total_swap() / (1024 * 1024);

    let disks = Disks::new_with_refreshed_list();
    let (disk_used_gb, disk_total_gb) = pick_main_disk_gb(&disks);
    let (net_rx_bytes, net_tx_bytes) = network_io_totals();
    let (disk_read_bytes_total, disk_write_bytes_total) = disk_io_bytes_totals();

    let battery = system::get_battery_state().unwrap_or(system::BatteryState {
        percentage: None,
        is_charging: None,
        time_to_empty_minutes: None,
    });

    let logical_cpus = sys.cpus().len() as u32;
    let physical_cores = sys.physical_core_count().map(|n| n as u32);
    let cpu_brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty());
    let load = sysinfo::System::load_average();
    let uptime_secs = sysinfo::System::uptime();

    Ok(DeviceStats {
        cpu_usage_pct,
        ram_used_mb,
        ram_total_mb,
        swap_used_mb,
        swap_total_mb,
        disk_used_gb,
        disk_total_gb,
        net_rx_bytes,
        net_tx_bytes,
        disk_read_bytes_total,
        disk_write_bytes_total,
        logical_cpus,
        physical_cores,
        uptime_secs,
        load_one: load.one,
        load_five: load.five,
        load_fifteen: load.fifteen,
        host_name: sysinfo::System::host_name(),
        os_name: sysinfo::System::name(),
        kernel_version: sysinfo::System::kernel_version(),
        long_os_version: sysinfo::System::long_os_version(),
        cpu_arch: sysinfo::System::cpu_arch(),
        cpu_brand,
        cpu_temp_c: get_cpu_temp_c(),
        gpu_usage_pct: get_gpu_usage_pct(),
        battery_pct: battery.percentage,
        battery_charging: battery.is_charging,
        battery_time_to_empty_minutes: battery.time_to_empty_minutes,
    })
}

/// When we have library excerpts, enforce strict grounding: no invented brands or names.
const LIBRARY_RULE_WITH_CONTEXT: &str = "The excerpts below are the user's own Library documents. Use them to answer.\n\nWhen the answer (e.g. address, house location, property details, names, brands) IS in the excerpts, say so clearly: e.g. \"According to your library, your house is at [address]\" or \"Your documents say…\" Do NOT say \"I don't have information about your house\" or \"the library doesn't contain\" when that information appears in the excerpts below.\n\nDo NOT say you cannot access their surroundings, see their house, or that you cannot see something—use the documents below. Only if the answer is truly not in the excerpts, say you don't have that information and suggest adding a document to the Library.\n\nCRITICAL (survival/repair safety): For brands, model names, part numbers, and any specific fact about the user's property or equipment, state ONLY what is explicitly written in the excerpts. Never substitute a different brand or product (e.g. do not say Nest, Ring, or Honeywell unless that exact name appears in the excerpts). If the document says Aqara, say Aqara—not another brand. Wrong specifics in repair or survival can be dangerous; when in doubt, say you don't see that in the documents.";

/// Shipped reference: user prefix `/athena` injects this (see `ATHENA_PROJECT_REFERENCE.md`).
const ATHENA_PROJECT_REFERENCE: &str = include_str!("../../ATHENA_PROJECT_REFERENCE.md");

const LIBRARY_RULE_ATHENA_REF_MODE: &str = "The user is asking about the ATHENA desktop application itself (Autonomous Tactical Heuristic Emergency Network Archive; users often call it \"Athena\"): features, settings, UI, Meshtastic, Library, shortcuts, limitations. Answer using the \"ATHENA application reference\" appendix in this system prompt. Do not invent features not described there. If something is not in the appendix, say you are not sure and suggest the in-app Help (?) menu.";

/// Strips leading `/athena` (case-insensitive). Returns `(message_for_model, is_athena_ref_query)`.
fn parse_athena_ref_message(message: &str) -> (String, bool) {
    let t = message.trim_start();
    if t.len() >= 7 && t[..7].eq_ignore_ascii_case("/athena") {
        let after = t.get(7..).unwrap_or("").trim_start();
        let user_part = if after.is_empty() {
            "Briefly describe what the ATHENA application (Autonomous Tactical Heuristic Emergency Network Archive) is and summarize its main capabilities, using the ATHENA application reference in your system instructions."
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
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            let snippet = if keywords.is_empty() {
                if t.len() > FULL_DOC_THRESHOLD {
                    format!("{}...", &t[..FULL_DOC_THRESHOLD])
                } else {
                    t.to_string()
                }
            } else if t.len() <= FULL_DOC_THRESHOLD
                && keywords.iter().any(|k| t.to_lowercase().contains(k))
            {
                // Small doc that matches: include full text so brands/specs are never cut off.
                t.to_string()
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
                    (0, t.len().min(FULL_DOC_THRESHOLD))
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
fn build_recent_docs_context(
    docs: &[IndexedFile],
    max_chars_per_doc: usize,
    max_total: usize,
) -> String {
    let mut out = String::new();
    let mut total = 0;
    for d in docs {
        let Some(ref t) = d.text_content else {
            continue;
        };
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
        let body = c.chunk_text.trim();
        if body.is_empty() {
            continue;
        }
        let name = std::path::Path::new(&c.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&c.file_path);
        let block = format!("\n--- From: {} ---\n{}\n", name, body);
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
        let Some(entry) = by_path.get(path) else {
            continue;
        };
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
        let remaining = RRF_COMBINED_MAX_CHARS.saturating_sub(total);
        let budget = per_path_budget.min(remaining);
        if budget == 0 {
            break;
        }
        let ctx = if let Some(ref doc) = entry.doc {
            build_context_from_docs(std::slice::from_ref(doc), query, budget)
        } else if !entry.chunks.is_empty() {
            build_semantic_context(&entry.chunks, budget)
        } else {
            String::new()
        };
        if !ctx.trim().is_empty() {
            if seen_names.insert(name.clone()) {
                source_names.push(name);
            }
            out.push_str(&ctx);
            total += ctx.len();
        }
    }
    (out, source_names)
}

fn format_hybrid_library_context(hits: &[HybridSearchHit]) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let title = h.display_title.as_deref().unwrap_or("Untitled");
        let page = h
            .page_number
            .map(|p| format!(" (page {p})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "--- Source {}: {}{} ---\n{}\n\n",
            i + 1,
            title,
            page,
            h.expanded_text
        ));
    }
    out
}

/// Single-turn RAG text for chat console: same RRF keyword + semantic merge as main chat (no history).
pub(crate) fn console_chat_library_context(db: &Database, query: &str) -> String {
    if let Ok(hits) = db.hybrid_search(query) {
        if !hits.is_empty() {
            return format!(
                "Relevant excerpts from the user's library (addresses and property details are from the user's files):\n{}",
                format_hybrid_library_context(&hits)
            );
        }
    }
    let mut docs = db
        .search_text_content(query, RRF_KEYWORD_LIMIT)
        .unwrap_or_default();
    sort_docs_learned_first(&mut docs);
    let chunks = db
        .search_similar_chunks(query, RRF_SEMANTIC_LIMIT)
        .unwrap_or_default();
    let (rrf, _) = build_context_rrf(&docs, &chunks, query, false);
    if !rrf.is_empty() {
        return format!(
            "Relevant excerpts from the user's library (addresses and property details are from the user's files):\n{}",
            rrf
        );
    }
        let recent = db.get_recent_text_files(5).unwrap_or_default();
    let fallback = build_recent_docs_context(&recent, 600, 4000);
    if !fallback.is_empty() {
        return format!(
            "Recent library documents (use if the question might be answered here):\n{}",
            fallback
        );
    }
    "No relevant chunks found in the local library.".to_string()
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

fn build_user_profile_section(config: &AppConfig) -> String {
    user_profile::line_display_name_for_prompt(config.user_display_name.as_deref())
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
            .map(|p| {
                format!(
                    "{}: {} at ({:.5}, {:.5}){}",
                    p.name,
                    p.id,
                    p.lat,
                    p.lon,
                    if p.notes.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", p.notes)
                    }
                )
            })
            .collect();
        parts.push(format!(
            "The user has saved points of interest (POIs) on their map. Use these when they refer to places by name (e.g. \"Homebase\"). You can suggest adding new POIs; the user can add them by clicking the map in the Map tab or by naming the place.\n{}",
            list.join("\n")
        ));
    }
    if !downloaded.is_empty() {
        let list: Vec<String> = downloaded
            .iter()
            .map(|m| format!("{} ({}, {})", m.name, m.region, m.style))
            .collect();
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
    // User turns only: assistant replies often repeat indexed doc text (e.g. long quotes) and would
    // dominate BM25 / embeddings / rerank, locking retrieval on one file across follow-up questions.
    let combined: String = history[start..]
        .iter()
        .filter(|m| m.role.eq_ignore_ascii_case("user"))
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    build_retrieval_from_combined_text(&combined, message)
}

/// Same as above but from saved chat messages (source of truth when we have a loaded chat).
fn build_retrieval_from_saved_messages(
    messages: &[SavedMessage],
    message: &str,
) -> (String, String) {
    if messages.len() < 2 {
        return (message.to_string(), String::new());
    }
    let start = messages.len().saturating_sub(4);
    let combined: String = messages[start..]
        .iter()
        .filter(|m| m.role.eq_ignore_ascii_case("user"))
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
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_content,
    }];
    let start = history.len().saturating_sub(max);
    for h in &history[start..] {
        if !h.content.is_empty() || h.role == "user" {
            messages.push(ChatMessage {
                role: h.role.clone(),
                content: h.content.clone(),
            });
        }
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: new_user_message.to_string(),
    });
    messages
}

/// Inserts a secondary system message immediately before the final user turn (Ollama receives it after history, before the latest user prompt).
fn insert_spatial_system_before_last_user_message(
    mut messages: Vec<ChatMessage>,
    spatial_content: String,
) -> Vec<ChatMessage> {
    if let Some(last) = messages.pop() {
        if last.role == "user" {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: spatial_content,
            });
            messages.push(last);
        } else {
            messages.push(last);
        }
    }
    messages
}

#[tauri::command]
fn send_message(
    app: tauri::AppHandle,
    message: String,
    history: Option<Vec<HistoryMessage>>,
    model_options: Option<ModelOptions>,
) -> Result<String, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        return Err(
            "Local AI is asleep. Meshtastic /ai and mesh auto-replies still run when enabled. Wake the AI from the lock screen or header."
                .to_string(),
        );
    }
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;

    let state = app.state::<AppState>();
    let history_ref = history.as_deref().unwrap_or(&[]);
    let max_memory = config.memory_messages.unwrap_or(20) as usize;
    let (message_for_model, athena_ref_mode) = parse_athena_ref_message(&message);

    let messages = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let (context_section, library_rule) = if athena_ref_mode {
            (String::new(), LIBRARY_RULE_ATHENA_REF_MODE)
        } else {
            let retrieval_query = message_for_model.as_str();
            let mentioned = db
                .get_indexed_files_by_name_mentioned(retrieval_query)
                .unwrap_or_default();
            let mentioned_section = if mentioned.is_empty() {
                String::new()
            } else {
                let ctx = build_context_from_docs(&mentioned, retrieval_query, 4000);
                format!(
                    "\n\nThe user referred to or asked about the following document(s); use them to answer:\n{}\n\n",
                    ctx
                )
            };

            let keyword_semantic_section = match db.hybrid_search(retrieval_query) {
                Ok(hits) if !hits.is_empty() => format!(
                    "\n\nRelevant excerpts from the user's library (use these to answer questions about the user's documents, property, or notes when the answer appears below):\n{}",
                    format_hybrid_library_context(&hits)
                ),
                _ => {
                    let mut docs = db
                        .search_text_content(retrieval_query, RRF_KEYWORD_LIMIT)
                        .unwrap_or_default();
                    sort_docs_learned_first(&mut docs);
                    let semantic_chunks = db
                        .search_similar_chunks(retrieval_query, RRF_SEMANTIC_LIMIT)
                        .unwrap_or_default();
                    let (rrf_context, _source_names) =
                        build_context_rrf(&docs, &semantic_chunks, retrieval_query, false);
                    if rrf_context.is_empty() {
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
                    }
                }
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
                "\n\n---\nATHENA application reference (authoritative for questions about the app):\n{}\n---\n",
                ATHENA_PROJECT_REFERENCE
            )
        } else {
            String::new()
        };
        let persistent_knowledge_section = build_persistent_knowledge_section(&app_data_dir);
        let location_section = build_location_section(&config);
        let poi_maps_section = build_poi_and_maps_section(&app_data_dir);
        let personality_section = build_personality_section(&config);
        let user_profile_section = build_user_profile_section(&config);
        let system_content = format!(
            "You are Athena, the assistant for {}. You are a local offline survival and repair assistant. You help with practical skills, first aid, repair, and survival knowledge. Answer concisely and clearly.\n\nCritical rule: Do NOT guess or make up facts. {}{}{}{}{}{}{}{}{}",
            ATHENA_FULL_NAME,
            library_rule,
            conversation_context_prompt,
            persistent_knowledge_section,
            context_section,
            athena_ref_section,
            location_section,
            poi_maps_section,
            personality_section,
            user_profile_section
        );

        build_chat_messages(system_content, history_ref, &message_for_model, max_memory)
    };

    chat_unified(&config, &models, messages, model_options).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_lan_cluster_peers() -> Vec<crate::lan_cluster::LanClusterPeerView> {
    crate::lan_cluster::list_cluster_peers_for_ui()
}

const STREAM_CHUNK_EVENT: &str = "athena:stream-chunk";
const STREAM_DONE_EVENT: &str = "athena:stream-done";
const STREAM_ERROR_EVENT: &str = "athena:stream-error";
pub(crate) const STREAM_SOURCES_EVENT: &str = "athena:stream-sources";

/// Emitted when `ai_inference_sleep` toggles (lock screen or header power control).
pub const AI_INFERENCE_SLEEP_EVENT: &str = "athena:ai-inference-sleep-changed";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamLibrarySource {
    pub file_path: String,
    pub display_title: String,
    pub page_number: Option<u32>,
}

pub(crate) fn hybrid_hit_to_stream_source(h: &db::HybridSearchHit) -> StreamLibrarySource {
    StreamLibrarySource {
        file_path: h.file_path.clone(),
        display_title: h.display_title.clone().unwrap_or_else(|| {
            std::path::Path::new(&h.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Library")
                .to_string()
        }),
        page_number: h.page_number.and_then(|p| u32::try_from(p).ok()),
    }
}

fn stream_sources_from_rrf(
    docs: &[IndexedFile],
    chunks: &[ChunkEmbedding],
    source_names: &[String],
) -> Vec<StreamLibrarySource> {
    use std::collections::HashMap;
    let mut map: HashMap<String, String> = HashMap::new();
    for d in docs {
        map.insert(d.name.clone(), d.path.clone());
    }
    for c in chunks {
        if let Some(fname) = std::path::Path::new(&c.file_path)
            .file_name()
            .and_then(|n| n.to_str())
        {
            map.entry(fname.to_string())
                .or_insert_with(|| c.file_path.clone());
        }
    }
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<(String, Option<u32>)> =
        std::collections::HashSet::new();
    for n in source_names {
        let path = map.get(n).cloned().or_else(|| {
            docs.iter()
                .find(|d| d.path == *n || d.path.ends_with(n))
                .map(|d| d.path.clone())
        }).or_else(|| {
            chunks
                .iter()
                .find(|c| c.file_path.ends_with(n))
                .map(|c| c.file_path.clone())
        });
        if let Some(fp) = path {
            let key = (fp.clone(), None);
            if seen.insert(key) {
                out.push(StreamLibrarySource {
                    file_path: fp,
                    display_title: n.clone(),
                    page_number: None,
                });
            }
        }
    }
    out
}

fn merge_mentioned_and_library_citations(
    mentioned: &[IndexedFile],
    library: Vec<StreamLibrarySource>,
    max: usize,
) -> Vec<StreamLibrarySource> {
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for d in mentioned {
        if !seen_paths.insert(d.path.clone()) {
            continue;
        }
        out.push(StreamLibrarySource {
            file_path: d.path.clone(),
            display_title: d.display_title.clone().unwrap_or_else(|| d.name.clone()),
            page_number: None,
        });
        if out.len() >= max {
            return out;
        }
    }
    for c in library {
        if !seen_paths.insert(c.file_path.clone()) {
            continue;
        }
        out.push(c);
        if out.len() >= max {
            break;
        }
    }
    out
}


const KIWIX_PROGRESS_EVENT: &str = "athena:kiwix-progress";
const KIWIX_DONE_EVENT: &str = "athena:kiwix-done";
const KIWIX_ERROR_EVENT: &str = "athena:kiwix-error";

#[tauri::command]
fn scan_kiwix_zim_files(app: tauri::AppHandle) -> Result<Vec<DiscoveredZim>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let state = app.state::<AppState>();
    let folders = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, &app_data_dir, &config)?
    };
    let roots: Vec<std::path::PathBuf> = folders.into_iter().map(std::path::PathBuf::from).collect();
    kiwix_scan_zim_files_multi(&roots)
}

#[derive(serde::Serialize)]
pub struct LibraryFile {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub exists_on_disk: bool,
}

#[tauri::command]
fn list_library_contents(app: tauri::AppHandle) -> Result<Vec<LibraryFile>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let state = app.state::<AppState>();
    let (folders, indexed_under) = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        let folders = compute_watcher_folders(&*g, &app_data_dir, &config)?;
        let roots_for_index = folders.clone();
        let idx = g.list_indexed_paths_under_roots(&roots_for_index)?;
        (folders, idx)
    };
    let mut on_disk: HashSet<String> = HashSet::new();
    let mut out: Vec<LibraryFile> = Vec::new();
    for folder in &folders {
        let root = std::path::Path::new(folder);
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with('.'))
            {
                continue;
            }
            let path_str = path.to_string_lossy().to_string();
            if on_disk.insert(path_str.clone()) {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                out.push(LibraryFile {
                    path: path_str,
                    name,
                    size_bytes,
                    exists_on_disk: true,
                });
            }
        }
    }
    for (path_str, name) in indexed_under {
        if std::path::Path::new(&path_str).exists() {
            continue;
        }
        if on_disk.contains(&path_str) {
            continue;
        }
        on_disk.insert(path_str.clone());
        out.push(LibraryFile {
            path: path_str,
            name,
            size_bytes: 0,
            exists_on_disk: false,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[tauri::command]
fn remove_library_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let g = state.db.lock().map_err(|e| e.to_string())?;
    let _ = g.delete_by_path_prefix(&path);
    let _ = g.delete_by_path(&path);
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Copies the live SQLite database to a path chosen in the UI (Tauri save dialog).
#[tauri::command]
fn backup_database(state: tauri::State<'_, AppState>, dest_path: String) -> Result<(), String> {
    let dest = std::path::PathBuf::from(dest_path.trim());
    if dest.as_os_str().is_empty() {
        return Err("Destination path is empty.".to_string());
    }
    let g = state.db.lock().map_err(|e| e.to_string())?;
    g.backup_to_path(&dest)?;
    Ok(())
}

/// Replaces `athena_index.db` with a user-selected file after closing the active connection.
#[tauri::command]
fn restore_database(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    source_path: String,
) -> Result<(), String> {
    let src = std::path::PathBuf::from(source_path.trim());
    if !src.is_file() {
        return Err("Please select a valid database file.".to_string());
    }
    rusqlite::Connection::open(&src)
        .map_err(|e| format!("Selected file is not a valid SQLite database: {e}"))?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let db_path = app_data_dir.join("athena_index.db");
    let mut guard = state.db.lock().map_err(|e| e.to_string())?;
    let _old = std::mem::replace(&mut *guard, Database::new_in_memory_placeholder()?);
    drop(_old);
    std::fs::copy(&src, &db_path).map_err(|e| e.to_string())?;
    *guard = Database::new(&db_path)?;
    Ok(())
}

// --- Maps & POIs ---

#[tauri::command]
fn get_map_catalog(app: tauri::AppHandle) -> Result<Vec<maps::MapCatalogEntry>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let location_enabled = config.feature_map_enabled.unwrap_or(false);
    let address = config.user_location_address.as_deref();
    Ok(maps::get_map_catalog(
        &app_data_dir,
        location_enabled,
        address,
    ))
}

#[tauri::command]
fn get_downloaded_maps(app: tauri::AppHandle) -> Result<Vec<maps::DownloadedMap>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(maps::get_downloaded_maps(&app_data_dir))
}

/// Returns the offline map manager catalog (currently embedded mock JSON; later a remote fetch).
#[tauri::command]
fn get_offline_map_manager_catalog(
) -> Result<Vec<map_manager_catalog::OfflineMapCatalogEntry>, String> {
    map_manager_catalog::catalog_from_embedded_mock_json()
}

#[tauri::command]
fn list_downloaded_offline_map_ids(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    let parsed =
        reqwest::Url::parse(&download_url).map_err(|e| format!("Invalid download URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Download URL must use http or https.".to_string());
    }

    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    map_manager_download::ensure_offline_maps_directory(&app_data_dir)?;

    let (final_path, part_path) =
        map_manager_download::destination_paths(&app_data_dir, &map_id, &download_url)?;

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
#[allow(clippy::type_complexity)]
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
fn get_smallest_kiwix_map_suggestion(
    app: tauri::AppHandle,
) -> Result<Option<maps::MapCatalogEntry>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(tile_pack::list_tile_packs(&app_data_dir))
}

#[tauri::command]
fn offline_basemap_paths_cmd(
    app: tauri::AppHandle,
) -> Result<tile_pack::OfflineBasemapPaths, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
fn start_mbtiles_import(
    app: tauri::AppHandle,
    source_path: String,
    display_name: String,
) -> Result<String, String> {
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
fn start_pmtiles_import(
    app: tauri::AppHandle,
    source_path: String,
    display_name: String,
) -> Result<String, String> {
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    tile_pack::remove_tile_pack(&app_data_dir, &id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let dest_dir = maps::map_downloads_dir(&app_data_dir);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let filename = url.split('/').next_back().unwrap_or("download.zim");
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
                if let Ok(app_data) = crate::portable::resolved_app_data_dir(&app_emit) {
                    let _ = maps::add_downloaded_map(
                        &app_data,
                        path_str.clone(),
                        name_clone,
                        region_clone,
                        style_clone,
                    );
                }
                let _ = app_emit.emit(
                    KIWIX_DONE_EVENT,
                    serde_json::json!({ "id": url_clone, "path": path_str }),
                );
            }
            Err(e) => {
                let _ = app_emit.emit(
                    KIWIX_ERROR_EVENT,
                    serde_json::json!({ "id": url_clone, "error": e }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn remove_downloaded_map(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    maps::remove_downloaded_map(&app_data_dir, &path)?;
    if std::path::Path::new(&path).exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_pois(app: tauri::AppHandle) -> Result<Vec<maps::Poi>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    Ok(maps::get_pois(&app_data_dir))
}

#[tauri::command]
fn add_poi(
    app: tauri::AppHandle,
    name: String,
    lat: f64,
    lon: f64,
    notes: String,
) -> Result<maps::Poi, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    maps::add_poi(&app_data_dir, name, lat, lon, notes)
}

#[tauri::command]
fn remove_poi(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    maps::update_poi(&app_data_dir, &id, name, lat, lon, notes)
}

/// Short Ollama Q&A for Help: no RAG, strict “Athena app only” system prompt.
#[tauri::command]
fn help_assistant_ask(app: tauri::AppHandle, question: String) -> Result<String, String> {
    let q = question.trim();
    if q.is_empty() {
        return Err("Type a question first.".to_string());
    }
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        return Err(
            "Local AI is asleep. Meshtastic /ai and mesh auto-replies still run when enabled. Wake the AI from the lock screen or header."
                .to_string(),
        );
    }
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;
    let mut system = format!(
        "You are Athena's in-app help assistant for {}. Answer ONLY about this desktop application (users call it \"Athena\"): chat, library, maps and tactical pins, mesh/Meshtastic, serial radio, Comms, settings, themes, Ollama, offline data, knowledge catalog, dashboard, and similar features. \
If the user asks about anything else (general homework, unrelated topics), reply in one or two sentences that you only explain how to use ATHENA, and suggest what they could ask instead. \
Be concise (under 8 short paragraphs). Do not invent features.",
        ATHENA_FULL_NAME
    );
    system.push_str(&user_profile::line_display_name_for_prompt(
        config.user_display_name.as_deref(),
    ));
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system,
        },
        ChatMessage {
            role: "user".to_string(),
            content: q.to_string(),
        },
    ];
    let options = ModelOptions {
        temperature: 0.35,
        num_ctx: 4096,
    };
    chat_unified(&config, &models, messages, Some(options))
}

#[tauri::command]
fn send_message_stream(
    app: tauri::AppHandle,
    session: tauri::State<'_, auth::AuthSession>,
    message: String,
    history: Option<Vec<HistoryMessage>>,
    user_lat: Option<f64>,
    user_lon: Option<f64>,
    chat_id: Option<String>,
    model_options: Option<ModelOptions>,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        let _ = app.emit(
            STREAM_ERROR_EVENT,
            "Local AI is asleep. Meshtastic /ai and mesh auto-replies still run when enabled. Wake the AI from the lock screen or header.",
        );
        let _ = app.emit(STREAM_DONE_EVENT, ());
        return Ok(());
    }
    let models = load_models_for_inference(&config);
    ensure_inference_ready(&config, &models)?;

    let session_user_id = auth::authenticated_user(&session)?.id.clone();
    let state = app.state::<AppState>();
    let db_arc = Arc::clone(&state.db);
    let model_opts_for_stream = model_options;
    let app_emit = app.clone();
    let history_clone = history;
    let app_data_dir_clone = app_data_dir.clone();
    let chat_id_clone = chat_id.clone();
    let history_owner_id = session_user_id.clone();
    std::thread::spawn(move || {
        let app_data_dir = app_data_dir_clone;
        let config = load_config(&app_data_dir);
        let history_ref = history_clone.as_deref().unwrap_or(&[]);

        let (message_for_model, athena_ref_mode) = parse_athena_ref_message(&message);

        // Build retrieval query and conversation prompt. Prefer full saved messages over context_summary
        // so we get maximum keyword signal (exact address, names) for follow-ups.
        // `library_lookup_query` is for BM25 / vectors / rerank only: avoid assistant-turn text (and put
        // the current user message before stale summary) so one document does not dominate every turn.
        let (retrieval_query, conversation_context_prompt, library_lookup_query) =
            if let Some(ref cid) = chat_id_clone {
                let chat_opt = db_arc
                    .lock()
                    .ok()
                    .and_then(|g| g.local_chat_get(cid, &history_owner_id).ok())
                    .flatten();
                if let Some(chat) = chat_opt {
                    if chat.messages.len() >= 2 {
                        let (rq, cp) =
                            build_retrieval_from_saved_messages(&chat.messages, &message_for_model);
                        let lq = rq.clone();
                        (rq, cp, lq)
                    } else {
                        let summary = chat.context_summary.unwrap_or_default();
                        if !summary.trim().is_empty() {
                            let retrieval_query = format!("{} {}", summary, message_for_model);
                            let conversation_context_prompt = format!(
                                "\n\nConversation context (use to resolve \"it\"/\"that\" and what the user is referring to): {}",
                                summary
                            );
                            let library_lookup_query =
                                format!("{} {}", message_for_model, summary);
                            (retrieval_query, conversation_context_prompt, library_lookup_query)
                        } else {
                            let m = message_for_model.clone();
                            (m.clone(), String::new(), m)
                        }
                    }
                } else {
                    let (rq, cp) = build_retrieval_from_history(history_ref, &message_for_model);
                    let lq = rq.clone();
                    (rq, cp, lq)
                }
            } else {
                let (rq, cp) = build_retrieval_from_history(history_ref, &message_for_model);
                let lq = rq.clone();
                (rq, cp, lq)
            };

        let (messages, stream_citations) = {
            let db = match db_arc.lock() {
                Ok(g) => g,
                Err(e) => {
                    let _ = app_emit.emit(STREAM_ERROR_EVENT, format!("Database lock failed: {e}"));
                    return;
                }
            };

            let (context_section, library_rule, stream_citations) = if athena_ref_mode {
                (String::new(), LIBRARY_RULE_ATHENA_REF_MODE, Vec::<StreamLibrarySource>::new())
            } else {
                let mentioned = db
                    .get_indexed_files_by_name_mentioned(&format!(
                        "{} {}",
                        retrieval_query, message_for_model
                    ))
                    .unwrap_or_default();
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

                let (keyword_semantic_section, library_citations) =
                    match db.hybrid_search(&library_lookup_query) {
                        Ok(hits) if !hits.is_empty() => {
                            let mut cites: Vec<StreamLibrarySource> = hits
                                .iter()
                                .map(hybrid_hit_to_stream_source)
                                .collect();
                            let mut cite_paths: std::collections::HashSet<String> =
                                cites.iter().map(|c| c.file_path.clone()).collect();
                            if let Ok(extra) =
                                db.distinct_library_source_paths(&library_lookup_query, 12)
                            {
                                for (path, title) in extra {
                                    if cites.len() >= 12 {
                                        break;
                                    }
                                    if !cite_paths.insert(path.clone()) {
                                        continue;
                                    }
                                    cites.push(StreamLibrarySource {
                                        file_path: path.clone(),
                                        display_title: title.unwrap_or_else(|| {
                                            std::path::Path::new(&path)
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("Library")
                                                .to_string()
                                        }),
                                        page_number: None,
                                    });
                                }
                            }
                            if cite_paths.len() < 3 && cites.len() < 12 {
                                if let Ok(more) = db.distinct_child_chunk_file_paths_breadth(
                                    &cite_paths,
                                    (12 - cites.len()).min(8),
                                ) {
                                    for (path, title) in more {
                                        if cites.len() >= 12 {
                                            break;
                                        }
                                        if !cite_paths.insert(path.clone()) {
                                            continue;
                                        }
                                        cites.push(StreamLibrarySource {
                                            file_path: path.clone(),
                                            display_title: title.unwrap_or_else(|| {
                                                std::path::Path::new(&path)
                                                    .file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("Library")
                                                    .to_string()
                                            }),
                                            page_number: None,
                                        });
                                    }
                                }
                            }
                            let mut body = format_hybrid_library_context(&hits);
                            let distinct_hit_files: std::collections::HashSet<&str> =
                                hits.iter().map(|h| h.file_path.as_str()).collect();
                            if distinct_hit_files.len() == 1 {
                                let exclude: std::collections::HashSet<String> =
                                    hits.iter().map(|h| h.file_path.clone()).collect();
                                if let Ok(others) = db.distinct_child_chunk_file_paths_breadth(&exclude, 2)
                                {
                                    for (path, title) in others {
                                        match db.first_child_chunk_text_preview(&path, 650) {
                                            Ok(snip) if !snip.trim().is_empty() => {
                                                let label = title.unwrap_or_else(|| {
                                                    std::path::Path::new(&path)
                                                        .file_name()
                                                        .and_then(|n| n.to_str())
                                                        .unwrap_or("Document")
                                                        .to_string()
                                                });
                                                body.push_str(&format!(
                                                    "\n---\nOther indexed library document: {}.\nExcerpt:\n{}\n",
                                                    label, snip
                                                ));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            (
                                format!(
                                    "\n\nRelevant excerpts from the user's library (use these to answer questions about the user's documents, property, or notes when the answer appears below). Addresses and property details here are from the user's Library—cite them as such. The \"current location\" line elsewhere is from Settings for general area only.\n{}",
                                    body
                                ),
                                cites,
                            )
                        }
                        _ => {
                            let mut docs = db
                                .search_text_content(&library_lookup_query, keyword_limit)
                                .unwrap_or_default();
                            sort_docs_learned_first(&mut docs);
                            let semantic_chunks = db
                                .search_similar_chunks(&library_lookup_query, semantic_limit)
                                .unwrap_or_default();
                            let (rrf_context, source_names) = build_context_rrf(
                                &docs,
                                &semantic_chunks,
                                &library_lookup_query,
                                has_conversation_context,
                            );
                            let cites = stream_sources_from_rrf(&docs, &semantic_chunks, &source_names);
                            let section = if rrf_context.is_empty() {
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
                            (section, cites)
                        }
                    };

                let context_section = format!("{}{}", mentioned_section, keyword_semantic_section);

                const MAX_SOURCES_EMIT: usize = 12;
                let stream_citations = merge_mentioned_and_library_citations(
                    &mentioned,
                    library_citations,
                    MAX_SOURCES_EMIT,
                );

                let library_rule = if context_section.trim().is_empty() {
                    "When the user asks about something that might be in their Library (e.g. their house, notes), say you don't have that information and suggest they add or place a relevant document in their Library folder, then try again."
                } else {
                    LIBRARY_RULE_WITH_CONTEXT
                };

                (context_section, library_rule, stream_citations)
            };

            let athena_ref_section = if athena_ref_mode {
                format!(
                    "\n\n---\nATHENA application reference (authoritative for questions about the app):\n{}\n---\n",
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
            let user_profile_section = build_user_profile_section(&config);
            let system_content = format!(
                "You are Athena, the assistant for {}. You are a local offline survival and repair assistant. You help with practical skills, first aid, repair, and survival knowledge. Answer concisely and clearly.\n\nCritical rule: Do NOT guess or make up facts. {}{}{}{}{}{}{}{}{}{}",
                ATHENA_FULL_NAME,
                library_rule,
                conversation_context_prompt,
                persistent_knowledge_section,
                context_section,
                athena_ref_section,
                location_section,
                poi_maps_section,
                user_gps_section,
                personality_section,
                user_profile_section
            );

            let max_memory = config.memory_messages.unwrap_or(20) as usize;
            let mut messages =
                build_chat_messages(system_content, history_ref, &message_for_model, max_memory);

            if let (Some(lat), Some(lon)) = (user_lat, user_lon) {
                if lat.is_finite()
                    && lon.is_finite()
                    && crate::spatial::user_query_suggests_spatial_context(&message_for_model)
                {
                    if let Ok(spatial_block) =
                        crate::spatial::build_spatial_context(&db, lat, lon, &message_for_model)
                    {
                        messages =
                            insert_spatial_system_before_last_user_message(messages, spatial_block);
                    }
                }
            }

            (messages, stream_citations)
        };

        if !stream_citations.is_empty() {
            let _ = app_emit.emit(STREAM_SOURCES_EVENT, &stream_citations);
        }

        let models = load_models_for_inference(&config);
        let mut on_chunk = |chunk: &str| {
            let _ = app_emit.emit(STREAM_CHUNK_EVENT, chunk);
        };
        let on_done = || {
            let _ = app_emit.emit(STREAM_DONE_EVENT, ());
        };
        let cluster_model = inference::resolve_model_name(&config, &models);
        if let Ok(ref m) = cluster_model {
            if crate::lan_cluster::should_attempt_cluster(&config, &models, &messages, m) {
                let app_c = app_emit.clone();
                let model_s = m.clone();
                let messages_c = messages.clone();
                let opts_c = model_opts_for_stream;
                let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
                tauri::async_runtime::spawn(async move {
                    let r = crate::lan_cluster::run_cluster_stream(
                        app_c.clone(),
                        model_s,
                        messages_c,
                        opts_c,
                    )
                    .await;
                    if let Err(e) = r {
                        let _ = app_c.emit(STREAM_ERROR_EVENT, e);
                        let _ = app_c.emit(STREAM_DONE_EVENT, ());
                    }
                    let _ = tx.send(());
                });
                let _ = rx.recv();
                return;
            }
        }
        if let Err(e) = chat_stream_unified(
            &config,
            &models,
            messages,
            &mut on_chunk,
            on_done,
            model_opts_for_stream,
        ) {
            let _ = app_emit.emit(STREAM_ERROR_EVENT, e);
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = athena_mbtiles_tile_protocol::register_with_builder(
        athena_map_protocol::register_with_builder(tauri::Builder::default()),
    )
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_dialog::init());
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_plugin_macos_permissions::init());
    builder.setup(|app| {
            // Resilience Layer: always initialize a shared status object so the UI can show
            // maintenance/health even if the rest of setup fails.
            app.manage(diagnostics::ResilienceState::default());
            let initial = diagnostics::run_startup_diagnostics(&app.handle());
            diagnostics::set_status(&app.handle(), initial);

            // Heartbeat loop: verify critical components periodically and broadcast status.
            // This intentionally does not crash the app; it only updates the resilience status.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    let mut components = std::collections::BTreeMap::<String, String>::new();
                    for name in ["pmtiles", "tesseract", "pdftoppm", "ollama"] {
                        let args: &[&str] = match name {
                            "pmtiles" => &["--version"],
                            "tesseract" => &["--version"],
                            "pdftoppm" => &["-h"],
                            "ollama" => &["--version"],
                            _ => &["--version"],
                        };
                        let status = match handle.shell().sidecar(name) {
                            Ok(cmd) => match cmd.args(args).spawn() {
                                Ok((_rx, child)) => {
                                    let _ = child.kill();
                                    "ok".to_string()
                                }
                                Err(e) => format!("spawn failed: {e}"),
                            },
                            Err(e) => format!("unavailable: {e}"),
                        };
                        components.insert(format!("sidecar:{name}"), status);
                    }
                    diagnostics::update_heartbeat(&handle, components);
                });
            }

            let result: Result<(), String> = (|| {
            // Ensure the native window always has a non-empty title (avoids macOS fullscreen crash when title is null).
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title("ATHENA");
                #[cfg(target_os = "macos")]
                {
                    if let Err(e) = macos_window::apply_native_fullscreen_mitigation(&w) {
                        eprintln!(
                            "[athena] macOS: could not apply fullscreen mitigation (FullScreenNone): {e}"
                        );
                    }
                    // Wry/Tauri may reset `collectionBehavior` after the webview attaches; re-apply
                    // shortly after startup on the main thread.
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        for delay_ms in [500u64, 2000] {
                            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                            let handle = h.clone();
                            let _ = h.run_on_main_thread(move || {
                                if let Some(win) = handle.get_webview_window("main") {
                                    if let Err(e) =
                                        macos_window::apply_native_fullscreen_mitigation(&win)
                                    {
                                        eprintln!(
                                            "[athena] macOS: delayed fullscreen mitigation failed: {e}"
                                        );
                                    }
                                }
                            });
                        }
                    });
                    app.manage(macos_window::MacosSimpleFullscreenState(Arc::new(
                        std::sync::atomic::AtomicBool::new(false),
                    )));
                }
            }
            ocr::init_ocr_environment(app.handle());
            let app_data_dir = crate::portable::resolved_app_data_dir(&app.handle())?;
            std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
            let model_paths = ensure_model_directories(&app_data_dir)?;
            let _ = map_manager_download::ensure_offline_maps_directory(&app_data_dir);
            let _ = map_manager_download::cleanup_stale_part_files(&app_data_dir);
            let db_path = app_data_dir.join("athena_index.db");
            let db = Arc::new(Mutex::new(Database::new(&db_path)?));
            {
                let g = db.lock().map_err(|e| e.to_string())?;
                g.migrate_archive_watch_roots_from_config(&app_data_dir)?;
                g.migrate_local_chats_from_legacy_json(&app_data_dir)?;
            }
            let startup_cfg = load_config(&app_data_dir);
            let embedding_dir_active = model_paths
                .embeddings_root
                .join(effective_embedding_folder(&startup_cfg));
            let reranker_dir_active = model_paths
                .rerankers_root
                .join(effective_reranker_folder(&startup_cfg));
            let embedding_resource_dir = app
                .path()
                .resolve("embedding", tauri::path::BaseDirectory::Resource)
                .ok()
                .map(|p| p.to_path_buf());
            let neural_embedder = Arc::new(neural_embed::NeuralEmbedder::load(
                Some(embedding_dir_active.as_path()),
                embedding_resource_dir.as_deref(),
            ));
            neural_embed::init_global(neural_embedder)?;
            let reranker_resource_dir = app
                .path()
                .resolve("reranker", tauri::path::BaseDirectory::Resource)
                .ok()
                .map(|p| p.to_path_buf());
            let reranker = Arc::new(rerank::Reranker::load(
                Some(reranker_dir_active.as_path()),
                reranker_resource_dir.as_deref(),
            ));
            rerank::init_global(reranker)?;
            {
                let paths_for_bootstrap = model_paths.clone();
                let bundle_emb = embedding_resource_dir.clone();
                let bundle_rr = reranker_resource_dir.clone();
                let app_data_for_bootstrap = app_data_dir.clone();
                tauri::async_runtime::spawn(async move {
                    match downloader::bootstrap_models(&paths_for_bootstrap).await {
                        Ok(()) => {
                            let cfg = load_config(&app_data_for_bootstrap);
                            let emb_path = paths_for_bootstrap
                                .embeddings_root
                                .join(effective_embedding_folder(&cfg));
                            let rr_path = paths_for_bootstrap
                                .rerankers_root
                                .join(effective_reranker_folder(&cfg));
                            let bundle_emb = bundle_emb.clone();
                            let bundle_rr = bundle_rr.clone();
                            match tokio::task::spawn_blocking(move || {
                                let new_emb = Arc::new(neural_embed::NeuralEmbedder::load(
                                    Some(emb_path.as_path()),
                                    bundle_emb.as_deref(),
                                ));
                                neural_embed::replace_global(Arc::clone(&new_emb));
                                let new_rr = Arc::new(rerank::Reranker::load(
                                    Some(rr_path.as_path()),
                                    bundle_rr.as_deref(),
                                ));
                                rerank::replace_global(Arc::clone(&new_rr));
                                eprintln!(
                                    "[athena] post-bootstrap: embedder ONNX = {}, reranker ONNX = {}",
                                    new_emb.is_onnx_loaded(),
                                    new_rr.is_onnx_loaded()
                                );
                            })
                            .await
                            {
                                Ok(()) => {}
                                Err(e) => eprintln!("[athena] post-bootstrap reload: {e}"),
                            }
                        }
                        Err(e) => eprintln!("[athena] model bootstrap failed: {e}"),
                    }
                });
            }
            let mesh_control = mesh_listener::spawn_mesh_radio_listener(
                app.handle().clone(),
                app_data_dir.clone(),
                Arc::clone(&db),
            );
            remote_queue::spawn_remote_ai_queue_worker(
                app.handle().clone(),
                app_data_dir.clone(),
                Arc::clone(&db),
                mesh_control.clone(),
            );
            app.manage(mesh_control);
            lan_sync::spawn_lan_sync(
                app.handle().clone(),
                app_data_dir.clone(),
                Arc::clone(&db),
            );
            let (watcher_tx, watcher_rx) = crossbeam_channel::unbounded();
            run_watcher_loop(Arc::clone(&db), watcher_rx, app_data_dir.clone(), app.handle().clone());
            let config = load_config(&app_data_dir);
            let folders = {
                let g = db.lock().map_err(|e| e.to_string())?;
                compute_watcher_folders(&*g, &app_data_dir, &config)?
            };
            app.manage(AppState {
                db,
                model_paths,
                watcher_tx: watcher_tx.clone(),
                active_offline_map_downloads: Arc::new(Mutex::new(HashSet::new())),
                active_package_downloads: Arc::new(Mutex::new(HashMap::new())),
                config_lock: Arc::new(Mutex::new(())),
            });
            let _ = watcher_tx.send(WatchCommand::SetFolders(folders));

            #[cfg(not(mobile))]
            let spawned_sidecar: Option<CommandChild> = match app.handle().shell().sidecar("ollama") {
                Ok(cmd) => match cmd.args(["serve"]).spawn() {
                    Ok((mut rx, child)) => {
                        tauri::async_runtime::spawn(async move {
                            while rx.recv().await.is_some() {}
                        });
                        Some(child)
                    }
                    Err(e) => {
                        eprintln!("[athena] Ollama sidecar spawn failed: {e}");
                        None
                    }
                },
                Err(e) => {
                    eprintln!("[athena] Ollama sidecar not available: {e}");
                    None
                }
            };

            #[cfg(not(mobile))]
            let (ollama_child, use_legacy_ollama_launch) = match spawned_sidecar {
                None => (None, true),
                Some(child) => {
                    let http_ok =
                        tauri::async_runtime::block_on(ollama::ollama_http_ready_after_sidecar(5, 500));
                    if http_ok {
                        (Some(child), false)
                    } else {
                        eprintln!(
                            "[athena] Ollama sidecar did not respond on the HTTP endpoint after spawn; terminating sidecar and enabling legacy launch."
                        );
                        if let Err(e) = child.kill() {
                            eprintln!("[athena] Failed to kill Ollama sidecar after health check failure: {e}");
                        }
                        (None, true)
                    }
                }
            };
            #[cfg(mobile)]
            let (ollama_child, use_legacy_ollama_launch) = (None, true);

            app.manage(OllamaState(Mutex::new(ollama_child)));
            app.manage(radio_manager::RadioState::new());
            app.manage(auth::AuthSession::default());

            if use_legacy_ollama_launch {
                let app_data_for_ollama = app_data_dir.clone();
                let config_for_ollama = config.clone();
                let app_handle_for_ollama = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    if !is_ollama_running() {
                        let prefs =
                            ollama_setup::resolve_launch_paths(&app_data_for_ollama, &config_for_ollama);
                        try_launch_ollama_server_with_preferences(&prefs);
                        std::thread::sleep(std::time::Duration::from_secs(3));
                    }
                    if is_ollama_running() {
                        return;
                    }
                    // Give the macOS Ollama.app / slow `ollama serve` startups time before downloading.
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    if is_ollama_running() {
                        return;
                    }
                    if config_for_ollama.ollama_disable_auto_cli_download == Some(true) {
                        return;
                    }
                    let managed = ollama_setup::managed_ollama_binary_path(&app_data_for_ollama);
                    if managed.is_file() {
                        let prefs =
                            ollama_setup::resolve_launch_paths(&app_data_for_ollama, &config_for_ollama);
                        try_launch_ollama_server_with_preferences(&prefs);
                        return;
                    }
                    let _ = app_handle_for_ollama.emit(
                        "ollama-setup-progress",
                        serde_json::json!({ "phase": "downloading" }),
                    );
                    match ollama_setup::install_ollama_cli_to_app_data(&app_data_for_ollama) {
                        Ok(bin) => {
                            let _ = app_handle_for_ollama.emit(
                                "ollama-setup-progress",
                                serde_json::json!({
                                    "phase": "installed",
                                    "path": bin.to_string_lossy(),
                                }),
                            );
                            let prefs = ollama_setup::resolve_launch_paths(
                                &app_data_for_ollama,
                                &config_for_ollama,
                            );
                            try_launch_ollama_server_with_preferences(&prefs);
                        }
                        Err(e) => {
                            let _ = app_handle_for_ollama.emit(
                                "ollama-setup-progress",
                                serde_json::json!({
                                    "phase": "error",
                                    "message": e,
                                }),
                            );
                        }
                    }
                });
            }
            let app_data_default_ollama = app_data_dir.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(18));
                maybe_ensure_default_ollama_model(&app_data_default_ollama);
            });
            Ok(())
            })();

            if let Err(e) = result {
                let mut status = diagnostics::run_startup_diagnostics(&app.handle());
                status.maintenance_mode = true;
                status.failures.push(diagnostics::DiagnosticFailure {
                    key: "startup_init_failed".to_string(),
                    message: e,
                });
                diagnostics::set_status(&app.handle(), status);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            diagnostics::get_resilience_status,
            auth::authenticate_pin,
            auth::logout,
            auth::set_my_personnel_display_name,
            auth::set_my_personnel_login_name,
            auth::set_my_personnel_pin,
            auth::refresh_auth_session,
            auth::set_personnel_display_name,
            auth::admin_update_personnel,
            auth::add_personnel,
            auth::remove_personnel,
            auth::list_personnel,
            inventory::list_inventory,
            inventory::add_inventory_item,
            inventory::update_inventory_quantity,
            inventory::update_inventory_category,
            inventory::list_inventory_log,
            inventory::edit_log_fingerprint,
            hardware_map::check_hardware_status,
            remote_queue::get_queue_status,
            remote_queue::enqueue_request,
            scan_chat_models,
            scan_embedding_models,
            scan_reranker_models,
            get_watch_folder,
            set_watch_folder,
            list_archive_watch_roots,
            add_archive_watch_root,
            remove_archive_watch_root,
            get_theme_config,
            set_theme_config,
            get_indexed_files,
            ingest_files,
            get_index_stats,
            get_rag_stats,
            test_rag_search,
            rebuild_vector_index,
            list_library_contents,
            remove_library_file,
            backup_database,
            restore_database,
            get_system_specs,
            get_lan_cluster_peers,
            get_battery_state,
            get_ollama_available_models,
            get_local_models,
            pull_new_model,
            get_sidecar_status,
            get_ollama_setup_status,
            install_ollama_cli_for_athena,
            copy_ollama_into_athena_managed,
            set_ollama_athena_cli_path,
            is_ollama_available,
            try_launch_ollama,
            get_recommended_model,
            get_model_override,
            set_model_override,
            get_user_and_inference_settings,
            set_user_display_name,
            set_user_birthdate_iso,
            set_time_format,
            set_calendar_region,
            set_calendar_quiet_hours,
            set_inference_backend,
            set_explicit_ollama_model,
            set_active_embedding_model,
            set_active_reranker_model,
            set_openai_compatible_base_url,
            set_openai_compatible_api_key,
            set_openai_compatible_model,
            set_allow_nsfw_unrestricted_models,
            probe_openai_compatible_connection,
            is_inference_ready,
            moondream_describe_image,
            kiwix_search_with_tool,
            get_offline_route_geojson,
            get_memory_messages,
            set_memory_messages,
            get_feature_map_enabled,
            set_feature_map_enabled,
            get_show_sources_in_chat,
            set_show_sources_in_chat,
            get_user_location,
            set_user_location,
            get_enabled_main_map_raster_archive_file_names,
            set_enabled_main_map_raster_archive_file_names,
            export_tactical_data,
            import_tactical_data,
            get_tactical_map_layers,
            add_tactical_poi,
            delete_tactical_poi,
            broadcast_tactical_poi,
            get_offline_mbtiles_zoom_range,
            get_offline_mbtiles_bounds,
            get_satellite_api_key,
            set_satellite_api_key,
            get_memory_estimate,
            get_local_time,
            get_sun_times,
            send_message,
            help_assistant_ask,
            send_message_stream,
            scan_kiwix_zim_files,
            get_map_catalog,
            get_offline_map_manager_catalog,
            list_downloaded_offline_map_ids,
            pmtiles_region_extract::extract_pmtiles_region,
            satellite_region_download::start_satellite_region_download,
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
            map_manager::get_map_markers,
            map_manager::add_map_marker,
            map_manager::delete_map_marker,
            map_manager::get_spatial_context,
            map_manager::broadcast_map_marker,
            map_manager::convert_coordinates,
            map_manager::convert_mgrs_to_lat_lng,
            map_manager::save_drawing,
            map_manager::get_drawings,
            map_manager::delete_drawing,
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
            telemetry::get_system_stats,
            build_library_knowledge,
            set_fullscreen,
            is_fullscreen,
            commands::package::get_library_manifest,
            commands::package::get_library_installer_location,
            commands::package::save_library_installer_location_nyc_default,
            commands::package::save_library_installer_location_geocode,
            commands::package::clear_library_installer_location,
            commands::package::get_model_manifest,
            commands::package::download_package,
            commands::package::cancel_package_download,
            commands::package::get_models_dir,
            commands::package::set_models_dir,
            commands::chat_console::chat_console_stream,
            portable::export_athena_to_usb,
            usb::get_available_usbs,
            commands::library_source::open_library_source,
            send_mesh_message,
            add_mesh_profile,
            remove_mesh_profile,
            list_mesh_profiles,
            mesh_emergency_list_active,
            mesh_emergency_operator_resolve,
            list_mesh_network_nodes_cmd,
            list_mesh_chats_for_node_cmd,
            send_mesh_operator_chat_message,
            get_mesh_ai_auto_reply,
            set_mesh_ai_auto_reply,
            get_ai_inference_sleep,
            set_ai_inference_sleep,
            enter_station_minimal_mode,
            is_mesh_radio_connected,
            get_mesh_radio_identity,
            is_mesh_remote_registration_enabled,
            set_mesh_registration_admin_password,
            logbook_list_entries,
            logbook_add_entry,
            logbook_delete_entry,
            logbook_broadcast_entry,
            logbook_request_mesh_sync,
            logbook_mesh_max_body_bytes,
            get_logbook_author_profile,
            set_logbook_author_profile,
            academy_get_curriculum,
            academy_list_completed,
            academy_mark_lesson_complete,
            academy_get_lesson_html,
            calendar_list_events,
            calendar_upsert_event,
            calendar_delete_event,
            calendar_request_mesh_sync,
            rag_indexer::ingest_local_document,
            rag_indexer::search_rag_chunks,
            rag_indexer::download_knowledge_package,
            rag_indexer::list_rag_documents,
            radio_manager::get_available_ports,
            radio_manager::connect_to_radio,
            radio_manager::send_radio_message,
            radio_manager::get_recent_radio_logs,
            radio_manager::get_radio_telemetry_samples,
            radio_manager::get_mesh_messages,
            radio_manager::list_radio_channels,
            radio_manager::upsert_radio_channel,
            radio_manager::delete_radio_channel,
            radio_manager::apply_channel_profile,
            radio_manager::set_radio_config,
            radio_manager::export_radio_logs,
            sdr_manager::sdr_list_devices,
            sdr_manager::sdr_start,
            sdr_manager::sdr_stop,
            sdr_manager::sdr_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                kill_ollama_sidecar_if_running(app_handle);
                sdr_manager::stop_sdr_internal(app_handle);
            }
        });
}
