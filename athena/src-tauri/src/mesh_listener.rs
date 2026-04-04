//! Background Tokio task: USB Meshtastic serial, decode text packets, emit Tauri events, send text,
//! and optional Meshtastic → Ollama → DM reply.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::db::{Database, LogbookEntry};
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

use crate::mesh_ai::{
    should_run_mesh_ai, MESHTASTIC_BROADCAST_TO,
    PRIMARY_MESH_CHANNEL,
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

    pub(crate) fn from_sender(cmd_tx: Sender<MeshCommand>) -> Self {
        Self { cmd_tx }
    }
}

fn fail_mesh_command(cmd: MeshCommand, err: String) {
    match cmd {
        MeshCommand::SendText { reply, .. } => {
            let _ = reply.send(Err(err));
        }
    }
}

/// After another device sends `ATHLB1:{"t":"req"}`, push recent local rows on Primary (paced for LoRa).
async fn respond_logbook_sync_request(
    db: Arc<Mutex<Database>>,
    cmd_tx: Sender<MeshCommand>,
    my_node: u32,
) {
    if my_node == 0 {
        return;
    }
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
            channel: PRIMARY_MESH_CHANNEL,
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

/// After another device sends `ATHCAL1:{"t":"req"}`, push recent local calendar rows on Primary.
async fn respond_calendar_sync_request(
    db: Arc<Mutex<Database>>,
    cmd_tx: Sender<MeshCommand>,
    my_node: u32,
) {
    if my_node == 0 {
        return;
    }
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
            channel: PRIMARY_MESH_CHANNEL,
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

/// Throttles mesh-text UI events to at most five per second; drops when the queue is full.
async fn mesh_text_emit_loop(app: tauri::AppHandle, mut rx: Receiver<MeshTextEventPayload>) {
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
                tokio::spawn(mesh_text_emit_loop(app_handle.clone(), mesh_rx));
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

/// Log every `FromRadio` received from the USB serial stream (post-framing protobuf, closest to “raw bridge” in app code).
fn log_mesh_raw_incoming(packet: &protobufs::FromRadio) {
    let mut buf = Vec::with_capacity(packet.encoded_len().saturating_add(32));
    match packet.encode(&mut buf) {
        Ok(()) => {
            eprintln!(
                "[MESH RAW] id={} protobuf_len={} hex_preview={}",
                packet.id,
                buf.len(),
                mesh_hex_preview(&buf, 384)
            );
        }
        Err(e) => {
            eprintln!(
                "[MESH RAW] id={} encode_error={e} (payload_variant present: {})",
                packet.id,
                packet.payload_variant.is_some()
            );
        }
    }
}

/// When a decoded mesh packet is a text or position app payload, log clearly for field testing.
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
            .take(240)
            .collect();
        eprintln!(
            "[MESH TEXT] from={} to={} channel={} payload_len={} preview={}",
            mesh_packet.from,
            mesh_packet.to,
            mesh_packet.channel,
            data.payload.len(),
            preview.replace('\n', "\\n")
        );
        return;
    }

    if port == pos_app {
        match protobufs::Position::decode(data.payload.as_slice()) {
            Ok(pos) => {
                let lat_deg = pos.latitude_i.map(|i| (i as f64) * 1e-7);
                let lon_deg = pos.longitude_i.map(|i| (i as f64) * 1e-7);
                eprintln!(
                    "[MESH GPS] from={} to={} channel={} latitude_i={:?} longitude_i={:?} lat_deg={:?} lon_deg={:?} altitude_msl={:?} time={} timestamp={}",
                    mesh_packet.from,
                    mesh_packet.to,
                    mesh_packet.channel,
                    pos.latitude_i,
                    pos.longitude_i,
                    lat_deg,
                    lon_deg,
                    pos.altitude,
                    pos.time,
                    pos.timestamp
                );
            }
            Err(e) => {
                eprintln!(
                    "[MESH GPS] from={} to={} channel={} decode_error={e} raw_len={}",
                    mesh_packet.from,
                    mesh_packet.to,
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

    loop {
        notify_mesh_connected(&app, false);
        let mut serial_open_failures: u32 = 0;
        let stream_handle = 'open_serial: loop {
            let link = MeshRadioSerialLink::new(None);
            match link.open_stream() {
                Ok(h) => break 'open_serial h,
                Err(e) => {
                    serial_open_failures += 1;
                    if serial_open_failures <= SERIAL_OPEN_BURST_ATTEMPTS {
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
        let (mut packet_rx, connected) = stream_api.connect(stream_handle).await;
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
                            ingest_node_display_name(&packet, &node_names, &my_node_num);

                            if let Some((from, to, channel, text)) = try_decode_mesh_text(&packet) {
                                if channel == PRIMARY_MESH_CHANNEL && to == MESHTASTIC_BROADCAST_TO {
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
                                                            parent_entry_id: None,
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
                                    } else {
                                        let payload =
                                            MeshTextEventPayload { sender_id: from, text: text.clone() };
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
                                if should_run_mesh_ai(from, to, my, channel)
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

