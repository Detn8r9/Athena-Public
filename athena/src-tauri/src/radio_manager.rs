//! USB serial radio terminal: background **read thread** uses `read()` with a **timeout** so the
//! main thread never blocks; `TimedOut` errors spin the loop. TX holds the mutex only for
//! `write_all` + `flush`.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64_ENGINE;
use base64::Engine;
use regex::Regex;
use serde::Serialize;
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;

use crate::db::{MapMarker, RadioChannel, RadioLog};
use crate::lan_sync;
use crate::AppState;

pub struct RadioState {
    pub tx_port: Mutex<Option<Box<dyn serialport::SerialPort>>>,
}

impl RadioState {
    pub fn new() -> Self {
        Self {
            tx_port: Mutex::new(None),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct PeerPositionPayload {
    node_id: String,
    latitude: f64,
    longitude: f64,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RadioTelemetryPayload {
    pub rssi: i64,
    pub snr: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshMessageAckPayload {
    pub packet_id: String,
    pub delivered: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendRadioMessageResult {
    pub packet_id: String,
}

fn parse_peer_position(line: &str) -> Option<PeerPositionPayload> {
    static NODE_RE: OnceLock<Regex> = OnceLock::new();
    static NODE_ALT_RE: OnceLock<Regex> = OnceLock::new();
    static LAT_RE: OnceLock<Regex> = OnceLock::new();
    static LON_RE: OnceLock<Regex> = OnceLock::new();

    let node_re = NODE_RE.get_or_init(|| {
        Regex::new(r#"(?i)(?:from|node(?:_id)?|id)\s*[:=]\s*["']?([!A-Za-z0-9_-]+)["']?"#)
            .expect("node regex must compile")
    });
    let node_alt_re = NODE_ALT_RE.get_or_init(|| {
        Regex::new(r#"(?i)\bnode\s+([!A-Za-z0-9_-]+)\b"#)
            .expect("alternate node regex must compile")
    });
    let lat_re = LAT_RE.get_or_init(|| {
        Regex::new(r#"(?i)(?:lat|latitude)\s*[:=]\s*(-?\d{1,2}(?:\.\d+)?)"#).expect("lat regex")
    });
    let lon_re = LON_RE.get_or_init(|| {
        Regex::new(r#"(?i)(?:lon|lng|longitude)\s*[:=]\s*(-?\d{1,3}(?:\.\d+)?)"#)
            .expect("lon regex")
    });

    let node_id = node_re
        .captures(line)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .or_else(|| {
            node_alt_re
                .captures(line)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        })?;
    let latitude = lat_re
        .captures(line)
        .and_then(|caps| caps.get(1).and_then(|m| m.as_str().parse::<f64>().ok()))?;
    let longitude = lon_re
        .captures(line)
        .and_then(|caps| caps.get(1).and_then(|m| m.as_str().parse::<f64>().ok()))?;

    if !((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)) {
        return None;
    }

    Some(PeerPositionPayload {
        node_id,
        latitude,
        longitude,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// `ATH:ACK id=<packet_id>`
fn parse_ath_ack(line: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"(?i)^ATH:ACK\s+id=([^\s]+)\s*$").expect("ATH:ACK regex"));
    re.captures(line)
        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// `ATH:SIG rssi=<int> snr=<float>`
fn parse_ath_sig(line: &str) -> Option<(i64, f64)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)^ATH:SIG\s+rssi=(-?\d+)\s+snr=(-?\d+(?:\.\d+)?)\s*$")
            .expect("ATH:SIG regex")
    });
    let caps = re.captures(line)?;
    let rssi = caps.get(1)?.as_str().parse().ok()?;
    let snr = caps.get(2)?.as_str().parse().ok()?;
    Some((rssi, snr))
}

#[tauri::command]
pub async fn get_available_ports() -> Result<Vec<String>, String> {
    let ports =
        serialport::available_ports().map_err(|e| format!("Could not list serial ports: {e}"))?;
    Ok(ports.into_iter().map(|p| p.port_name).collect())
}

#[tauri::command]
pub async fn connect_to_radio(
    port_name: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, RadioState>,
) -> Result<bool, String> {
    let port_name = port_name.trim().to_string();
    if port_name.is_empty() {
        return Err("Port name is required.".to_string());
    }
    let port = serialport::new(&port_name, 115_200)
        .timeout(Duration::from_millis(500))
        .open()
        .map_err(|e| format!("Could not open {port_name} at 115200 baud: {e}"))?;
    let mut read_port = port
        .try_clone()
        .map_err(|e| format!("Could not clone serial port for receive loop: {e}"))?;
    let tx_port = port
        .try_clone()
        .map_err(|e| format!("Could not clone serial port for transmit: {e}"))?;
    {
        let mut guard = state
            .tx_port
            .lock()
            .map_err(|e| format!("Radio TX state lock failed: {e}"))?;
        *guard = Some(tx_port);
    }

    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match read_port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let incoming = String::from_utf8_lossy(&buf[..n]).replace('\r', "");
                    for line_raw in incoming.split('\n') {
                        let line = line_raw.trim();
                        if line.is_empty() {
                            continue;
                        }

                        // Order: ATH:ACK → ATH:SIG → ATK: / peer → generic RX log
                        if let Some(packet_id) = parse_ath_ack(line) {
                            let delivered = if let Some(state) = app_handle.try_state::<AppState>()
                            {
                                if let Ok(g) = state.db.lock() {
                                    match g.mark_mesh_message_delivered(&packet_id) {
                                        Ok(Some(m)) => {
                                            if let Ok(v) = serde_json::to_value(&m) {
                                                lan_sync::publish_local_sync_row(
                                                    lan_sync::LAN_TBL_MESH_MESSAGES,
                                                    v,
                                                );
                                            }
                                            true
                                        }
                                        Ok(None) => false,
                                        Err(_) => false,
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            let _ = app_handle.emit(
                                "mesh-message-ack",
                                MeshMessageAckPayload {
                                    packet_id: packet_id.clone(),
                                    delivered,
                                },
                            );
                            continue;
                        }

                        if let Some((rssi, snr)) = parse_ath_sig(line) {
                            let ts = chrono::Utc::now().to_rfc3339();
                            if let Some(state) = app_handle.try_state::<AppState>() {
                                if let Ok(g) = state.db.lock() {
                                    if let Ok(Some(sample)) = g.insert_radio_telemetry(
                                        lan_sync::lan_local_node_id(),
                                        rssi,
                                        snr,
                                    ) {
                                        if let Ok(v) = serde_json::to_value(&sample) {
                                            lan_sync::publish_local_sync_row(
                                                lan_sync::LAN_TBL_RADIO_TELEMETRY,
                                                v,
                                            );
                                        }
                                    }
                                }
                            }
                            let _ = app_handle.emit(
                                "radio-telemetry",
                                RadioTelemetryPayload {
                                    rssi,
                                    snr,
                                    timestamp: ts,
                                },
                            );
                            continue;
                        }

                        if let Some(rest) = line.strip_prefix("ATK:") {
                            if let Ok(marker) = serde_json::from_str::<MapMarker>(rest) {
                                if let Some(state) = app_handle.try_state::<AppState>() {
                                    if let Ok(g) = state.db.lock() {
                                        if g.upsert_map_marker(&marker).is_ok() {
                                            if let Ok(v) = serde_json::to_value(&marker) {
                                                lan_sync::publish_local_sync_row(
                                                    lan_sync::LAN_TBL_MAP_MARKERS,
                                                    v,
                                                );
                                            }
                                        }
                                    }
                                }
                                let _ = app_handle.emit("peer-marker-rx", marker);
                            }
                            continue;
                        }
                        if let Some(peer_position) = parse_peer_position(line) {
                            let _ = app_handle.emit("peer-position-rx", peer_position);
                            continue;
                        }
                        let _ = app_handle.emit("radio-rx", line.to_string());
                        let log = RadioLog {
                            id: Uuid::new_v4().to_string(),
                            direction: "RX".to_string(),
                            message: line.to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };
                        if let Some(state) = app_handle.try_state::<AppState>() {
                            if let Ok(g) = state.db.lock() {
                                let _ = g.insert_radio_log(&log);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // Keep listening; non-blocking from the app's perspective.
                }
                Err(_) => {
                    break;
                }
            }
        }
    });

    Ok(true)
}

#[tauri::command]
pub async fn send_radio_message(
    message: String,
    state: tauri::State<'_, RadioState>,
    app_state: tauri::State<'_, AppState>,
) -> Result<SendRadioMessageResult, String> {
    let msg = message.trim();
    if msg.is_empty() {
        return Err("Message is empty.".to_string());
    }
    let packet_id = Uuid::new_v4().to_string();
    let mesh_row = app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .insert_mesh_message_pending(&packet_id, msg)?;
    if let Ok(v) = serde_json::to_value(&mesh_row) {
        lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_MESH_MESSAGES, v);
    }

    let body_b64 = B64_ENGINE.encode(msg.as_bytes());
    let wire = format!("ATH:MSG id={packet_id} body={body_b64}\n");

    let mut guard = state
        .tx_port
        .lock()
        .map_err(|e| format!("Radio TX state lock failed: {e}"))?;
    let Some(port) = guard.as_mut() else {
        return Err("No radio port connected.".to_string());
    };
    port.write_all(wire.as_bytes())
        .map_err(|e| format!("Failed to write radio message: {e}"))?;
    port.flush()
        .map_err(|e| format!("Failed to flush radio port: {e}"))?;

    let log = RadioLog {
        id: Uuid::new_v4().to_string(),
        direction: "TX".to_string(),
        message: format!("packet={packet_id} {msg}"),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .insert_radio_log(&log)?;
    Ok(SendRadioMessageResult { packet_id })
}

#[tauri::command]
pub async fn get_recent_radio_logs(
    app_state: tauri::State<'_, AppState>,
) -> Result<Vec<RadioLog>, String> {
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_recent_radio_logs(50)
}

#[tauri::command]
pub async fn get_radio_telemetry_samples(
    limit: Option<u32>,
    app_state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::db::RadioTelemetrySample>, String> {
    let lim = limit.unwrap_or(100) as usize;
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_recent_radio_telemetry(lim)
}

#[tauri::command]
pub async fn get_mesh_messages(
    limit: Option<u32>,
    app_state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::db::MeshMessage>, String> {
    let lim = limit.unwrap_or(100) as usize;
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_recent_mesh_messages(lim)
}

#[tauri::command]
pub async fn list_radio_channels(
    app_state: tauri::State<'_, AppState>,
) -> Result<Vec<RadioChannel>, String> {
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_radio_channels()
}

#[tauri::command]
pub async fn upsert_radio_channel(
    channel: RadioChannel,
    app_state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if channel.id.trim().is_empty() || channel.name.trim().is_empty() {
        return Err("Channel id and name are required.".to_string());
    }
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .upsert_radio_channel(&channel)
}

#[tauri::command]
pub async fn delete_radio_channel(
    id: String,
    app_state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("Channel id is required.".to_string());
    }
    app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .delete_radio_channel(&id)
}

/// Applies a stored channel profile: sends `ADMIN:channel:*` lines (name, frequency MHz, PSK).
/// Encryption key is never written to `radio_logs`.
#[tauri::command]
pub async fn apply_channel_profile(
    channel_id: String,
    state: tauri::State<'_, RadioState>,
    app_state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let id = channel_id.trim();
    if id.is_empty() {
        return Err("Channel id is required.".to_string());
    }
    let channel = app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .get_radio_channel_by_id(id)?
        .ok_or_else(|| "Channel not found.".to_string())?;

    let name_enc = urlencoding::encode(channel.name.trim());
    let psk_b64 = B64_ENGINE.encode(channel.encryption_key.as_bytes());
    let freq = channel.frequency_mhz;

    let lines = vec![
        format!("ADMIN:channel:name:{name_enc}\n"),
        format!("ADMIN:channel:freq_mhz:{freq}\n"),
        format!("ADMIN:channel:psk:{psk_b64}\n"),
    ];

    let mut guard = state
        .tx_port
        .lock()
        .map_err(|e| format!("Radio TX state lock failed: {e}"))?;
    let Some(port) = guard.as_mut() else {
        return Err("No radio port connected.".to_string());
    };

    for line in lines {
        port.write_all(line.as_bytes())
            .map_err(|e| format!("Failed to write channel profile command: {e}"))?;
        port.flush()
            .map_err(|e| format!("Failed to flush channel profile command: {e}"))?;
    }

    Ok(true)
}

#[tauri::command]
pub async fn set_radio_config(
    config_type: String,
    value: String,
    state: tauri::State<'_, RadioState>,
) -> Result<bool, String> {
    let cfg = config_type.trim();
    if cfg.is_empty() {
        return Err("Config type is required.".to_string());
    }
    let val = value.trim();
    if val.is_empty() {
        return Err("Config value is required.".to_string());
    }
    let payload = format!("ADMIN:{cfg}:{val}\n");

    let mut guard = state
        .tx_port
        .lock()
        .map_err(|e| format!("Radio TX state lock failed: {e}"))?;
    let Some(port) = guard.as_mut() else {
        return Err("No radio port connected.".to_string());
    };

    port.write_all(payload.as_bytes())
        .map_err(|e| format!("Failed to write radio config command: {e}"))?;
    port.flush()
        .map_err(|e| format!("Failed to flush radio config command: {e}"))?;
    Ok(true)
}

fn csv_escape_field(s: &str) -> String {
    if s.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Writes all `radio_logs` rows to a CSV file at a path from the Tauri save dialog.
#[tauri::command]
pub async fn export_radio_logs(
    app_state: tauri::State<'_, AppState>,
    dest_path: String,
) -> Result<(), String> {
    let dest = PathBuf::from(dest_path.trim());
    if dest.as_os_str().is_empty() {
        return Err("Destination path is empty.".to_string());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let logs = app_state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_all_radio_logs()?;
    let mut csv = String::from("Timestamp,Direction,Message\n");
    for row in logs {
        csv.push_str(&csv_escape_field(&row.timestamp));
        csv.push(',');
        csv.push_str(&csv_escape_field(&row.direction));
        csv.push(',');
        csv.push_str(&csv_escape_field(&row.message));
        csv.push('\n');
    }
    std::fs::write(&dest, csv).map_err(|e| e.to_string())?;
    Ok(())
}
