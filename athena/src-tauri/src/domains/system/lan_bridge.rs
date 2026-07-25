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
use tower_http::services::{ServeDir, ServeFile};
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

/// Fan-out channel for SSE clients (JSON lines for `athena` events).
static SSE_CLIENTS: OnceLock<Mutex<Vec<mpsc::UnboundedSender<String>>>> = OnceLock::new();

fn sse_clients() -> &'static Mutex<Vec<mpsc::UnboundedSender<String>>> {
    SSE_CLIENTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn publish_sse_event(event: &str, payload: Value) {
    let data = json!({ "event": event, "payload": payload }).to_string();
    if let Ok(mut clients) = sse_clients().lock() {
        clients.retain(|tx| tx.send(data.clone()).is_ok());
    }
}

fn wire_tauri_events_to_sse(app: &AppHandle) {
    let app_chunk = app.clone();
    let _ = app_chunk.listen("athena:stream-chunk", |e| {
        // Payload is typically a JSON string (quoted chunk) or raw text.
        let raw = e.payload().to_string();
        let payload = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
        publish_sse_event("athena:stream-chunk", payload);
    });
    let app_done = app.clone();
    let _ = app_done.listen("athena:stream-done", |_| {
        publish_sse_event("athena:stream-done", Value::Null);
    });
    let app_err = app.clone();
    let _ = app_err.listen("athena:stream-error", |e| {
        let raw = e.payload().to_string();
        let payload = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
        publish_sse_event("athena:stream-error", payload);
    });
    let app_src = app.clone();
    let _ = app_src.listen("athena:stream-sources", |e| {
        let raw = e.payload().to_string();
        let payload = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
        publish_sse_event("athena:stream-sources", payload);
    });
    let app_mesh = app.clone();
    let _ = app_mesh.listen("mesh-connection", |e| {
        let raw = e.payload().to_string();
        let payload = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
        publish_sse_event("mesh-connection", payload);
    });
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
            let out = {
                let db = app_state
                    .db
                    .lock()
                    .map_err(|_| "Database is busy. Try again in a moment.".to_string())?;
                let row = db
                    .personnel_fetch_by_name(&name)?
                    .ok_or_else(|| "Unknown operator name.".to_string())?;
                if personnel_pin_hash(&pin) != row.pin_hash {
                    return Err("Invalid PIN.".to_string());
                }
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
                .ok_or_else(|| "Session expired.".to_string())?;
            Ok((serde_json::to_value(user).map_err(|e| e.to_string())?, None))
        }
        "get_theme_config" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let cfg = load_config(&app_data);
            let payload = json!([
                cfg.theme_id,
                cfg.personality_id,
                cfg.custom_themes
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
            crate::start_send_message_stream_from_lan_args(state.app.clone(), user, args)?;
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
        "is_inference_ready" => Ok((json!(false), None)),
        "get_ai_inference_sleep" => {
            let app_data = portable::resolved_app_data_dir(&state.app)?;
            let cfg = load_config(&app_data);
            Ok((json!(cfg.ai_inference_sleep.unwrap_or(false)), None))
        }
        "set_ai_inference_sleep" => {
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
        other => Err(format!(
            "LAN bridge: command `{other}` is not exposed yet. Use the desktop app for full capability."
        )),
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
        .ok_or_else(|| "Session expired.".to_string())
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
    #[allow(dead_code)]
    token: Option<String>,
}

async fn events_handler(
    State(state): State<LanHttpState>,
    Query(q): Query<EventsQuery>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let _ = (state, q);
    CONNECTED_CLIENTS.fetch_add(1, Ordering::SeqCst);
    let guard = SseClientGuard;
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    if let Ok(mut clients) = sse_clients().lock() {
        clients.push(tx);
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
    let dist_ready = dist.join("index.html").is_file();
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
    let sessions: Arc<Mutex<HashMap<String, AuthenticatedPersonnel>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let http_state = LanHttpState {
        app: app.clone(),
        sessions,
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

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let dist_ready = dist_dir.join("index.html").is_file();
        let index_path = dist_dir.join("index.html");
        let static_svc = ServeDir::new(dist_dir.clone())
            .append_index_html_on_directories(true)
            .fallback(ServeFile::new(index_path.clone()));

        let api = Router::new()
            .route("/api/health", get(health))
            .route("/api/invoke", post(invoke_handler))
            .route("/api/events", get(events_handler))
            .with_state(http_state.clone());

        let app_router = if dist_ready {
            Router::new()
                .merge(api)
                .fallback_service(static_svc)
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
