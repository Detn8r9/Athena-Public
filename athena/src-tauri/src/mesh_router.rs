//! DB-backed mesh network chat: profiles, first-message auth, `/ai` inference, operator-visible log.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::config::load_config;
use crate::db::Database;
use crate::mesh_ai::{chunk_mesh_utf8, should_run_mesh_ai};

pub const MESH_NETWORK_CHAT_UPDATED: &str = "athena:mesh-network-chat-updated";

const ATHREG1_PREFIX: &str = "ATHREG1:";

#[derive(Debug, Deserialize)]
struct Athreg1Payload {
    p: String,
    u: String,
    c: String,
}

fn try_parse_athreg1(text: &str) -> Option<Athreg1Payload> {
    let rest = text.trim().strip_prefix(ATHREG1_PREFIX)?;
    serde_json::from_str::<Athreg1Payload>(rest.trim_start()).ok()
}

async fn send_dm_chunks(
    sink: &Arc<dyn MeshDmSink>,
    lines: &[&str],
    channel: u32,
    dest: u32,
) {
    for (i, line) in lines.iter().enumerate() {
        let s = (*line).to_string();
        if s.is_empty() {
            continue;
        }
        let last = i + 1 == lines.len();
        if sink
            .send_dm(s, last, channel, Some(dest))
            .await
            .is_err()
        {
            break;
        }
    }
}

/// Send a DM (or broadcast if `destination` is `None`) via the Meshtastic command queue.
pub trait MeshDmSink: Send + Sync {
    fn send_dm(
        &self,
        text: String,
        want_ack: bool,
        channel: u32,
        destination: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>>;
}

#[derive(Serialize, Clone)]
struct MeshNetworkChatUpdatedPayload {
    node_id: u32,
}

pub fn spawn_network_job(
    sink: Arc<dyn MeshDmSink>,
    app: tauri::AppHandle,
    app_data_dir: PathBuf,
    db: Arc<Mutex<Database>>,
    ai_permits: Arc<Semaphore>,
    from: u32,
    to: u32,
    my: u32,
    channel: u32,
    text: String,
) {
    if !should_run_mesh_ai(from, to, my, channel) {
        return;
    }

    tokio::spawn(async move {
        let _permit = match ai_permits.acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };

        let trimmed = text.trim();
        let node_i64 = i64::from(from);
        let profile_opt = {
            let g = match db.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            match g.mesh_profile_get_by_node(node_i64) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[mesh-router] profile lookup: {e}");
                    return;
                }
            }
        };

        if profile_opt.is_none() {
            let cfg = load_config(app_data_dir.as_path());
            if let Some(reg) = try_parse_athreg1(trimmed) {
                let u = reg.u.trim();
                let c = reg.c.trim();
                let p = reg.p.trim();
                if u.is_empty() || c.is_empty() || p.is_empty() {
                    send_dm_chunks(
                        &sink,
                        &["Registration incomplete. Need u, c, and p in JSON."],
                        channel,
                        from,
                    )
                    .await;
                    return;
                }
                match cfg.mesh_registration_admin_password.as_deref() {
                    None | Some("") => {
                        send_dm_chunks(
                            &sink,
                            &["Remote signup is off. The operator must add your radio in Athena."],
                            channel,
                            from,
                        )
                        .await;
                    }
                    Some(expected) if p != expected => {
                        send_dm_chunks(
                            &sink,
                            &["Registration failed. The phrase does not match what the operator set."],
                            channel,
                            from,
                        )
                        .await;
                    }
                    Some(_) => {
                        let welcome_id = Uuid::new_v4().to_string();
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        let ins = {
                            let g = match db.lock() {
                                Ok(g) => g,
                                Err(_) => return,
                            };
                            let r = g.mesh_profile_upsert(node_i64, u, c);
                            if r.is_ok() {
                                let _ = g.mesh_chat_insert(
                                    &welcome_id,
                                    node_i64,
                                    "human",
                                    "chat",
                                    "Registered this radio from the mesh (remote signup).",
                                    now_ms,
                                );
                            }
                            r
                        };
                        match ins {
                            Ok(_) => {
                                let _ = app.emit(
                                    MESH_NETWORK_CHAT_UPDATED,
                                    MeshNetworkChatUpdatedPayload { node_id: from },
                                );
                                send_dm_chunks(
                                    &sink,
                                    &[
                                        "You are registered. This station will remember your radio.",
                                        "Send messages normally. Use: /ai then your question for local AI help.",
                                        &format!("Your counter-sign (keep private): {}", c),
                                    ],
                                    channel,
                                    from,
                                )
                                .await;
                            }
                            Err(e) => {
                                let _ = sink
                                    .send_dm(
                                        format!("Could not save profile: {}", e),
                                        true,
                                        channel,
                                        Some(from),
                                    )
                                    .await;
                            }
                        }
                    }
                }
                return;
            }

            if cfg.mesh_registration_admin_password.as_deref().map(str::len).unwrap_or(0) > 0 {
                let parts = chunk_mesh_utf8(
                    "This station does not know your radio yet. Ask the operator for the mesh signup phrase, then send ONE line (copy carefully): ATHREG1:{\"p\":\"PHRASE\",\"u\":\"YourCallsign\",\"c\":\"YourSecretCode\"} — Replace PHRASE with the operator phrase, YourCallsign is your name here, YourSecretCode is a private code you and the operator agree on.",
                    200,
                );
                for (i, part) in parts.iter().enumerate() {
                    if part.is_empty() {
                        continue;
                    }
                    let last = i + 1 == parts.len();
                    if sink
                        .send_dm(part.clone(), last, channel, Some(from))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            } else {
                send_dm_chunks(
                    &sink,
                    &[
                        "This station does not know your radio. The operator adds contacts in Athena: Mesh network screen, then profiles.",
                    ],
                    channel,
                    from,
                )
                .await;
            }
            return;
        }

        let profile = profile_opt.unwrap();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if trimmed.starts_with("/ai") {
            let cfg_ai = load_config(app_data_dir.as_path());
            if !cfg_ai.mesh_ai_auto_reply.unwrap_or(true) {
                let _ = sink
                    .send_dm(
                        "Mesh AI is off on this station. The operator can turn it on under Settings → Mesh."
                            .to_string(),
                        true,
                        channel,
                        Some(from),
                    )
                    .await;
                return;
            }
            let prompt = trimmed[3..].trim_start();
            if prompt.is_empty() {
                let _ = sink
                    .send_dm(
                        "Usage: /ai your question".to_string(),
                        false,
                        channel,
                        Some(from),
                    )
                    .await;
                return;
            }

            let remote_before = {
                let g = match db.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                match g.mesh_chats_remote_count_for_node(node_i64) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[mesh-router] remote count: {e}");
                        return;
                    }
                }
            };

            let row_id = Uuid::new_v4().to_string();
            {
                let g = match db.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if let Err(e) = g.mesh_chat_insert(
                    &row_id,
                    node_i64,
                    "remote",
                    "prompt",
                    prompt,
                    now_ms,
                ) {
                    eprintln!("[mesh-router] insert prompt: {e}");
                    return;
                }
            }

            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );

            if remote_before == 0 {
                let msg = format!(
                    "Authentication Confirmed. ATHENA Counter-Sign: {}",
                    profile.counter_sign
                );
                let _ = sink
                    .send_dm(msg, true, channel, Some(from))
                    .await;
            }

            let db_ai = Arc::clone(&db);
            let app_dir = app_data_dir.clone();
            let reply_result = tokio::task::spawn_blocking(move || {
                crate::mesh_ai::generate_mesh_slash_ai_reply(&db_ai, app_dir.as_path(), node_i64)
            })
            .await;

            let reply_text = match reply_result {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    eprintln!("[mesh-router] /ai inference: {e}");
                    let _ = sink
                        .send_dm(format!("Error: {e}"), false, channel, Some(from))
                        .await;
                    return;
                }
                Err(e) => {
                    eprintln!("[mesh-router] /ai join: {e}");
                    return;
                }
            };

            if reply_text.is_empty() {
                let _ = sink
                    .send_dm(
                        "Error: model returned an empty reply.".to_string(),
                        false,
                        channel,
                        Some(from),
                    )
                    .await;
                return;
            }

            let resp_id = Uuid::new_v4().to_string();
            let resp_now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            {
                let g = match db.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if let Err(e) = g.mesh_chat_insert(
                    &resp_id,
                    node_i64,
                    "ai",
                    "response",
                    &reply_text,
                    resp_now,
                ) {
                    eprintln!("[mesh-router] insert ai response: {e}");
                }
            }

            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );

            let chunks = chunk_mesh_utf8(&reply_text, 180);
            let n = chunks.len().max(1);
            for (i, part) in chunks.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }
                let msg = format!("[{}/{}] {}", i + 1, n, part);
                let last = i + 1 == n;
                if sink
                    .send_dm(msg.clone(), last, channel, Some(from))
                    .await
                    .is_err()
                {
                    break;
                }
            }

            return;
        }

        let remote_before = {
            let g = match db.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            match g.mesh_chats_remote_count_for_node(node_i64) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[mesh-router] remote count: {e}");
                    return;
                }
            }
        };

        let row_id = Uuid::new_v4().to_string();
        {
            let g = match db.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Err(e) = g.mesh_chat_insert(
                &row_id,
                node_i64,
                "remote",
                "chat",
                trimmed,
                now_ms,
            ) {
                eprintln!("[mesh-router] insert chat: {e}");
                return;
            }
        }

        let _ = app.emit(
            MESH_NETWORK_CHAT_UPDATED,
            MeshNetworkChatUpdatedPayload { node_id: from },
        );

        if remote_before == 0 {
            let msg = format!(
                "Authentication Confirmed. ATHENA Counter-Sign: {}",
                profile.counter_sign
            );
            let _ = sink
                .send_dm(msg, true, channel, Some(from))
                .await;
        }
    });
}
