//! Handheld API: small JSON answers for pocket screens (ESP32 "CYD" boards) that join the
//! station's Wi-Fi. Everything rides on the LAN bridge: same port, same PIN sessions, same
//! permission checks. Answers are kept to a few kilobytes so a board without PSRAM can parse them.
//!
//!   POST /api/handheld/login   {name, pin}            -> {token, name, rank, clearance}
//!   GET  /api/handheld/status                         -> station, radio, peers, alert counts
//!   GET  /api/handheld/chat?limit=N                   -> newest N messages (LAN chat + mesh radio)
//!   POST /api/handheld/chat    {text, via:"lan"|"radio"}
//!   GET  /api/handheld/peers                          -> LAN nodes, radio nodes, map markers
//!   GET  /api/handheld/alerts                         -> watch findings + command notifications
//!   POST /api/handheld/ask     {q}                    -> {answer} from the station's local model
//!
//! The station also advertises `_athena._tcp` over mDNS while the bridge runs, so a handheld can
//! find it without anyone typing an address.

use std::sync::Mutex;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use super::{bearer_token, dispatch_invoke, forward_command, require_session, LanHttpState};
use crate::AppState;

const MAX_TEXT: usize = 160;
const MAX_ITEMS: usize = 40;

pub(super) fn router() -> Router<LanHttpState> {
    Router::new()
        .route("/api/handheld/login", post(login))
        .route("/api/handheld/status", get(status))
        .route("/api/handheld/chat", get(chat_list).post(chat_send))
        .route("/api/handheld/peers", get(peers))
        .route("/api/handheld/alerts", get(alerts))
        .route("/api/handheld/ask", post(ask))
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────────

fn fail(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({ "ok": false, "error": msg.into() }))).into_response()
}

fn unauthorized() -> Response {
    fail(StatusCode::UNAUTHORIZED, "Sign in again.")
}

/// Trim to `max` characters and fold to the ASCII the handheld's fonts can draw.
fn short(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max));
    for ch in s.chars() {
        let c = match ch {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            '\n' | '\r' | '\t' => ' ',
            c if c.is_ascii() && !c.is_ascii_control() => c,
            _ => continue,
        };
        out.push(c);
        if out.len() >= max {
            out.push_str("...");
            break;
        }
    }
    out.trim().to_string()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn station_name() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "ATHENA".to_string())
}

/// Run a bridge command (which may block on the main webview) off the async runtime.
async fn run_cmd(state: &LanHttpState, cmd: &'static str, args: Value, token: String) -> Result<Value, String> {
    let st = state.clone();
    tokio::task::spawn_blocking(move || forward_command(&st, cmd, &args, Some(&token)).map(|(v, _)| v))
        .await
        .map_err(|e| e.to_string())?
}

// ── login ───────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoginBody {
    name: String,
    pin: String,
}

async fn login(State(state): State<LanHttpState>, Json(b): Json<LoginBody>) -> Response {
    let args = json!({ "name": b.name, "pin": b.pin });
    let st = state.clone();
    let res = tokio::task::spawn_blocking(move || dispatch_invoke(&st, "authenticate_pin", &args, None)).await;
    match res {
        Ok(Ok((user, Some(token)))) => Json(json!({
            "ok": true,
            "token": token,
            "name": short(user.get("displayName").and_then(Value::as_str)
                .or_else(|| user.get("name").and_then(Value::as_str)).unwrap_or("Operator"), 24),
            "rank": user.get("rankLevel").or_else(|| user.get("rank_level")).and_then(Value::as_i64).unwrap_or(0),
            "clearance": user.get("clearanceLevel").or_else(|| user.get("clearance_level")).and_then(Value::as_i64).unwrap_or(0),
        }))
        .into_response(),
        Ok(Ok(_)) => fail(StatusCode::INTERNAL_SERVER_ERROR, "No session issued."),
        Ok(Err(e)) => fail(StatusCode::FORBIDDEN, short(&e, 80)),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ── status ──────────────────────────────────────────────────────────────────────────────────

async fn status(State(state): State<LanHttpState>, headers: HeaderMap) -> Response {
    let token = bearer_token(&headers);
    let Ok(user) = require_session(&state, token.as_deref()) else {
        return unauthorized();
    };
    let Some(app_state) = state.app.try_state::<AppState>() else {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Station is starting.");
    };
    let app_data = crate::portable::resolved_app_data_dir(&state.app).ok();
    let cfg = app_data.as_ref().map(|d| crate::load_config(d));
    let power_save = app_state.power_save_mode.load(std::sync::atomic::Ordering::SeqCst);
    let (unacked, radio_nodes) = match app_state.db.lock() {
        Ok(db) => (
            if user.clearance_level >= 4 { db.count_unacked_command_notifications().unwrap_or(0) } else { 0 },
            db.list_mesh_network_nodes().map(|v| v.len()).unwrap_or(0),
        ),
        Err(_) => (0, 0),
    };
    let lan_peers = crate::mesh_discovery::peer_registry().read().await.len();
    Json(json!({
        "ok": true,
        "station": short(&station_name(), 24),
        "user": short(user.display_name.as_deref().unwrap_or(&user.name), 24),
        "time": now(),
        "tz": chrono::Local::now().offset().local_minus_utc(),
        "radio": crate::mesh_listener::mesh_radio_is_connected(),
        "lanPeers": lan_peers,
        "radioNodes": radio_nodes,
        "unacked": unacked,
        "powerSave": power_save,
        "aiSleep": cfg.as_ref().and_then(|c| c.ai_inference_sleep).unwrap_or(false),
    }))
    .into_response()
}

// ── chat ────────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatQuery {
    limit: Option<usize>,
}

async fn chat_list(State(state): State<LanHttpState>, headers: HeaderMap, Query(q): Query<ChatQuery>) -> Response {
    let token = bearer_token(&headers);
    if require_session(&state, token.as_deref()).is_err() {
        return unauthorized();
    }
    let Some(app_state) = state.app.try_state::<AppState>() else {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Station is starting.");
    };
    let limit = q.limit.unwrap_or(20).clamp(1, MAX_ITEMS);
    let mut msgs: Vec<(i64, Value)> = Vec::new();
    {
        let Ok(db) = app_state.db.lock() else {
            return fail(StatusCode::SERVICE_UNAVAILABLE, "Database is busy.");
        };
        for m in db.list_lan_chat_messages(limit as i64).unwrap_or_default() {
            msgs.push((m.created_at, json!({
                "id": m.id, "from": short(&m.sender_label, 20), "text": short(&m.content, MAX_TEXT),
                "ts": m.created_at, "via": "lan",
            })));
        }
        // Radio traffic: the busiest few nodes, newest messages of each.
        let nodes = db.list_mesh_network_nodes().unwrap_or_default();
        for n in nodes.iter().take(6) {
            let rows = db.list_mesh_chats_for_node(n.node_id as i64, 500).unwrap_or_default();
            let who = n.username.clone().unwrap_or_else(|| format!("!{:08x}", n.node_id));
            for r in rows.iter().rev().take(limit) {
                let from = if r.sender_type == "remote" { short(&who, 20) } else { "Station".to_string() };
                msgs.push((r.created_at, json!({
                    "id": r.id, "from": from, "text": short(&r.content, MAX_TEXT),
                    "ts": r.created_at, "via": "radio",
                    "urgent": r.message_type == "emergency",
                })));
            }
        }
    }
    msgs.sort_by_key(|(t, _)| *t);
    let skip = msgs.len().saturating_sub(limit);
    let out: Vec<Value> = msgs.into_iter().skip(skip).map(|(_, v)| v).collect();
    Json(json!({ "ok": true, "msgs": out })).into_response()
}

#[derive(Deserialize)]
struct ChatSend {
    text: String,
    via: Option<String>,
}

async fn chat_send(State(state): State<LanHttpState>, headers: HeaderMap, Json(b): Json<ChatSend>) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return unauthorized();
    };
    let Ok(user) = require_session(&state, Some(&token)) else {
        return unauthorized();
    };
    let text = b.text.trim().to_string();
    if text.is_empty() {
        return fail(StatusCode::BAD_REQUEST, "Message is empty.");
    }
    let res = if b.via.as_deref() == Some("radio") {
        run_cmd(&state, "send_mesh_message", json!({ "text": text }), token).await
    } else {
        let label = user.display_name.clone().unwrap_or(user.name.clone());
        run_cmd(&state, "lan_chat_send_message", json!({ "text": text, "sender_label": label }), token).await
    };
    match res {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(e) => fail(StatusCode::BAD_REQUEST, short(&e, 80)),
    }
}

// ── peers ───────────────────────────────────────────────────────────────────────────────────

async fn peers(State(state): State<LanHttpState>, headers: HeaderMap) -> Response {
    let token = bearer_token(&headers);
    if require_session(&state, token.as_deref()).is_err() {
        return unauthorized();
    }
    let Some(app_state) = state.app.try_state::<AppState>() else {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Station is starting.");
    };
    let t = now();
    let lan: Vec<Value> = crate::mesh_discovery::peer_registry()
        .read()
        .await
        .values()
        .take(MAX_ITEMS)
        .map(|p| json!({
            "id": p.node_id, "name": format!("Node {}", p.node_id), "ip": p.ipv4,
            "ago": (t - (p.last_seen_unix_ms / 1000) as i64).max(0),
        }))
        .collect();
    let (radio, markers) = match app_state.db.lock() {
        Ok(db) => {
            let radio: Vec<Value> = db.list_mesh_network_nodes().unwrap_or_default().into_iter().take(MAX_ITEMS)
                .map(|n| json!({
                    "id": n.node_id,
                    "name": short(&n.username.unwrap_or_else(|| format!("!{:08x}", n.node_id)), 20),
                    "ago": (t - n.last_message_at).max(0),
                }))
                .collect();
            let markers: Vec<Value> = db.list_map_markers().unwrap_or_default().into_iter().take(MAX_ITEMS)
                .map(|m| json!({
                    "title": short(&m.title, 28), "cat": short(&m.category, 10),
                    "lat": (m.lat * 1e5).round() / 1e5, "lng": (m.lng * 1e5).round() / 1e5,
                }))
                .collect();
            (radio, markers)
        }
        Err(_) => (vec![], vec![]),
    };
    Json(json!({ "ok": true, "lan": lan, "radio": radio, "markers": markers })).into_response()
}

// ── alerts ──────────────────────────────────────────────────────────────────────────────────

async fn alerts(State(state): State<LanHttpState>, headers: HeaderMap) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return unauthorized();
    };
    let Ok(user) = require_session(&state, Some(&token)) else {
        return unauthorized();
    };
    let mut items: Vec<Value> = Vec::new();
    let mut headline = String::new();
    if user.clearance_level > 0 {
        if let Ok(brief) = run_cmd(&state, "get_watch_brief", json!({ "with_ai": false }), token.clone()).await {
            headline = short(brief.get("headline").and_then(Value::as_str).unwrap_or(""), 80);
            for f in brief.get("findings").and_then(Value::as_array).cloned().unwrap_or_default().iter().take(12) {
                items.push(json!({
                    "sev": f.get("severity").and_then(Value::as_str).unwrap_or("info"),
                    "title": short(f.get("title").and_then(Value::as_str).unwrap_or(""), 48),
                    "detail": short(f.get("detail").and_then(Value::as_str).unwrap_or(""), MAX_TEXT),
                    "src": "watch",
                }));
            }
        }
    }
    if user.clearance_level >= 4 {
        if let Some(app_state) = state.app.try_state::<AppState>() {
            if let Ok(db) = app_state.db.lock() {
                for n in db.list_command_notifications(12).unwrap_or_default().into_iter().filter(|n| n.ack_at.is_none()) {
                    items.push(json!({
                        "sev": short(&n.severity, 10),
                        "title": short(&n.title, 48),
                        "detail": short(n.body.as_deref().unwrap_or(""), MAX_TEXT),
                        "src": "command", "ts": n.created_at,
                    }));
                }
            }
        }
    }
    Json(json!({ "ok": true, "headline": headline, "items": items })).into_response()
}

// ── AI quick ask ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AskBody {
    q: String,
}

async fn ask(State(state): State<LanHttpState>, headers: HeaderMap, Json(b): Json<AskBody>) -> Response {
    let token = bearer_token(&headers);
    if require_session(&state, token.as_deref()).is_err() {
        return unauthorized();
    }
    let q = short(&b.q, 200);
    if q.is_empty() {
        return fail(StatusCode::BAD_REQUEST, "Ask something.");
    }
    let app = state.app.clone();
    let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let app_data = crate::portable::resolved_app_data_dir(&app)?;
        let config = crate::load_config(&app_data);
        if config.ai_inference_sleep.unwrap_or(false) {
            return Err("The station's AI is asleep (power saving).".to_string());
        }
        let models = crate::inference::load_models_for_inference(&config);
        crate::inference::ensure_inference_ready(&config, &models)?;
        let messages = vec![
            crate::ollama::ChatMessage {
                role: "system".to_string(),
                content: "You are ATHENA, the assistant of an off-grid group. The person is reading your answer on a tiny pocket screen. Answer in at most four short plain sentences. No markdown, no lists, no headings.".to_string(),
                images: None,
            },
            crate::ollama::ChatMessage { role: "user".to_string(), content: q, images: None },
        ];
        let opts = crate::ollama::ModelOptions { temperature: 0.3, num_ctx: 2048, num_predict: Some(220) };
        crate::inference::chat_unified(&config, &models, messages, Some(opts))
    })
    .await;
    match res {
        Ok(Ok(a)) => Json(json!({ "ok": true, "answer": short(&a, 700) })).into_response(),
        Ok(Err(e)) => fail(StatusCode::SERVICE_UNAVAILABLE, short(&e, 100)),
        Err(e) => fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ── mDNS ────────────────────────────────────────────────────────────────────────────────────

pub const HANDHELD_SERVICE_TYPE: &str = "_athena._tcp.local.";

static ADVERT: Mutex<Option<mdns_sd::ServiceDaemon>> = Mutex::new(None);

/// Announce the bridge so handhelds find it. Best effort: a failure only means typing the address.
pub(super) fn advertise(port: u16) {
    stop_advertising();
    let daemon = match mdns_sd::ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[athena-handheld] mDNS: {e}");
            return;
        }
    };
    let name = station_name();
    let node = crate::lan_identity::lan_local_node_id();
    let mut props = std::collections::HashMap::<String, String>::new();
    props.insert("station".to_string(), short(&name, 24));
    props.insert("api".to_string(), "handheld/1".to_string());
    let instance = format!("athena-{node}");
    let host = format!("athena-{node}.local.");
    match mdns_sd::ServiceInfo::new(HANDHELD_SERVICE_TYPE, &instance, &host, "", port, props) {
        Ok(info) => {
            if let Err(e) = daemon.register(info.enable_addr_auto()) {
                eprintln!("[athena-handheld] mDNS register: {e}");
                return;
            }
            if let Ok(mut g) = ADVERT.lock() {
                *g = Some(daemon);
            }
        }
        Err(e) => eprintln!("[athena-handheld] mDNS info: {e}"),
    }
}

pub(super) fn stop_advertising() {
    if let Ok(mut g) = ADVERT.lock() {
        if let Some(d) = g.take() {
            let _ = d.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn short_folds_and_trims() {
        assert_eq!(short("  “Hi” — there\n", 40), "\"Hi\" - there");
        assert_eq!(short("abcdef", 3), "abc...");
        assert_eq!(short("caf\u{e9}", 10), "caf");
    }
}
