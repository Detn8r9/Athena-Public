//! Background Tokio task: USB Meshtastic serial, decode text packets, emit Tauri events, send text,
//! and optional Meshtastic → Ollama → DM reply.
//!
//! **Terminal noise:** By default only short `[mesh] …` lines are printed for decoded text and position
//! packets. Set **`ATHENA_MESH_SERIAL_DEBUG=1`** to log full protobuf hex previews (`[MESH RAW] …`)
//! for low-level debugging.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::db::{
    Database, LogbookEntry, SOS_STATE_ACTIVE, SOS_STATE_NORMAL, SOS_STATE_SUSPENDED,
};
use crate::lan_sync;
use crate::mesh_calendar;
use crate::mesh_logbook;

use meshtastic::api::StreamApi;
use meshtastic::packet::{PacketDestination, PacketRouter};
use meshtastic::protobufs;
use meshtastic::types::{MeshChannel, NodeId};
use prost::Message;
use serde::Serialize;
use tauri::Emitter;
use thiserror::Error;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Semaphore;
use tokio::time::{interval, MissedTickBehavior};

use crate::mesh_ai::{MESHTASTIC_BROADCAST_TO, PRIMARY_MESH_CHANNEL};
use crate::mesh_policy;
use crate::mesh_router::{
    emit_mesh_emergency, parse_sos_clear_tail, send_dm_chunks, MeshDmSink,
    MeshNetworkChatUpdatedPayload, MeshOperatorQueuePayload, MESH_NETWORK_CHAT_UPDATED,
    MESH_OPERATOR_QUEUE_MESSAGE,
};
use crate::mesh_radio::MeshRadioSerialLink;

/// Capacity for outgoing mesh commands (send_text from UI and mesh-AI replies).
const MESH_CMD_QUEUE_CAPACITY: usize = 100;
/// Capacity for pending UI mesh-text notifications before throttle drains.
const MESH_TEXT_QUEUE_CAPACITY: usize = 200;
/// At most ~3 `athena:mesh-text` emissions per second (battery-friendly; still feels instant).
const MESH_TEXT_EMIT_INTERVAL_MS: u64 = 350;
/// If the UI cannot keep up, drop oldest pending mesh-text payloads after this many.
const MESH_TEXT_PENDING_CAP: usize = 500;

/// Error type for [`PacketRouter`] (must implement [`std::error::Error`], not plain `String`).
#[derive(Debug, Error)]
#[error("{0}")]
struct PacketRouterError(String);

/// Event name for decoded mesh text messages (listen in the frontend with `listen`).
pub const MESH_TEXT_EVENT: &str = "athena:mesh-text";

/// Event when USB Meshtastic serial connects or disconnects (for hiding/showing UI).
pub const MESH_CONNECTION_EVENT: &str = "athena:mesh-connection";

static MESH_RADIO_CONNECTED: AtomicBool = AtomicBool::new(false);
static MESH_MY_NODE_NUM: AtomicU32 = AtomicU32::new(0);

static MESH_SERIAL_DEBUG_HEX: OnceLock<bool> = OnceLock::new();

/// Full protobuf hex dumps (`[MESH RAW]`) — off unless `ATHENA_MESH_SERIAL_DEBUG=1` (or `true`).
fn mesh_serial_debug_hex_enabled() -> bool {
    *MESH_SERIAL_DEBUG_HEX.get_or_init(|| {
        std::env::var("ATHENA_MESH_SERIAL_DEBUG")
            .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn mesh_local_display_name_storage() -> &'static Mutex<String> {
    static NAME: OnceLock<Mutex<String>> = OnceLock::new();
    NAME.get_or_init(|| Mutex::new(String::new()))
}

/// Long or short name for this USB-connected node when known (from NodeInfo).
pub fn mesh_local_display_name() -> String {
    mesh_local_display_name_storage()
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default()
}

fn set_mesh_local_display_name(name: &str) {
    if let Ok(mut g) = mesh_local_display_name_storage().lock() {
        *g = name.to_string();
    }
}

fn clear_mesh_local_display_name() {
    if let Ok(mut g) = mesh_local_display_name_storage().lock() {
        g.clear();
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct MeshConnectionPayload {
    pub connected: bool,
}

/// Whether the Meshtastic serial link is currently up (configured and streaming).
pub fn mesh_radio_is_connected() -> bool {
    MESH_RADIO_CONNECTED.load(Ordering::Relaxed)
}

/// This device's Meshtastic node number when known (0 if radio not configured yet).
pub fn mesh_local_node_id() -> u32 {
    MESH_MY_NODE_NUM.load(Ordering::Relaxed)
}

fn notify_mesh_connected(app: &tauri::AppHandle, connected: bool) {
    MESH_RADIO_CONNECTED.store(connected, Ordering::Relaxed);
    if !connected {
        MESH_MY_NODE_NUM.store(0, Ordering::Relaxed);
        clear_mesh_local_display_name();
    }
    let payload = MeshConnectionPayload { connected };
    if let Err(e) = app.emit(MESH_CONNECTION_EVENT, &payload) {
        eprintln!("[mesh] emit {MESH_CONNECTION_EVENT} failed: {e}");
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct MeshTextEventPayload {
    /// Meshtastic `MeshPacket.from` (sending node number).
    pub sender_id: u32,
    /// Meshtastic `MeshPacket.to` (broadcast is `0xFFFF_FFFF`).
    pub to: u32,
    /// Meshtastic channel index.
    pub channel: u32,
    pub text: String,
}

pub(crate) enum MeshCommand {
    SendText {
        text: String,
        want_ack: bool,
        channel: u32,
        /// `None` = broadcast; `Some(node_id)` = direct message to that node.
        destination: Option<u32>,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Create (or reuse) the private ATHENA channel on the radio. Replies with its index.
    SetupAthenaChannel {
        reply: tokio::sync::oneshot::Sender<Result<u32, String>>,
    },
}

/// Enqueue outgoing mesh traffic from Tauri commands; the dedicated Meshtastic thread performs I/O.
#[derive(Clone)]
pub struct MeshRadioControl {
    cmd_tx: Sender<MeshCommand>,
}

impl MeshRadioControl {
    pub async fn send_text_message(
        &self,
        text: String,
        want_ack: bool,
        channel: u32,
        destination_node_id: Option<u32>,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let cmd = MeshCommand::SendText {
            text,
            want_ack,
            channel,
            destination: destination_node_id,
            reply: reply_tx,
        };
        match self.cmd_tx.try_send(cmd) {
            Ok(()) => {}
            Err(TrySendError::Full(cmd)) => {
                fail_mesh_command(
                    cmd,
                    "Meshtastic command queue is full; wait and retry.".to_string(),
                );
                return Err("Meshtastic command queue is full; wait and retry.".to_string());
            }
            Err(TrySendError::Closed(cmd)) => {
                fail_mesh_command(
                    cmd,
                    "Meshtastic background thread is not running.".to_string(),
                );
                return Err("Meshtastic background thread is not running.".to_string());
            }
        }
        reply_rx
            .await
            .map_err(|_| "Meshtastic send was cancelled.".to_string())?
    }

    pub async fn setup_athena_channel(&self) -> Result<u32, String> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .try_send(MeshCommand::SetupAthenaChannel { reply })
            .map_err(|_| "The radio link is busy or not running; try again.".to_string())?;
        rx.await.map_err(|_| "Channel setup was cancelled.".to_string())?
    }

    pub(crate) fn from_sender(cmd_tx: Sender<MeshCommand>) -> Self {
        Self { cmd_tx }
    }
}

fn fail_mesh_command(cmd: MeshCommand, err: String) {
    match cmd {
        MeshCommand::SendText { reply, .. } => {
            let _ = reply.send(Err(err));
        }
        MeshCommand::SetupAthenaChannel { reply } => {
            let _ = reply.send(Err(err));
        }
    }
}

/// After another device sends `ATHLB1:{"t":"req"}`, push recent local rows on the ATHENA channel (paced for LoRa).
async fn respond_logbook_sync_request(
    db: Arc<Mutex<Database>>,
    cmd_tx: Sender<MeshCommand>,
    my_node: u32,
) {
    if my_node == 0 {
        return;
    }
    // Station records go out only on the private ATHENA channel.
    let Ok(sync_ch) = mesh_policy::sync_channel() else { return };
    let entries = match db
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|g| g.list_logbook_recent_for_mesh_share(12))
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[mesh] logbook sync: list failed: {e}");
            return;
        }
    };
    for entry in entries {
        let wire = mesh_logbook::wire_from_logbook_entry(&entry, my_node);
        let packet = match mesh_logbook::encode_logbook_packet(&wire) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        match cmd_tx.try_send(MeshCommand::SendText {
            text: packet,
            want_ack: false,
            channel: sync_ch,
            destination: None,
            reply: reply_tx,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(cmd)) => {
                fail_mesh_command(cmd, "Meshtastic command queue is full.".to_string());
                break;
            }
            Err(TrySendError::Closed(cmd)) => {
                fail_mesh_command(cmd, "Meshtastic command channel closed.".to_string());
                break;
            }
        }
        let _ = reply_rx.await;
        tokio::time::sleep(std::time::Duration::from_millis(420)).await;
    }
}

/// After another device sends `ATHCAL1:{"t":"req"}`, push recent local calendar rows on the ATHENA channel.
async fn respond_calendar_sync_request(
    db: Arc<Mutex<Database>>,
    cmd_tx: Sender<MeshCommand>,
    my_node: u32,
) {
    if my_node == 0 {
        return;
    }
    // Station records go out only on the private ATHENA channel.
    let Ok(sync_ch) = mesh_policy::sync_channel() else { return };
    let rows = match db
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|g| g.list_calendar_recent_for_mesh_share(16))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[mesh] calendar sync: list failed: {e}");
            return;
        }
    };
    for ev in rows {
        let wire = mesh_calendar::wire_from_calendar_event(&ev, my_node);
        let packet = match mesh_calendar::encode_calendar_packet(&wire) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        match cmd_tx.try_send(MeshCommand::SendText {
            text: packet,
            want_ack: false,
            channel: sync_ch,
            destination: None,
            reply: reply_tx,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(cmd)) => {
                fail_mesh_command(cmd, "Meshtastic command queue is full.".to_string());
                break;
            }
            Err(TrySendError::Closed(cmd)) => {
                fail_mesh_command(cmd, "Meshtastic command channel closed.".to_string());
                break;
            }
        }
        let _ = reply_rx.await;
        tokio::time::sleep(std::time::Duration::from_millis(420)).await;
    }
}

pub const ATHENA_COMMAND_NOTIFICATION: &str = "athena:notification";

fn maybe_emit_distress_notification(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    p: &MeshTextEventPayload,
) {
    if p.channel != 0 {
        return;
    }
    let preview: String = p.text.chars().take(200).collect();
    if preview.trim().is_empty() {
        return;
    }
    if let Ok(g) = db.lock() {
        if let Ok(n) = g.command_notification_insert(
            "distress",
            "high",
            "Channel 0 (open / distress)",
            Some(&format!("Node {}: {}", p.sender_id, preview)),
            None,
        ) {
            let _ = app.emit(ATHENA_COMMAND_NOTIFICATION, &n);
        }
    }
}

/// Throttles mesh-text UI events to at most five per second; drops when the queue is full.
async fn mesh_text_emit_loop(
    app: tauri::AppHandle,
    db: Arc<Mutex<Database>>,
    mut rx: Receiver<MeshTextEventPayload>,
) {
    let mut ticker = interval(std::time::Duration::from_millis(MESH_TEXT_EMIT_INTERVAL_MS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut pending = VecDeque::new();
    loop {
        tokio::select! {
            opt = rx.recv() => {
                match opt {
                    Some(p) => {
                        pending.push_back(p);
                        while pending.len() > MESH_TEXT_PENDING_CAP {
                            pending.pop_front();
                        }
                    }
                    None => {
                        while let Some(p) = pending.pop_front() {
                            maybe_emit_distress_notification(&app, &db, &p);
                            if let Err(e) = app.emit(MESH_TEXT_EVENT, &p) {
                                eprintln!("[mesh] emit {MESH_TEXT_EVENT} failed: {e}");
                            }
                        }
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                if let Some(p) = pending.pop_front() {
                    maybe_emit_distress_notification(&app, &db, &p);
                    if let Err(e) = app.emit(MESH_TEXT_EVENT, &p) {
                        eprintln!("[mesh] emit {MESH_TEXT_EVENT} failed: {e}");
                    }
                }
            }
        }
    }
}

/// Spawns a dedicated thread with a multi-thread Tokio runtime. Register the returned
/// [`MeshRadioControl`] with [`tauri::Manager::manage`].
pub fn spawn_mesh_radio_listener(
    app_handle: tauri::AppHandle,
    app_data_dir: PathBuf,
    db: Arc<Mutex<Database>>,
) -> MeshRadioControl {
    let (mesh_tx, mesh_rx) = mpsc::channel::<MeshTextEventPayload>(MESH_TEXT_QUEUE_CAPACITY);
    let (cmd_tx, cmd_rx) = mpsc::channel::<MeshCommand>(MESH_CMD_QUEUE_CAPACITY);
    let control = MeshRadioControl {
        cmd_tx: cmd_tx.clone(),
    };
    let spawn_result = std::thread::Builder::new()
        .name("athena-meshtastic".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("athena-mesh-tokio")
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[mesh] failed to create Tokio runtime: {e}");
                    return;
                }
            };
            runtime.block_on(async move {
                let db_emit = Arc::clone(&db);
                tokio::spawn(mesh_text_emit_loop(app_handle.clone(), db_emit, mesh_rx));
                mesh_radio_main_loop(app_handle, app_data_dir, db, cmd_rx, cmd_tx, mesh_tx).await;
            });
        });
    if let Err(e) = spawn_result {
        eprintln!("[mesh] failed to spawn listener thread: {e}");
    }
    control
}

/// Required by `ConnectedStreamApi::send_text` (source node id + echo handling).
struct AthenaPacketRouter {
    my_node_num: Arc<AtomicU32>,
}

impl AthenaPacketRouter {
    fn new(my_node_num: Arc<AtomicU32>) -> Self {
        Self { my_node_num }
    }
}

impl PacketRouter<(), PacketRouterError> for AthenaPacketRouter {
    fn handle_packet_from_radio(
        &mut self,
        _packet: protobufs::FromRadio,
    ) -> Result<(), PacketRouterError> {
        Ok(())
    }

    fn handle_mesh_packet(
        &mut self,
        _packet: protobufs::MeshPacket,
    ) -> Result<(), PacketRouterError> {
        Ok(())
    }

    fn source_node_id(&self) -> NodeId {
        NodeId::new(self.my_node_num.load(Ordering::Relaxed))
    }
}

fn extract_my_node_num(from_radio: &protobufs::FromRadio) -> Option<u32> {
    match &from_radio.payload_variant {
        Some(protobufs::from_radio::PayloadVariant::MyInfo(info)) => Some(info.my_node_num),
        _ => None,
    }
}

fn ingest_node_display_name(
    from_radio: &protobufs::FromRadio,
    map: &Mutex<HashMap<u32, String>>,
    my_node_num: &Arc<AtomicU32>,
) {
    let Some(protobufs::from_radio::PayloadVariant::NodeInfo(ni)) = &from_radio.payload_variant
    else {
        return;
    };
    let Some(ref user) = ni.user else {
        return;
    };
    let name = if !user.long_name.is_empty() {
        user.long_name.clone()
    } else if !user.short_name.is_empty() {
        user.short_name.clone()
    } else {
        return;
    };
    if let Ok(mut g) = map.lock() {
        g.insert(ni.num, name.clone());
    }
    if ni.num == my_node_num.load(Ordering::Relaxed) {
        set_mesh_local_display_name(&name);
    }
}

/// Hex preview of raw bytes for terminal debugging (Meshtastic serial is framed into protobuf `FromRadio` by the `meshtastic` crate before we see it here).
fn mesh_hex_preview(bytes: &[u8], max: usize) -> String {
    let n = bytes.len().min(max);
    let mut s = String::with_capacity(n.saturating_mul(3).saturating_add(4));
    for (i, b) in bytes.iter().take(n).enumerate() {
        if i > 0 {
            s.push(' ');
        }
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    if bytes.len() > max {
        s.push_str(" …");
    }
    s
}

/// Log every `FromRadio` as hex — only when `ATHENA_MESH_SERIAL_DEBUG=1` (very noisy on busy meshes).
fn log_mesh_raw_incoming(packet: &protobufs::FromRadio) {
    if !mesh_serial_debug_hex_enabled() {
        return;
    }
    let mut buf = Vec::with_capacity(packet.encoded_len().saturating_add(32));
    match packet.encode(&mut buf) {
        Ok(()) => {
            eprintln!(
                "[MESH RAW] id={} len={} hex={}",
                packet.id,
                buf.len(),
                mesh_hex_preview(&buf, 96)
            );
        }
        Err(e) => {
            eprintln!(
                "[MESH RAW] id={} encode_error={e} (has_payload={})",
                packet.id,
                packet.payload_variant.is_some()
            );
        }
    }
}

fn format_lat_lon_short(lat_deg: Option<f64>, lon_deg: Option<f64>) -> String {
    match (lat_deg, lon_deg) {
        (Some(lat), Some(lon)) => {
            let ns = if lat >= 0.0 { 'N' } else { 'S' };
            let ew = if lon >= 0.0 { 'E' } else { 'W' };
            format!("{:.4}°{} {:.4}°{}", lat.abs(), ns, lon.abs(), ew)
        }
        _ => "no fix".to_string(),
    }
}

/// Decoded text / position packets: one short terminal line each (busy meshes stay readable).
fn log_mesh_text_and_gps_payloads(packet: &protobufs::FromRadio) {
    let Some(protobufs::from_radio::PayloadVariant::Packet(mesh_packet)) = packet.payload_variant.as_ref()
    else {
        return;
    };
    let Some(inner) = mesh_packet.payload_variant.as_ref() else {
        return;
    };
    let data = match inner {
        protobufs::mesh_packet::PayloadVariant::Decoded(d) => d,
        protobufs::mesh_packet::PayloadVariant::Encrypted(_) => return,
    };
    let port = data.portnum;
    let text_app = protobufs::PortNum::TextMessageApp as i32;
    let pos_app = protobufs::PortNum::PositionApp as i32;

    if port == text_app {
        let preview: String = String::from_utf8_lossy(&data.payload)
            .chars()
            .map(|c| if c == '\n' { ' ' } else { c })
            .take(72)
            .collect();
        let tail = if data.payload.len() > 72 { "…" } else { "" };
        eprintln!(
            "[mesh] text from={} ch={} ({} B): {}{}",
            mesh_packet.from,
            mesh_packet.channel,
            data.payload.len(),
            preview,
            tail
        );
        return;
    }

    if port == pos_app {
        match protobufs::Position::decode(data.payload.as_slice()) {
            Ok(pos) => {
                let lat_deg = pos.latitude_i.map(|i| (i as f64) * 1e-7);
                let lon_deg = pos.longitude_i.map(|i| (i as f64) * 1e-7);
                let coords = format_lat_lon_short(lat_deg, lon_deg);
                let alt = pos
                    .altitude
                    .map(|m| format!("{m} m MSL"))
                    .unwrap_or_else(|| "alt ?".to_string());
                eprintln!(
                    "[mesh] pos from={} ch={} {} · {}",
                    mesh_packet.from, mesh_packet.channel, coords, alt
                );
            }
            Err(e) => {
                eprintln!(
                    "[mesh] pos decode err from={} ch={}: {e} ({} B)",
                    mesh_packet.from,
                    mesh_packet.channel,
                    data.payload.len()
                );
            }
        }
    }
}

/// Decoded mesh text: sender, destination, channel field, payload.
fn try_decode_mesh_text(from_radio: &protobufs::FromRadio) -> Option<(u32, u32, u32, String)> {
    let payload_variant = from_radio.payload_variant.as_ref()?;
    let mesh_packet = match payload_variant {
        protobufs::from_radio::PayloadVariant::Packet(p) => p,
        _ => return None,
    };
    let inner = mesh_packet.payload_variant.as_ref()?;
    let data = match inner {
        protobufs::mesh_packet::PayloadVariant::Decoded(d) => d,
        protobufs::mesh_packet::PayloadVariant::Encrypted(_) => return None,
    };
    if data.portnum != protobufs::PortNum::TextMessageApp as i32 {
        return None;
    }
    let text = String::from_utf8_lossy(&data.payload).into_owned();
    Some((mesh_packet.from, mesh_packet.to, mesh_packet.channel, text))
}



/// Store sensor readings carried by a telemetry packet (battery, temperature, pressure, …).
fn ingest_mesh_telemetry_from_packet(
    packet: &protobufs::FromRadio,
    db: &Arc<Mutex<Database>>,
    node_labels: &Arc<Mutex<HashMap<u32, String>>>,
) {
    let Some(protobufs::from_radio::PayloadVariant::Packet(mesh_packet)) = packet.payload_variant.as_ref()
    else {
        return;
    };
    let Some(protobufs::mesh_packet::PayloadVariant::Decoded(data)) = mesh_packet.payload_variant.as_ref()
    else {
        return;
    };
    if data.portnum != protobufs::PortNum::TelemetryApp as i32 {
        return;
    }
    let Ok(telemetry) = protobufs::Telemetry::decode(data.payload.as_slice()) else {
        return;
    };
    let Some(variant) = telemetry.variant.as_ref() else {
        return;
    };
    if mesh_packet.from == MESH_MY_NODE_NUM.load(Ordering::Relaxed) {
        if let protobufs::telemetry::Variant::DeviceMetrics(m) = variant {
            if let Some(u) = m.channel_utilization {
                mesh_policy::set_channel_utilization(u);
            }
        }
    }
    let metrics = crate::field_ops::sensors::metrics_from_telemetry(variant);
    if metrics.is_empty() {
        return;
    }
    let label = node_labels
        .lock()
        .ok()
        .and_then(|m| m.get(&mesh_packet.from).cloned())
        .unwrap_or_else(|| format!("Node {}", mesh_packet.from));
    if let Ok(g) = db.lock() {
        if let Err(e) = crate::field_ops::sensors::ingest_node_metrics(
            &g,
            i64::from(mesh_packet.from),
            &label,
            &metrics,
            chrono::Utc::now().timestamp(),
        ) {
            eprintln!("[mesh] telemetry store failed: {e}");
        }
    }
}

fn ingest_mesh_position_from_packet(
    packet: &protobufs::FromRadio,
    db: &Arc<Mutex<Database>>,
    node_labels: &Arc<Mutex<HashMap<u32, String>>>,
) {
    let Some(protobufs::from_radio::PayloadVariant::Packet(mesh_packet)) = packet.payload_variant.as_ref()
    else {
        return;
    };
    let Some(inner) = mesh_packet.payload_variant.as_ref() else {
        return;
    };
    let data = match inner {
        protobufs::mesh_packet::PayloadVariant::Decoded(d) => d,
        protobufs::mesh_packet::PayloadVariant::Encrypted(_) => return,
    };
    if data.portnum != protobufs::PortNum::PositionApp as i32 {
        return;
    }
    let Ok(pos) = protobufs::Position::decode(data.payload.as_slice()) else {
        return;
    };
    let lat_deg = pos.latitude_i.map(|i| (i as f64) * 1e-7);
    let lon_deg = pos.longitude_i.map(|i| (i as f64) * 1e-7);
    let (Some(lat), Some(lon)) = (lat_deg, lon_deg) else {
        return;
    };
    if !lat.is_finite() || !lon.is_finite() || (lat.abs() < 1e-6 && lon.abs() < 1e-6) {
        return;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let label = node_labels
        .lock()
        .ok()
        .and_then(|g| g.get(&mesh_packet.from).cloned())
        .unwrap_or_default();
    let _ = db.lock().ok().and_then(|g| {
        g.upsert_mesh_node_position_live(mesh_packet.from, lat, lon, now_ms, &label)
            .ok()
    });
}

/// Whether a packet was encrypted to this radio's public key, and the sender's key.
fn packet_security(from_radio: &protobufs::FromRadio) -> (bool, Vec<u8>) {
    match from_radio.payload_variant.as_ref() {
        Some(protobufs::from_radio::PayloadVariant::Packet(p)) => (p.pki_encrypted, p.public_key.clone()),
        _ => (false, Vec::new()),
    }
}

/// Channels on the connected radio, as it reported them while connecting.
fn radio_channels() -> &'static Mutex<HashMap<u32, protobufs::Channel>> {
    static C: OnceLock<Mutex<HashMap<u32, protobufs::Channel>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn record_radio_channel(from_radio: &protobufs::FromRadio) {
    if let Some(protobufs::from_radio::PayloadVariant::Channel(ch)) = from_radio.payload_variant.as_ref() {
        if let (Ok(idx), Ok(mut g)) = (u32::try_from(ch.index), radio_channels().lock()) {
            g.insert(idx, ch.clone());
        }
    }
}

const ATHENA_CHANNEL_NAME: &str = "ATHENA";

/// What the operator sees about the private channel. The join link carries the channel key, so
/// it is shown only to people with clearance 4 or higher.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AthenaChannelStatus {
    pub radio_connected: bool,
    pub configured_index: Option<u32>,
    /// The radio has an enabled channel at that index named ATHENA.
    pub on_radio: bool,
    pub join_url: Option<String>,
    pub channel_utilization: Option<f32>,
}

pub fn athena_channel_status(include_join_url: bool) -> AthenaChannelStatus {
    let idx = mesh_policy::athena_channel();
    let ch = idx.and_then(|i| radio_channels().lock().ok().and_then(|g| g.get(&i).cloned()));
    let on_radio = ch.as_ref().is_some_and(|c| {
        c.role != protobufs::channel::Role::Disabled as i32
            && c.settings.as_ref().is_some_and(|s| s.name == ATHENA_CHANNEL_NAME && s.psk.len() == 32)
    });
    let join_url = if include_join_url && on_radio {
        ch.and_then(|c| c.settings).map(|settings| join_url_for(settings))
    } else {
        None
    };
    AthenaChannelStatus {
        radio_connected: mesh_radio_is_connected(),
        configured_index: idx,
        on_radio,
        join_url,
        channel_utilization: mesh_policy::channel_utilization(),
    }
}

/// A Meshtastic "add channel" link, which phones scan to join the channel without changing
/// their other channels.
fn join_url_for(settings: protobufs::ChannelSettings) -> String {
    use base64::Engine;
    let set = protobufs::ChannelSet { settings: vec![settings], lora_config: None };
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(set.encode_to_vec());
    format!("https://meshtastic.org/e/?add=true#{b64}")
}

/// Pick where the ATHENA channel goes: an existing ATHENA channel, else the first free slot.
fn athena_channel_slot(channels: &HashMap<u32, protobufs::Channel>) -> Result<(u32, Option<protobufs::ChannelSettings>), String> {
    for i in 1..=7u32 {
        if let Some(c) = channels.get(&i) {
            if c.role != protobufs::channel::Role::Disabled as i32 {
                if let Some(s) = c.settings.as_ref().filter(|s| s.name == ATHENA_CHANNEL_NAME && s.psk.len() == 32) {
                    return Ok((i, Some(s.clone())));
                }
            }
        }
    }
    for i in 1..=7u32 {
        let free = channels.get(&i).is_none_or(|c| c.role == protobufs::channel::Role::Disabled as i32);
        if free {
            return Ok((i, None));
        }
    }
    Err("All 7 secondary channels on the radio are in use. Remove one in the Meshtastic app first.".to_string())
}

fn try_decode_mesh_alert_text(from_radio: &protobufs::FromRadio) -> Option<(u32, u32, u32, String)> {
    let payload_variant = from_radio.payload_variant.as_ref()?;
    let mesh_packet = match payload_variant {
        protobufs::from_radio::PayloadVariant::Packet(p) => p,
        _ => return None,
    };
    let inner = mesh_packet.payload_variant.as_ref()?;
    let data = match inner {
        protobufs::mesh_packet::PayloadVariant::Decoded(d) => d,
        protobufs::mesh_packet::PayloadVariant::Encrypted(_) => return None,
    };
    if data.portnum != protobufs::PortNum::AlertApp as i32 {
        return None;
    }
    let text = String::from_utf8_lossy(&data.payload).into_owned();
    Some((mesh_packet.from, mesh_packet.to, mesh_packet.channel, text))
}

async fn mesh_listener_handle_priority_sos(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    sink: &std::sync::Arc<dyn MeshDmSink>,
    from: u32,
    to: u32,
    channel: u32,
    text: &str,
    my_node: u32,
) -> bool {
    let on_public = channel == PRIMARY_MESH_CHANNEL && to == MESHTASTIC_BROADCAST_TO;
    let on_private = mesh_policy::accept_sync(channel, to);
    if !on_public && !on_private {
        return false;
    }
    if my_node != 0 && from == my_node {
        return false;
    }
    let node_i64 = i64::from(from);
    let trimmed = text.trim();
    let upper = trimmed.to_ascii_uppercase();
    // "HELP" is common in ordinary chat on the public channel; only "SOS" counts there.
    if upper == "HELP" && !on_private {
        return false;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    if upper == "SOS" || upper == "HELP" {
        let had_profile = db
            .lock()
            .ok()
            .and_then(|g| g.mesh_profile_get_by_node(node_i64).ok())
            .flatten()
            .is_some();
        if !had_profile {
            let ok_ins = db
                .lock()
                .ok()
                .and_then(|g| g.mesh_profile_insert_sos_unknown_node(node_i64).ok())
                .is_some();
            if !ok_ins {
                let _ = db
                    .lock()
                    .ok()
                    .and_then(|g| g.mesh_profile_set_sos_state(node_i64, SOS_STATE_ACTIVE).ok());
            }
        } else {
            let _ = db
                .lock()
                .ok()
                .and_then(|g| g.mesh_profile_set_sos_state(node_i64, SOS_STATE_ACTIVE).ok());
        }
        emit_mesh_emergency(app, db, from);
        let (registered, pos_line) = {
            let g = match db.lock() {
                Ok(g) => g,
                Err(_) => return true,
            };
            let prof = g.mesh_profile_get_by_node(node_i64).ok().flatten();
            let reg = prof.as_ref().map(|p| p.is_registered).unwrap_or(false);
            let pos = g.mesh_last_position_for_node(from).ok().flatten();
            let line = pos.map(|(la, lo, _)| {
                format!(
                    "Last known position: {:.5}, {:.5} (from mesh position packet).",
                    la, lo
                )
            });
            (reg, line)
        };
        if !registered {
            send_dm_chunks(
                sink,
                &["SOS Received. Command notified. Identity unknown. Reply with your operating callsign or PIN for verification."],
                channel,
                from,
            )
            .await;
        } else {
            let mut lines: Vec<String> = vec!["SOS Received. Command notified.".to_string()];
            if let Some(p) = pos_line {
                lines.push(p);
            }
            lines.push(
                "Reply SOS CLEAR followed by your secret phrase to request clearance. Command will verify."
                    .to_string(),
            );
            let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
            send_dm_chunks(sink, &refs, channel, from).await;
        }
        return true;
    }

    if let Some(phrase) = parse_sos_clear_tail(trimmed) {
        let st = db
            .lock()
            .ok()
            .map(|g| g.mesh_profile_sos_state(node_i64))
            .unwrap_or(SOS_STATE_NORMAL);
        if st == SOS_STATE_ACTIVE {
            let row_id = uuid::Uuid::new_v4().to_string();
            let content = format!("[CLEARANCE] secret_phrase={}", phrase);
            {
                let g = match db.lock() {
                    Ok(g) => g,
                    Err(_) => return true,
                };
                let _ = g.mesh_profile_set_sos_state(node_i64, SOS_STATE_SUSPENDED);
                let _ = g.mesh_chat_insert(
                    &row_id,
                    node_i64,
                    "remote",
                    "clearance",
                    &content,
                    now_ms,
                );
            }
            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );
            let preview: String = content.chars().take(200).collect();
            let _ = app.emit(
                MESH_OPERATOR_QUEUE_MESSAGE,
                MeshOperatorQueuePayload {
                    node_id: from,
                    preview,
                    kind: "clearance".to_string(),
                },
            );
            send_dm_chunks(
                sink,
                &["Clear command received. Standby for Command verification."],
                channel,
                from,
            )
            .await;
            emit_mesh_emergency(app, db, from);
            return true;
        }
    }

    false
}

enum RadioStream {
    Serial(meshtastic::api::StreamHandle<tokio_serial::SerialStream>),
    Tcp(meshtastic::api::StreamHandle<tokio::net::TcpStream>),
}

/// `host` or `host:port` for a radio on the network (Meshtastic's API port is 4403).
pub fn radio_tcp_address(raw: &str) -> Option<String> {
    let t = raw.trim().trim_start_matches("tcp://").trim_end_matches('/');
    if t.is_empty() || t.contains(char::is_whitespace) || t.contains('/') {
        return None;
    }
    let has_port = t.rsplit_once(':').is_some_and(|(h, p)| !h.is_empty() && !h.ends_with(':') && p.parse::<u16>().is_ok());
    Some(if has_port { t.to_string() } else { format!("{t}:4403") })
}

async fn setup_athena_channel(
    stream_api: &mut meshtastic::api::ConnectedStreamApi<meshtastic::api::state::Configured>,
    router: &mut AthenaPacketRouter,
    app_data_dir: &std::path::Path,
) -> Result<u32, String> {
    let channels = radio_channels().lock().map_err(|e| e.to_string())?.clone();
    if channels.is_empty() {
        return Err("The radio has not reported its channels yet. Wait a few seconds after it connects.".to_string());
    }
    let (index, existing) = athena_channel_slot(&channels)?;
    if existing.is_none() {
        use rand::RngCore;
        let mut psk = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut psk);
        let settings = protobufs::ChannelSettings {
            psk,
            name: ATHENA_CHANNEL_NAME.to_string(),
            // Keep station traffic off the internet bridge.
            uplink_enabled: false,
            downlink_enabled: false,
            ..Default::default()
        };
        let channel = protobufs::Channel {
            index: index as i32,
            settings: Some(settings),
            role: protobufs::channel::Role::Secondary as i32,
        };
        stream_api.start_config_transaction().await.map_err(|e| e.to_string())?;
        stream_api.update_channel_config(router, channel.clone()).await.map_err(|e| e.to_string())?;
        stream_api.commit_config_transaction().await.map_err(|e| e.to_string())?;
        if let Ok(mut g) = radio_channels().lock() {
            g.insert(index, channel);
        }
    }
    let mut cfg = crate::config::load_config(app_data_dir);
    cfg.mesh_athena_channel = Some(index);
    crate::config::save_config(app_data_dir, &cfg)?;
    mesh_policy::set_athena_channel(Some(index));
    Ok(index)
}

async fn sleep_or_command(
    duration: std::time::Duration,
    cmd_rx: &mut Receiver<MeshCommand>,
) -> Option<MeshCommand> {
    tokio::select! {
        _ = tokio::time::sleep(duration) => None,
        cmd = cmd_rx.recv() => cmd,
    }
}

/// Fast retries while the user may be connecting, then slow polling to reduce CPU/log overhead.
/// A plugged-in radio is still detected on the next slow interval.
const SERIAL_OPEN_BURST_ATTEMPTS: u32 = 5;
const SERIAL_OPEN_BURST_DELAY_SECS: u64 = 3;
const SERIAL_OPEN_IDLE_DELAY_SECS: u64 = 90;

async fn mesh_radio_main_loop(
    app: tauri::AppHandle,
    app_data_dir: PathBuf,
    db: Arc<Mutex<Database>>,
    mut cmd_rx: Receiver<MeshCommand>,
    cmd_tx: Sender<MeshCommand>,
    mesh_tx: Sender<MeshTextEventPayload>,
) {
    let my_node_num = Arc::new(AtomicU32::new(0));
    let node_names: Arc<Mutex<HashMap<u32, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let ai_permits = Arc::new(Semaphore::new(3));
    let mut last_sent: Option<std::time::Instant> = None;

    loop {
        notify_mesh_connected(&app, false);
        let cfg = crate::config::load_config(app_data_dir.as_path());
        mesh_policy::set_athena_channel(cfg.mesh_athena_channel);
        if let Ok(mut g) = radio_channels().lock() {
            g.clear();
        }
        let mut serial_open_failures: u32 = 0;
        let radio_address = cfg.mesh_radio_address.as_deref().and_then(radio_tcp_address);
        let stream_handle = 'open_serial: loop {
            if let Some(addr) = radio_address.clone() {
                match meshtastic::utils::stream::build_tcp_stream(addr.clone()).await {
                    Ok(h) => break 'open_serial RadioStream::Tcp(h),
                    Err(e) => {
                        serial_open_failures += 1;
                        if serial_open_failures == 1 || serial_open_failures % 30 == 0 {
                            eprintln!("[mesh] radio at {addr} not reachable: {e}");
                        }
                        let delay = if serial_open_failures <= SERIAL_OPEN_BURST_ATTEMPTS { SERIAL_OPEN_BURST_DELAY_SECS } else { SERIAL_OPEN_IDLE_DELAY_SECS };
                        if let Some(cmd) = sleep_or_command(std::time::Duration::from_secs(delay), &mut cmd_rx).await {
                            fail_mesh_command(cmd, format!("Meshtastic radio at {addr} is not reachable."));
                        }
                        continue;
                    }
                }
            }
            let link = MeshRadioSerialLink::new(None);
            match link.open_stream() {
                Ok(h) => break 'open_serial RadioStream::Serial(h),
                Err(e) => {
                    serial_open_failures += 1;
                    if serial_open_failures == 1 {
                        eprintln!("[mesh] no serial yet: {e}");
                    } else if serial_open_failures == SERIAL_OPEN_BURST_ATTEMPTS + 1 {
                        eprintln!(
                            "[mesh] no serial after {SERIAL_OPEN_BURST_ATTEMPTS} tries; polling every {SERIAL_OPEN_IDLE_DELAY_SECS}s (plug in radio when ready)"
                        );
                    }
                    let delay_secs = if serial_open_failures <= SERIAL_OPEN_BURST_ATTEMPTS {
                        SERIAL_OPEN_BURST_DELAY_SECS
                    } else {
                        SERIAL_OPEN_IDLE_DELAY_SECS
                    };
                    match sleep_or_command(std::time::Duration::from_secs(delay_secs), &mut cmd_rx)
                        .await
                    {
                        None => continue,
                        Some(cmd) => fail_mesh_command(
                            cmd,
                            "Meshtastic radio not connected (no serial port).".to_string(),
                        ),
                    }
                }
            }
        };

        let stream_api = StreamApi::new();
        let (mut packet_rx, connected) = match stream_handle {
            RadioStream::Serial(h) => stream_api.connect(h).await,
            RadioStream::Tcp(h) => stream_api.connect(h).await,
        };
        let config_id = meshtastic::utils::generate_rand_id();
        let mut stream_api = match connected.configure(config_id).await {
            Ok(api) => api,
            Err(e) => {
                eprintln!("[mesh] configure failed: {e}");
                drop(packet_rx);
                match sleep_or_command(std::time::Duration::from_secs(2), &mut cmd_rx).await {
                    None => {}
                    Some(cmd) => fail_mesh_command(
                        cmd,
                        "Meshtastic configure failed; reconnecting.".to_string(),
                    ),
                }
                continue;
            }
        };

        let mut router = AthenaPacketRouter::new(Arc::clone(&my_node_num));
        eprintln!("[mesh] serial connected and configured");
        notify_mesh_connected(&app, true);

        loop {
            tokio::select! {
                from_radio = packet_rx.recv() => {
                    match from_radio {
                        Some(packet) => {
                            log_mesh_raw_incoming(&packet);
                            log_mesh_text_and_gps_payloads(&packet);

                            if let Some(n) = extract_my_node_num(&packet) {
                                my_node_num.store(n, Ordering::Relaxed);
                                MESH_MY_NODE_NUM.store(n, Ordering::Relaxed);
                                if let Ok(g) = node_names.lock() {
                                    if let Some(nm) = g.get(&n) {
                                        set_mesh_local_display_name(nm);
                                    }
                                }
                            }
                            record_radio_channel(&packet);
                            ingest_node_display_name(&packet, &node_names, &my_node_num);
                            ingest_mesh_position_from_packet(&packet, &db, &node_names);
                            ingest_mesh_telemetry_from_packet(&packet, &db, &node_names);

                            if let Some((from, to, channel, text)) = try_decode_mesh_text(&packet) {
                                let sink_sos: std::sync::Arc<dyn MeshDmSink> =
                                    std::sync::Arc::new(MeshRadioControl::from_sender(cmd_tx.clone()));
                                let my_n = my_node_num.load(Ordering::Relaxed);
                                let consumed_sos = mesh_listener_handle_priority_sos(
                                    &app,
                                    &db,
                                    &sink_sos,
                                    from,
                                    to,
                                    channel,
                                    &text,
                                    my_n,
                                )
                                .await;
                                let is_station_record = mesh_logbook::is_logbook_mesh_text(&text)
                                    || mesh_calendar::is_calendar_mesh_text(&text)
                                    || crate::field_ops::picture::is_marker_packet(&text);
                                if !consumed_sos
                                    && to == MESHTASTIC_BROADCAST_TO
                                    && is_station_record
                                    && !mesh_policy::accept_sync(channel, to)
                                {
                                    // Station records from outside the private channel are not trusted.
                                    eprintln!("[mesh] ignored station record on channel {channel} (not the ATHENA channel)");
                                } else if !consumed_sos && to == MESHTASTIC_BROADCAST_TO {
                                    if let Some(inbound) = mesh_logbook::try_parse_logbook_packet(&text) {
                                        match inbound {
                                            mesh_logbook::LogbookMeshInbound::SyncRequest => {
                                                let my = my_node_num.load(Ordering::Relaxed);
                                                // Ignore our own echoed sync request (saves airtime).
                                                if my != 0 && from != my {
                                                    let db_s = Arc::clone(&db);
                                                    let cmd_s = cmd_tx.clone();
                                                    tokio::spawn(async move {
                                                        respond_logbook_sync_request(db_s, cmd_s, my).await;
                                                    });
                                                }
                                            }
                                            mesh_logbook::LogbookMeshInbound::Entry(wire) => {
                                                let author = wire.n.unwrap_or(from);
                                                match db.lock().ok().map(|g| {
                                                    g.try_insert_logbook_from_mesh(
                                                        &wire.id,
                                                        &wire.b,
                                                        wire.c,
                                                        author,
                                                    )
                                                }) {
                                                    Some(Ok(true)) => {
                                                        let entry = LogbookEntry {
                                                            id: wire.id.clone(),
                                                            body: wire.b.clone(),
                                                            created_at: wire.c,
                                                            from_node_id: Some(author),
                                                            author_name: String::new(),
                                                            author_occupation: String::new(),
                                                            user_id: None,
                                                            author_rank_level: None,
                                                            author_division: None,
                                                            author_directorate: None,
                                                            author_tier: None,
                                                            author_job_title: None,
                                                            map_lat: None,
                                                            map_lon: None,
                                                            saved_place_id: None,
                                                            location_text: None,
                                                            parent_entry_id: None,
                                                            notebook: "general".to_string(),
                                                        };
                                                        if let Ok(v) = serde_json::to_value(&entry) {
                                                            lan_sync::publish_local_sync_row(
                                                                lan_sync::LAN_TBL_LOGBOOK_ENTRIES,
                                                                v,
                                                            );
                                                        }
                                                        let _ = app.emit("athena:logbook-updated", ());
                                                    }
                                                    Some(Ok(false)) => {}
                                                    Some(Err(e)) => {
                                                        eprintln!("[mesh] logbook insert failed: {e}");
                                                    }
                                                    None => {
                                                        eprintln!("[mesh] logbook insert skipped: database lock poisoned");
                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(inbound) = mesh_calendar::try_parse_calendar_packet(&text) {
                                        match inbound {
                                            mesh_calendar::CalendarMeshInbound::SyncRequest => {
                                                let my = my_node_num.load(Ordering::Relaxed);
                                                if my != 0 && from != my {
                                                    let db_s = Arc::clone(&db);
                                                    let cmd_s = cmd_tx.clone();
                                                    tokio::spawn(async move {
                                                        respond_calendar_sync_request(db_s, cmd_s, my).await;
                                                    });
                                                }
                                            }
                                            mesh_calendar::CalendarMeshInbound::Entry(wire) => {
                                                let author = wire.o.unwrap_or(from);
                                                let event = mesh_calendar::calendar_event_from_wire(wire, author);
                                                match db.lock().ok().map(|g| {
                                                    g.upsert_calendar_event_from_mesh(&event)
                                                }) {
                                                    Some(Ok(true)) => {
                                                        let _ = app.emit("athena:calendar-updated", ());
                                                    }
                                                    Some(Ok(false)) => {}
                                                    Some(Err(e)) => {
                                                        eprintln!("[mesh] calendar insert failed: {e}");
                                                    }
                                                    None => {
                                                        eprintln!("[mesh] calendar insert skipped: database lock poisoned");
                                                    }
                                                }
                                            }
                                        }
                                    } else if crate::field_ops::picture::is_marker_packet(&text) {
                                        // A map report shared by another station.
                                        if let Some((marker, _)) = crate::field_ops::picture::decode_marker(&text) {
                                            match db.lock().ok().map(|g| crate::field_ops::picture::ingest_marker(&g, &marker)) {
                                                Some(Ok(true)) => {
                                                    let _ = app.emit("peer-marker-rx", &marker);
                                                }
                                                Some(Ok(false)) | None => {}
                                                Some(Err(e)) => eprintln!("[mesh] map marker store failed: {e}"),
                                            }
                                        }
                                    } else {
                                        let payload =
                                            MeshTextEventPayload { sender_id: from, to, channel, text: text.clone() };
                                        match mesh_tx.try_send(payload) {
                                            Ok(()) => {}
                                            Err(TrySendError::Full(_)) => {
                                                eprintln!("[mesh] mesh-text throttle queue full; dropping broadcast notification");
                                            }
                                            Err(TrySendError::Closed(_)) => {}
                                        }
                                    }
                                }

                                let my = my_node_num.load(Ordering::Relaxed);
                                let (pki, sender_key) = packet_security(&packet);
                                if !consumed_sos
                                    && !mesh_logbook::is_logbook_mesh_text(&text)
                                    && !mesh_calendar::is_calendar_mesh_text(&text)
                                {
                                    let app_dir = app_data_dir.clone();
                                    let app_h = app.clone();
                                    let db_r = Arc::clone(&db);
                                    let sem = ai_permits.clone();
                                    let sink: std::sync::Arc<dyn crate::mesh_router::MeshDmSink> =
                                        std::sync::Arc::new(MeshRadioControl::from_sender(cmd_tx.clone()));
                                    crate::mesh_router::spawn_network_job(
                                        sink,
                                        app_h,
                                        app_dir,
                                        db_r,
                                        sem,
                                        from,
                                        to,
                                        my,
                                        channel,
                                        text.clone(),
                                        pki,
                                        sender_key,
                                    );
                                }
                            }
                            if let Some((from, to, channel, alert_text)) =
                                try_decode_mesh_alert_text(&packet)
                            {
                                let t_eff = if alert_text.trim().is_empty() {
                                    "SOS".to_string()
                                } else {
                                    alert_text.clone()
                                };
                                let sink_a: std::sync::Arc<dyn MeshDmSink> =
                                    std::sync::Arc::new(MeshRadioControl::from_sender(cmd_tx.clone()));
                                let my_a = my_node_num.load(Ordering::Relaxed);
                                let consumed_alert = mesh_listener_handle_priority_sos(
                                    &app,
                                    &db,
                                    &sink_a,
                                    from,
                                    to,
                                    channel,
                                    &t_eff,
                                    my_a,
                                )
                                .await;
                                let (pki_a, key_a) = packet_security(&packet);
                                if !consumed_alert {
                                    let app_dir = app_data_dir.clone();
                                    let app_h = app.clone();
                                    let db_r = Arc::clone(&db);
                                    let sem = ai_permits.clone();
                                    let sink: std::sync::Arc<dyn crate::mesh_router::MeshDmSink> =
                                        std::sync::Arc::new(MeshRadioControl::from_sender(
                                            cmd_tx.clone(),
                                        ));
                                    crate::mesh_router::spawn_network_job(
                                        sink,
                                        app_h,
                                        app_dir,
                                        db_r,
                                        sem,
                                        from,
                                        to,
                                        my_a,
                                        channel,
                                        alert_text,
                                        pki_a,
                                        key_a,
                                    );
                                }
                            }
                        }
                        None => break,
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(MeshCommand::SendText { text, want_ack, channel, destination, reply }) => {
                            if my_node_num.load(Ordering::Relaxed) == 0 {
                                let _ = reply.send(Err(
                                    "Meshtastic node ID not known yet; wait a few seconds after connect."
                                        .to_string(),
                                ));
                                continue;
                            }
                            // Planned radio silence. This is the one place every transmission
                            // passes through (operator sends, AI replies, logbook and calendar
                            // sync), so nothing can slip past it. Distress traffic is exempt.
                            if let Err(reason) =
                                crate::field_ops::radio_discipline::check_transmit(&text)
                            {
                                let _ = reply.send(Err(reason));
                                continue;
                            }
                            // Sign free text with the group key when the station is set to.
                            // Structured sync packets are parsed by exact format, so they are
                            // left as they are.
                            let text = if mesh_logbook::is_logbook_mesh_text(&text)
                                || mesh_calendar::is_calendar_mesh_text(&text)
                                || text.starts_with("ATHREG1:")
                            {
                                text
                            } else {
                                crate::field_ops::trust::sign_if_enabled(
                                    text,
                                    chrono::Utc::now().timestamp(),
                                )
                            };
                            // Station records never go out on a channel others can read.
                            let is_station_record = mesh_logbook::is_logbook_mesh_text(&text)
                                || mesh_calendar::is_calendar_mesh_text(&text)
                                || crate::field_ops::picture::is_marker_packet(&text);
                            if is_station_record && destination.is_none() && mesh_policy::athena_channel() != Some(channel) {
                                let _ = reply.send(Err(mesh_policy::sync_channel().err().unwrap_or_else(|| {
                                    "Station records are sent only on the ATHENA channel.".to_string()
                                })));
                                continue;
                            }
                            // Leave the channel room between packets; distress goes straight out.
                            if !mesh_policy::is_distress(&text) {
                                if let Some(t) = last_sent {
                                    let gap = std::time::Duration::from_millis(mesh_policy::send_gap_ms(mesh_policy::channel_utilization()));
                                    let since = t.elapsed();
                                    if since < gap {
                                        tokio::time::sleep(gap - since).await;
                                    }
                                }
                            }
                            // Direct messages ask for an acknowledgement, so the radio retries
                            // until the other end confirms it got the packet.
                            let want_ack = want_ack || destination.is_some();
                            let idx = channel.clamp(0, 7);
                            let mesh_ch =
                                MeshChannel::new(idx).expect("channel index clamped to 0..=7");
                            let dest = match destination {
                                Some(id) => PacketDestination::Node(NodeId::new(id)),
                                None => PacketDestination::Broadcast,
                            };
                            let result = stream_api
                                .send_text(&mut router, text, dest, want_ack, mesh_ch)
                                .await
                                .map_err(|e| e.to_string());
                            last_sent = Some(std::time::Instant::now());
                            let _ = reply.send(result);
                        }
                        Some(MeshCommand::SetupAthenaChannel { reply }) => {
                            let result = setup_athena_channel(&mut stream_api, &mut router, &app_data_dir).await;
                            let _ = reply.send(result);
                        }
                        None => {
                            if let Err(e) = stream_api.disconnect().await {
                                eprintln!("[mesh] disconnect: {e}");
                            }
                            notify_mesh_connected(&app, false);
                            return;
                        }
                    }
                }
            }
        }

        eprintln!("[mesh] packet stream ended; disconnecting");
        notify_mesh_connected(&app, false);
        my_node_num.store(0, Ordering::Relaxed);
        MESH_MY_NODE_NUM.store(0, Ordering::Relaxed);
        if let Err(e) = stream_api.disconnect().await {
            eprintln!("[mesh] disconnect: {e}");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

impl crate::mesh_router::MeshDmSink for MeshRadioControl {
    fn send_dm(
        &self,
        text: String,
        want_ack: bool,
        channel: u32,
        destination: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'static>>
    {
        let this = self.clone();
        Box::pin(async move {
            this.send_text_message(text, want_ack, channel, destination).await
        })
    }
}


#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn network_radio_addresses_get_the_meshtastic_port() {
        assert_eq!(radio_tcp_address("192.168.1.40").as_deref(), Some("192.168.1.40:4403"));
        assert_eq!(radio_tcp_address(" tcp://meshradio.local:4403/ ").as_deref(), Some("meshradio.local:4403"));
        assert_eq!(radio_tcp_address("10.0.0.5:9000").as_deref(), Some("10.0.0.5:9000"));
        assert_eq!(radio_tcp_address(""), None);
        assert_eq!(radio_tcp_address("http://x/y"), None);
    }

    fn ch(index: i32, name: &str, role: protobufs::channel::Role, psk_len: usize) -> protobufs::Channel {
        protobufs::Channel {
            index,
            settings: Some(protobufs::ChannelSettings { name: name.into(), psk: vec![7; psk_len], ..Default::default() }),
            role: role as i32,
        }
    }

    #[test]
    fn the_athena_channel_reuses_its_slot_or_takes_a_free_one() {
        use protobufs::channel::Role;
        let mut m = HashMap::new();
        m.insert(0, ch(0, "", Role::Primary, 1));
        m.insert(1, ch(1, "Family", Role::Secondary, 32));
        m.insert(2, ch(2, "", Role::Disabled, 0));
        assert_eq!(athena_channel_slot(&m).unwrap().0, 2);
        m.insert(3, ch(3, ATHENA_CHANNEL_NAME, Role::Secondary, 32));
        let (i, existing) = athena_channel_slot(&m).unwrap();
        assert_eq!((i, existing.is_some()), (3, true), "an existing ATHENA channel is kept, not replaced");
        for i in 1..=7 {
            m.insert(i, ch(i as i32, "Other", Role::Secondary, 32));
        }
        assert!(athena_channel_slot(&m).is_err());
    }

    #[test]
    fn join_links_decode_back_to_the_channel() {
        use base64::Engine;
        let settings = protobufs::ChannelSettings { name: "ATHENA".into(), psk: vec![9; 32], ..Default::default() };
        let url = join_url_for(settings.clone());
        let b64 = url.split_once('#').unwrap().1;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64).unwrap();
        let set = protobufs::ChannelSet::decode(bytes.as_slice()).unwrap();
        assert_eq!(set.settings, vec![settings]);
        assert!(url.starts_with("https://meshtastic.org/e/?add=true#"));
    }
}
