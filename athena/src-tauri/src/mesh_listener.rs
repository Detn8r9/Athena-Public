//! Background Tokio task: USB Meshtastic serial, decode text packets, emit Tauri events, send text,
//! and optional Meshtastic → Ollama → DM reply.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use meshtastic::api::StreamApi;
use meshtastic::packet::{PacketDestination, PacketRouter};
use meshtastic::protobufs;
use meshtastic::types::{MeshChannel, NodeId};
use serde::Serialize;
use tauri::Emitter;
use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Semaphore;

use crate::config::load_config;
use crate::mesh_ai::{
    process_mesh_message_and_reply, should_run_mesh_ai, MESHTASTIC_BROADCAST_TO, PRIMARY_MESH_CHANNEL,
};
use crate::mesh_radio::MeshRadioSerialLink;

/// Error type for [`PacketRouter`] (must implement [`std::error::Error`], not plain `String`).
#[derive(Debug, Error)]
#[error("{0}")]
struct PacketRouterError(String);

/// Event name for decoded mesh text messages (listen in the frontend with `listen`).
pub const MESH_TEXT_EVENT: &str = "athena:mesh-text";

/// Event when USB Meshtastic serial connects or disconnects (for hiding/showing UI).
pub const MESH_CONNECTION_EVENT: &str = "athena:mesh-connection";

static MESH_RADIO_CONNECTED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug)]
pub struct MeshConnectionPayload {
    pub connected: bool,
}

/// Whether the Meshtastic serial link is currently up (configured and streaming).
pub fn mesh_radio_is_connected() -> bool {
    MESH_RADIO_CONNECTED.load(Ordering::Relaxed)
}

fn notify_mesh_connected(app: &tauri::AppHandle, connected: bool) {
    MESH_RADIO_CONNECTED.store(connected, Ordering::Relaxed);
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

enum MeshCommand {
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
    cmd_tx: UnboundedSender<MeshCommand>,
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
        self.cmd_tx
            .send(MeshCommand::SendText {
                text,
                want_ack,
                channel,
                destination: destination_node_id,
                reply: reply_tx,
            })
            .map_err(|_| "Meshtastic background thread is not running.".to_string())?;
        reply_rx
            .await
            .map_err(|_| "Meshtastic send was cancelled.".to_string())?
    }
}

fn fail_mesh_command(cmd: MeshCommand, err: String) {
    match cmd {
        MeshCommand::SendText { reply, .. } => {
            let _ = reply.send(Err(err));
        }
    }
}

/// Spawns a dedicated thread with a multi-thread Tokio runtime. Register the returned
/// [`MeshRadioControl`] with [`tauri::Manager::manage`].
pub fn spawn_mesh_radio_listener(app_handle: tauri::AppHandle, app_data_dir: PathBuf) -> MeshRadioControl {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let control = MeshRadioControl { cmd_tx: cmd_tx.clone() };
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
            runtime.block_on(mesh_radio_main_loop(
                app_handle,
                app_data_dir,
                cmd_rx,
                cmd_tx,
            ));
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
    fn handle_packet_from_radio(&mut self, _packet: protobufs::FromRadio) -> Result<(), PacketRouterError> {
        Ok(())
    }

    fn handle_mesh_packet(&mut self, _packet: protobufs::MeshPacket) -> Result<(), PacketRouterError> {
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

fn ingest_node_display_name(from_radio: &protobufs::FromRadio, map: &Mutex<HashMap<u32, String>>) {
    let Some(protobufs::from_radio::PayloadVariant::NodeInfo(ni)) = &from_radio.payload_variant else {
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
        g.insert(ni.num, name);
    }
}

/// Decoded mesh text: sender, destination, channel field, payload.
fn try_decode_mesh_text(from_radio: protobufs::FromRadio) -> Option<(u32, u32, u32, String)> {
    let payload_variant = from_radio.payload_variant?;
    let mesh_packet = match payload_variant {
        protobufs::from_radio::PayloadVariant::Packet(p) => p,
        _ => return None,
    };
    let inner = mesh_packet.payload_variant?;
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
    cmd_rx: &mut UnboundedReceiver<MeshCommand>,
) -> Option<MeshCommand> {
    tokio::select! {
        _ = tokio::time::sleep(duration) => None,
        cmd = cmd_rx.recv() => cmd,
    }
}

async fn mesh_radio_main_loop(
    app: tauri::AppHandle,
    app_data_dir: PathBuf,
    mut cmd_rx: UnboundedReceiver<MeshCommand>,
    cmd_tx: UnboundedSender<MeshCommand>,
) {
    let my_node_num = Arc::new(AtomicU32::new(0));
    let node_names: Arc<Mutex<HashMap<u32, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let ai_permits = Arc::new(Semaphore::new(3));

    loop {
        notify_mesh_connected(&app, false);
        let stream_handle = 'open_serial: loop {
            let link = MeshRadioSerialLink::new(None);
            match link.open_stream() {
                Ok(h) => break 'open_serial h,
                Err(e) => {
                    eprintln!("[mesh] no serial yet: {e}");
                    match sleep_or_command(std::time::Duration::from_secs(3), &mut cmd_rx).await {
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
                            if let Some(n) = extract_my_node_num(&packet) {
                                my_node_num.store(n, Ordering::Relaxed);
                            }
                            ingest_node_display_name(&packet, &node_names);

                            if let Some((from, to, channel, text)) = try_decode_mesh_text(packet) {
                                // Mesh panel + UI events: Primary channel broadcast only (visible to all nodes).
                                if channel == PRIMARY_MESH_CHANNEL && to == MESHTASTIC_BROADCAST_TO {
                                    let payload = MeshTextEventPayload { sender_id: from, text: text.clone() };
                                    if let Err(e) = app.emit(MESH_TEXT_EVENT, &payload) {
                                        eprintln!("[mesh] emit {MESH_TEXT_EVENT} failed: {e}");
                                    }
                                }

                                let my = my_node_num.load(Ordering::Relaxed);
                                let cfg = load_config(app_data_dir.as_path());
                                if cfg.mesh_ai_auto_reply.unwrap_or(true)
                                    && should_run_mesh_ai(from, to, my, channel)
                                {
                                    let display_name = node_names
                                        .lock()
                                        .ok()
                                        .and_then(|g| g.get(&from).cloned())
                                        .unwrap_or_else(|| format!("Node {}", from));
                                    let app_dir = app_data_dir.clone();
                                    let app_h = app.clone();
                                    let cmd_tx_ai = cmd_tx.clone();
                                    let sem = ai_permits.clone();
                                    tokio::spawn(async move {
                                        let _permit = match sem.acquire_owned().await {
                                            Ok(p) => p,
                                            Err(_) => return,
                                        };
                                        let app_dir2 = app_dir.clone();
                                        let app_h2 = app_h.clone();
                                        let chunks = match tokio::task::spawn_blocking(move || {
                                            process_mesh_message_and_reply(
                                                app_dir2.as_path(),
                                                Some(&app_h2),
                                                from,
                                                &text,
                                                &display_name,
                                            )
                                        })
                                        .await
                                        {
                                            Ok(Ok(c)) => c,
                                            Ok(Err(e)) => {
                                                eprintln!("[mesh-ai] {}", e);
                                                return;
                                            }
                                            Err(e) => {
                                                eprintln!("[mesh-ai] join error: {e}");
                                                return;
                                            }
                                        };
                                        for (i, part) in chunks.iter().enumerate() {
                                            if part.is_empty() {
                                                continue;
                                            }
                                            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                                            if cmd_tx_ai
                                                .send(MeshCommand::SendText {
                                                    text: part.clone(),
                                                    want_ack: i + 1 == chunks.len(),
                                                    channel,
                                                    destination: Some(from),
                                                    reply: reply_tx,
                                                })
                                                .is_err()
                                            {
                                                return;
                                            }
                                            if reply_rx.await.is_err() {
                                                return;
                                            }
                                        }
                                    });
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
        if let Err(e) = stream_api.disconnect().await {
            eprintln!("[mesh] disconnect: {e}");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
