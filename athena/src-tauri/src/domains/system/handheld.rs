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
//!   GET  /api/handheld/day                            -> location (for sun/moon), supplies running
//!                                                        out, next events, radio silence, SOS beacons
//!   POST /api/handheld/character {character}          -> the operator's companion on every handheld
//!
//! Each handheld names itself with `X-Handheld-Id` (its MAC), `X-Handheld-Fw`, `X-Handheld-Ip`
//! and `X-Handheld-Rssi`. The station remembers which operator each one is signed in as, so the
//! device shows under that operator's profile (Settings → Me), next to their companion choice.
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
        .route("/api/handheld/day", get(day))
        .route("/api/handheld/character", post(set_character_http))
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

async fn login(State(state): State<LanHttpState>, headers: HeaderMap, Json(b): Json<LoginBody>) -> Response {
    let args = json!({ "name": b.name, "pin": b.pin });
    let st = state.clone();
    let res = tokio::task::spawn_blocking(move || dispatch_invoke(&st, "authenticate_pin", &args, None)).await;
    match res {
        Ok(Ok((user, Some(token)))) => {
            let character = match require_session(&state, Some(&token)) {
                Ok(u) => {
                    note_device(&state, &headers, &u);
                    character_of(&state, &u.id)
                }
                Err(_) => DEFAULT_CHARACTER.to_string(),
            };
            Json(json!({
            "ok": true,
            "token": token,
            "character": character,
            "name": short(user.get("displayName").and_then(Value::as_str)
                .or_else(|| user.get("name").and_then(Value::as_str)).unwrap_or("Operator"), 24),
            "rank": user.get("rankLevel").or_else(|| user.get("rank_level")).and_then(Value::as_i64).unwrap_or(0),
            "clearance": user.get("clearanceLevel").or_else(|| user.get("clearance_level")).and_then(Value::as_i64).unwrap_or(0),
        }))
        .into_response()
        }
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
    note_device(&state, &headers, &user);
    Json(json!({
        "ok": true,
        "character": character_of(&state, &user.id),
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

// ── day: the facts a field screen keeps in view ─────────────────────────────────────────────

async fn day(State(state): State<LanHttpState>, headers: HeaderMap) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return unauthorized();
    };
    let Ok(user) = require_session(&state, Some(&token)) else {
        return unauthorized();
    };
    let Some(app_state) = state.app.try_state::<AppState>() else {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Station is starting.");
    };
    let t = now();
    let cfg = crate::portable::resolved_app_data_dir(&state.app).ok().map(|d| crate::load_config(&d));
    // Station position: last map GPS fix, else the saved region pin.
    let loc = cfg.as_ref().and_then(|c| match (c.last_known_gps_lat, c.last_known_gps_lng) {
        (Some(lat), Some(lng)) => Some((lat, lng)),
        _ => c.library_installer_location.as_ref().map(|l| (l.lat, l.lon)),
    });

    let (supplies, events, sos) = {
        let Ok(db) = app_state.db.lock() else {
            return fail(StatusCode::SERVICE_UNAVAILABLE, "Database is busy.");
        };
        let items: Vec<crate::runway::StockItem> = db.inventory_list().unwrap_or_default().into_iter()
            .map(|r| crate::runway::StockItem {
                id: r.id, name: r.item_name, category: r.category, division: r.division, quantity: r.quantity,
            })
            .collect();
        let changes: Vec<crate::runway::StockChange> = db.inventory_log_list().unwrap_or_default().into_iter()
            .map(|l| crate::runway::StockChange {
                item_id: l.item_id, old_qty: l.old_qty, new_qty: l.new_qty,
                timestamp: crate::runway::log_timestamp_to_seconds(l.timestamp),
            })
            .collect();
        let heads = db.personnel_count().unwrap_or(1).max(1);
        let report = crate::runway::build_runway_report(&items, &changes, t, crate::runway::DEFAULT_WINDOW_DAYS, heads, heads);
        let supplies: Vec<Value> = report.items.iter()
            .filter(|i| i.days_left.is_some() || i.quantity <= 0)
            .take(4)
            .map(|i| json!({
                "name": short(&i.name, 18),
                "days": i.days_left.map(|d| (d * 10.0).round() / 10.0).unwrap_or(0.0),
                "status": serde_json::to_value(i.status).unwrap_or(Value::Null),
            }))
            .collect();
        let supplies = json!({
            "items": supplies, "out": report.out_count, "critical": report.critical_count,
            "low": report.low_count, "tracked": report.items.len(),
        });

        let mut upcoming: Vec<(i64, String, Option<String>)> = db.list_calendar_events(None, None, false)
            .unwrap_or_default().into_iter()
            .map(|e| (crate::runway::log_timestamp_to_seconds(e.start_ts), e.title, e.location_label))
            .filter(|(start, _, _)| *start >= t - 15 * 60)
            .collect();
        upcoming.sort_by_key(|(s, _, _)| *s);
        let events: Vec<Value> = upcoming.into_iter().take(3)
            .map(|(start, title, place)| json!({
                "title": short(&title, 28), "start": start, "where": short(place.as_deref().unwrap_or(""), 20),
            }))
            .collect();

        let sos: Vec<Value> = db.mesh_profile_list().unwrap_or_default().into_iter()
            .filter(|p| p.sos_state == crate::db::SOS_STATE_ACTIVE)
            .take(6)
            .map(|p| json!(short(if p.username.trim().is_empty() { "Unknown node" } else { &p.username }, 18)))
            .collect();
        (supplies, events, sos)
    };

    let silence = if user.clearance_level > 0 {
        run_cmd(&state, "get_radio_discipline", json!({}), token).await.ok()
    } else {
        None
    };
    let silent_now = silence.as_ref().and_then(|v| v.get("silentNow")).and_then(Value::as_str).map(|s| short(s, 40));
    let silent_until = silence.as_ref().and_then(|v| v.get("silentUntil")).and_then(Value::as_i64);

    Json(json!({
        "ok": true,
        "lat": loc.map(|l| (l.0 * 1e3).round() / 1e3),
        "lng": loc.map(|l| (l.1 * 1e3).round() / 1e3),
        "supplies": supplies,
        "events": events,
        "silentNow": silent_now,
        "silentUntil": silent_until,
        "sos": sos,
    }))
    .into_response()
}

// ── companions and devices ──────────────────────────────────────────────────────────────────

/// The companions a handheld can show: survival roles, drawn full body. Their pixel art lives in
/// the firmware (devices/handheld-cyd/src/art.h) and, exported from it, in the Me panel preview
/// (src/pages/Settings/handheldHeroes.ts). A stored id that is no longer here (the old creature
/// companions) falls back to the default.
pub const CHARACTERS: &[(&str, &str, &str)] = &[
    ("wren", "Wren", "Pathfinder. Knows every trail out and carries the lantern."),
    ("bastion", "Bastion", "Gate knight. Keeps watch on the perimeter."),
    ("marigold", "Marigold", "Homesteader. Tends the garden, larder and rain barrels."),
    ("juniper", "Juniper", "Field medic. First aid kit always in hand."),
    ("sparks", "Sparks", "Radio operator. Keeps the mesh talking, wrench ready."),
    ("bramble", "Bramble", "Forager. Reads the woods for food, water and trouble."),
];
pub const DEFAULT_CHARACTER: &str = "wren";
const PROFILE_KIND: &str = "handheld_profile";
const DEVICE_KIND: &str = "handheld_device";
/// A device that has not polled for this long shows as offline (it polls every 10 s).
const ONLINE_SECS: i64 = 60;
/// Live sightings are written to the database at most this often per device.
const PERSIST_EVERY_SECS: i64 = 300;

fn valid_character(c: &str) -> bool {
    CHARACTERS.iter().any(|(id, _, _)| *id == c)
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HandheldProfile {
    character: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandheldDevice {
    pub device_id: String,
    pub user_id: String,
    pub user_name: String,
    pub ip: String,
    pub rssi: i32,
    pub firmware: String,
    pub last_seen: i64,
}

fn profile_record_id(user_id: &str) -> String {
    format!("handheld-profile:{user_id}")
}

fn device_record_id(device_id: &str) -> String {
    format!("handheld:{device_id}")
}

/// Latest sighting of each device, and when it was last written to the database.
static LIVE: Mutex<Option<std::collections::HashMap<String, (HandheldDevice, i64)>>> = Mutex::new(None);

fn character_in(db: &crate::db::Database, user_id: &str) -> String {
    crate::field_ops::store::get::<HandheldProfile>(db, PROFILE_KIND, &profile_record_id(user_id))
        .ok()
        .flatten()
        .map(|r| r.data.character)
        .filter(|c| valid_character(c))
        .unwrap_or_else(|| DEFAULT_CHARACTER.to_string())
}

fn character_of(state: &LanHttpState, user_id: &str) -> String {
    state
        .app
        .try_state::<AppState>()
        .and_then(|s| s.db.lock().ok().map(|db| character_in(&db, user_id)))
        .unwrap_or_else(|| DEFAULT_CHARACTER.to_string())
}

fn header_str(headers: &HeaderMap, name: &str, max: usize) -> String {
    headers.get(name).and_then(|v| v.to_str().ok()).map(|s| short(s, max)).unwrap_or_default()
}

/// Remember that this handheld is signed in as `user`. Cheap: memory on every poll, the database
/// when the device is new, changed hands, or every few minutes.
fn note_device(state: &LanHttpState, headers: &HeaderMap, user: &crate::auth::AuthenticatedPersonnel) {
    let id: String = header_str(headers, "x-handheld-id", 24)
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase();
    if id.len() != 12 {
        return;
    }
    let t = now();
    let dev = HandheldDevice {
        device_id: id.clone(),
        user_id: user.id.clone(),
        user_name: short(user.display_name.as_deref().unwrap_or(&user.name), 32),
        ip: header_str(headers, "x-handheld-ip", 40),
        rssi: header_str(headers, "x-handheld-rssi", 6).parse().unwrap_or(0),
        firmware: header_str(headers, "x-handheld-fw", 16),
        last_seen: t,
    };
    let persist = {
        let Ok(mut g) = LIVE.lock() else { return };
        let map = g.get_or_insert_with(Default::default);
        let persist = match map.get(&id) {
            Some((prev, at)) => prev.user_id != dev.user_id || t - at >= PERSIST_EVERY_SECS,
            None => true,
        };
        let at = if persist { t } else { map.get(&id).map(|(_, a)| *a).unwrap_or(t) };
        map.insert(id.clone(), (dev.clone(), at));
        persist
    };
    if persist {
        if let Some(app_state) = state.app.try_state::<AppState>() {
            if let Ok(db) = app_state.db.lock() {
                let _ = crate::field_ops::store::put(&db, DEVICE_KIND, Some(&device_record_id(&id)), &dev);
            }
        }
    }
}

fn set_character_in(db: &crate::db::Database, user_id: &str, character: &str) -> Result<String, String> {
    let c = character.trim().to_ascii_lowercase();
    if !valid_character(&c) {
        return Err(format!("Unknown companion `{c}`."));
    }
    crate::field_ops::store::put(db, PROFILE_KIND, Some(&profile_record_id(user_id)), &HandheldProfile { character: c.clone() })?;
    Ok(c)
}

#[derive(Deserialize)]
struct CharacterBody {
    character: String,
}

async fn set_character_http(State(state): State<LanHttpState>, headers: HeaderMap, Json(b): Json<CharacterBody>) -> Response {
    let token = bearer_token(&headers);
    let Ok(user) = require_session(&state, token.as_deref()) else {
        return unauthorized();
    };
    let Some(app_state) = state.app.try_state::<AppState>() else {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Station is starting.");
    };
    let res = match app_state.db.lock() {
        Ok(db) => set_character_in(&db, &user.id, &b.character),
        Err(_) => Err("Database is busy.".to_string()),
    };
    match res {
        Ok(c) => Json(json!({ "ok": true, "character": c })).into_response(),
        Err(e) => fail(StatusCode::BAD_REQUEST, e),
    }
}

/// The signed-in operator's handhelds, newest sighting first: live data where there is some,
/// otherwise the last stored sighting.
fn devices_of(db: &crate::db::Database, user_id: &str) -> Vec<HandheldDevice> {
    let mut out: std::collections::HashMap<String, HandheldDevice> = crate::field_ops::store::list::<HandheldDevice>(db, DEVICE_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.data.device_id.clone(), r.data))
        .collect();
    if let Ok(g) = LIVE.lock() {
        if let Some(map) = g.as_ref() {
            for (id, (d, _)) in map {
                out.insert(id.clone(), d.clone());
            }
        }
    }
    let mut v: Vec<HandheldDevice> = out.into_values().filter(|d| d.user_id == user_id).collect();
    v.sort_by_key(|d| -d.last_seen);
    v
}

fn overview_value(db: &crate::db::Database, user_id: &str) -> Value {
    let t = now();
    json!({
        "character": character_in(db, user_id),
        "characters": CHARACTERS.iter().map(|(id, name, blurb)| json!({ "id": id, "name": name, "blurb": blurb })).collect::<Vec<_>>(),
        "devices": devices_of(db, user_id).into_iter().map(|d| json!({
            "deviceId": d.device_id,
            "ip": d.ip,
            "rssi": d.rssi,
            "firmware": d.firmware,
            "lastSeen": d.last_seen,
            "online": t - d.last_seen <= ONLINE_SECS,
        })).collect::<Vec<_>>(),
    })
}

/// Settings → Me: the operator's companion and the handhelds signed in as them.
#[tauri::command]
pub fn handheld_overview(app: tauri::State<'_, AppState>, session: crate::auth::CallerSession) -> Result<Value, String> {
    let user = crate::auth::authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    Ok(overview_value(&db, &user.id))
}

/// Pick the companion every handheld shows for this operator. Devices switch on their next poll.
#[tauri::command]
pub fn handheld_set_character(
    character: String,
    app: tauri::State<'_, AppState>,
    session: crate::auth::CallerSession,
) -> Result<Value, String> {
    let user = crate::auth::authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    set_character_in(&db, &user.id, &character)?;
    Ok(overview_value(&db, &user.id))
}

/// Remove one of the operator's handhelds from their profile. It reappears if it signs in again.
#[tauri::command]
pub fn handheld_forget_device(
    device_id: String,
    app: tauri::State<'_, AppState>,
    session: crate::auth::CallerSession,
) -> Result<Value, String> {
    let user = crate::auth::authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let id = device_id.trim().to_ascii_lowercase();
    if !devices_of(&db, &user.id).iter().any(|d| d.device_id == id) {
        return Err("That handheld is not on your profile.".to_string());
    }
    crate::field_ops::store::remove(&db, DEVICE_KIND, &device_record_id(&id))?;
    if let Ok(mut g) = LIVE.lock() {
        if let Some(map) = g.as_mut() {
            map.remove(&id);
        }
    }
    Ok(overview_value(&db, &user.id))
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
    fn companions_are_known() {
        assert!(super::valid_character("bastion"));
        assert!(super::valid_character("bramble"));
        assert!(!super::valid_character("pip"), "old companions fall back to the default");
        assert!(!super::valid_character("dragon"));
        assert!(super::valid_character(super::DEFAULT_CHARACTER));
    }

    #[test]
    fn short_folds_and_trims() {
        assert_eq!(short("  “Hi” — there\n", 40), "\"Hi\" - there");
        assert_eq!(short("abcdef", 3), "abc...");
        assert_eq!(short("caf\u{e9}", 10), "caf");
    }
}
