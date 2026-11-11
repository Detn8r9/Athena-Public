#[path = "domains/academy/academy.rs"]
mod academy;
#[path = "domains/auth/auth.rs"]
mod auth;
#[path = "platform/protocols/athena_map_protocol.rs"]
mod athena_map_protocol;
#[path = "platform/protocols/athena_mbtiles_tile_protocol.rs"]
mod athena_mbtiles_tile_protocol;
#[path = "domains/chat/chats.rs"]
mod chats;
#[path = "domains/ingestion/chunking.rs"]
mod chunking;
#[path = "platform/tauri/commands/mod.rs"]
mod commands;
#[path = "shared/config.rs"]
mod config;
#[path = "shared/athena_paths.rs"]
mod athena_paths;
#[path = "domains/db/db.rs"]
mod db;
#[path = "domains/ingestion/downloader.rs"]
mod downloader;
#[path = "domains/vector/neural_embed.rs"]
mod neural_embed;
#[path = "domains/vector/rerank.rs"]
mod rerank;
#[path = "domains/vector/embeddings.rs"]
mod embeddings;
#[path = "domains/hardware/hardware_map.rs"]
mod hardware_map;
#[path = "domains/indexing/indexer.rs"]
mod indexer;
#[path = "domains/ai/inference.rs"]
mod inference;
#[path = "domains/inventory/inventory.rs"]
mod inventory;
#[path = "domains/inventory/runway.rs"]
mod runway;
#[path = "domains/inventory/logistics_ai.rs"]
mod logistics_ai;
#[path = "domains/inventory/logistics_apply.rs"]
mod logistics_apply;
#[cfg(target_os = "linux")]
#[path = "platform/os/isoc23_shim.rs"]
mod isoc23_shim;
#[path = "domains/ingestion/kiwix.rs"]
mod kiwix;
#[path = "domains/ingestion/kiwix_serve.rs"]
mod kiwix_serve;
#[path = "domains/ingestion/kiwix_tools.rs"]
mod kiwix_tools;
#[path = "domains/system/watch_officer.rs"]
mod watch_officer;
#[path = "domains/system/os.rs"]
mod os;
#[path = "domains/system/lan_bridge.rs"]
mod lan_bridge;
#[path = "domains/system/connectivity.rs"]
mod connectivity;
#[path = "domains/marketplace/marketplace.rs"]
mod marketplace;
#[path = "domains/mesh/lan_cluster.rs"]
mod lan_cluster;
#[path = "domains/mesh/lan_identity.rs"]
mod lan_identity;
#[path = "domains/mesh/lan_chat.rs"]
mod lan_chat;
#[path = "domains/mesh/lan_packets.rs"]
mod lan_packets;
#[path = "domains/mesh/lan_sync.rs"]
mod lan_sync;
#[path = "domains/mesh/discovery.rs"]
mod mesh_discovery;
#[path = "domains/mesh/sync_api.rs"]
mod mesh_sync_api;
#[path = "domains/mesh/p2p_sync.rs"]
mod mesh_p2p_sync;
#[path = "domains/academy/learner.rs"]
mod learner;
#[path = "domains/ingestion/library_installer_location.rs"]
mod library_installer_location;
#[cfg(target_os = "macos")]
#[path = "platform/os/macos_window.rs"]
mod macos_window;
#[path = "domains/maps/map_manager.rs"]
mod map_manager;
#[path = "domains/inventory/saved_places.rs"]
mod saved_places;
#[path = "domains/inventory/blockages.rs"]
mod blockages;
#[path = "domains/inventory/territories.rs"]
mod territories;
#[path = "domains/ai/ai_staging.rs"]
mod ai_staging;
#[path = "domains/routing/valhalla_service.rs"]
mod valhalla_service;
#[path = "domains/ai/ai_tools.rs"]
mod ai_tools;
#[path = "domains/maps/map_manager_catalog.rs"]
mod map_manager_catalog;
#[path = "domains/maps/map_manager_download.rs"]
mod map_manager_download;
#[path = "domains/maps/maps.rs"]
mod maps;
#[path = "domains/maps/mbtiles_io.rs"]
mod mbtiles_io;
#[path = "domains/mesh/mesh_ai.rs"]
mod mesh_ai;
#[path = "domains/mesh/mesh_calendar.rs"]
mod mesh_calendar;
#[path = "domains/mesh/mesh_router.rs"]
mod mesh_router;
#[path = "domains/mesh/mesh_listener.rs"]
mod mesh_listener;
#[path = "domains/mesh/mesh_logbook.rs"]
mod mesh_logbook;
#[path = "domains/mesh/mesh_sos.rs"]
mod mesh_sos;
#[path = "domains/mesh/mesh_radio.rs"]
mod mesh_radio;
#[path = "domains/ai/moondream.rs"]
mod moondream;
#[path = "domains/ingestion/ocr.rs"]
mod ocr;
#[path = "domains/routing/offline_routing.rs"]
mod offline_routing;
#[path = "domains/routing/offline_geocoder.rs"]
mod offline_geocoder;
#[path = "domains/routing/directions.rs"]
mod directions;
#[path = "domains/routing/valhalla_lifecycle.rs"]
mod valhalla_lifecycle;
#[path = "domains/ingestion/geofabrik_downloader.rs"]
mod geofabrik_downloader;
#[path = "domains/ai/ollama.rs"]
mod ollama;
#[path = "domains/ai/ollama_setup.rs"]
mod ollama_setup;
#[path = "domains/ai/openai_chat.rs"]
mod openai_chat;
#[path = "domains/ai/model_manager.rs"]
mod model_manager;
#[path = "domains/ai/chat.rs"]
mod ai_console_chat;
#[path = "domains/inventory/personnel_commander.rs"]
mod personnel_commander;
#[path = "domains/deployment/package_manager.rs"]
mod package_manager;
#[path = "domains/deployment/portable.rs"]
mod portable;
#[path = "shared/diagnostics.rs"]
mod diagnostics;
#[path = "domains/maps/pmtiles_io.rs"]
mod pmtiles_io;
#[path = "domains/maps/pmtiles_region_extract.rs"]
mod pmtiles_region_extract;
#[path = "domains/queue/quarantine.rs"]
mod quarantine;
#[path = "domains/radio/radio_manager.rs"]
mod radio_manager;
#[path = "domains/vector/rag_indexer.rs"]
mod rag_indexer;
mod field_ops;
mod library_rag_gate;
mod library_context;
#[cfg(test)]
mod library_eval;
#[path = "domains/queue/remote_queue.rs"]
mod remote_queue;
#[path = "domains/ingestion/satellite_region_download.rs"]
mod satellite_region_download;
#[path = "domains/radio/sdr_manager.rs"]
mod sdr_manager;
#[path = "domains/db/spatial.rs"]
mod spatial;
#[path = "domains/routing/routing.rs"]
mod routing;
#[path = "domains/telemetry/monitor.rs"]
mod telemetry_monitor;
#[path = "domains/telemetry/logging.rs"]
mod telemetry_logging;
#[path = "domains/db/storage.rs"]
mod storage;
#[path = "domains/deployment/deployment.rs"]
mod deployment;
#[path = "domains/ingestion/library_ingest.rs"]
mod library_ingest;
#[path = "domains/ingestion/media_harvest.rs"]
mod media_harvest;
#[path = "domains/deployment/deployment_manifest_cache.rs"]
mod deployment_manifest_cache;
#[path = "domains/deployment/deployment_queue_state.rs"]
mod deployment_queue_state;
#[path = "shared/system.rs"]
mod system;
#[path = "shared/telemetry.rs"]
mod telemetry;
#[path = "shared/theme.rs"]
mod theme;
#[path = "domains/maps/tile_pack.rs"]
mod tile_pack;
#[cfg(feature = "piper-tts")]
#[path = "domains/ai/tts.rs"]
mod tts;
#[path = "domains/ai/ptt_record.rs"]
mod ptt_record;
#[path = "domains/auth/user_profile.rs"]
mod user_profile;
#[path = "domains/hardware/usb.rs"]
mod usb;
#[path = "domains/vector/vector_index.rs"]
mod vector_index;
#[path = "domains/vector/document_rag_read.rs"]
mod document_rag_read;
#[path = "domains/vector/rag_chunk_worker.rs"]
mod rag_chunk_worker;
#[path = "domains/indexing/watcher.rs"]
mod watcher;
#[path = "domains/ingestion/zim_download.rs"]
mod zim_download;
#[path = "domains/ingestion/background_ingestion.rs"]
mod background_ingestion;

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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    IndexStats, IndexedFile, LogbookEntry, RagCounts, TacticalMeshNodeLocation,
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
    pub active_zim_downloads: Arc<Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
    /// Serialize reads/writes of `athena_config.json`.
    pub config_lock: Arc<Mutex<()>>,
    /// Set by `abort_chat_console_stream`; read by the console streaming pipeline to stop early.
    pub console_stream_cancel: Arc<AtomicBool>,
    /// Ephemeral HTTP port for the managed local llama server (`http://127.0.0.1:{port}/v1`); shared with `model_manager`.
    pub managed_local_llm_http_port: Arc<Mutex<Option<u16>>>,
    /// Global power-save mode flag (battery < 20%); used to gate background jobs like P2P replication.
    pub power_save_mode: Arc<AtomicBool>,
    /// Background ingestion queue (download-complete + manual file drops).
    pub ingestion_queue: background_ingestion::IngestionQueue,
}


/// Chat models: only validated `.gguf` files under `models/chat`; skip hidden names (e.g. `.DS_Store`).
fn list_sorted_gguf_files(dir: &std::path::Path) -> Result<Vec<String>, String> {
    model_manager::list_sorted_validated_gguf_filenames(dir)
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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveredLocalGgufModel {
    file_name: String,
    absolute_path: String,
    size_bytes: u64,
}

fn scan_gguf_under_root(root: &std::path::Path, max_depth: usize) -> Result<Vec<DiscoveredLocalGgufModel>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out: Vec<DiscoveredLocalGgufModel> = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
        .into_iter()
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // permission errors etc; skip noisy roots
        };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        if !name.to_ascii_lowercase().ends_with(".gguf") {
            continue;
        }
        let len = match p.metadata() {
            Ok(m) => m.len(),
            Err(_) => 0,
        };
        out.push(DiscoveredLocalGgufModel {
            file_name: name,
            absolute_path: p.to_string_lossy().to_string(),
            size_bytes: len,
        });
        if out.len() >= 4000 {
            break;
        }
    }
    out.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes).then_with(|| a.file_name.cmp(&b.file_name)));
    out.dedup_by(|a, b| a.absolute_path == b.absolute_path);
    Ok(out)
}

#[tauri::command]
fn scan_local_device_for_models(app: tauri::AppHandle) -> Result<Vec<DiscoveredLocalGgufModel>, String> {
    let mut roots: Vec<(std::path::PathBuf, usize)> = Vec::new();

    // Downloads (user-facing catch-all).
    if let Ok(d) = app.path().download_dir() {
        roots.push((d, 3));
    }

    // Home-derived roots.
    let home = app.path().home_dir().ok();
    if let Some(ref h) = home {
        // Ollama (mac/linux/windows via dot folder when present).
        roots.push((h.join(".ollama").join("models"), 6));

        #[cfg(target_os = "macos")]
        {
            roots.push((h.join("Library").join("Application Support").join("LM Studio").join("models"), 7));
            roots.push((h.join("Library").join("Application Support").join("nomic.ai").join("GPT4All"), 7));
            roots.push((h.join("Library").join("Application Support").join("gpt4all"), 7));
        }

        #[cfg(target_os = "linux")]
        {
            roots.push((h.join(".local").join("share").join("LM Studio").join("models"), 7));
            roots.push((h.join(".local").join("share").join("nomic.ai").join("GPT4All"), 7));
            roots.push((h.join(".local").join("share").join("gpt4all"), 7));
            roots.push((h.join(".cache").join("lm-studio").join("models"), 7));
        }

        #[cfg(target_os = "windows")]
        {
            // Some installs put LM Studio / GPT4All under LocalAppData.
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                let l = std::path::PathBuf::from(local);
                roots.push((l.join("LM Studio").join("models"), 7));
                roots.push((l.join("nomic.ai").join("GPT4All"), 7));
                roots.push((l.join("gpt4all"), 7));
            }
        }
    }

    // Scan and merge.
    let mut all: Vec<DiscoveredLocalGgufModel> = Vec::new();
    for (root, depth) in roots {
        let mut batch = scan_gguf_under_root(&root, depth)?;
        all.append(&mut batch);
        if all.len() >= 5000 {
            break;
        }
    }
    all.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes).then_with(|| a.file_name.cmp(&b.file_name)));
    all.dedup_by(|a, b| a.absolute_path == b.absolute_path);
    Ok(all)
}

#[tauri::command]
fn scan_chat_models(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    list_sorted_gguf_files(&state.model_paths.chat_models)
}

/// Filenames of validated `.gguf` files present under `models/chat` (resolved app data path).
#[tauri::command]
fn get_downloaded_models(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    list_sorted_gguf_files(&state.model_paths.chat_models)
}

/// Spawn a llama.cpp-compatible `llama-server` when `ATHENA_LLAMA_SERVER_BIN` is set; binds to `127.0.0.1` on an ephemeral port.
#[tauri::command]
fn start_managed_local_llm_server(
    state: tauri::State<'_, AppState>,
    gguf_file_name: String,
) -> Result<u16, String> {
    let name = gguf_file_name.trim();
    if name.is_empty() {
        return Err("Model file name is empty.".to_string());
    }
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err("Invalid model file name.".to_string());
    }
    let path = state.model_paths.chat_models.join(name);
    model_manager::try_spawn_llama_server_localhost(&path)
}

#[tauri::command]
fn get_managed_local_llm_http_base_url() -> Option<String> {
    model_manager::managed_llama_openai_base_url()
}

#[tauri::command]
async fn mesh_p2p_list_peers() -> Result<Vec<mesh_discovery::MeshDiscoveredPeer>, String> {
    let reg = mesh_discovery::peer_registry();
    let g = reg.read().await;
    let mut v: Vec<_> = g.values().cloned().collect();
    v.sort_by(|a, b| {
        a.ipv4
            .cmp(&b.ipv4)
            .then_with(|| a.rest_port.cmp(&b.rest_port))
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    Ok(v)
}

#[tauri::command]
async fn mesh_p2p_sync_with_peer(manifest_url: String) -> Result<serde_json::Value, String> {
    let u = manifest_url.trim();
    let parsed = reqwest::Url::parse(u).map_err(|e| e.to_string())?;
    if !mesh_sync_api::mesh_manifest_url_is_lan_safe(&parsed) {
        return Err(
            "manifest_url must use http:// with a localhost, .local, or private (RFC1918) IPv4 host."
                .to_string(),
        );
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let txt = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&txt).map_err(|e| e.to_string())
}

#[tauri::command]
async fn mesh_p2p_manifest_diff(
    state: tauri::State<'_, AppState>,
    manifest_url: String,
) -> Result<mesh_p2p_sync::P2pManifestDiffResult, String> {
    mesh_p2p_sync::fetch_and_diff_manifest(Arc::clone(&state.db), &manifest_url).await
}

#[tauri::command]
async fn mesh_p2p_replicate_from_peer(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    peer_base_url: String,
) -> Result<(), String> {
    let base = peer_base_url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("peer_base_url is empty.".to_string());
    }
    let manifest_url = format!("{base}/mesh/manifest");
    let diff = mesh_p2p_sync::fetch_and_diff_manifest(Arc::clone(&state.db), &manifest_url).await?;
    let athena_data_root = crate::portable::resolved_app_data_dir(&app)?;
    mesh_p2p_sync::replicate_missing_files(
        app,
        Arc::clone(&state.db),
        base,
        athena_data_root,
        diff.missing,
    )
    .await
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

#[cfg(not(mobile))]
fn kill_kiwix_serve_sidecar_if_running(app: &tauri::AppHandle) {
    kiwix_serve::kill_if_running(app);
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Send a plain text packet on a Meshtastic channel (default 0). Broadcast if `destination_node_id` is omitted; otherwise DM that node.
#[tauri::command]
async fn send_mesh_message(
    control: tauri::State<'_, mesh_listener::MeshRadioControl>,
    session: tauri::State<'_, auth::AuthSession>,
    text: String,
    want_ack: Option<bool>,
    channel: Option<u32>,
    destination_node_id: Option<u32>,
) -> Result<(), String> {
    let u = auth::authenticated_user(&session)?;
    if u.clearance_level <= 0 {
        if destination_node_id.is_some() {
            return Err("Unauthorized: Guest may only broadcast on Open/Distress.".to_string());
        }
        if channel.unwrap_or(0) != 0 {
            return Err("Unauthorized: Guest may only use Open/Distress channel (0).".to_string());
        }
        if want_ack.unwrap_or(false) {
            return Err("Unauthorized: Guest cannot request ACK.".to_string());
        }
    }
    let effective_channel = if u.clearance_level <= 0 {
        0
    } else {
        // Channel 0 = open distress; Channel 1 = general operations default.
        channel.unwrap_or(1)
    };
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
            effective_channel,
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
fn academy_list_pdf_units(
    app: tauri::AppHandle,
) -> Result<Vec<academy::AcademyPdfUnit>, String> {
    academy::list_pdf_units(&app)
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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogisticsSubmitAttach {
    map_lat: Option<f64>,
    map_lon: Option<f64>,
    saved_place_id: Option<String>,
    location_text: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogisticsLibrarySubmitResponse {
    entry: db::LogisticsLibraryRow,
    proposal_lines: Vec<db::InventoryProposalItemRow>,
    line_outcomes: Vec<logistics_apply::LineOutcome>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InvQueueCountResponse {
    pending_proposals: i64,
    new_item_prompts: i64,
    suppressed_duplicates: i64,
}

#[tauri::command]
fn inv_queue_count(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
) -> Result<InvQueueCountResponse, String> {
    let u = auth::authenticated_user(&session)?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let g = state.db.lock().map_err(|e| e.to_string())?;
    let day_ms: i64 = 86_400_000;
    let pending = g.count_pending_proposals_for_submitter(&u.id)?;
    let new_item = g.count_pending_add_item_proposals_for_submitter(&u.id)?;
    let sup = g.count_suppressed_intakes_for_submitter(&u.id, now_ms - day_ms)?;
    Ok(InvQueueCountResponse {
        pending_proposals: pending,
        new_item_prompts: new_item,
        suppressed_duplicates: sup,
    })
}

#[tauri::command]
fn logistics_library_list_entries(state: tauri::State<'_, AppState>) -> Result<Vec<db::LogisticsLibraryRow>, String> {
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_logistics_library_entries(300)
}

#[tauri::command]
fn reject_inventory_proposal(
    proposal_id: String,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
) -> Result<(), String> {
    let actor = auth::authenticated_user(&session)?;
    if actor.rank_level < 3 {
        return Err("Insufficient rank to reject inventory proposals.".to_string());
    }
    let pid = proposal_id.trim();
    if pid.is_empty() {
        return Err("proposalId is required.".to_string());
    }
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .inventory_proposal_set_status(pid, "rejected", None, None)
}

#[tauri::command]
fn logistics_library_submit_message(
    app: tauri::AppHandle,
    message: String,
    attached: Option<LogisticsSubmitAttach>,
    force_apply: Option<bool>,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
) -> Result<LogisticsLibrarySubmitResponse, String> {
    let actor = auth::authenticated_user(&session)?;
    let body = message.trim().to_string();
    if body.is_empty() {
        return Err("Message is empty.".to_string());
    }
    if body.len() > 32_000 {
        return Err("Message is too long (32k character limit).".to_string());
    }

    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let id = format!(
        "ll-{}-{}",
        created_at,
        LOGBOOK_ENTRY_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );

    let author_name = actor
        .display_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(actor.name.as_str());
    let author_occupation = actor.division.as_str();

    let (map_lat, map_lon, saved_place_id, location_text) = if let Some(ref a) = attached {
        (a.map_lat, a.map_lon, a.saved_place_id.clone(), a.location_text.clone())
    } else {
        (None, None, None, None)
    };

    {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        g.insert_logistics_library_entry(
            &id,
            &body,
            created_at,
            author_name,
            author_occupation,
            Some(actor.id.as_str()),
            Some(actor.rank_level),
            Some(actor.division.as_str()),
            actor.directorate.as_deref(),
            actor.tier.as_deref(),
            actor.job_title.as_deref(),
            map_lat,
            map_lon,
            saved_place_id.as_deref(),
            location_text.as_deref(),
            "general",
        )?;
    }

    let mut loc_hint: Option<String> = location_text
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if loc_hint.is_none() {
        if let Some(ref spid) = saved_place_id {
            let g = state.db.lock().map_err(|e| e.to_string())?;
            if let Some(sp) = g.get_saved_place(spid)? {
                loc_hint = Some(sp.name);
            }
        }
    }

    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    let models = inference::load_models_for_inference(&cfg);
    inference::ensure_inference_ready(&cfg, &models)?;

    let ai = logistics_ai::classify_logistics_message(&cfg, &models, &body)
        .unwrap_or_else(|_| logistics_ai::LogisticsAiResult {
            section: "General".to_string(),
            kind: "log_note".to_string(),
            inventory: Vec::new(),
        });
    let section = ai.section.trim().to_string();
    let ai_json = serde_json::to_string(&ai).unwrap_or_else(|_| "{}".to_string());
    let force = force_apply.unwrap_or(false);

    let (mut line_outcomes, pending_lines) = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        logistics_apply::process_inventory_lines(
            &g,
            &actor,
            &id,
            &ai,
            created_at,
            force,
            loc_hint.as_deref(),
        )?
    };

    let mut proposal_id: Option<String> = None;
    {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        g.logistics_annotation_upsert(&id, &section, &ai_json, "pending", created_at)?;
        if !pending_lines.is_empty() {
            let pid = uuid::Uuid::new_v4().to_string();
            g.inventory_proposal_create(&pid, &id, Some(actor.id.as_str()), created_at)?;
            for pl in &pending_lines {
                g.inventory_proposal_item_insert(
                    &pl.id,
                    &pid,
                    pl.action.as_str(),
                    pl.division.as_str(),
                    pl.item_name.as_str(),
                    pl.category.as_str(),
                    pl.quantity,
                    pl.confidence,
                    pl.line_location.as_deref(),
                )?;
            }
            proposal_id = Some(pid);
        }
    }

    let proposal_lines = if let Some(ref pid) = proposal_id {
        state
            .db
            .lock()
            .map_err(|e| e.to_string())?
            .inventory_proposal_list_items(pid)?
    } else {
        Vec::new()
    };

    if let Some(ref pid) = proposal_id {
        for o in &mut line_outcomes {
            if o.kind == "newItemPrompt" || o.kind == "pendingProposal" {
                o.proposal_id = Some(pid.clone());
            }
        }
    }

    let proposal_status = if proposal_id.is_some() {
        Some("pending".to_string())
    } else {
        None
    };

    Ok(LogisticsLibrarySubmitResponse {
        entry: db::LogisticsLibraryRow {
            entry_id: id,
            body,
            created_at,
            author_name: author_name.to_string(),
            author_rank_level: Some(actor.rank_level),
            author_division: Some(actor.division),
            author_directorate: actor.directorate.clone(),
            author_tier: actor.tier.clone(),
            author_job_title: actor.job_title.clone(),
            section,
            proposal_id,
            proposal_status,
            map_lat,
            map_lon,
            saved_place_id,
            location_text,
        },
        proposal_lines,
        line_outcomes,
    })
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
fn open_path_in_file_manager(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let t = path.trim();
    if t.is_empty() {
        return Err("Path is empty.".to_string());
    }
    // For directories that don't exist yet, create them (useful for freshly set watch folders).
    let p = std::path::PathBuf::from(t);
    if !p.exists() {
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    }
    app.opener()
        .open_path(p.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[allow(clippy::type_complexity)]
fn get_theme_config(
    app: tauri::AppHandle,
) -> Result<(Option<String>, Option<String>, Vec<CustomTheme>, Option<String>), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    Ok((
        cfg.theme_id,
        cfg.personality_id,
        cfg.custom_themes,
        cfg.theme_appearance,
    ))
}

#[tauri::command]
fn set_theme_config(
    app: tauri::AppHandle,
    theme_id: Option<String>,
    personality_id: Option<String>,
    custom_themes: Vec<CustomTheme>,
    theme_appearance: Option<String>,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data_dir);
    cfg.theme_id = theme_id;
    cfg.personality_id = personality_id;
    cfg.custom_themes = custom_themes;
    if theme_appearance.is_some() {
        cfg.theme_appearance = theme_appearance;
    }
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
    // Unified downloads root: ZIMs live under the managed packages downloads.
    crate::athena_paths::AthenaPaths::new(app_data_dir)
        .user_downloads_dir()
        .join("packages")
}

pub(crate) fn downloads_root(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    crate::athena_paths::AthenaPaths::new(app_data_dir).user_downloads_dir()
}

pub(crate) fn ensure_download_dirs(app_data_dir: &std::path::Path) -> Result<(), String> {
    let root = downloads_root(app_data_dir);
    let dirs = [
        root.join("packages"),
        root.join("academy"),
        root.join("maps"),
        root.join("models"),
        root.join("documents"),
    ];
    for d in dirs {
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn compute_watcher_folders(
    db: &Database,
    app_data_dir: &std::path::Path,
    config: &AppConfig,
) -> Result<Vec<String>, String> {
    ensure_download_dirs(app_data_dir)?;
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
        if indexer::skip_if_up_to_date(path, &*db, ctx_tuple, None).is_some() {
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
                tracing::info!("ingest_files: index_file returned Some; upsert_file");
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
                    tracing::error!(error = %e, "Fatal Ingestion Error: ingest upsert_file failed");
                    continue;
                }
                tracing::info!("ingest_files: upsert ok; maybe index_document");
                if let Some(ref text) = data.text_content {
                    if !text.trim().is_empty() {
                        tracing::info!("ingest_files: calling vector_index::index_document");
                        match vector_index::index_document(
                            &db,
                            &p,
                            &data.display_title,
                            data.index_pages.as_deref(),
                            text,
                        ) {
                            Ok(()) => {}
                            Err(e) => {
                                tracing::error!(error = %e, "Fatal Ingestion Error: vector index");
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
fn cancel_aegis_export() {
    portable::request_aegis_export_cancel();
}

#[tauri::command]
async fn export_athena_to_usb(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    destination_path: String,
    export_type: String,
    wipe_after: bool,
    format_first: bool,
    device_path: Option<String>,
    expedition_budget_bytes: Option<u64>,
    selected_external_chat_model_path: Option<String>,
    include_docker_volumes: Option<bool>,
) -> Result<(), String> {
    let db = Arc::clone(&state.db);
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        portable::export_athena_to_usb(
            &app_handle,
            &db,
            destination_path,
            export_type,
            wipe_after,
            format_first,
            device_path,
            expedition_budget_bytes,
            selected_external_chat_model_path,
            include_docker_volumes.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| format!("Export task failed: {e}"))?
}

#[tauri::command]
async fn aegis_export_preflight(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    destination_path: Option<String>,
) -> Result<portable::AegisExportPreflight, String> {
    let db = Arc::clone(&state.db);
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        portable::aegis_export_preflight(&app_handle, &db, destination_path)
    })
    .await
    .map_err(|e| format!("Preflight task failed: {e}"))?
}

/// Non-blocking: returns immediately; results arrive on `athena:aegis-preflight-progress`.
#[tauri::command]
fn start_aegis_export_preflight(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    destination_path: Option<String>,
) -> Result<(), String> {
    let db = Arc::clone(&state.db);
    portable::start_aegis_export_preflight_background(&app, db, destination_path);
    Ok(())
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
fn get_ollama_available_models(app: tauri::AppHandle) -> Result<Vec<ModelInfo>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    get_available_models(Some(&config))
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
    let ollama_models = crate::athena_paths::AthenaPaths::new(&app_data_dir).user_ollama_models_dir();
    let _ = std::fs::create_dir_all(&ollama_models);
    try_launch_ollama_server_with_preferences(&prefs, Some(ollama_models.as_path()));
    Ok(())
}

#[tauri::command]
fn get_recommended_model(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let specs = get_specs();
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let models = get_available_models(Some(&config))?;
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
    hive_mind_seed_url: Option<String>,
    explicit_ollama_model: Option<String>,
    openai_compatible_base_url: Option<String>,
    openai_compatible_model: Option<String>,
    openai_compatible_api_key_set: bool,
    /// Effective ONNX embedding folder under `models/embeddings`.
    active_embedding_model_folder: String,
    /// Effective ONNX reranker folder under `models/rerankers`.
    active_reranker_model_folder: String,
    ai_inference_sleep: bool,
    connectivity_mode: String,
    connectivity_label: String,
    internet_allowed: bool,
}

fn user_and_inference_settings_for_app(
    app: &tauri::AppHandle,
) -> Result<UserAndInferenceSettings, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(app)?;
    let c = load_config(&app_data_dir);
    let bd = c.user_birthdate_iso.as_deref();
    let conn = connectivity::status_for_config(&c);
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
        hive_mind_seed_url: c.hive_mind_seed_url.clone(),
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
        connectivity_mode: conn.mode,
        connectivity_label: conn.label,
        internet_allowed: conn.internet_allowed,
    })
}

#[tauri::command]
fn get_user_and_inference_settings(
    app: tauri::AppHandle,
) -> Result<UserAndInferenceSettings, String> {
    user_and_inference_settings_for_app(&app)
}

pub(crate) fn lan_get_user_and_inference_settings_value(
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let s = user_and_inference_settings_for_app(&app)?;
    serde_json::to_value(s).map_err(|e| e.to_string())
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
fn get_connectivity_status(app: tauri::AppHandle) -> Result<connectivity::ConnectivityStatus, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    Ok(connectivity::status_for_config(&config))
}

#[tauri::command]
fn set_connectivity_mode(
    app: tauri::AppHandle,
    value: String,
) -> Result<connectivity::ConnectivityStatus, String> {
    let mode = connectivity::ConnectivityMode::parse(Some(value.trim()));
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.connectivity_mode = Some(mode.as_str().to_string());
    save_config(&app_data_dir, &config)?;
    Ok(connectivity::status_for_config(&config))
}

#[tauri::command]
fn marketplace_overview(app: tauri::AppHandle) -> Result<marketplace::MarketplaceOverview, String> {
    marketplace::overview(&app)
}

#[tauri::command]
fn marketplace_install(app: tauri::AppHandle, id: String) -> Result<(), String> {
    marketplace::install_async(&app, id.trim())
}

#[tauri::command]
fn marketplace_recreate(app: tauri::AppHandle, id: String) -> Result<(), String> {
    marketplace::recreate_async(&app, id.trim())
}

#[tauri::command]
fn marketplace_set_auto_update(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    marketplace::set_auto_update(&app, enabled)
}

#[tauri::command]
fn marketplace_tick_auto_updates(app: tauri::AppHandle) -> Result<usize, String> {
    marketplace::tick_auto_updates(&app)
}

#[tauri::command]
fn marketplace_start(app: tauri::AppHandle, id: String) -> Result<marketplace::InstalledItem, String> {
    marketplace::start(&app, id.trim())
}

#[tauri::command]
fn marketplace_stop(app: tauri::AppHandle, id: String) -> Result<marketplace::InstalledItem, String> {
    marketplace::stop(&app, id.trim())
}

#[tauri::command]
fn marketplace_open_url(app: tauri::AppHandle, id: String) -> Result<String, String> {
    marketplace::open_url_for(&app, id.trim())
}

#[tauri::command]
fn marketplace_health(app: tauri::AppHandle, id: String) -> Result<bool, String> {
    marketplace::health_check(&app, id.trim())
}

#[tauri::command]
fn marketplace_docker_status() -> Result<marketplace::DockerAvailability, String> {
    Ok(marketplace::docker_available())
}

#[tauri::command]
fn marketplace_docker_volume_bytes(app: tauri::AppHandle) -> Result<u64, String> {
    let app_data = crate::portable::resolved_app_data_dir(&app)?;
    Ok(marketplace::estimate_volume_bytes(&app_data))
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
fn set_hive_mind_seed_url(app: tauri::AppHandle, value: Option<String>) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut config = load_config(&app_data_dir);
    config.hive_mind_seed_url = value.and_then(|s| {
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HiveMindRouteStatus {
    mode: String,
    base_url: String,
}

#[tauri::command]
fn get_hive_mind_route_status(app: tauri::AppHandle) -> Result<HiveMindRouteStatus, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    if inference::is_openai_compatible_backend(&config) {
        return Ok(HiveMindRouteStatus {
            mode: "openai_compatible".to_string(),
            base_url: String::new(),
        });
    }
    let base = ollama::effective_ollama_base_url(Some(&config));
    let mode = if ollama::ollama_base_is_local_node(&base) {
        "local"
    } else {
        "hive"
    };
    Ok(HiveMindRouteStatus {
        mode: mode.to_string(),
        base_url: base,
    })
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
    use base64::Engine;
    let bytes = std::fs::read(&image_path).map_err(|e| format!("Read image failed: {e}"))?;
    if bytes.len() > 12_000_000 {
        return Err("Image too large for local vision (max ~12MB).".to_string());
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    moondream::describe_images_locally(&[b64], "")
}

/// Describe one or more in-memory camera/attachment frames (base64, no data: prefix).
#[tauri::command]
fn describe_vision_frames(images_b64: Vec<String>, question: Option<String>) -> Result<String, String> {
    if images_b64.is_empty() {
        return Err("No frames provided.".to_string());
    }
    if images_b64.len() > 4 {
        return Err("Too many frames (max 4).".to_string());
    }
    for img in &images_b64 {
        if img.len() > 16_000_000 {
            return Err("Frame payload too large.".to_string());
        }
    }
    moondream::describe_images_locally(&images_b64, question.as_deref().unwrap_or(""))
}

#[tauri::command]
fn get_vision_capability(preferred_model: Option<String>) -> Result<moondream::VisionCapability, String> {
    Ok(moondream::vision_capability(preferred_model.as_deref()))
}

const OLLAMA_PULL_PROGRESS_EVENT: &str = "athena:ollama-pull-progress";
const OLLAMA_PULL_DONE_EVENT: &str = "athena:ollama-pull-done";

/// Pull an Ollama model in a background thread (e.g. `moondream` for camera vision).
#[tauri::command]
fn pull_ollama_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let model = model.trim().to_string();
    if model.is_empty() || model.len() > 120 || model.contains([' ', '\n', '\r', ';', '|', '&']) {
        return Err("Invalid model name.".to_string());
    }
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let mut candidates = ollama_setup::resolve_launch_paths(&app_data_dir, &config);
    if let Ok(w) = which::which("ollama") {
        candidates.push(w);
    }
    candidates.push(std::path::PathBuf::from("ollama"));
    let bin = candidates
        .into_iter()
        .find(|p| p.as_os_str() == "ollama" || p.is_file())
        .ok_or_else(|| "Ollama CLI not found. Install Ollama, then try again.".to_string())?;

    let app_emit = app.clone();
    let model_clone = model.clone();
    std::thread::spawn(move || {
        let _ = app_emit.emit(
            OLLAMA_PULL_PROGRESS_EVENT,
            format!("Pulling {model_clone}…"),
        );
        let output = std::process::Command::new(&bin)
            .arg("pull")
            .arg(&model_clone)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let _ = app_emit.emit(
                    OLLAMA_PULL_DONE_EVENT,
                    serde_json::json!({ "ok": true, "model": model_clone }),
                );
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                let msg = if err.trim().is_empty() {
                    format!("ollama pull {model_clone} failed")
                } else {
                    err.chars().take(400).collect()
                };
                let _ = app_emit.emit(OLLAMA_PULL_PROGRESS_EVENT, msg.clone());
                let _ = app_emit.emit(
                    OLLAMA_PULL_DONE_EVENT,
                    serde_json::json!({ "ok": false, "model": model_clone, "error": msg }),
                );
            }
            Err(e) => {
                let msg = format!("Failed to run ollama pull: {e}");
                let _ = app_emit.emit(OLLAMA_PULL_PROGRESS_EVENT, msg.clone());
                let _ = app_emit.emit(
                    OLLAMA_PULL_DONE_EVENT,
                    serde_json::json!({ "ok": false, "model": model_clone, "error": msg }),
                );
            }
        }
    });
    Ok(())
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

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct MapDisplaySettings {
    #[serde(default)]
    coord_format: Option<String>,
    #[serde(default)]
    unit_system: Option<String>,
    #[serde(default)]
    default_travel_mode: Option<String>,
    #[serde(default)]
    last_known_gps_lat: Option<f64>,
    #[serde(default)]
    last_known_gps_lng: Option<f64>,
}

#[tauri::command]
fn get_map_display_settings(app: tauri::AppHandle) -> Result<MapDisplaySettings, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let c = load_config(&app_data_dir);
    Ok(MapDisplaySettings {
        coord_format: c.map_coord_format,
        unit_system: c.map_unit_system,
        default_travel_mode: c.map_default_travel_mode,
        last_known_gps_lat: c.last_known_gps_lat,
        last_known_gps_lng: c.last_known_gps_lng,
    })
}

#[tauri::command]
fn set_map_display_settings(
    app: tauri::AppHandle,
    value: MapDisplaySettings,
) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let mut c = load_config(&app_data_dir);
    c.map_coord_format = value.coord_format;
    c.map_unit_system = value.unit_system;
    c.map_default_travel_mode = value.default_travel_mode;
    c.last_known_gps_lat = value.last_known_gps_lat;
    c.last_known_gps_lng = value.last_known_gps_lng;
    save_config(&app_data_dir, &c)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MapAssistantResponse {
    reply: String,
    tool_results: Vec<ai_tools::MapToolResult>,
}

#[tauri::command]
async fn map_assistant_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    user_message: String,
    context_json: Option<serde_json::Value>,
) -> Result<MapAssistantResponse, String> {
    println!("--- ATHENA INVOCATION START ---");
    let _u = auth::authenticated_user(&session)?;
    let msg = user_message.trim();
    if msg.is_empty() {
        return Err("Message is empty.".to_string());
    }
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let config = load_config(&app_data_dir);
    let mut sys = "You are ATHENA, a tactical command map assistant. Never assume internet access. \
        There is NO online geocoding; place names resolve from offline OSM index (if built), map markers, and tactical POIs. \
        CRITICAL: The user's current location and focus is defined by the coordinates in the injected JSON context. NEVER say you do not have access to their location. You have their exact viewport center. \
        CRITICAL: NEVER invent a street address or city. If you need a human-readable label, use the reverse_geocode tool. If reverse geocode is unavailable, report coordinates only. \
        If the injected context contains 'inside_territories', you MUST use that data to answer questions about where the user is or what territory they are looking at. \
        For A-to-B directions use `directions` with free-text from/to. \
        To run a tool, output exactly one line: TOOL: {\"tool\":\"NAME\",\"args\":{...}} \
        Tools: \
        - search_markers {query} -> saved markers with lat/lng \
        - search_pois {query} -> tactical POIs with lat/lon \
        - reverse_geocode {lat, lon, max_results?, region_id?} -> nearest offline geocoder labels (do not guess) \
        - directions {from, to, mode} -> road/path route when Valhalla is up, else straight-line; distance, ETA; mode: drive|walk|bike|offroad \
        - update_territory_status {name, new_status: friendly|enemy|disputed|unexplored} -> updates a saved territory by name (clearance 4+ only) \
        If nothing resolves, ask the user to build a geocode index from PBF or add markers."
        .to_string();
    let merged_context = match context_json {
        Some(mut cj) => {
            let center = cj.get("center").and_then(|c| c.as_array());
            let lng = center.and_then(|c| c.get(0)).and_then(|v| v.as_f64());
            let lat = center.and_then(|c| c.get(1)).and_then(|v| v.as_f64());
            if let (Some(lng), Some(lat)) = (lng, lat) {
                if let Ok(db) = state.db.lock() {
                    if let Ok(sc) = crate::spatial::get_spatial_context(&db, lng, lat) {
                        if let Some(it) = sc.get("inside_territories").cloned() {
                            cj["inside_territories"] = it;
                        }
                    }
                }
            }
            println!("SPATIAL CONTEXT: {:?}", cj);
            Some(cj)
        }
        None => None,
    };
    if let Some(ref ctx) = merged_context {
        if let Ok(s) = serde_json::to_string_pretty(ctx) {
            use std::fmt::Write;
            let _ = write!(sys, "\nCurrent Viewport Spatial Context:\n{s}");
        }
    }
    let messages = vec![
        ollama::ChatMessage {
            role: "system".to_string(),
            content: sys,
        images: None,
        },
        ollama::ChatMessage {
            role: "user".to_string(),
            content: msg.to_string(),
        images: None,
        },
    ];
    // CRITICAL: LLM inference must not run on the Tauri main thread (macOS pinwheel / UI stall).
    // Also: hard-timeout so the UI never waits forever if the backend hangs.
    println!("SENDING TO OLLAMA...");
    let raw = {
        use std::time::Duration;
        let fut = tauri::async_runtime::spawn_blocking(move || {
            // reqwest::blocking (used by Ollama/OpenAI-compatible clients) can internally create a Tokio runtime.
            // That runtime must be created/dropped on a blocking thread, not on a Tokio worker thread, or Tokio will panic.
            let ollama_models = inference::load_models_for_inference(&config);
            inference::chat_unified(&config, &ollama_models, messages, None)
        });
        match tokio::time::timeout(Duration::from_secs(60), fut).await {
            Ok(joined) => joined.map_err(|e| format!("Inference task join error: {e}"))??,
            Err(_) => {
                return Err(
                    "Map assistant timed out waiting for the inference backend. Verify Ollama (or your OpenAI-compatible server) is running and responsive."
                        .to_string(),
                );
            }
        }
    };
    println!("OLLAMA RAW RESPONSE: {:?}", raw);
    let default_center = merged_context
        .as_ref()
        .and_then(|cj| cj.get("center"))
        .and_then(|c| c.as_array())
        .and_then(|a| {
            let lng = a.get(0).and_then(|v| v.as_f64())?;
            let lat = a.get(1).and_then(|v| v.as_f64())?;
            Some([lng, lat])
        });
    let (reply, tool_results) =
        ai_tools::extract_and_run_tools(&app, &state, &*session, default_center, &raw);
    Ok(MapAssistantResponse {
        reply,
        tool_results,
    })
}

#[tauri::command]
fn geocode_search_cmd(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    query: String,
    region_id: Option<String>,
    max_results: Option<u32>,
) -> Result<Vec<db::GeocodeMatch>, String> {
    let _u = auth::authenticated_user(&session)?;
    let fts = crate::offline_geocoder::user_query_to_fts_match(&query)
        .ok_or_else(|| "Enter a place name or address keywords.".to_string())?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let take = max_results.unwrap_or(12).clamp(1, 100) as usize;
    db.geocode_search_fts(&fts, region_id.as_deref().filter(|s| !s.is_empty()), take)
}

#[tauri::command]
fn geocode_reverse_cmd(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    lat: f64,
    lon: f64,
    region_id: Option<String>,
    max_results: Option<u32>,
) -> Result<Vec<db::GeocodeMatch>, String> {
    let _u = auth::authenticated_user(&session)?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let take = max_results.unwrap_or(8).clamp(1, 50) as usize;
    db.geocode_nearest(
        lat,
        lon,
        region_id.as_deref().filter(|s| !s.is_empty()),
        take,
    )
}

#[tauri::command]
fn geocode_rebuild_from_pbf_cmd(
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    region_id: String,
    pbf_path: String,
) -> Result<crate::offline_geocoder::GeocodeBuildStats, String> {
    let _u = auth::authenticated_user(&session)?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    crate::offline_geocoder::build_index_from_pbf(&db, &region_id, std::path::Path::new(&pbf_path))
}

#[tauri::command]
fn directions_resolve_cmd(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, auth::AuthSession>,
    from: String,
    to: String,
    mode: String,
) -> Result<crate::directions::DirectionsResolveResult, String> {
    let _u = auth::authenticated_user(&session)?;
    crate::directions::directions_resolve_impl(&app, &state, from, to, mode)
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

    let models = get_available_models(Some(&config)).map_err(|e| e.to_string())?;
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
        images: None,
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
    let paths = crate::athena_paths::AthenaPaths::new(app_data_dir);
    let modern = paths.user_config_dir().join(ERROR_LOG_FILENAME);
    if modern.exists() {
        return modern;
    }
    let legacy = app_data_dir.join(ERROR_LOG_FILENAME);
    if legacy.exists() {
        return legacy;
    }
    modern
}

#[tauri::command]
fn append_error_log(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let path = error_log_path(&app_data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
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
        let ts = c
            .timestamp_start
            .filter(|t| t.is_finite() && *t >= 0.0)
            .map(|t| format!(" [~{:.1}s]", t))
            .unwrap_or_default();
        let block = format!("\n--- From: {}{} ---\n{}\n", name, ts, body);
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
        "fruit" => "You speak like a modern consumer assistant from a polished product team: calm, clear, and warm without being cute. Prefer plain language over tactical jargon, briefing tone, or terminal theater. Keep answers concise, well-structured, and easy to skim.",
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
    let paths = crate::athena_paths::AthenaPaths::new(app_data_dir);
    let modern = paths.user_config_dir().join(KNOWLEDGE_BLOCK_FILENAME);
    if modern.exists() {
        return modern;
    }
    let legacy = app_data_dir.join(KNOWLEDGE_BLOCK_FILENAME);
    if legacy.exists() {
        return legacy;
    }
    modern
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
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
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
    fn keywords(s: &str) -> std::collections::HashSet<String> {
        s.split(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
            .filter(|w| w.len() >= 4)
            .map(|w| w.to_ascii_lowercase())
            .collect()
    }

    fn should_include_prior_context(combined: &str, message: &str) -> bool {
        // If the user is making a short follow-up or using referential language, keep context for disambiguation.
        let m = message.to_ascii_lowercase();
        let referential = [
            " it ", " that ", " this ", " those ", " these ", " they ", " them ", " he ", " she ", " above ",
            " earlier ", " previous ", " same ", " again ", "as before",
        ]
        .iter()
        .any(|tok| m.contains(tok));
        let msg_kw = keywords(message);
        // A self-contained question searches best on its own words. Earlier turns are only added
        // when the question cannot stand alone ("what about it?", "and the second one?").
        let _ = combined;
        referential || msg_kw.len() < 3 || message.trim().len() < 28
    }

    let words: Vec<&str> = combined
        .split_whitespace()
        .filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '\''))
        .take(40)
        .collect();
    let context = words.join(" ");
    if context.is_empty() || !should_include_prior_context(&context, message) {
        (message.to_string(), String::new())
    } else {
        // Current question first so it dominates keyword, embedding and reranker scoring.
        let retrieval_query = format!("{} {}", message, context);
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
    images: None,
    }];
    let start = history.len().saturating_sub(max);
    for h in &history[start..] {
        if !h.content.is_empty() || h.role == "user" {
            messages.push(ChatMessage {
                role: h.role.clone(),
                content: h.content.clone(),
            images: None,
            });
        }
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: new_user_message.to_string(),
    images: None,
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
            images: None,
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
        } else if library_rag_gate::should_skip_library_rag(&message_for_model, false) {
            (
                String::new(),
                "Answer from conversation context and general knowledge. Do not invent library citations. If the user wants a document looked up, ask them to say so clearly.",
            )
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

            let excerpt_budget = library_context::library_context_char_budget(
                model_options.map(|o| o.num_ctx),
                message_for_model.len() + mentioned_section.len(),
            );
            let keyword_semantic_section = match db.hybrid_search(retrieval_query) {
                Ok(hits) => {
                    let assembled = library_context::assemble_library_context(&hits, excerpt_budget);
                    if assembled.body.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "\n\nRelevant excerpts from the user's library (use these to answer questions about the user's documents, property, or notes when the answer appears below, and name the source you used):\n{}",
                            assembled.body
                        )
                    }
                }
                Err(e) => {
                    eprintln!("[athena] hybrid_search failed, using rank-fusion fallback: {e}");
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
                        String::new()
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

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamLibrarySource {
    pub file_path: String,
    pub display_title: String,
    pub page_number: Option<u32>,
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
    let mut roots: Vec<std::path::PathBuf> = folders.into_iter().map(std::path::PathBuf::from).collect();
    if let Ok(zdir) = kiwix_serve::zim_archives_dir(&app) {
        if !roots.iter().any(|r| r == &zdir) {
            roots.push(zdir);
        }
    }
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
    let db_path = crate::athena_paths::AthenaPaths::new(&app_data_dir).resolve_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
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
        images: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: q.to_string(),
        images: None,
        },
    ];
    let options = ModelOptions {
        temperature: 0.35,
        num_ctx: 4096,
        num_predict: None,
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
    model: Option<String>,
    images: Option<Vec<String>>,
    skip_library_rag: Option<bool>,
    conversation_mode: Option<bool>,
    retrieval_text: Option<String>,
) -> Result<(), String> {
    let session_user = auth::authenticated_user(&session)?;
    start_send_message_stream_for_user(
        app,
        session_user,
        message,
        history,
        user_lat,
        user_lon,
        chat_id,
        model_options,
        model,
        images,
        skip_library_rag.unwrap_or(false),
        conversation_mode.unwrap_or(false),
        retrieval_text,
    )
}

/// Shared by desktop IPC and the LAN bridge (LAN supplies the authenticated user explicitly).
pub(crate) fn start_send_message_stream_for_user(
    app: tauri::AppHandle,
    session_user: auth::AuthenticatedPersonnel,
    message: String,
    history: Option<Vec<HistoryMessage>>,
    user_lat: Option<f64>,
    user_lon: Option<f64>,
    chat_id: Option<String>,
    model_options: Option<ModelOptions>,
    model: Option<String>,
    images: Option<Vec<String>>,
    skip_library_rag: bool,
    conversation_mode: bool,
    retrieval_text: Option<String>,
) -> Result<(), String> {
    // What the user typed, without the instructions and context the client wraps around it.
    // Library search, the chitchat gate and `/athena` detection all run on this, never on `message`.
    let user_question = retrieval_text
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| library_context::strip_client_prompt_wrapping(&message));
    // Client hint OR heuristic gate (greetings / chitchat / short voice turns).
    let skip_library_rag = skip_library_rag
        || library_rag_gate::should_skip_library_rag(&user_question, conversation_mode);
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
    if let Err(reason) = ensure_inference_ready(&config, &models) {
        // No model can run (runtime down, no model installed, weak hardware). The station still
        // answers from the library: search needs only the small embedding and reranker models.
        if session_user.clearance_level > 0 && !skip_library_rag {
            let db_arc = Arc::clone(&app.state::<AppState>().db);
            let app_emit = app.clone();
            let question = user_question.clone();
            std::thread::spawn(move || {
                let answer = db_arc
                    .lock()
                    .ok()
                    .and_then(|db| db.hybrid_search(&question).ok())
                    .and_then(|hits| library_context::format_library_only_answer(&hits, &reason));
                match answer {
                    Some((text, sources)) => {
                        let _ = app_emit.emit(STREAM_SOURCES_EVENT, &sources);
                        let _ = app_emit.emit(STREAM_CHUNK_EVENT, text);
                    }
                    None => {
                        let _ = app_emit.emit(
                            STREAM_ERROR_EVENT,
                            format!("{reason} Nothing in your library matched this question either."),
                        );
                    }
                }
                let _ = app_emit.emit(STREAM_DONE_EVENT, ());
            });
            return Ok(());
        }
        return Err(reason);
    }

    let session_user_id = session_user.id.clone();
    let is_guest = session_user.clearance_level <= 0;
    let state = app.state::<AppState>();
    let db_arc = Arc::clone(&state.db);
    let model_opts_for_stream = model_options;
    let stream_model_override = model.clone();
    let app_emit = app.clone();
    let history_clone = history;
    let app_data_dir_clone = app_data_dir.clone();
    let chat_id_clone = chat_id.clone();
    let history_owner_id = session_user_id.clone();
    let images_clone = images
        .unwrap_or_default()
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .take(4)
        .collect::<Vec<_>>();
    std::thread::spawn(move || {
        let enrich_with_vision = |user_text: &str| -> String {
            if images_clone.is_empty() {
                return user_text.to_string();
            }
            // Prefer attaching images directly when the primary chat model can see them.
            let primary = stream_model_override
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let primary_vision = primary
                .map(moondream::name_is_vision_capable)
                .unwrap_or(false)
                || moondream::vision_capability(primary)
                    .model
                    .as_deref()
                    .map(|m| primary == Some(m))
                    .unwrap_or(false);
            if primary_vision {
                return format!(
                    "{user_text}\n\n[Visual input is attached to this message. Analyze the image(s) directly to identify objects, text, and scene context. Camera frames stay on this device and are never uploaded to the cloud.]"
                );
            }
            match moondream::describe_images_locally(&images_clone, user_text) {
                Ok(desc) => format!(
                    "{user_text}\n\n---\nLocal camera / image vision context (use this to identify objects the user is showing and answer accordingly). Frames stay on this device:\n{desc}\n---"
                ),
                Err(e) => format!(
                    "{user_text}\n\n[Camera frame was provided but local vision failed: {e}. Answer without visual identification.]"
                ),
            }
        };

        if is_guest {
            // Level 0 guest: bypass ALL RAG / vector DB / local chat context.
            let config = load_config(&app_data_dir_clone);
            let models = load_models_for_inference(&config);
            let system_content = "You are a basic survival assistant. You do not know about Project ATHENA, its personnel, or its operations. Provide general, safe advice only. Do not mention internal systems, security, or databases. If asked about ATHENA, say you don't know.";
            let guest_user = enrich_with_vision(&message);
            let mut messages = vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_content.to_string(),
                    images: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: guest_user,
                    images: None,
                },
            ];
            if !images_clone.is_empty() {
                if let Some(last) = messages.last_mut() {
                    if last.role == "user" {
                        last.images = Some(images_clone.clone());
                    }
                }
            }
            let mut on_chunk = |chunk: &str| {
                let _ = app_emit.emit(STREAM_CHUNK_EVENT, chunk);
            };
            let on_done = || {
                let _ = app_emit.emit(STREAM_DONE_EVENT, ());
            };
            if let Err(e) = chat_stream_unified(
                &config,
                &models,
                messages,
                &mut on_chunk,
                on_done,
                model_opts_for_stream,
                stream_model_override.as_deref(),
                None,
            ) {
                let _ = app_emit.emit(STREAM_ERROR_EVENT, e);
                let _ = app_emit.emit(STREAM_DONE_EVENT, ());
            }
            return;
        }
        let app_data_dir = app_data_dir_clone;
        let config = load_config(&app_data_dir);
        let history_ref = history_clone.as_deref().unwrap_or(&[]);
        let message = enrich_with_vision(&message);

        let (message_for_model, athena_ref_mode) = parse_athena_ref_message(&message);
        // `/athena` is typed at the start of the question; the wrapped `message` starts with the
        // mode prefix, so detect it on the clean question as well.
        let (user_question, athena_from_question) = parse_athena_ref_message(&user_question);
        let athena_ref_mode = athena_ref_mode || athena_from_question;

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
                            build_retrieval_from_saved_messages(&chat.messages, &user_question);
                        let lq = rq.clone();
                        (rq, cp, lq)
                    } else {
                        let summary = chat.context_summary.unwrap_or_default();
                        if !summary.trim().is_empty() {
                            let retrieval_query = format!("{} {}", summary, user_question);
                            let conversation_context_prompt = format!(
                                "\n\nConversation context (use to resolve \"it\"/\"that\" and what the user is referring to): {}",
                                summary
                            );
                            let library_lookup_query =
                                format!("{} {}", user_question, summary);
                            (retrieval_query, conversation_context_prompt, library_lookup_query)
                        } else {
                            let m = user_question.clone();
                            (m.clone(), String::new(), m)
                        }
                    }
                } else {
                    let (rq, cp) = build_retrieval_from_history(history_ref, &user_question);
                    let lq = rq.clone();
                    (rq, cp, lq)
                }
            } else {
                let (rq, cp) = build_retrieval_from_history(history_ref, &user_question);
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

            let (context_section, library_rule, stream_citations) =
                if athena_ref_mode || skip_library_rag {
                if athena_ref_mode {
                    (
                        String::new(),
                        LIBRARY_RULE_ATHENA_REF_MODE,
                        Vec::<StreamLibrarySource>::new(),
                    )
                } else {
                    // Chitchat / gate skip: no hybrid library retrieval.
                    let rule = if conversation_mode {
                        "You are Athena in a live voice conversation. Reply briefly and naturally. Do not cite library documents unless the user asked about one."
                    } else {
                        "Answer from conversation context and general knowledge. Do not invent library citations. If the user wants a document looked up, ask them to say so clearly."
                    };
                    (String::new(), rule, Vec::<StreamLibrarySource>::new())
                }
            } else {
                // Everything that competes with library excerpts for the context window.
                let memory_window = config.memory_messages.unwrap_or(20) as usize;
                let committed_chars: usize = history_ref
                    .iter()
                    .rev()
                    .take(memory_window.max(1))
                    .map(|h| h.content.len())
                    .sum::<usize>()
                    + message_for_model.len()
                    + conversation_context_prompt.len();
                let library_budget = library_context::library_context_char_budget(
                    model_opts_for_stream.map(|o| o.num_ctx),
                    committed_chars,
                );

                let mentioned = db
                    .get_indexed_files_by_name_mentioned(&format!(
                        "{} {}",
                        retrieval_query, user_question
                    ))
                    .unwrap_or_default();
                let mentioned_section = if mentioned.is_empty() {
                    String::new()
                } else {
                    let ctx = build_context_from_docs(
                        &mentioned,
                        &retrieval_query,
                        (library_budget / 2).min(4000),
                    );
                    format!(
                        "\n\nThe user referred to or asked about the following document(s); use them to answer:\n{}\n\n",
                        ctx
                    )
                };

                let has_conversation_context = retrieval_query != user_question;
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

                const LIBRARY_EXCERPTS_HEADER: &str = "\n\nRelevant excerpts from the user's library (use these to answer questions about the user's documents, property, or notes when the answer appears below, and name the source you used). Addresses and property details here are from the user's Library—cite them as such. The \"current location\" line elsewhere is from Settings for general area only.\n";
                let excerpt_budget = library_budget.saturating_sub(mentioned_section.len());
                let (keyword_semantic_section, library_citations) =
                    match db.hybrid_search(&library_lookup_query) {
                        Ok(hits) => {
                            // Hits are relevance-filtered and ranked; keep what fits the model's
                            // context window. Sources are exactly the documents that made it in.
                            let assembled =
                                library_context::assemble_library_context(&hits, excerpt_budget);
                            eprintln!(
                                "[athena] library retrieval: {} relevant chunk(s), {} block(s) in prompt, budget {} chars",
                                hits.len(),
                                assembled.blocks,
                                excerpt_budget
                            );
                            if assembled.body.is_empty() {
                                (String::new(), Vec::new())
                            } else {
                                (
                                    format!("{LIBRARY_EXCERPTS_HEADER}{}", assembled.body),
                                    assembled.sources,
                                )
                            }
                        }
                        Err(e) => {
                            // Reranker or index failure: fall back to keyword + vector rank fusion.
                            eprintln!("[athena] hybrid_search failed, using rank-fusion fallback: {e}");
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
                            if rrf_context.is_empty() {
                                (String::new(), Vec::new())
                            } else {
                                let cut = rrf_context
                                    .char_indices()
                                    .nth(excerpt_budget)
                                    .map(|(i, _)| &rrf_context[..i])
                                    .unwrap_or(rrf_context.as_str());
                                (format!("{LIBRARY_EXCERPTS_HEADER}{cut}"), cites)
                            }
                        }
                    };

                let context_section = format!("{}{}", mentioned_section, keyword_semantic_section);

                const MAX_SOURCES_EMIT: usize = 12;
                let mut stream_citations = merge_mentioned_and_library_citations(
                    &mentioned,
                    library_citations,
                    MAX_SOURCES_EMIT,
                );
                // If retrieval produced no usable context (e.g. all semantic matches fell below min similarity),
                // do not emit any sources to the frontend.
                if context_section.trim().is_empty() {
                    stream_citations.clear();
                }

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
                "You are ATHENA, a local AI assistant for {}. You have access to a local knowledge base. \
1. If the provided context is directly relevant to the user's question, use it to answer. \
2. If the context is irrelevant or absent, answer using your general knowledge, but mention that you are not using a local source for that specific info. \
If you are not using local sources, do NOT thank, cite, or credit the provided context.\n\n\
Critical rule: Do NOT guess or make up facts. {}{}{}{}{}{}{}{}{}{}",
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

            if !images_clone.is_empty() {
                if let Some(last) = messages.last_mut() {
                    if last.role == "user" {
                        last.images = Some(images_clone.clone());
                    }
                }
            }

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
        let cluster_model = inference::resolve_model_name_with_request_override(
            &config,
            &models,
            stream_model_override.as_deref(),
        );
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
            stream_model_override.as_deref(),
            None,
        ) {
            let _ = app_emit.emit(STREAM_ERROR_EVENT, e);
        }
    });
    Ok(())
}

/// Parse LAN `/api/invoke` args for `send_message_stream` and start the same stream path as desktop.
pub(crate) fn start_send_message_stream_from_lan_args(
    app: tauri::AppHandle,
    session_user: auth::AuthenticatedPersonnel,
    args: &serde_json::Value,
) -> Result<(), String> {
    let message = args
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "message required".to_string())?
        .to_string();
    let history = args
        .get("history")
        .cloned()
        .and_then(|v| serde_json::from_value::<Vec<HistoryMessage>>(v).ok());
    let user_lat = args
        .get("user_lat")
        .or_else(|| args.get("userLat"))
        .and_then(|v| v.as_f64());
    let user_lon = args
        .get("user_lon")
        .or_else(|| args.get("userLon"))
        .and_then(|v| v.as_f64());
    let chat_id = args
        .get("chat_id")
        .or_else(|| args.get("chatId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let model_options = args
        .get("model_options")
        .or_else(|| args.get("modelOptions"))
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok());
    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let images = args
        .get("images")
        .cloned()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok());
    let skip_library_rag = args
        .get("skip_library_rag")
        .or_else(|| args.get("skipLibraryRag"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let conversation_mode = args
        .get("conversation_mode")
        .or_else(|| args.get("conversationMode"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let retrieval_text = args
        .get("retrieval_text")
        .or_else(|| args.get("retrievalText"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    start_send_message_stream_for_user(
        app,
        session_user,
        message,
        history,
        user_lat,
        user_lon,
        chat_id,
        model_options,
        model,
        images,
        skip_library_rag,
        conversation_mode,
        retrieval_text,
    )
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
            #[cfg(not(mobile))]
            app.manage(kiwix_serve::KiwixServeState::default());
            let initial = diagnostics::run_startup_diagnostics(&app.handle());
            diagnostics::set_status(&app.handle(), initial);

            // Heartbeat loop: verify critical components periodically and broadcast status.
            // This intentionally does not crash the app; it only updates the resilience status.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    let mut components = std::collections::BTreeMap::<String, String>::new();
                    let sidecar_names: &[&str] = if cfg!(not(mobile)) {
                        &["pmtiles", "tesseract", "pdftoppm", "ollama", "kiwix-serve"]
                    } else {
                        &["pmtiles", "tesseract", "pdftoppm", "ollama"]
                    };
                    for name in sidecar_names {
                        let args: &[&str] = match *name {
                            "pmtiles" => &["--version"],
                            "tesseract" => &["--version"],
                            "pdftoppm" => &["-h"],
                            "ollama" => &["--version"],
                            "kiwix-serve" => &["--version"],
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
                    #[cfg(not(mobile))]
                    kiwix_serve::supervise_or_restart(&handle);
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
            // Layout v2: AthenaData/app (bundled) + AthenaData/user (installed) — migrate flat roots once.
            let athena_paths = crate::athena_paths::ensure_layout(&app_data_dir)
                .map_err(|e| format!("AthenaData layout: {e}"))?;
            // Seed app/ (and empty user model packs) from bundled Tauri resources when present.
            {
                use tauri::Manager;
                let ollama_res = app
                    .handle()
                    .path()
                    .resolve("ollama", tauri::path::BaseDirectory::Resource)
                    .ok();
                let emb_res = app
                    .handle()
                    .path()
                    .resolve("embedding", tauri::path::BaseDirectory::Resource)
                    .ok();
                let rr_res = app
                    .handle()
                    .path()
                    .resolve("reranker", tauri::path::BaseDirectory::Resource)
                    .ok();
                if let Err(e) = crate::athena_paths::seed_app_from_bundled_resources(
                    &athena_paths,
                    ollama_res.as_deref(),
                    emb_res.as_deref(),
                    rr_res.as_deref(),
                ) {
                    eprintln!("[athena] seed bundled resources into AthenaData/app: {e}");
                }
            }
            // One-time migration into unified `downloads/*` structure (best-effort, non-fatal).
            {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    if let Err(e) = tauri::async_runtime::block_on(async {
                        os::migrate_existing_downloads(h).await.map(|_| ())
                    }) {
                        eprintln!("[athena] downloads migration skipped/failed: {e}");
                    }
                });
            }
            let deployment_queue_state = std::sync::Arc::new(
                deployment_queue_state::DeploymentQueueStateManager::open_clearing_stale_session(
                    &app_data_dir,
                )?,
            );
            app.manage(deployment_queue_state);
            let model_paths = ensure_model_directories(&app_data_dir)?;
            let _ = map_manager_download::ensure_offline_maps_directory(&app_data_dir);
            let _ = map_manager_download::cleanup_stale_part_files(&app_data_dir);
            let db_path = athena_paths.resolve_db_path();
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
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
            watch_officer::spawn_background_watch(app.handle().clone());
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
            lan_chat::spawn_lan_chat_listener(app.handle().clone(), Arc::clone(&db));
            let db_for_mesh_p2p = Arc::clone(&db);
            let (watcher_tx, watcher_rx) = crossbeam_channel::unbounded();
            run_watcher_loop(Arc::clone(&db), watcher_rx, app_data_dir.clone(), app.handle().clone());
            let config = load_config(&app_data_dir);
            let folders = {
                let g = db.lock().map_err(|e| e.to_string())?;
                compute_watcher_folders(&*g, &app_data_dir, &config)?
            };
            let power_save_mode = Arc::new(AtomicBool::new(false));
            telemetry_logging::init_field_logging(&app_data_dir);
            app.manage(AppState {
                db,
                model_paths,
                watcher_tx: watcher_tx.clone(),
                active_offline_map_downloads: Arc::new(Mutex::new(HashSet::new())),
                active_package_downloads: Arc::new(Mutex::new(HashMap::new())),
                active_zim_downloads: Arc::new(Mutex::new(HashMap::new())),
                config_lock: Arc::new(Mutex::new(())),
                console_stream_cancel: Arc::new(AtomicBool::new(false)),
                managed_local_llm_http_port: model_manager::managed_llm_port_holder(),
                power_save_mode: Arc::clone(&power_save_mode),
                ingestion_queue: background_ingestion::IngestionQueue::default(),
            });
            app.manage(ptt_record::managed_state());
            let _ = watcher_tx.send(WatchCommand::SetFolders(folders));

            // Telemetry + power-save toggling (dedicated thread).
            telemetry_monitor::spawn_telemetry_monitor(
                app.handle().clone(),
                app_data_dir.clone(),
                Arc::clone(&power_save_mode),
            );
            // Gate new P2P replication work under power-save mode.
            mesh_p2p_sync::set_power_save_flag(Arc::clone(&power_save_mode));

            // OOBE: if we're using OS AppData fallback, prompt the UI to pick a field storage root.
            if !portable::portable_layout_detected() {
                let _ = app.handle().emit(
                    "athena:oobe-required",
                    serde_json::json!({
                        "reason": "portable_layout_missing",
                        "currentRoot": app_data_dir.to_string_lossy(),
                        "hint": "Select a designated storage drive for AthenaData (external SSD recommended). Restart required."
                    }),
                );
            }

            // Field laptop pruning on boot (runs off the reactor).
            {
                let db_prune = Arc::clone(&app.state::<AppState>().db);
                let root = app_data_dir.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Ok(g) = db_prune.lock() {
                        let _ = g.prune_lan_chat_messages_older_than_days(14);
                    }
                    crate::db::prune_orphan_p2p_parts_in_unsorted(&root, 24);
                });
            }

            #[cfg(not(mobile))]
            mesh_sync_api::spawn_mesh_p2p_stack(
                app.handle().clone(),
                db_for_mesh_p2p,
                app_data_dir.clone(),
            );

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
                    // Avoid `tauri::async_runtime::block_on` during setup (can panic when called from a Tokio worker).
                    // A simple TCP connect readiness gate is enough here; the legacy launcher thread below remains a fallback.
                    let http_ok = {
                        use std::net::{SocketAddr, TcpStream};
                        use std::time::Duration;
                        let addr: SocketAddr = "127.0.0.1:11434".parse().unwrap();
                        let mut ok = false;
                        for _ in 0..5 {
                            if TcpStream::connect_timeout(&addr, Duration::from_millis(350)).is_ok() {
                                ok = true;
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        ok
                    };
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
            #[cfg(not(mobile))]
            if let Err(e) = kiwix_serve::spawn_or_update(&app.handle()) {
                eprintln!("[athena] kiwix-serve: initial spawn skipped or failed: {e}");
            }
            app.manage(radio_manager::RadioState::new());
            app.manage(auth::AuthSession::default());

            if use_legacy_ollama_launch {
                let app_data_for_ollama = app_data_dir.clone();
                let config_for_ollama = config.clone();
                let app_handle_for_ollama = app.handle().clone();
                std::thread::spawn(move || {
                    let ollama_models = crate::athena_paths::AthenaPaths::new(&app_data_for_ollama)
                        .user_ollama_models_dir();
                    let _ = std::fs::create_dir_all(&ollama_models);
                    let models = Some(ollama_models.as_path());
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    if !is_ollama_running() {
                        let prefs =
                            ollama_setup::resolve_launch_paths(&app_data_for_ollama, &config_for_ollama);
                        try_launch_ollama_server_with_preferences(&prefs, models);
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
                        try_launch_ollama_server_with_preferences(&prefs, models);
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
                            try_launch_ollama_server_with_preferences(&prefs, models);
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
            diagnostics::export_diagnostics_json,
            diagnostics::get_app_data_dir_path,
            auth::authenticate_pin,
            auth::oobe_first_account_pending,
            auth::oobe_create_account,
            auth::authenticate_guest,
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
            auth::get_org_chart,
            auth::list_role_catalog,
            auth::add_role_catalog_entry,
            auth::get_personnel_profile,
            auth::list_command_notifications,
            auth::count_unacked_command_notifications,
            auth::ack_command_notification,
            auth::ack_all_command_notifications,
            inventory::list_inventory,
            runway::get_supply_runway,
            watch_officer::get_watch_brief,
            field_ops::skills::get_skills_overview,
            field_ops::skills::save_skill_entry,
            field_ops::skills::delete_skill_entry,
            field_ops::intake::list_intake_records,
            field_ops::intake::save_intake_record,
            field_ops::intake::set_intake_status,
            field_ops::intake::delete_intake_record,
            field_ops::roster::get_roster_setup,
            field_ops::roster::save_roster_duty,
            field_ops::roster::delete_roster_duty,
            field_ops::roster::set_roster_exclusion,
            field_ops::roster::generate_duty_roster,
            field_ops::ledger::list_decisions,
            field_ops::ledger::save_decision,
            field_ops::ledger::cast_decision_vote,
            field_ops::ledger::close_decision,
            field_ops::ledger::delete_decision,
            field_ops::ledger::get_ledger,
            field_ops::ledger::save_ledger_entry,
            field_ops::ledger::delete_ledger_entry,
            field_ops::checkins::list_checkin_plans,
            field_ops::checkins::save_checkin_plan,
            field_ops::checkins::record_checkin,
            field_ops::checkins::delete_checkin_plan,
            field_ops::memory::list_memory_notes,
            field_ops::memory::save_memory_note,
            field_ops::memory::delete_memory_note,
            field_ops::patients::list_patients,
            field_ops::patients::save_patient,
            field_ops::patients::delete_patient,
            field_ops::patients::list_chart_entries,
            field_ops::patients::add_chart_entry,
            field_ops::patients::delete_chart_entry,
            field_ops::needs::get_needs_overview,
            field_ops::needs::save_needs_settings,
            field_ops::needs::save_supply_profile,
            field_ops::needs::delete_supply_profile,
            field_ops::power::get_power_overview,
            field_ops::power::save_power_system,
            field_ops::power::save_power_load,
            field_ops::power::delete_power_load,
            field_ops::simple::list_field_records,
            field_ops::simple::save_field_record,
            field_ops::simple::delete_field_record,
            field_ops::radio_discipline::get_radio_discipline,
            field_ops::radio_discipline::save_silence_window,
            field_ops::radio_discipline::delete_silence_window,
            field_ops::radio_discipline::set_radio_silence,
            field_ops::trust::get_trust_settings,
            field_ops::trust::generate_group_key,
            field_ops::trust::set_group_key,
            field_ops::trust::set_trust_options,
            field_ops::courier::list_courier_groups,
            field_ops::courier::export_courier_bundle,
            field_ops::courier::import_courier_bundle,
            field_ops::sensors::list_sensors,
            field_ops::sensors::save_sensor,
            field_ops::sensors::delete_sensor,
            field_ops::sensors::add_sensor_reading,
            field_ops::picture::list_shared_picture,
            field_ops::picture::share_marker_over_mesh,
            field_ops::modem::modem_write_audio,
            field_ops::modem::modem_read_audio,
            field_ops::modem::modem_transmit,
            field_ops::modem::modem_listen,
            field_ops::modem::modem_air_time,
            field_ops::intel::get_intel,
            field_ops::intel::add_intercept,
            field_ops::intel::delete_intercept,
            field_ops::intel::set_intel_watch_words,
            field_ops::intel::transcribe_intercept_recording,
            inventory::add_inventory_item,
            inventory::update_inventory_quantity,
            inventory::update_inventory_category,
            inventory::list_inventory_log,
            inventory::edit_log_fingerprint,
            inventory::apply_inventory_proposal,
            inventory::rollback_inventory_change,
            inventory::update_inventory_directorate,
            inv_queue_count,
            logistics_library_list_entries,
            logistics_library_submit_message,
            reject_inventory_proposal,
            hardware_map::check_hardware_status,
            remote_queue::get_queue_status,
            remote_queue::enqueue_request,
            scan_chat_models,
            get_downloaded_models,
            scan_local_device_for_models,
            start_managed_local_llm_server,
            get_managed_local_llm_http_base_url,
            mesh_p2p_list_peers,
            mesh_p2p_sync_with_peer,
            mesh_p2p_manifest_diff,
            mesh_p2p_replicate_from_peer,
            scan_embedding_models,
            scan_reranker_models,
            get_watch_folder,
            set_watch_folder,
            open_path_in_file_manager,
            os::reveal_downloads_folder,
            os::migrate_existing_downloads,
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
            get_connectivity_status,
            set_connectivity_mode,
            marketplace_overview,
            marketplace_install,
            marketplace_recreate,
            marketplace_set_auto_update,
            marketplace_tick_auto_updates,
            marketplace_start,
            marketplace_stop,
            marketplace_open_url,
            marketplace_health,
            marketplace_docker_status,
            marketplace_docker_volume_bytes,
            set_explicit_ollama_model,
            set_hive_mind_seed_url,
            get_hive_mind_route_status,
            set_active_embedding_model,
            set_active_reranker_model,
            set_openai_compatible_base_url,
            set_openai_compatible_api_key,
            set_openai_compatible_model,
            set_allow_nsfw_unrestricted_models,
            probe_openai_compatible_connection,
            is_inference_ready,
            moondream_describe_image,
            describe_vision_frames,
            get_vision_capability,
            pull_ollama_model,
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
            storage::check_available_storage,
            deployment::deployment_check_disk_space,
            deployment::deployment_scan_files,
            deployment::deployment_start_downloads,
            deployment::deployment_pause_downloads,
            deployment::deployment_is_running,
            deployment::deployment_get_queue_snapshot,
            deployment::deployment_probe_remote_sizes,
            deployment::deployment_open_athena_data_folder,
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
            get_map_display_settings,
            set_map_display_settings,
            map_assistant_send,
            saved_places::list_saved_places,
            saved_places::add_saved_place,
            saved_places::delete_saved_place,
            blockages::list_route_blockages,
            blockages::propose_route_blockage,
            blockages::review_route_blockage,
            territories::get_territories,
            territories::upsert_territory,
            territories::delete_territory,
            ai_staging::ai_stage_change,
            ai_staging::ai_list_staged,
            ai_staging::ai_count_pending,
            ai_staging::ai_review_change,
            ai_staging::ai_get_autoapprove_rules,
            ai_staging::ai_set_autoapprove_rule,
            ai_staging::ai_delete_autoapprove_rule,
            valhalla_service::valhalla_get_status,
            valhalla_service::valhalla_route,
            valhalla_service::map_offline_route_geojson,
            valhalla_service::valhalla_list_regions,
            valhalla_service::valhalla_set_active_region,
            valhalla_service::valhalla_register_routing_region,
            geocode_search_cmd,
            geocode_reverse_cmd,
            geocode_rebuild_from_pbf_cmd,
            directions_resolve_cmd,
            valhalla_lifecycle::valhalla_docker_start,
            valhalla_lifecycle::valhalla_docker_stop,
            routing::start_valhalla_engine,
            routing::stop_valhalla_engine,
            geofabrik_downloader::geofabrik_download_regions_within_radius,
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
            commands::chat_console::abort_chat_console_stream,
            export_athena_to_usb,
            cancel_aegis_export,
            aegis_export_preflight,
            start_aegis_export_preflight,
            lan_bridge::get_lan_bridge_status,
            lan_bridge::set_lan_bridge_enabled,
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
            academy_list_pdf_units,
            zim_download::download_zim,
            calendar_list_events,
            calendar_upsert_event,
            calendar_delete_event,
            calendar_request_mesh_sync,
            rag_indexer::ingest_local_document,
            rag_indexer::search_rag_chunks,
            rag_indexer::test_library_search,
            rag_indexer::download_knowledge_package,
            rag_indexer::list_rag_documents,
            media_harvest::harvest_media_url,
            media_harvest::harvest_local_documents,
            media_harvest::resolve_media_harvest_dir,
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
            sdr_manager::sdr_set_frequency,
            lan_chat::lan_chat_list_messages,
            lan_chat::lan_chat_send_message,
            portable::oobe_set_athena_data_root,
            ptt_record::start_ptt_recording,
            ptt_record::stop_ptt_recording,
            ptt_record::stop_ptt_and_transcribe,
            ptt_record::cancel_ptt_recording,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                #[cfg(not(mobile))]
                mesh_sync_api::signal_mesh_p2p_shutdown();
                kill_ollama_sidecar_if_running(app_handle);
                model_manager::shutdown_managed_local_inference();
                #[cfg(not(mobile))]
                kill_kiwix_serve_sidecar_if_running(app_handle);
                #[cfg(not(mobile))]
                {
                    let app = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        // If the UI started Valhalla via `docker compose up -d`, request a bounded shutdown on exit.
                        let _ = routing::stop_valhalla_engine(app).await;
                    });
                }
                lan_chat::stop_lan_chat_listener();
                sdr_manager::stop_sdr_internal(app_handle);
                if let Some(st) = app_handle.try_state::<std::sync::Arc<ptt_record::PttRecordState>>() {
                    st.request_stop();
                }
            }
        });
}
