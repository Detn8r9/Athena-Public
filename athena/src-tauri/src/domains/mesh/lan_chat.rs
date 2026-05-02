//! LAN tactical chat: lightweight UDP broadcast messages between Athena nodes (separate from file replication).
//!
//! Messages are signed with a per-install secret (HMAC-like SHA-256 over structured fields) and persisted to SQLite.
//! The receive loop runs on a dedicated thread and emits Tauri events for realtime UI updates.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use base64::Engine;
use socket2::{Domain, Protocol, Socket, Type};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;

use crate::db::Database;
use crate::lan_identity;

pub const LAN_CHAT_EVENT_UPDATED: &str = "athena:lan-chat-updated";

const LAN_CHAT_PORT: u16 = 45877;
const MAX_MSG_BYTES: usize = 2048;

static LAN_CHAT_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanChatMessageRow {
    pub id: String,
    pub sender_node_id: u32,
    pub sender_label: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanChatWire {
    node_id: u32,
    unix_ms: u64,
    nonce: u64,
    sender_label: String,
    content: String,
    sig_b64: String,
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

fn sign_payload(secret: &[u8], p: &LanChatWire) -> String {
    let mut h = Sha256::new();
    h.update(secret);
    h.update(p.node_id.to_le_bytes());
    h.update(p.unix_ms.to_le_bytes());
    h.update(p.nonce.to_le_bytes());
    h.update(p.sender_label.as_bytes());
    h.update(b"\n");
    h.update(p.content.as_bytes());
    let sig = h.finalize();
    base64::engine::general_purpose::STANDARD.encode(sig)
}

fn verify_payload(secret: &[u8], p: &LanChatWire) -> bool {
    sign_payload(secret, p) == p.sig_b64
}

fn bind_socket() -> Result<UdpSocket, String> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        // `socket2` v0.5 does not expose reuse_port on all targets; set it manually when available.
        unsafe {
            let opt: libc::c_int = 1;
            let _ = libc::setsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                &opt as *const _ as *const libc::c_void,
                std::mem::size_of_val(&opt) as libc::socklen_t,
            );
        }
    }
    let addr: socket2::SockAddr = SocketAddr::from(([0, 0, 0, 0], LAN_CHAT_PORT)).into();
    socket.bind(&addr).map_err(|e| e.to_string())?;
    let sock: UdpSocket = socket.into();
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(250)));
    Ok(sock)
}

pub fn spawn_lan_chat_listener(app: AppHandle, db: Arc<Mutex<Database>>) {
    if LAN_CHAT_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        let sock = match bind_socket() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "lan chat bind failed");
                LAN_CHAT_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut buf = vec![0u8; MAX_MSG_BYTES];
        loop {
            if !LAN_CHAT_RUNNING.load(Ordering::SeqCst) {
                break;
            }
            match sock.recv_from(&mut buf) {
                Ok((n, _addr)) => {
                    if n == 0 {
                        continue;
                    }
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let msg: LanChatWire = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if msg.content.trim().is_empty() || msg.content.len() > 1400 {
                        continue;
                    }
                    let local_id = lan_identity::lan_local_node_id();
                    if local_id != 0 && msg.node_id == local_id {
                        // Sender already persists locally; ignore loopback.
                        continue;
                    }
                    let secret = {
                        let guard = db.lock();
                        if guard.is_err() {
                            continue;
                        }
                        guard.unwrap().get_or_create_lan_chat_secret_b64().ok()
                    };
                    let Some(secret_b64) = secret else { continue };
                    let secret_bytes: Vec<u8> = match base64::engine::general_purpose::STANDARD.decode(secret_b64) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    if !verify_payload(&secret_bytes, &msg) {
                        continue;
                    }
                    let row = LanChatMessageRow {
                        id: Uuid::new_v4().to_string(),
                        sender_node_id: msg.node_id,
                        sender_label: msg.sender_label.clone(),
                        content: msg.content.clone(),
                        created_at: chrono::Utc::now().timestamp(),
                    };
                    if let Ok(guard) = db.lock() {
                        let _ = guard.insert_lan_chat_message(&row);
                    }
                    let _ = app.emit(LAN_CHAT_EVENT_UPDATED, serde_json::json!({}));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(_) => continue,
            }
        }
        LAN_CHAT_RUNNING.store(false, Ordering::SeqCst);
    });
}

pub fn stop_lan_chat_listener() {
    LAN_CHAT_RUNNING.store(false, Ordering::SeqCst);
    // socket read loop will exit on next timeout
}

#[tauri::command]
pub fn lan_chat_list_messages(state: tauri::State<'_, crate::AppState>, limit: Option<u32>) -> Result<Vec<LanChatMessageRow>, String> {
    let lim = limit.unwrap_or(200).clamp(1, 2000) as i64;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_lan_chat_messages(lim)
}

#[tauri::command]
pub fn lan_chat_send_message(app: AppHandle, state: tauri::State<'_, crate::AppState>, text: String, sender_label: Option<String>) -> Result<(), String> {
    let content = text.trim().to_string();
    if content.is_empty() {
        return Err("Message is empty.".to_string());
    }
    if content.len() > 1400 {
        return Err("Message is too long.".to_string());
    }
    let node_id = lan_identity::lan_local_node_id();
    if node_id == 0 {
        return Err("LAN node id is not initialized yet.".to_string());
    }
    let label = sender_label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Athena")
        .to_string();

    let secret_b64 = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_or_create_lan_chat_secret_b64()?
    };
    let secret_bytes = base64::engine::general_purpose::STANDARD
        .decode(secret_b64)
        .map_err(|_| "Invalid LAN chat secret.".to_string())?;

    let wire_base = LanChatWire {
        node_id,
        unix_ms: now_unix_ms(),
        nonce: rand::random::<u64>(),
        sender_label: label.clone(),
        content: content.clone(),
        sig_b64: "".to_string(),
    };
    let mut wire = wire_base.clone();
    wire.sig_b64 = sign_payload(&secret_bytes, &wire_base);

    let pkt = serde_json::to_vec(&wire).map_err(|e| e.to_string())?;
    let sock = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
    let _ = sock.set_broadcast(true);
    let dest = SocketAddr::from(([255, 255, 255, 255], LAN_CHAT_PORT));
    let _ = sock.send_to(&pkt, dest);

    let row = LanChatMessageRow {
        id: Uuid::new_v4().to_string(),
        sender_node_id: node_id,
        sender_label: label,
        content,
        created_at: chrono::Utc::now().timestamp(),
    };
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.insert_lan_chat_message(&row)?;
    }
    let _ = app.emit(LAN_CHAT_EVENT_UPDATED, serde_json::json!({}));
    Ok(())
}

