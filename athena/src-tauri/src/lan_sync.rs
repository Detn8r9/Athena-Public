use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::db::{Database, LogbookEntry, MapMarker, MeshMessage, RadioTelemetrySample};
use crate::lan_cluster::{self, InjectQueue};
use crate::lan_identity::set_lan_local_node_id;
use crate::lan_packets::{LanCursors, LanPacket, LAN_PROTOCOL_VERSION};
use crate::radio_manager::{MeshMessageAckPayload, RadioState, RadioTelemetryPayload};

pub use crate::lan_identity::lan_local_node_id;

pub const LAN_SERVICE_TYPE: &str = "_athena-lan._tcp.local.";
pub const LAN_TBL_MAP_MARKERS: &str = "map_markers";
pub const LAN_TBL_LOGBOOK_ENTRIES: &str = "logbook_entries";
pub const LAN_TBL_MESH_MESSAGES: &str = "mesh_messages";
pub const LAN_TBL_RADIO_TELEMETRY: &str = "radio_telemetry";

const LAN_EPOCH_RFC3339: &str = "1970-01-01T00:00:00Z";

const LAN_OUTBOX_CAPACITY: usize = 512;
const LAN_MAX_FRAME_BYTES: usize = 256 * 1024;
const LAN_CURSORS_NONE_TS: &str = LAN_EPOCH_RFC3339;
const LAN_MAX_MAP_MARKERS_PER_SNAPSHOT: usize = 200;
const LAN_MAX_LOGBOOK_ENTRIES_PER_SNAPSHOT: usize = 64;
const LAN_MAX_MESH_MESSAGES_PER_SNAPSHOT: usize = 200;
const LAN_MAX_TELEMETRY_SAMPLES_PER_SNAPSHOT: usize = 200;

const LAN_PING_INTERVAL: Duration = Duration::from_secs(5);
/// If no inbound frame is received within this window, the TCP session is torn down.
const LAN_PEER_STALE: Duration = Duration::from_secs(10);

const LAN_INBOUND_PKT_QUEUE: usize = 64;

static LAN_LOCAL_TS_CREATED_AT: OnceLock<u64> = OnceLock::new();
static LAN_OUTBOX_TX: OnceLock<broadcast::Sender<LanLocalRowEvent>> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct LanLocalRowEvent {
    pub table: &'static str,
    pub payload: serde_json::Value,
}

/// Fire-and-forget: local writers call this after a successful DB commit (never from LAN apply paths).
pub fn publish_local_sync_row(table: &'static str, payload: serde_json::Value) {
    if let Some(tx) = LAN_OUTBOX_TX.get() {
        let _ = tx.send(LanLocalRowEvent { table, payload });
    }
}

#[derive(Debug, Clone)]
struct LanPeerAddr {
    addr: SocketAddr,
    _node_id: u32,
}

fn resolve_or_create_lan_identity(app_data_dir: &Path) -> (u32, u64) {
    let identity_path = app_data_dir.join("athena_lan_identity.json");
    if let Ok(s) = fs::read_to_string(&identity_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            let node_id = v.get("node_id").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let created_at = v.get("created_at_ms").and_then(|x| x.as_u64()).unwrap_or(0);
            if node_id != 0 {
                return (node_id, created_at);
            }
        }
    }

    let node_id = (uuid::Uuid::new_v4().as_u128() % (u32::MAX as u128)) as u32 | 1;
    let created_at_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64;

    let payload = serde_json::json!({ "node_id": node_id, "created_at_ms": created_at_ms });
    let _ = fs::create_dir_all(app_data_dir);
    if let Ok(s) = serde_json::to_string_pretty(&payload) {
        let _ = fs::write(&identity_path, s);
    }
    (node_id, created_at_ms)
}

async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("read len failed: {e}"))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > LAN_MAX_FRAME_BYTES {
        return Err(format!("invalid frame length {len}"));
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("read frame failed: {e}"))?;
    Ok(buf)
}

async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    json_bytes: &[u8],
) -> Result<(), String> {
    let len = json_bytes.len();
    if len > LAN_MAX_FRAME_BYTES {
        return Err(format!("frame too large {len}"));
    }
    let len_buf = (len as u32).to_be_bytes();
    writer
        .write_all(&len_buf)
        .await
        .map_err(|e| format!("write len failed: {e}"))?;
    writer
        .write_all(json_bytes)
        .await
        .map_err(|e| format!("write frame failed: {e}"))?;
    writer
        .flush()
        .await
        .map_err(|e| format!("flush failed: {e}"))?;
    Ok(())
}

async fn read_packet<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<LanPacket, String> {
    let bytes = read_frame(reader).await?;
    serde_json::from_slice(&bytes).map_err(|e| format!("decode json failed: {e}"))
}

async fn write_packet<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    packet: &LanPacket,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(packet).map_err(|e| format!("encode json failed: {e}"))?;
    write_frame(writer, &bytes).await
}

fn to_cursor_defaults(c: &LanCursors) -> LanCursors {
    LanCursors {
        map_markers_created_at: if c.map_markers_created_at.trim().is_empty() {
            LAN_CURSORS_NONE_TS.to_string()
        } else {
            c.map_markers_created_at.clone()
        },
        logbook_created_at: c.logbook_created_at,
        mesh_messages_updated_at: if c.mesh_messages_updated_at.trim().is_empty() {
            LAN_CURSORS_NONE_TS.to_string()
        } else {
            c.mesh_messages_updated_at.clone()
        },
        radio_telemetry_timestamp: if c.radio_telemetry_timestamp.trim().is_empty() {
            LAN_CURSORS_NONE_TS.to_string()
        } else {
            c.radio_telemetry_timestamp.clone()
        },
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

async fn current_cursors(
    app_state: &Arc<std::sync::Mutex<Database>>,
) -> Result<LanCursors, String> {
    let db = app_state.lock().map_err(|e| e.to_string())?;
    let map_max = db
        .get_max_map_marker_created_at()?
        .unwrap_or_else(|| LAN_CURSORS_NONE_TS.to_string());
    let log_max = db.get_max_logbook_created_at()?.unwrap_or(0);
    let mesh_max = db
        .get_max_mesh_messages_updated_at()?
        .unwrap_or_else(|| LAN_CURSORS_NONE_TS.to_string());
    let tele_max = db
        .get_max_radio_telemetry_timestamp()?
        .unwrap_or_else(|| LAN_CURSORS_NONE_TS.to_string());
    Ok(LanCursors {
        map_markers_created_at: map_max,
        logbook_created_at: log_max,
        mesh_messages_updated_at: mesh_max,
        radio_telemetry_timestamp: tele_max,
    })
}

async fn snapshot_from_peer(
    app_state: &Arc<std::sync::Mutex<Database>>,
    cursors: &LanCursors,
) -> Result<LanPacket, String> {
    let db = app_state.lock().map_err(|e| e.to_string())?;

    let cursors = to_cursor_defaults(cursors);
    let since_map = cursors.map_markers_created_at.as_str();
    let since_log = cursors.logbook_created_at;
    let since_mesh = cursors.mesh_messages_updated_at.as_str();
    let since_tel = cursors.radio_telemetry_timestamp.as_str();

    let map_markers =
        db.list_map_markers_since_created_at(since_map, LAN_MAX_MAP_MARKERS_PER_SNAPSHOT)?;
    let logbook_entries =
        db.list_logbook_entries_since(since_log, LAN_MAX_LOGBOOK_ENTRIES_PER_SNAPSHOT)?;
    let mesh_messages =
        db.list_mesh_messages_updated_since(since_mesh, LAN_MAX_MESH_MESSAGES_PER_SNAPSHOT)?;
    let radio_telemetry =
        db.list_radio_telemetry_since(since_tel, LAN_MAX_TELEMETRY_SAMPLES_PER_SNAPSHOT)?;

    Ok(LanPacket::Snapshot {
        map_markers,
        logbook_entries,
        mesh_messages,
        radio_telemetry,
    })
}

async fn apply_snapshot_to_local_db(
    app_handle: &AppHandle,
    app_state: &Arc<std::sync::Mutex<Database>>,
    snapshot: &LanPacket,
) -> Result<(), String> {
    let (map_markers_emits, logbook_changed, mesh_changed, telemetry_samples) = match snapshot {
        LanPacket::Snapshot {
            map_markers,
            logbook_entries,
            mesh_messages: _,
            radio_telemetry: _,
        } => {
            let mut map_markers_emits: Vec<MapMarker> = Vec::new();
            let mut logbook_changed = false;
            let mut mesh_changed = false;

            let mut telemetry_samples: Vec<RadioTelemetryPayload> = Vec::new();

            let db = app_state.lock().map_err(|e| e.to_string())?;

            for marker in map_markers {
                if db.upsert_map_marker_if_newer(marker)? {
                    map_markers_emits.push(marker.clone());
                }
            }

            for entry in logbook_entries {
                if db.try_insert_logbook_from_lan(entry)? {
                    logbook_changed = true;
                }
            }

            if let LanPacket::Snapshot {
                mesh_messages,
                radio_telemetry,
                ..
            } = snapshot
            {
                for msg in mesh_messages {
                    if db.merge_mesh_message_from_lan(msg)? {
                        mesh_changed = true;
                    }
                }

                for s in radio_telemetry {
                    if db.insert_radio_telemetry_from_lan(
                        s.source_node_id,
                        &s.timestamp,
                        s.rssi,
                        s.snr,
                    )? {
                        telemetry_samples.push(RadioTelemetryPayload {
                            rssi: s.rssi,
                            snr: s.snr,
                            timestamp: s.timestamp.clone(),
                        });
                    }
                }
            }

            (
                map_markers_emits,
                logbook_changed,
                mesh_changed,
                telemetry_samples,
            )
        }
        _ => return Err("not a snapshot".to_string()),
    };

    for marker in map_markers_emits {
        let _ = app_handle.emit("peer-marker-rx", marker);
    }
    if logbook_changed {
        let _ = app_handle.emit("athena:logbook-updated", ());
    }
    if mesh_changed {
        let _ = app_handle.emit(
            "mesh-message-ack",
            MeshMessageAckPayload {
                packet_id: "".to_string(),
                delivered: true,
            },
        );
    }
    for sample in telemetry_samples {
        let _ = app_handle.emit("radio-telemetry", sample);
    }

    Ok(())
}

async fn apply_push_row_from_peer(
    app_handle: &AppHandle,
    app_state: &Arc<std::sync::Mutex<Database>>,
    table: &str,
    row: &serde_json::Value,
) -> Result<(), String> {
    match table {
        LAN_TBL_MAP_MARKERS => {
            let marker: MapMarker =
                serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
            let changed = {
                let db = app_state.lock().map_err(|e| e.to_string())?;
                db.upsert_map_marker_if_newer(&marker)?
            };
            if changed {
                let _ = app_handle.emit("peer-marker-rx", marker);
            }
        }
        LAN_TBL_LOGBOOK_ENTRIES => {
            let entry: LogbookEntry =
                serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
            let changed = {
                let db = app_state.lock().map_err(|e| e.to_string())?;
                db.try_insert_logbook_from_lan(&entry)?
            };
            if changed {
                let _ = app_handle.emit("athena:logbook-updated", ());
            }
        }
        LAN_TBL_MESH_MESSAGES => {
            let msg: MeshMessage =
                serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
            let changed = {
                let db = app_state.lock().map_err(|e| e.to_string())?;
                db.merge_mesh_message_from_lan(&msg)?
            };
            if changed {
                let _ = app_handle.emit(
                    "mesh-message-ack",
                    MeshMessageAckPayload {
                        packet_id: msg.packet_id,
                        delivered: true,
                    },
                );
            }
        }
        LAN_TBL_RADIO_TELEMETRY => {
            let sample: RadioTelemetrySample =
                serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
            let changed = {
                let db = app_state.lock().map_err(|e| e.to_string())?;
                db.insert_radio_telemetry_from_lan(
                    sample.source_node_id,
                    &sample.timestamp,
                    sample.rssi,
                    sample.snr,
                )?
            };
            if changed {
                let _ = app_handle.emit(
                    "radio-telemetry",
                    RadioTelemetryPayload {
                        rssi: sample.rssi,
                        snr: sample.snr,
                        timestamp: sample.timestamp,
                    },
                );
            }
        }
        _ => return Err(format!("unknown push table {table}")),
    }
    Ok(())
}

async fn inbound_reader_loop(mut reader: OwnedReadHalf, in_tx: mpsc::Sender<LanPacket>) {
    while let Ok(p) = read_packet(&mut reader).await {
        if in_tx.send(p).await.is_err() {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(unused_assignments)]
async fn run_duplex_writer_loop(
    mut writer: OwnedWriteHalf,
    mut in_rx: mpsc::Receiver<LanPacket>,
    mut outbox: broadcast::Receiver<LanLocalRowEvent>,
    inject: Arc<InjectQueue>,
    db: Arc<std::sync::Mutex<Database>>,
    app_handle: AppHandle,
    mut cursors: LanCursors,
    last_peer_seen_ms: Arc<AtomicU64>,
) -> Result<(), String> {
    let mut ping = tokio::time::interval(LAN_PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut seq: u64 = 0;

    let peer_stale_sleep = tokio::time::sleep_until(tokio::time::Instant::now() + LAN_PEER_STALE);
    tokio::pin!(peer_stale_sleep);

    loop {
        tokio::select! {
            _ = &mut peer_stale_sleep => {
                return Err("peer stale (no inbound frames)".to_string());
            }
            _ = ping.tick() => {
                seq = seq.wrapping_add(1);
                write_packet(&mut writer, &LanPacket::Ping { seq }).await?;
            }
            out = inject.pop() => {
                write_packet(&mut writer, &out).await?;
            }
            recv = outbox.recv() => {
                match recv {
                    Ok(ev) => {
                        let pkt = LanPacket::PushRow {
                            table: ev.table.to_string(),
                            row: ev.payload,
                        };
                        write_packet(&mut writer, &pkt).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
            in_pkt = in_rx.recv() => {
                let Some(pkt) = in_pkt else {
                    return Ok(());
                };
                last_peer_seen_ms.store(now_unix_ms(), Ordering::Relaxed);
                peer_stale_sleep.as_mut().reset(tokio::time::Instant::now() + LAN_PEER_STALE);

                match pkt {
                    LanPacket::Ping { seq: peer_seq } => {
                        write_packet(&mut writer, &LanPacket::Pong { seq: peer_seq }).await?;
                    }
                    LanPacket::Pong { .. } => {}
                    LanPacket::PushRow { table, row } => {
                        apply_push_row_from_peer(&app_handle, &db, &table, &row).await?;
                    }
                    LanPacket::CapabilitiesAnnounce { .. } => {
                        lan_cluster::merge_capabilities_from_packet(&pkt);
                        let _ = app_handle.emit("athena:lan-cluster-updated", ());
                    }
                    LanPacket::AiJobOffer {
                        job_id,
                        model,
                        chunk_index,
                        chunk_total,
                        chunk_text,
                        system_hint,
                        temperature,
                        num_ctx,
                    } => {
                        let inj = inject.clone();
                        let offer = LanPacket::AiJobOffer {
                            job_id: job_id.clone(),
                            model,
                            chunk_index,
                            chunk_total,
                            chunk_text,
                            system_hint,
                            temperature,
                            num_ctx,
                        };
                        tokio::task::spawn_blocking(move || {
                            let result = lan_cluster::execute_worker_ai_job_blocking(&offer);
                            inj.push(result);
                        });
                    }
                    LanPacket::AiJobResult {
                        job_id,
                        chunk_index,
                        ok,
                        partial_text,
                        error,
                    } => {
                        if let Ok(uuid) = Uuid::parse_str(&job_id) {
                            let r = if ok {
                                Ok(partial_text)
                            } else {
                                Err(error.unwrap_or_else(|| "worker error".to_string()))
                            };
                            lan_cluster::complete_job_result(uuid, chunk_index, r);
                        }
                    }
                    LanPacket::Pull {
                        cursors: next_cursors,
                        protocol_version: pv,
                        ..
                    } => {
                        if pv != LAN_PROTOCOL_VERSION {
                            return Err("protocol version mismatch".to_string());
                        }
                        cursors = next_cursors;
                        let snap = snapshot_from_peer(&db, &cursors).await?;
                        write_packet(&mut writer, &snap).await?;
                    }
                    LanPacket::Hello {
                        cursors: next_cursors,
                        protocol_version: pv,
                        ..
                    } => {
                        if pv != LAN_PROTOCOL_VERSION {
                            return Err("protocol version mismatch".to_string());
                        }
                        cursors = next_cursors;
                        let snap = snapshot_from_peer(&db, &cursors).await?;
                        write_packet(&mut writer, &snap).await?;
                    }
                    LanPacket::Snapshot { .. } => {
                        return Err("unexpected snapshot in duplex".to_string());
                    }
                }
            }
        }
    }
}

async fn run_server_session(
    mut stream: TcpStream,
    db: Arc<std::sync::Mutex<Database>>,
    app_handle: AppHandle,
    outbox_tx: broadcast::Sender<LanLocalRowEvent>,
    last_peer_seen_ms: Arc<AtomicU64>,
) -> Result<(), String> {
    let first = read_packet(&mut stream).await?;
    let (cursors, remote_node_id) = match first {
        LanPacket::Hello {
            cursors,
            protocol_version,
            node_id,
            ..
        } => {
            if protocol_version != LAN_PROTOCOL_VERSION {
                return Err("protocol version mismatch".to_string());
            }
            (cursors, node_id)
        }
        LanPacket::Pull {
            cursors,
            protocol_version,
            node_id,
            ..
        } => {
            if protocol_version != LAN_PROTOCOL_VERSION {
                return Err("protocol version mismatch".to_string());
            }
            (cursors, node_id)
        }
        _ => return Err("expected hello or pull".to_string()),
    };

    let snap = snapshot_from_peer(&db, &cursors).await?;
    write_packet(&mut stream, &snap).await?;

    let (reader, writer) = stream.into_split();
    let (in_tx, in_rx) = mpsc::channel(LAN_INBOUND_PKT_QUEUE);
    let _read_task = tauri::async_runtime::spawn(inbound_reader_loop(reader, in_tx));
    let outbox = outbox_tx.subscribe();
    let inject = Arc::new(InjectQueue::new());
    lan_cluster::CLUSTER.hub.register(remote_node_id, &inject);
    let app_caps = app_handle.clone();
    let inj_caps = inject.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(Ok(pkt)) =
            tokio::task::spawn_blocking(lan_cluster::build_capabilities_packet).await
        {
            inj_caps.push(pkt)
        }
        let _ = app_caps.emit("athena:lan-cluster-updated", ());
    });
    let r = run_duplex_writer_loop(
        writer,
        in_rx,
        outbox,
        inject.clone(),
        db,
        app_handle,
        cursors,
        last_peer_seen_ms,
    )
    .await;
    lan_cluster::CLUSTER.hub.unregister(remote_node_id);
    r
}

async fn run_client_session(
    peer: LanPeerAddr,
    db: Arc<std::sync::Mutex<Database>>,
    app_handle: AppHandle,
    outbox_tx: broadcast::Sender<LanLocalRowEvent>,
    last_peer_seen_ms: Arc<AtomicU64>,
) -> Result<(), String> {
    let mut stream = TcpStream::connect(peer.addr)
        .await
        .map_err(|e| format!("connect failed: {e}"))?;
    stream.set_nodelay(true).ok();

    let remote_node_id = peer._node_id;
    let node_id = lan_local_node_id();
    let cursors = current_cursors(&db).await?;
    let hello = LanPacket::Hello {
        node_id,
        protocol_version: LAN_PROTOCOL_VERSION,
        cursors: cursors.clone(),
    };
    write_packet(&mut stream, &hello).await?;

    let snap = read_packet(&mut stream).await?;
    if !matches!(snap, LanPacket::Snapshot { .. }) {
        return Err("expected snapshot after hello".to_string());
    }
    apply_snapshot_to_local_db(&app_handle, &db, &snap).await?;

    let cursors = current_cursors(&db).await?;
    let (reader, writer) = stream.into_split();
    let (in_tx, in_rx) = mpsc::channel(LAN_INBOUND_PKT_QUEUE);
    let _read_task = tauri::async_runtime::spawn(inbound_reader_loop(reader, in_tx));
    let outbox = outbox_tx.subscribe();
    let inject = Arc::new(InjectQueue::new());
    lan_cluster::CLUSTER.hub.register(remote_node_id, &inject);
    let app_caps = app_handle.clone();
    let inj_caps = inject.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(Ok(pkt)) =
            tokio::task::spawn_blocking(lan_cluster::build_capabilities_packet).await
        {
            inj_caps.push(pkt)
        }
        let _ = app_caps.emit("athena:lan-cluster-updated", ());
    });
    let r = run_duplex_writer_loop(
        writer,
        in_rx,
        outbox,
        inject.clone(),
        db,
        app_handle,
        cursors,
        last_peer_seen_ms,
    )
    .await;
    lan_cluster::CLUSTER.hub.unregister(remote_node_id);
    r
}

pub fn spawn_lan_sync(
    app_handle: AppHandle,
    app_data_dir: PathBuf,
    db: Arc<std::sync::Mutex<Database>>,
) {
    let (node_id, created_at_ms) = resolve_or_create_lan_identity(&app_data_dir);
    set_lan_local_node_id(node_id);
    let _ = LAN_LOCAL_TS_CREATED_AT.set(created_at_ms);

    let (outbox_tx, _outbox_rx) = broadcast::channel::<LanLocalRowEvent>(LAN_OUTBOX_CAPACITY);
    let _ = LAN_OUTBOX_TX.set(outbox_tx.clone());

    let active_peers = Arc::new(AtomicUsize::new(0));
    let last_peer_seen_ms = Arc::new(AtomicU64::new(now_unix_ms()));
    let peer_cache: Arc<tokio::sync::Mutex<HashSet<SocketAddr>>> =
        Arc::new(tokio::sync::Mutex::new(HashSet::new()));

    let db_for_server = db.clone();
    let last_peer_seen_ms_for_server = last_peer_seen_ms.clone();
    let server_active = active_peers.clone();
    let app_handle_for_server = app_handle.clone();
    let outbox_for_server = outbox_tx.clone();
    let (port_tx, port_rx) = tokio::sync::oneshot::channel::<u16>();

    tauri::async_runtime::spawn(async move {
        let listener = TcpListener::bind(("0.0.0.0", 0))
            .await
            .map_err(|e| format!("LAN TCP bind failed: {e}"))
            .expect("LAN TCP bind failed");
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let _ = port_tx.send(port);

        eprintln!("[lan] TCP server listening on port {port}");

        loop {
            if let Ok((stream, _peer_addr)) = listener.accept().await {
                server_active.fetch_add(1, Ordering::Relaxed);
                last_peer_seen_ms_for_server.store(now_unix_ms(), Ordering::Relaxed);
                let db = db_for_server.clone();
                let server_active_inner = server_active.clone();
                let app_h = app_handle_for_server.clone();
                let outbox = outbox_for_server.clone();
                let last_ms = last_peer_seen_ms_for_server.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = run_server_session(stream, db, app_h, outbox, last_ms).await;
                    server_active_inner.fetch_sub(1, Ordering::Relaxed);
                });
            }
        }
    });

    let app_handle_for_mdns = app_handle.clone();
    let db_for_mdns = db.clone();
    let active_peers_for_mdns = active_peers.clone();
    let last_peer_seen_ms_for_mdns = last_peer_seen_ms.clone();
    let peer_cache_for_mdns = peer_cache.clone();
    let outbox_for_mdns = outbox_tx.clone();

    tauri::async_runtime::spawn(async move {
        let port = match port_rx.await {
            Ok(p) => p,
            Err(_) => return,
        };
        let hostname = "athena";
        let instance_name = format!("athena-lan-{node_id}");

        let mut props = std::collections::HashMap::<String, String>::new();
        props.insert("node_id".to_string(), node_id.to_string());

        let service_info = ServiceInfo::new(
            LAN_SERVICE_TYPE,
            &instance_name,
            hostname,
            "0.0.0.0",
            port,
            props,
        )
        .expect("ServiceInfo create")
        .enable_addr_auto();

        let daemon = ServiceDaemon::new().expect("mdns daemon");
        let _ = daemon.register(service_info);

        let receiver = daemon.browse(LAN_SERVICE_TYPE).expect("mdns browse failed");

        let rx = receiver;
        while let Ok(event) = rx.recv_async().await {
            if let ServiceEvent::ServiceResolved(resolved) = event {
                let peer_node_id = resolved
                    .txt_properties
                    .get_property_val_str("node_id")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                if peer_node_id == 0 || peer_node_id == node_id {
                    continue;
                }
                let ip = resolved.addresses.iter().find_map(|scoped| match scoped {
                    mdns_sd::ScopedIp::V4(v4) => Some(*v4.addr()),
                    _ => None,
                });
                let Some(ip) = ip else {
                    continue;
                };
                let addr = SocketAddr::new(ip.into(), resolved.port);

                let should_spawn = {
                    let mut guard = peer_cache_for_mdns.lock().await;
                    if guard.contains(&addr) {
                        false
                    } else {
                        guard.insert(addr);
                        true
                    }
                };
                if !should_spawn {
                    continue;
                }

                active_peers_for_mdns.fetch_add(1, Ordering::Relaxed);
                last_peer_seen_ms_for_mdns.store(now_unix_ms(), Ordering::Relaxed);
                let peer = LanPeerAddr {
                    addr,
                    _node_id: peer_node_id,
                };
                let db = db_for_mdns.clone();
                let app_handle = app_handle_for_mdns.clone();
                let active_peers_for_mdns_inner = active_peers_for_mdns.clone();
                let last_ms = last_peer_seen_ms_for_mdns.clone();
                let outbox = outbox_for_mdns.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = run_client_session(peer, db, app_handle, outbox, last_ms).await;
                    active_peers_for_mdns_inner.fetch_sub(1, Ordering::Relaxed);
                });
            }
        }
    });

    let db_for_fallback = db.clone();
    let app_handle_for_fallback = app_handle.clone();
    let last_peer_seen_for_fallback = last_peer_seen_ms.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;

            let peers = active_peers.load(Ordering::Relaxed);
            let last_seen = last_peer_seen_for_fallback.load(Ordering::Relaxed);
            if peers == 0 && now_unix_ms().saturating_sub(last_seen) > 60_000 {
                if let Some(radio_state) = app_handle_for_fallback.try_state::<RadioState>() {
                    if let Ok(db_guard) = db_for_fallback.lock() {
                        if let Ok(markers) = db_guard.list_map_markers() {
                            let recent = markers.into_iter().take(30);
                            if let Ok(mut port_guard) = radio_state.tx_port.lock() {
                                if let Some(port) = port_guard.as_mut() {
                                    for marker in recent {
                                        let encoded_json = match serde_json::to_string(&marker) {
                                            Ok(s) => s,
                                            Err(_) => continue,
                                        };
                                        let encoded = format!("ATK:{encoded_json}");
                                        let _ = port.write_all(encoded.as_bytes());
                                        let _ = port.write_all(b"\n");
                                        let _ = port.flush();
                                    }
                                }
                            }
                        }
                    }
                }

                // Do not auto-broadcast logbook/calendar mesh sync here: it wastes LoRa airtime and
                // repeats ~every minute when no LAN peers. Use Logbook / Calendar "pull from mesh"
                // (or `logbook_request_mesh_sync` / calendar equivalent) when you want a sync.
                last_peer_seen_for_fallback.store(now_unix_ms(), Ordering::Relaxed);
            }
        }
    });
}
