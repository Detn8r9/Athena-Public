//! LAN HTTP bridge: serve the same Athena UI (`dist/`) and expose `invoke`-style RPC for browsers.
//! Desktop continues to use Tauri IPC; LAN clients use POST `/api/invoke` + SSE `/api/events`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Listener, Manager};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::auth::{session_personnel_from_row, status_is_locked, AuthenticatedPersonnel};
use crate::chats::SavedMessage;
use crate::config::{load_config, save_config};
use crate::db::personnel_pin_hash;
use crate::diagnostics;
use crate::maps;
use crate::portable;
use crate::tile_pack;
use crate::AppState;

#[path = "handheld.rs"]
pub(crate) mod handheld;

/// One open `/api/events` stream. `token` is the browser's session when it connected signed in.
struct SseClient {
    token: Option<String>,
    tx: mpsc::UnboundedSender<String>,
}

/// Fan-out channel for SSE clients (JSON lines for `athena` events).
static SSE_CLIENTS: OnceLock<Mutex<Vec<SseClient>>> = OnceLock::new();

fn sse_clients() -> &'static Mutex<Vec<SseClient>> {
    SSE_CLIENTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Station-wide events go to every signed-in browser. Signed-out browsers get only heartbeats.
fn publish_sse_event(event: &str, payload: Value) {
    let data = json!({ "event": event, "payload": payload }).to_string();
    if let Ok(mut clients) = sse_clients().lock() {
        clients.retain(|c| c.token.is_none() || c.tx.send(data.clone()).is_ok());
    }
}

/// An event for one browser session only, such as the chat reply it asked for.
pub fn publish_to_session(token: &str, event: &str, payload: Value) {
    let data = json!({ "event": event, "payload": payload }).to_string();
    if let Ok(mut clients) = sse_clients().lock() {
        clients.retain(|c| c.token.as_deref() != Some(token) || c.tx.send(data.clone()).is_ok());
    }
}

/// Signed-in browser sessions, shared by every start of the bridge and by command identity lookups.
static LAN_SESSIONS: OnceLock<Arc<Mutex<HashMap<String, AuthenticatedPersonnel>>>> = OnceLock::new();

fn lan_sessions() -> Arc<Mutex<HashMap<String, AuthenticatedPersonnel>>> {
    LAN_SESSIONS.get_or_init(|| Arc::new(Mutex::new(HashMap::new()))).clone()
}

fn resolve_lan_token(token: &str) -> Option<AuthenticatedPersonnel> {
    lan_sessions().lock().ok()?.get(token).cloned()
}

/// A command changed a user's own profile: refresh every browser session of that user.
fn update_lan_sessions(user: &AuthenticatedPersonnel) {
    if let Ok(mut sessions) = lan_sessions().lock() {
        for u in sessions.values_mut().filter(|u| u.id == user.id) {
            *u = user.clone();
        }
    }
}

/// Station-wide events forwarded to signed-in browsers. Chat replies are not here: they go only to
/// the browser that asked (see [`publish_to_session`]).
const FORWARDED_EVENTS: &[&str] = &[
    "athena:notification", "athena:ai-inference-sleep-changed", "athena:archive-fs-changed",
    "athena:calendar-updated", "athena:harvest-progress", "athena:ingestion-queue", "athena:kiwix-done",
    "athena:kiwix-error", "athena:kiwix-progress", "athena:lan-chat-updated", "athena:lan-cluster-updated",
    "athena:logbook-updated", "athena:marketplace-install-progress", "athena:mesh-chats-updated",
    "athena:mesh-connection", "athena:mesh-network-chat-updated", "athena:mesh-text",
    "athena:offline-maps-updated", "athena:ollama-pull-done", "athena:power-save-mode",
    "athena:territories-updated", "athena:vault-progress", "download-progress", "geofabrik-download-progress", "map_download_complete",
    "map_download_error", "map_download_progress", "mesh-message-ack", "mesh-connection",
    "package_download_cancelled", "package_download_complete", "package_download_error",
    "package_download_progress", "peer-blockage-rx", "peer-marker-rx", "peer-saved-place-rx",
    "peer_discovered", "peer_lost", "pmtiles_region_extract_complete", "pmtiles_region_extract_error",
    "pmtiles_region_extract_progress", "radio-rx", "radio-telemetry", "rag-ingestion-progress", "road-router-progress",
    "satellite_region_download_complete", "satellite_region_download_error",
    "satellite_region_download_progress", "zim_download_complete", "zim_download_error",
    "zim_download_progress",
];

fn wire_tauri_events_to_sse(app: &AppHandle) {
    static WIRED: AtomicBool = AtomicBool::new(false);
    if WIRED.swap(true, Ordering::SeqCst) {
        return;
    }
    for name in FORWARDED_EVENTS {
        let event = name.to_string();
        let _ = app.listen_any(*name, move |e| {
            let raw = e.payload().to_string();
            let payload = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
            publish_sse_event(&event, payload);
        });
    }
}

pub const DEFAULT_LAN_PORT: u16 = 8787;

static LAN_ENABLED: AtomicBool = AtomicBool::new(false);
static LAN_CANCEL: Mutex<Option<CancellationToken>> = Mutex::new(None);
static LAN_PORT: Mutex<u16> = Mutex::new(DEFAULT_LAN_PORT);
/// Open SSE (/api/events) connections — approx. browsers attached to the bridge.
static CONNECTED_CLIENTS: AtomicUsize = AtomicUsize::new(0);
/// Logged-in LAN sessions (PIN / guest tokens).
static AUTH_SESSIONS: AtomicUsize = AtomicUsize::new(0);

struct SseClientGuard;
impl Drop for SseClientGuard {
    fn drop(&mut self) {
        CONNECTED_CLIENTS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct LanHttpState {
    app: AppHandle,
    sessions: Arc<Mutex<HashMap<String, AuthenticatedPersonnel>>>,
}

#[derive(Debug, Deserialize)]
struct InvokeBody {
    cmd: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct InvokeOk {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanBridgeStatus {
    pub enabled: bool,
    pub port: u16,
    pub listening: bool,
    pub dist_ready: bool,
    pub dist_path: String,
    /// Best-effort IPv4 on the default route (for “open on phone” URLs).
    pub primary_lan_ip: Option<String>,
    /// Additional non-loopback IPv4 addresses when discoverable.
    pub lan_ips: Vec<String>,
    /// Browsers with an open events stream to this bridge.
    pub connected_clients: u32,
    /// Active PIN/guest sessions over LAN.
    pub authenticated_sessions: u32,
}

/// Prefer the IPv4 used for outbound traffic (typical Wi‑Fi / Ethernet LAN address).
fn primary_lan_ipv4() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // No packets are sent; connect selects the interface for that route.
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => {
            Some(ip.to_string())
        }
        _ => None,
    }
}

fn collect_lan_ipv4s() -> Vec<String> {
    let mut ips = Vec::new();
    if let Some(primary) = primary_lan_ipv4() {
        ips.push(primary);
    }
    ips
}

fn host_only_blocked(cmd: &str) -> bool {
    matches!(
        cmd,
        "export_athena_to_usb"
            | "cancel_aegis_export"
            | "aegis_export_preflight"
            | "start_aegis_export_preflight"
            | "get_available_usbs"
            | "open_library_source"
            | "set_models_dir"
            | "save"
            | "open"
            | "marketplace_install"
            | "marketplace_start"
            | "marketplace_stop"
            | "marketplace_open_url"
    )
}

/// Wave-1 LAN commands required for phone post-login parity (auth/chat/themes/map read).
#[cfg(test)]
const LAN_WAVE1_EXPOSED: &[&str] = &[
    "authenticate_pin",
    "authenticate_guest",
    "logout",
    "refresh_auth_session",
    "list_chats",
    "get_chat",
    "save_chat",
    "send_message_stream",
    "get_theme_config",
    "set_theme_config",
    "get_feature_map_enabled",
    "get_pois",
    "get_map_markers",
    "offline_basemap_paths_cmd",
    "list_personnel",
];

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            let s = s.trim();
            s.strip_prefix("Bearer ")
                .or_else(|| s.strip_prefix("bearer "))
                .map(|t| t.trim().to_string())
        })
}

fn resolve_dist_dir(app: &AppHandle) -> PathBuf {
    // Prefer sibling `athena/dist` when running from source / tauri dev cwd.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let candidates = [
        cwd.join("dist"),
        cwd.join("../dist"),
        cwd.join("athena/dist"),
        app.path()
            .resource_dir()
            .ok()
            .map(|p| p.join("../dist"))
            .unwrap_or_default(),
    ];
    for c in candidates {
        if c.join("index.html").is_file() {
            return c;
        }
    }
    cwd.join("dist")
}

fn arg_key_alts(key: &str) -> Vec<String> {
    let mut keys = vec![key.to_string()];
    if key.contains('_') {
        let camel: String = key
            .split('_')
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    p.to_string()
                } else {
                    let mut c = p.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        None => String::new(),
                    }
                }
            })
            .collect();
        keys.push(camel);
    }
    keys
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    for k in arg_key_alts(key) {
        if let Some(s) = args.get(&k).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn arg_bool(args: &Value, key: &str) -> Option<bool> {
    for k in arg_key_alts(key) {
        if let Some(v) = args.get(&k) {
            if let Some(b) = v.as_bool() {
                return Some(b);
            }
        }
    }
    None
}

fn arg_value<'a>(args: &'a Value, key: &str) -> Option<&'a Value> {
    for k in arg_key_alts(key) {
        if let Some(v) = args.get(&k) {
            return Some(v);
        }
    }
    None
}

fn dispatch_invoke(
    state: &LanHttpState,
    cmd: &str,
    args: &Value,
    token: Option<&str>,
) -> Result<(Value, Option<String>), String> {
    if host_only_blocked(cmd) {
        return Err(
            "This action requires the Athena desktop host (USB / native dialogs)."
                .to_string(),
        );
    }

    let app_state = state
        .app
        .try_state::<AppState>()
        .ok_or_else(|| "App state unavailable.".to_string())?;

    match cmd {
        "get_resilience_status" => {
            let s = diagnostics::get_resilience_status(state.app.clone())?;
            Ok((serde_json::to_value(s).map_err(|e| e.to_string())?, None))
        }
        "authenticate_pin" => {
            let name = arg_str(args, "name").ok_or_else(|| "name required".to_string())?;
            let pin = arg_str(args, "pin").ok_or_else(|| "pin required".to_string())?;
            pin_attempts::check(&name)?;
            let out = {
                let db = app_state
                    .db
                    .lock()
                    .map_err(|_| "Database is busy. Try again in a moment.".to_string())?;
                let row = db
                    .personnel_fetch_by_name(&name)?
                    .ok_or_else(|| "Unknown operator name.".to_string())?;
                if personnel_pin_hash(&pin) != row.pin_hash {
                    return Err(pin_attempts::failed(&name));
                }
                pin_attempts::succeeded(&name);
                if status_is_locked(&row.operational_status) {
                    return Err("This account is locked (MIA/KIA). Contact Command.".to_string());
                }
                session_personnel_from_row(&row)
            };
            let tok = Uuid::new_v4().to_string();
            {
                let mut sessions = state
                    .sessions
                    .lock()
                    .map_err(|e| e.to_string())?;
                sessions.insert(tok.clone(), out.clone());
                AUTH_SESSIONS.store(sessions.len(), Ordering::SeqCst);
            }
            Ok((serde_json::to_value(out).map_err(|e| e.to_string())?, Some(tok)))
        }
        "authenticate_guest" => {
            let out = AuthenticatedPersonnel {
                id: "guest".to_string(),
                name: "Guest".to_string(),
                division: "GUEST".to_string(),
                rank_level: 0,
                clearance_level: 0,
                display_name: None,
                operational_status: "active".to_string(),
                directorate: None,
                tier: None,
                job_title: None,
                athena_architect: false,
                domain: None,
                role_l1: None,
                role_l2: None,
                role_l3: None,
                role_l4: None,
            };
            let tok = Uuid::new_v4().to_string();
            {
                let mut sessions = state
                    .sessions
                    .lock()
                    .map_err(|e| e.to_string())?;
                sessions.insert(tok.clone(), out.clone());
                AUTH_SESSIONS.store(sessions.len(), Ordering::SeqCst);
            }
            Ok((serde_json::to_value(out).map_err(|e| e.to_string())?, Some(tok)))
        }
        "logout" => {
            if let Some(t) = token {
                if let Ok(mut sessions) = state.sessions.lock() {
                    sessions.remove(t);
                    AUTH_SESSIONS.store(sessions.len(), Ordering::SeqCst);
                }
            }
            Ok((Value::Null, None))
        }
        "refresh_auth_session" => {
            let t = token.ok_or_else(|| "Not authenticated.".to_string())?;
            let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
            let user = sessions
                .get(t)
                .cloned()
                .ok_or_else(|| "Not authenticated.".to_string())?;
            Ok((serde_json::to_value(user).map_err(|e| e.to_string())?, None))
        }
        "get_theme_config" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let cfg = load_config(&app_data);
            let payload = json!([
                cfg.theme_id,
                cfg.personality_id,
                cfg.custom_themes,
                cfg.theme_appearance
            ]);
            Ok((payload, None))
        }
        "set_theme_config" => {
            require_session(state, token)?;
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let mut cfg = load_config(&app_data);
            if let Some(tid) = arg_str(args, "theme_id").or_else(|| arg_str(args, "themeId")) {
                cfg.theme_id = if tid.is_empty() { None } else { Some(tid) };
            }
            if let Some(pid) =
                arg_str(args, "personality_id").or_else(|| arg_str(args, "personalityId"))
            {
                cfg.personality_id = if pid.is_empty() { None } else { Some(pid) };
            }
            if let Some(appearance) =
                arg_str(args, "theme_appearance").or_else(|| arg_str(args, "themeAppearance"))
            {
                cfg.theme_appearance = if appearance.is_empty() {
                    None
                } else {
                    Some(appearance)
                };
            }
            save_config(&app_data, &cfg)?;
            Ok((Value::Null, None))
        }
        "get_feature_map_enabled" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let cfg = load_config(&app_data);
            Ok((json!(cfg.feature_map_enabled.unwrap_or(false)), None))
        }
        "set_feature_map_enabled" => {
            require_session(state, token)?;
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let mut cfg = load_config(&app_data);
            let value = arg_bool(args, "value").unwrap_or(false);
            cfg.feature_map_enabled = Some(value);
            save_config(&app_data, &cfg)?;
            Ok((Value::Null, None))
        }
        "get_user_and_inference_settings" => {
            require_session(state, token)?;
            let payload =
                crate::lan_get_user_and_inference_settings_value(state.app.clone())?;
            Ok((payload, None))
        }
        "set_user_display_name" => {
            require_session(state, token)?;
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let mut cfg = load_config(&app_data);
            cfg.user_display_name = arg_str(args, "value").and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            });
            save_config(&app_data, &cfg)?;
            Ok((Value::Null, None))
        }
        "set_user_birthdate_iso" => {
            require_session(state, token)?;
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let mut cfg = load_config(&app_data);
            cfg.user_birthdate_iso = arg_str(args, "value").and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            });
            save_config(&app_data, &cfg)?;
            Ok((Value::Null, None))
        }
        "get_lan_bridge_status" => {
            let st = lan_bridge_status_inner(&state.app);
            Ok((serde_json::to_value(st).map_err(|e| e.to_string())?, None))
        }
        "list_chats" => {
            let user = require_session(state, token)?;
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            let list = db.local_chat_list_summaries(&user.id)?;
            Ok((serde_json::to_value(list).map_err(|e| e.to_string())?, None))
        }
        "get_chat" => {
            let user = require_session(state, token)?;
            let id = arg_str(args, "id").ok_or_else(|| "id required".to_string())?;
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            let chat = db.local_chat_get(&id, &user.id)?;
            Ok((serde_json::to_value(chat).map_err(|e| e.to_string())?, None))
        }
        "save_chat" => {
            let user = require_session(state, token)?;
            let title = arg_str(args, "title").unwrap_or_else(|| "New chat".to_string());
            let id = arg_str(args, "id");
            let description = arg_str(args, "description");
            let messages: Vec<SavedMessage> = arg_value(args, "messages")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            let chat_id = db.local_chat_save(&user.id, id, title, messages, description)?;
            Ok((json!(chat_id), None))
        }
        "delete_chat" => {
            let user = require_session(state, token)?;
            let id = arg_str(args, "id").ok_or_else(|| "id required".to_string())?;
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            db.local_chat_delete(&user.id, &id)?;
            Ok((Value::Null, None))
        }
        "update_chat_metadata" => {
            let user = require_session(state, token)?;
            let id = arg_str(args, "id").ok_or_else(|| "id required".to_string())?;
            let title = arg_str(args, "title");
            let description = arg_str(args, "description");
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            db.local_chat_update_metadata(&user.id, &id, title, description)?;
            Ok((Value::Null, None))
        }
        "send_message_stream" => {
            let user = require_session(state, token)?;
            crate::start_send_message_stream_from_lan_args(state.app.clone(), user, args, token.unwrap_or_default().to_string())?;
            Ok((Value::Null, None))
        }
        "get_enabled_main_map_raster_archive_file_names" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let cfg = load_config(&app_data);
            Ok((
                json!(cfg.enabled_main_map_raster_archive_file_names),
                None,
            ))
        }
        "offline_basemap_paths_cmd" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let paths = tile_pack::offline_basemap_paths(&app_data);
            Ok((serde_json::to_value(paths).map_err(|e| e.to_string())?, None))
        }
        "get_pois" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let pois = maps::get_pois(&app_data);
            Ok((serde_json::to_value(pois).map_err(|e| e.to_string())?, None))
        }
        "get_map_markers" => {
            require_session(state, token)?;
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            let markers = db.list_map_markers()?;
            Ok((serde_json::to_value(markers).map_err(|e| e.to_string())?, None))
        }
        "list_personnel" => {
            let user = require_session(state, token)?;
            if user.rank_level < 4 {
                return Err("Insufficient rank for personnel administration.".to_string());
            }
            let db = app_state.db.lock().map_err(|e| e.to_string())?;
            let list = db.personnel_list_public()?;
            Ok((serde_json::to_value(list).map_err(|e| e.to_string())?, None))
        }
        "append_error_log" => {
            // Best-effort; desktop command owns rotation. LAN clients should not fail on log write.
            if let Some(msg) = arg_str(args, "message") {
                if let Ok(app_data) = portable::resolved_app_data_dir(&state.app) {
                    let path = app_data.join("athena_error_log.txt");
                    let _ = std::fs::create_dir_all(&app_data);
                    let line = format!(
                        "[{}] {}\n",
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        msg.replace('\n', " ")
                    );
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .and_then(|mut f| {
                            use std::io::Write;
                            f.write_all(line.as_bytes())
                        });
                }
            }
            Ok((Value::Null, None))
        }
        "get_error_log" => {
            let log = portable::resolved_app_data_dir(&state.app)
                .ok()
                .and_then(|d| std::fs::read_to_string(d.join("athena_error_log.txt")).ok())
                .unwrap_or_default();
            Ok((json!(log), None))
        }
        // Lock-screen / ambient probes — fast stubs so login is never blocked by host-only work.
        "get_device_stats" => Ok((
            json!({
                "cpu_usage_pct": 0.0,
                "ram_used_mb": 0,
                "ram_total_mb": 0,
                "swap_used_mb": 0,
                "swap_total_mb": 0,
                "disk_used_gb": 0.0,
                "disk_total_gb": 0.0,
                "net_rx_bytes": 0,
                "net_tx_bytes": 0,
                "disk_read_bytes_total": 0,
                "disk_write_bytes_total": 0,
                "logical_cpus": 0,
                "physical_cores": null,
                "uptime_secs": 0,
                "load_one": 0.0,
                "load_five": 0.0,
                "load_fifteen": 0.0,
                "host_name": null,
                "os_name": "LAN client",
                "cpu_temp_c": null
            }),
            None,
        )),
        "get_mesh_radio_identity" => Ok((
            json!({
                "connected": false,
                "nodeId": 0,
                "displayName": ""
            }),
            None,
        )),
        "get_ai_inference_sleep" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let cfg = load_config(&app_data);
            Ok((json!(cfg.ai_inference_sleep.unwrap_or(false)), None))
        }
        "set_ai_inference_sleep" => {
            require_session(state, token)?;
            require_session(state, token)?;
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let mut cfg = load_config(&app_data);
            cfg.ai_inference_sleep = Some(arg_bool(args, "value").unwrap_or(false));
            save_config(&app_data, &cfg)?;
            Ok((Value::Null, None))
        }
        "lan_presence_ping" => {
            Ok((
                json!({
                    "connectedClients": CONNECTED_CLIENTS.load(Ordering::SeqCst),
                    "authenticatedSessions": AUTH_SESSIONS.load(Ordering::SeqCst)
                }),
                None,
            ))
        }
        "marketplace_overview" => {
            let ov = crate::marketplace::overview(&state.app)?;
            Ok((serde_json::to_value(ov).map_err(|e| e.to_string())?, None))
        }
        "marketplace_docker_status" => {
            Ok((
                serde_json::to_value(crate::marketplace::docker_available()).map_err(|e| e.to_string())?,
                None,
            ))
        }
        other => forward_command(state, other, args, token),
    }
}

/// Commands a browser may not run through the bridge: they change the station's own sign-in,
/// window or bridge, or need the host's file dialogs.
const NOT_FORWARDED: &[&str] = &[
    "dev_bridge_reply",
    "enter_station_minimal_mode",
    "set_lan_bridge_enabled",
];

/// How long the bridge waits for a command. Packing, scanning and downloads report progress by
/// event, but their command only returns when the job ends.
const FORWARD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Run any other app command for a signed-in browser, exactly as the desktop window would, under
/// that browser's identity (see `auth::with_request_user` and `auth::CallerSession`).
fn forward_command(
    state: &LanHttpState,
    cmd: &str,
    args: &Value,
    token: Option<&str>,
) -> Result<(Value, Option<String>), String> {
    if cmd.starts_with("plugin:") || NOT_FORWARDED.contains(&cmd) {
        return Err("This action is only available on the station itself.".to_string());
    }
    // Status checks the sign-in screen makes before anyone is signed in. They read no user data.
    const PUBLIC: &[&str] = &["is_ollama_available", "is_inference_ready"];
    let user = match require_session(state, token) {
        Ok(u) => Some(u),
        Err(_) if PUBLIC.contains(&cmd) => None,
        Err(_) => return Err("Not authenticated.".to_string()),
    };
    let token = token.unwrap_or_default();
    let webview = state
        .app
        .get_webview_window("main")
        .ok_or_else(|| "The station is still starting. Try again in a moment.".to_string())?;
    let url = webview.url().map_err(|e| e.to_string())?;
    let mut headers = tauri::http::HeaderMap::new();
    headers.insert(
        crate::auth::LAN_SESSION_HEADER,
        tauri::http::HeaderValue::from_str(token).map_err(|e| e.to_string())?,
    );
    let request = tauri::webview::InvokeRequest {
        cmd: cmd.to_string(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url,
        body: tauri::ipc::InvokeBody::Json(with_camel_case_keys(args)),
        headers,
        invoke_key: state.app.invoke_key().to_string(),
    };
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let target: tauri::Webview = webview.as_ref().clone();
    let dispatch = move || {
        target.on_message(
            request,
            Box::new(move |_webview, _cmd, response, _callback, _error| {
                let _ = tx.send(response);
            }),
        );
    };
    match user {
        Some(u) => crate::auth::with_request_user(u, dispatch),
        None => dispatch(),
    }
    let response = rx
        .recv_timeout(FORWARD_TIMEOUT)
        .map_err(|_| format!("`{cmd}` did not answer."))?;
    match response {
        tauri::ipc::InvokeResponse::Ok(tauri::ipc::InvokeResponseBody::Json(text)) => {
            Ok((serde_json::from_str(&text).unwrap_or(Value::Null), None))
        }
        tauri::ipc::InvokeResponse::Ok(tauri::ipc::InvokeResponseBody::Raw(bytes)) => Ok((json!(bytes), None)),
        tauri::ipc::InvokeResponse::Err(tauri::ipc::InvokeError(v)) => Err(match v {
            Value::String(s) => s,
            other => other.to_string(),
        }),
    }
}

/// Tauri matches arguments by camelCase name; browsers may send snake_case. Send both, as the
/// desktop client does.
fn with_camel_case_keys(args: &Value) -> Value {
    let Some(map) = args.as_object() else {
        return if args.is_null() { json!({}) } else { args.clone() };
    };
    let mut out = map.clone();
    for (k, v) in map {
        if !k.contains('_') {
            continue;
        }
        let mut camel = String::with_capacity(k.len());
        let mut upper = false;
        for ch in k.chars() {
            if ch == '_' {
                upper = true;
            } else if upper {
                camel.extend(ch.to_uppercase());
                upper = false;
            } else {
                camel.push(ch);
            }
        }
        out.entry(camel).or_insert_with(|| v.clone());
    }
    Value::Object(out)
}

/// Stops PIN guessing from the network: after 5 wrong PINs for a name, that name is locked out of
/// browser sign-in for 5 minutes. Each wrong PIN also costs a second.
mod pin_attempts {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const MAX_FAILURES: u32 = 5;
    const LOCKOUT: Duration = Duration::from_secs(5 * 60);
    static FAILURES: Mutex<Option<HashMap<String, (u32, Instant)>>> = Mutex::new(None);

    fn key(name: &str) -> String {
        name.trim().to_lowercase()
    }

    pub fn check(name: &str) -> Result<(), String> {
        let mut g = FAILURES.lock().map_err(|e| e.to_string())?;
        let map = g.get_or_insert_with(HashMap::new);
        if let Some((count, last)) = map.get(&key(name)).copied() {
            if last.elapsed() >= LOCKOUT {
                map.remove(&key(name));
            } else if count >= MAX_FAILURES {
                let wait = (LOCKOUT - last.elapsed()).as_secs() / 60 + 1;
                return Err(format!("Too many wrong PINs. Try again in {wait} minute(s)."));
            }
        }
        Ok(())
    }

    pub fn failed(name: &str) -> String {
        if let Ok(mut g) = FAILURES.lock() {
            let entry = g.get_or_insert_with(HashMap::new).entry(key(name)).or_insert((0, Instant::now()));
            entry.0 += 1;
            entry.1 = Instant::now();
        }
        std::thread::sleep(Duration::from_secs(1));
        "Invalid PIN.".to_string()
    }

    pub fn succeeded(name: &str) {
        if let Ok(mut g) = FAILURES.lock() {
            if let Some(map) = g.as_mut() {
                map.remove(&key(name));
            }
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn locks_after_five_wrong_pins() {
            let n = "lockout-test-user";
            for _ in 0..5 {
                assert!(super::check(n).is_ok());
                super::failed(n);
            }
            assert!(super::check(n).unwrap_err().contains("Too many"));
            super::succeeded(n);
            assert!(super::check(n).is_ok());
        }
    }
}

fn require_session(
    state: &LanHttpState,
    token: Option<&str>,
) -> Result<AuthenticatedPersonnel, String> {
    let t = token.ok_or_else(|| "Not authenticated.".to_string())?;
    state
        .sessions
        .lock()
        .map_err(|e| e.to_string())?
        .get(t)
        .cloned()
        .ok_or_else(|| "Not authenticated.".to_string())
}

async fn invoke_handler(
    State(state): State<LanHttpState>,
    headers: HeaderMap,
    Json(body): Json<InvokeBody>,
) -> impl IntoResponse {
    let token = bearer_token(&headers);
    let cmd = body.cmd.trim().to_string();
    let args = body.args;
    // Keep heavy/sync work off the async runtime so login isn't starved by other probes.
    let result = tokio::task::spawn_blocking(move || {
        dispatch_invoke(&state, &cmd, &args, token.as_deref())
    })
    .await
    .unwrap_or_else(|e| Err(format!("LAN invoke task failed: {e}")));

    match result {
        Ok((result, new_token)) => (
            StatusCode::OK,
            Json(InvokeOk {
                ok: true,
                result: Some(result),
                error: None,
                token: new_token,
            }),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(InvokeOk {
                ok: false,
                result: None,
                error: Some(e),
                token: None,
            }),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[allow(dead_code)]
    event: Option<String>,
    token: Option<String>,
}

async fn events_handler(
    State(state): State<LanHttpState>,
    Query(q): Query<EventsQuery>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let token = q
        .token
        .filter(|t| state.sessions.lock().map(|m| m.contains_key(t)).unwrap_or(false));
    CONNECTED_CLIENTS.fetch_add(1, Ordering::SeqCst);
    let guard = SseClientGuard;
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    if let Ok(mut clients) = sse_clients().lock() {
        clients.push(SseClient { token, tx });
    }
    let stream = stream::unfold((0u32, guard, rx), |(n, guard, mut rx)| async move {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(data) => {
                        let ev = Event::default().event("athena").data(data);
                        Some((Ok(ev), (n, guard, rx)))
                    }
                    None => None,
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                let ev = Event::default()
                    .event("athena")
                    .data(
                        json!({
                            "event": "lan:heartbeat",
                            "payload": {
                                "n": n,
                                "connectedClients": CONNECTED_CLIENTS.load(Ordering::SeqCst),
                                "authenticatedSessions": AUTH_SESSIONS.load(Ordering::SeqCst)
                            }
                        })
                        .to_string(),
                    );
                Some((Ok(ev), (n.wrapping_add(1), guard, rx)))
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Serve a web file built into the executable; unknown paths get the app page (client routing).
/// Build files under `/assets/` have the content hash in their name: cache them for good. The app
/// page must be checked every time, or a browser keeps an old index.html that points at chunks a
/// station update deleted ("'text/html' is not a valid JavaScript MIME type").
async fn web_cache_headers(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    if path.starts_with("/api/") || path.starts_with("/maps/") || !res.status().is_success() {
        return res;
    }
    let policy = if path.starts_with("/assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    res.headers_mut().insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static(policy));
    res
}

/// True when a request for a build file (`/assets/…`, not itself HTML) got the app page instead.
fn is_index_fallback_for_build_file(path: &str, mime: &str) -> bool {
    path.starts_with("/assets/") && !path.ends_with(".html") && mime.starts_with("text/html")
}

#[cfg(test)]
mod embedded_asset_tests {
    use super::is_index_fallback_for_build_file;

    #[test]
    fn missing_build_file_is_not_served_as_the_app_page() {
        assert!(is_index_fallback_for_build_file("/assets/LockScreen-YAS_j5YD.js", "text/html"));
        assert!(is_index_fallback_for_build_file("/assets/index-x.css", "text/html; charset=utf-8"));
        assert!(!is_index_fallback_for_build_file("/assets/LockScreen-BUsrwPBN.js", "text/javascript"));
        assert!(!is_index_fallback_for_build_file("/settings", "text/html"));
        assert!(!is_index_fallback_for_build_file("/assets/page.html", "text/html"));
    }
}

fn serve_embedded(app: &AppHandle, path: &str) -> axum::response::Response {
    let resolver = app.asset_resolver();
    let wanted = if path.is_empty() || path == "/" { "/index.html" } else { path };
    // A missing build file is a 404, never the app page: served as HTML it fails as a module
    // script with a MIME error the app cannot recover from. Tauri's resolver itself falls back to
    // index.html for unknown paths, so a build file that comes back as HTML counts as missing.
    let asset = resolver
        .get(wanted.to_string())
        .filter(|a| !is_index_fallback_for_build_file(wanted, &a.mime_type))
        .or_else(|| {
            if wanted.starts_with("/assets/") {
                None
            } else {
                resolver.get("/index.html".to_string())
            }
        });
    match asset {
        Some(a) => ([(header::CONTENT_TYPE, a.mime_type)], a.bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Offline map archives for browsers: `/maps/{file}` (range requests, for PMTiles) and
/// `/maps/{file}/{z}/{x}/{y}` (raster tiles). Same files and checks as the station's own
/// `athena-map` protocol: only `.pmtiles` and `.mbtiles` files in the maps folder.
async fn maps_handler(
    State(state): State<LanHttpState>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
) -> axum::response::Response {
    let path = uri.path().strip_prefix("/maps").unwrap_or("/").to_string();
    let mut builder = tauri::http::Request::builder().method(method.as_str()).uri(if path.is_empty() { "/".to_string() } else { path });
    for name in [header::RANGE, header::ACCEPT] {
        if let Some(v) = headers.get(&name).and_then(|v| v.to_str().ok()) {
            builder = builder.header(name.as_str(), v);
        }
    }
    let Ok(request) = builder.body(Vec::new()) else {
        return (StatusCode::BAD_REQUEST, "Bad map request").into_response();
    };
    let app = state.app.clone();
    let response = match tauri::async_runtime::spawn_blocking(move || crate::athena_map_protocol::process_request(&app, request)).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Map read failed").into_response(),
    };
    let (parts, body) = response.into_parts();
    let mut out = axum::response::Response::builder().status(parts.status.as_u16());
    for (k, v) in parts.headers.iter() {
        out = out.header(k.as_str(), v.as_bytes());
    }
    out.body(axum::body::Body::from(body)).unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Map response failed").into_response())
}

/// The offline library (kiwix-serve) for browsers: `/kiwix/...` is passed to the station's own
/// kiwix-serve, which only listens on 127.0.0.1. Read-only reference content, like `/maps`.
async fn kiwix_handler(
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
) -> axum::response::Response {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default()
    });
    let path_q = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/kiwix/");
    if !path_q.starts_with(crate::kiwix_serve::KIWIX_URL_ROOT) {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let url = format!("http://127.0.0.1:{}{}", crate::kiwix_serve::KIWIX_SERVE_PORT, path_q);
    let m = if method == axum::http::Method::HEAD { reqwest::Method::HEAD } else { reqwest::Method::GET };
    let mut req = client.request(m, &url);
    for name in [header::RANGE, header::ACCEPT, header::ACCEPT_LANGUAGE, header::IF_NONE_MATCH] {
        if let Some(v) = headers.get(&name).and_then(|v| v.to_str().ok()) {
            req = req.header(name.as_str(), v);
        }
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "The offline library is starting or has no archives yet. Try again in a minute.",
            )
                .into_response()
        }
    };
    let mut out = axum::response::Response::builder().status(resp.status().as_u16());
    for (k, v) in resp.headers().iter() {
        let n = k.as_str();
        if matches!(n, "content-type" | "content-length" | "content-range" | "accept-ranges" | "etag" | "cache-control" | "location" | "last-modified" | "content-encoding") {
            out = out.header(n, v.as_bytes());
        }
    }
    out.body(axum::body::Body::from_stream(resp.bytes_stream()))
        .unwrap_or_else(|_| (StatusCode::BAD_GATEWAY, "Library response failed").into_response())
}

async fn health() -> impl IntoResponse {
    Json(json!({"ok": true, "service": "athena-lan-bridge"}))
}

fn fallback_index_html(dist_ready: bool, dist_path: &str) -> Html<String> {
    let body = if dist_ready {
        format!(
            r#"<!doctype html><html><head><meta charset="utf-8"><title>ATHENA LAN</title>
<meta http-equiv="refresh" content="0;url=/"></head>
<body><p>Loading ATHENA…</p></body></html>"#
        )
    } else {
        format!(
            r#"<!doctype html><html><head><meta charset="utf-8"><title>ATHENA LAN</title>
<style>body{{font-family:system-ui;background:#0b1220;color:#e2e8f0;padding:2rem;max-width:40rem}}
code{{background:#1e293b;padding:.15rem .4rem;border-radius:4px}}</style></head>
<body>
<h1>ATHENA LAN bridge</h1>
<p>The API is listening, but UI assets are missing.</p>
<p>On the host machine run:</p>
<pre><code>cd athena
npm run build</code></pre>
<p>Then reload this page. Expected folder: <code>{dist_path}</code></p>
<p>Health: <a href="/api/health">/api/health</a></p>
</body></html>"#
        )
    };
    Html(body)
}

fn lan_bridge_status_inner(app: &AppHandle) -> LanBridgeStatus {
    let port = *LAN_PORT.lock().unwrap_or_else(|e| e.into_inner());
    let dist = resolve_dist_dir(app);
    let dist_ready = dist.join("index.html").is_file()
        || app.asset_resolver().get("/index.html".to_string()).is_some();
    let listening = LAN_ENABLED.load(Ordering::SeqCst)
        && LAN_CANCEL
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|c| !c.is_cancelled()))
            .unwrap_or(false);
    let lan_ips = collect_lan_ipv4s();
    let primary_lan_ip = lan_ips.first().cloned().or_else(primary_lan_ipv4);
    LanBridgeStatus {
        enabled: LAN_ENABLED.load(Ordering::SeqCst),
        port,
        listening,
        dist_ready,
        dist_path: dist.display().to_string(),
        primary_lan_ip,
        lan_ips,
        connected_clients: CONNECTED_CLIENTS.load(Ordering::SeqCst) as u32,
        authenticated_sessions: AUTH_SESSIONS.load(Ordering::SeqCst) as u32,
    }
}

pub fn stop_lan_bridge() {
    LAN_ENABLED.store(false, Ordering::SeqCst);
    handheld::stop_advertising();
    if let Ok(mut g) = LAN_CANCEL.lock() {
        if let Some(c) = g.take() {
            c.cancel();
        }
    }
    CONNECTED_CLIENTS.store(0, Ordering::SeqCst);
    AUTH_SESSIONS.store(0, Ordering::SeqCst);
}

pub fn start_lan_bridge(app: AppHandle, port: Option<u16>) -> Result<LanBridgeStatus, String> {
    stop_lan_bridge();
    let port = port.unwrap_or(DEFAULT_LAN_PORT).max(1);
    if let Ok(mut g) = LAN_PORT.lock() {
        *g = port;
    }
    let cancel = CancellationToken::new();
    if let Ok(mut g) = LAN_CANCEL.lock() {
        *g = Some(cancel.clone());
    }
    LAN_ENABLED.store(true, Ordering::SeqCst);
    wire_tauri_events_to_sse(&app);

    let dist_dir = resolve_dist_dir(&app);
    let embedded_ready = app.asset_resolver().get("/index.html".to_string()).is_some();
    let app_for_assets = app.clone();
    crate::auth::register_lan_sessions(resolve_lan_token, update_lan_sessions);
    let http_state = LanHttpState {
        app: app.clone(),
        sessions: lan_sessions(),
    };

    tauri::async_runtime::spawn(async move {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[athena-lan] bind {addr} failed: {e}");
                LAN_ENABLED.store(false, Ordering::SeqCst);
                return;
            }
        };
        eprintln!("[athena-lan] listening on http://0.0.0.0:{port} (dist={})", dist_dir.display());
        handheld::advertise(port);

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let dist_ready = dist_dir.join("index.html").is_file();
        let index_path = dist_dir.join("index.html");
        // Unknown app routes get index.html (the app routes itself); unknown build files get 404.
        let spa_index = index_path.clone();
        let spa_fallback = move |uri: axum::http::Uri| {
            let index = spa_index.clone();
            async move {
                if uri.path().starts_with("/assets/") {
                    return (StatusCode::NOT_FOUND, "Not found").into_response();
                }
                match tokio::fs::read(&index).await {
                    Ok(b) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], b).into_response(),
                    Err(_) => StatusCode::NOT_FOUND.into_response(),
                }
            }
        };
        let static_svc = ServeDir::new(dist_dir.clone())
            .append_index_html_on_directories(true)
            .fallback(axum::handler::HandlerWithoutStateExt::into_service(spa_fallback));

        let api = Router::new()
            .route("/api/health", get(health))
            .route("/api/invoke", post(invoke_handler))
            .route("/api/events", get(events_handler))
            .route("/maps/*rest", get(maps_handler).head(maps_handler))
            .route("/kiwix", get(kiwix_handler).head(kiwix_handler))
            .route("/kiwix/", get(kiwix_handler).head(kiwix_handler))
            .route("/kiwix/*rest", get(kiwix_handler).head(kiwix_handler))
            .merge(handheld::router())
            .with_state(http_state.clone());

        let app_router = if dist_ready {
            Router::new()
                .merge(api)
                .fallback_service(static_svc)
                .layer(axum::middleware::from_fn(web_cache_headers))
                .layer(cors)
        } else if embedded_ready {
            // Installed build: the web files are inside the executable, not in a folder.
            let assets_app = app_for_assets.clone();
            Router::new()
                .merge(api)
                .fallback(move |uri: axum::http::Uri| {
                    let app = assets_app.clone();
                    async move { serve_embedded(&app, uri.path()) }
                })
                .layer(axum::middleware::from_fn(web_cache_headers))
                .layer(cors)
        } else {
            let dist_path = dist_dir.display().to_string();
            Router::new()
                .merge(api)
                .fallback(move || {
                    let p = dist_path.clone();
                    async move { fallback_index_html(false, &p) }
                })
                .layer(cors)
        };

        let shutdown = {
            let c = cancel.clone();
            async move {
                c.cancelled().await;
            }
        };
        if let Err(e) = axum::serve(listener, app_router.into_make_service())
            .with_graceful_shutdown(shutdown)
            .await
        {
            eprintln!("[athena-lan] server stopped: {e}");
        }
        LAN_ENABLED.store(false, Ordering::SeqCst);
    });

    // Brief yield so bind errors surface in status
    std::thread::sleep(std::time::Duration::from_millis(80));
    Ok(lan_bridge_status_inner(&app))
}

#[tauri::command]
pub fn get_lan_bridge_status(app: AppHandle) -> Result<LanBridgeStatus, String> {
    Ok(lan_bridge_status_inner(&app))
}

#[tauri::command]
pub fn set_lan_bridge_enabled(
    app: AppHandle,
    enabled: bool,
    port: Option<u16>,
) -> Result<LanBridgeStatus, String> {
    if enabled {
        start_lan_bridge(app, port)
    } else {
        stop_lan_bridge();
        Ok(lan_bridge_status_inner(&app))
    }
}

#[cfg(test)]
mod smoke_tests {
    use super::*;

    #[test]
    fn usb_export_stays_host_only() {
        assert!(host_only_blocked("export_athena_to_usb"));
        assert!(host_only_blocked("get_available_usbs"));
        assert!(!host_only_blocked("list_chats"));
        assert!(!host_only_blocked("get_theme_config"));
        assert!(!host_only_blocked("get_pois"));
    }

    #[test]
    fn wave1_cmds_are_not_host_blocked() {
        for cmd in LAN_WAVE1_EXPOSED {
            assert!(
                !host_only_blocked(cmd),
                "wave-1 cmd `{cmd}` must not be host-only blocked"
            );
        }
    }
}
