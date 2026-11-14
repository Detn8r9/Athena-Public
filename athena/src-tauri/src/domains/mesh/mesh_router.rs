//! DB-backed mesh network chat: profiles, OTA REGISTER auth, `/ai` inference, operator queue, remote AI FIFO.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::config::load_config;
use crate::db::personnel_pin_hash;
use crate::db::Database;
use crate::db::SOS_STATE_ACTIVE;
use crate::db::SOS_STATE_SUSPENDED;
use crate::mesh_policy;
use crate::mesh_sos::{MeshEmergencyPayload, MESH_EMERGENCY_EVENT};

pub const MESH_NETWORK_CHAT_UPDATED: &str = "athena:mesh-network-chat-updated";
pub const MESH_OPERATOR_QUEUE_MESSAGE: &str = "athena:mesh-operator-queue-message";

const ATHREG1_PREFIX: &str = "ATHREG1:";

#[derive(Debug, Deserialize)]
struct Athreg1Payload {
    p: String,
    u: String,
    c: String,
}

#[derive(Serialize, Clone)]
pub(crate) struct MeshNetworkChatUpdatedPayload {
    pub node_id: u32,
}

#[derive(Serialize, Clone)]
pub struct MeshOperatorQueuePayload {
    pub node_id: u32,
    pub preview: String,
    #[serde(default)]
    pub kind: String,
}

fn try_parse_athreg1(text: &str) -> Option<Athreg1Payload> {
    let rest = text.trim().strip_prefix(ATHREG1_PREFIX)?;
    serde_json::from_str::<Athreg1Payload>(rest.trim_start()).ok()
}

/// `REGISTER Your Name 123456` — last token must be six digits; everything between REGISTER and PIN is the name.
fn parse_register_line(text: &str) -> Option<(String, String)> {
    let t = text.trim();
    let upper = t.to_ascii_uppercase();
    let rest = if upper.starts_with("REGISTER") {
        t.get(8..).map(str::trim)?
    } else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let pin = *parts.last()?;
    if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let name = parts[..parts.len() - 1].join(" ");
    if name.trim().is_empty() {
        return None;
    }
    Some((name, pin.to_string()))
}

pub(crate) fn parse_sos_clear_tail(t: &str) -> Option<String> {
    let tr = t.trim();
    let head = "SOS CLEAR";
    let u = tr.to_ascii_uppercase();
    if u == head {
        return Some(String::new());
    }
    let prefix = format!("{head} ");
    if u.starts_with(&prefix.to_ascii_uppercase()) {
        return Some(tr[head.len()..].trim().to_string());
    }
    if u.starts_with("SOS CLEAR	") {
        return Some(tr[head.len()..].trim().to_string());
    }
    None
}

pub(crate) fn emit_mesh_emergency(
    app: &tauri::AppHandle,
    db: &Mutex<Database>,
    node_id: u32,
) {
    let node_i = i64::from(node_id);
    let prof = {
        let g = match db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match g.mesh_profile_get_by_node(node_i) {
            Ok(p) => p,
            Err(_) => return,
        }
    };
    let Some(p) = prof else {
        return;
    };
    let pos = {
        let g = match db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.mesh_last_position_for_node(node_id).ok().flatten()
    };
    let (lat, lon, last_seen_at) = match pos {
        Some((a, b, c)) => (Some(a), Some(b), Some(c)),
        None => (None, None, None),
    };
    let payload = MeshEmergencyPayload {
        node_id,
        username: p.username.clone(),
        sos_state: p.sos_state,
        is_registered: p.is_registered,
        lat,
        lon,
        last_seen_at,
    };
    let _ = app.emit(MESH_EMERGENCY_EVENT, payload);
}

fn new_mesh_counter_sign() -> String {
    Uuid::new_v4()
        .to_string()
        .chars()
        .filter(|c| *c != '-')
        .take(10)
        .collect()
}

/// Compare secrets without stopping at the first difference.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) async fn send_dm_chunks(
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

#[allow(clippy::too_many_arguments)]
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
    pki_encrypted: bool,
    public_key: Vec<u8>,
) {
    let disposition = mesh_policy::disposition(from, to, my, pki_encrypted);
    if disposition == mesh_policy::Disposition::Ignore {
        return;
    }

    tokio::spawn(async move {
        let _permit = match ai_permits.acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };
        if disposition == mesh_policy::Disposition::Unencrypted {
            // Readable by anyone holding the channel key, and the sender is unproven: act on
            // nothing, and say why at most once a day.
            let now_s = chrono::Utc::now().timestamp();
            let allowed = mesh_policy::hints().lock().map(|mut g| g.allow(&format!("enc:{from}"), now_s, mesh_policy::HINT_EVERY_SECS)).unwrap_or(false);
            if allowed {
                send_dm_chunks(
                    &sink,
                    &["This station only answers encrypted direct messages. Use Meshtastic 2.5 or newer, wait until your radio has this station's key (it appears in your node list with a lock), then send again."],
                    channel,
                    from,
                )
                .await;
            }
            return;
        }

        // Group-key check and duress word, before anything acts on the message.
        let incoming = crate::field_ops::trust::inspect_incoming(
            text.trim(),
            chrono::Utc::now().timestamp(),
        );
        let trimmed = incoming.body.trim();
        let node_i64 = i64::from(from);
        if incoming.duress {
            // Raise the alarm here and carry on as normal, so the reply gives nothing away.
            if let Ok(g) = db.lock() {
                let who = g
                    .mesh_profile_get_by_node(node_i64)
                    .ok()
                    .flatten()
                    .map(|p| p.username)
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| format!("Node {from}"));
                if let Ok(n) = g.command_notification_insert(
                    "duress",
                    "high",
                    &format!("Duress signal from {who}"),
                    Some("Their last message contained the duress word. Treat anything they send as sent under pressure, and do not reveal that you know."),
                    None,
                ) {
                    let _ = app.emit(crate::mesh_listener::ATHENA_COMMAND_NOTIFICATION, &n);
                }
            }
        }
        if incoming.authenticity == crate::field_ops::trust::Authenticity::Invalid {
            // Signed, but not with our key (or the clocks are more than half an hour apart).
            // Log it for the operator and act on nothing.
            if let Ok(g) = db.lock() {
                let _ = g.mesh_chat_insert(
                    &Uuid::new_v4().to_string(),
                    node_i64,
                    "remote",
                    "chat",
                    &format!("[SIGNATURE CHECK FAILED] {trimmed}"),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0),
                );
            }
            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );
            return;
        }
        let sos_st = {
            let g = match db.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            g.mesh_profile_sos_state(node_i64)
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        if let Some(phrase) = parse_sos_clear_tail(trimmed) {
            if sos_st == SOS_STATE_ACTIVE {
                let row_id = Uuid::new_v4().to_string();
                let content = format!("[CLEARANCE] secret_phrase={}", phrase);
                {
                    let g = match db.lock() {
                        Ok(g) => g,
                        Err(_) => return,
                    };
                    if let Err(e) = g.mesh_profile_set_sos_state(node_i64, SOS_STATE_SUSPENDED) {
                        eprintln!("[mesh-router] sos suspend: {e}");
                        return;
                    }
                    if let Err(e) = g.mesh_chat_insert(
                        &row_id,
                        node_i64,
                        "remote",
                        "clearance",
                        &content,
                        now_ms,
                    ) {
                        eprintln!("[mesh-router] clearance insert: {e}");
                        return;
                    }
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
                    &sink,
                    &["Clear command received. Standby for Command verification."],
                    channel,
                    from,
                )
                .await;
                emit_mesh_emergency(&app, &db, from);
                return;
            }
        }

        if sos_st == SOS_STATE_ACTIVE || sos_st == SOS_STATE_SUSPENDED {
            let row_id = Uuid::new_v4().to_string();
            let store = trimmed;
            let body = if store.is_empty() {
                "(emergency channel; empty body)"
            } else {
                store
            };
            {
                let g = match db.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if let Err(e) = g.mesh_chat_insert(
                    &row_id,
                    node_i64,
                    "remote",
                    "emergency",
                    body,
                    now_ms,
                ) {
                    eprintln!("[mesh-router] emergency insert: {e}");
                    return;
                }
            }
            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );
            let preview: String = body.chars().take(120).collect();
            let _ = app.emit(
                MESH_OPERATOR_QUEUE_MESSAGE,
                MeshOperatorQueuePayload {
                    node_id: from,
                    preview,
                    kind: "emergency".to_string(),
                },
            );
            return;
        }

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

        // A radio counts as linked only with a live link pinned to the key this message came from.
        let now_s = now_ms / 1000;
        let link_days = load_config(app_data_dir.as_path()).mesh_link_days.unwrap_or(mesh_policy::DEFAULT_LINK_DAYS);
        let profile_opt = profile_opt.filter(|p| p.is_registered);
        let status = match &profile_opt {
            Some(p) => {
                let link = db.lock().ok().and_then(|g| g.mesh_link_get(node_i64).ok()).flatten();
                if link.is_none() && !public_key.is_empty() {
                    // A radio the operator added by hand (or linked before keys were pinned):
                    // pin the key it first speaks with, and tell the operator which key that was.
                    if let Ok(g) = db.lock() {
                        if g.mesh_link_set(node_i64, &public_key, now_s).is_ok() {
                            if let Ok(n) = g.command_notification_insert(
                                "mesh_key",
                                "normal",
                                &format!("Radio key recorded for {}", p.username),
                                Some(&format!(
                                    "Node {from} sent its first encrypted message. Its key (fingerprint {}) is now the only one accepted for this radio.",
                                    mesh_policy::key_fingerprint(&public_key)
                                )),
                                None,
                            ) {
                                let _ = app.emit(crate::mesh_listener::ATHENA_COMMAND_NOTIFICATION, &n);
                            }
                        }
                    }
                    mesh_policy::LinkStatus::Linked
                } else {
                    mesh_policy::link_status(link.as_ref(), &public_key, now_s, link_days)
                }
            }
            None => mesh_policy::LinkStatus::NotLinked,
        };
        if status == mesh_policy::LinkStatus::KeyMismatch {
            if let (Ok(g), Some(p)) = (db.lock(), profile_opt.as_ref()) {
                if let Ok(n) = g.command_notification_insert(
                    "mesh_key",
                    "high",
                    &format!("Radio key mismatch for {}", p.username),
                    Some(&format!(
                        "A message claiming node {from} came from a different radio key (fingerprint {}). It was refused. If {} reset or replaced their radio, they can link it again with their PIN; otherwise someone may be impersonating them.",
                        mesh_policy::key_fingerprint(&public_key),
                        p.username
                    )),
                    None,
                ) {
                    let _ = app.emit(crate::mesh_listener::ATHENA_COMMAND_NOTIFICATION, &n);
                }
            }
        }
        let profile_opt = if status == mesh_policy::LinkStatus::Linked { profile_opt } else { None };

        if profile_opt.is_none() {
            if let Some((name, pin)) = parse_register_line(trimmed) {
                let keys = mesh_policy::pin_keys(from, &name);
                let locked = mesh_policy::pins().lock().ok().and_then(|g| g.locked_for(&keys, now_s));
                if let Some(secs) = locked {
                    let line = format!("Too many wrong PINs. Try again in {} minutes.", (secs + 59) / 60);
                    send_dm_chunks(&sink, &[line.as_str()], channel, from).await;
                    return;
                }
                let row = db.lock().ok().and_then(|g| g.personnel_fetch_by_name(&name).ok()).flatten();
                // Same answer for an unknown name and a wrong PIN, so names cannot be probed.
                let matched = row.filter(|r| r.pin_hash == personnel_pin_hash(&pin));
                let Some(row) = matched else {
                    let now_locked = mesh_policy::pins().lock().map(|mut g| g.fail(&keys, now_s)).unwrap_or(false);
                    let line = if now_locked {
                        "Too many wrong PINs. Linking is locked for 30 minutes.".to_string()
                    } else {
                        "Name or PIN not recognised. Ask the station operator to add you to the roster if you are not on it.".to_string()
                    };
                    send_dm_chunks(&sink, &[line.as_str()], channel, from).await;
                    if now_locked {
                        if let Ok(g) = db.lock() {
                            if let Ok(n) = g.command_notification_insert(
                                "mesh_pin",
                                "high",
                                &format!("Repeated wrong PINs from node {from}"),
                                Some(&format!("Five wrong PINs for the name \"{}\". Linking from that radio and for that name is locked for 30 minutes.", name.trim())),
                                None,
                            ) {
                                let _ = app.emit(crate::mesh_listener::ATHENA_COMMAND_NOTIFICATION, &n);
                            }
                        }
                    }
                    return;
                };
                if crate::auth::status_is_locked(&row.operational_status) {
                    send_dm_chunks(&sink, &["This account is not active on this station."], channel, from).await;
                    return;
                }
                if let Ok(mut g) = mesh_policy::pins().lock() {
                    g.succeed(&keys);
                }
                let cs = new_mesh_counter_sign();
                let linked = db.lock().map_err(|e| e.to_string()).and_then(|g| {
                    g.mesh_profile_upsert(node_i64, row.name.trim(), &cs, row.rank_level)?;
                    g.mesh_link_set(node_i64, &public_key, now_s)
                });
                match linked {
                    Ok(()) => {
                        let _ = app.emit(MESH_NETWORK_CHAT_UPDATED, MeshNetworkChatUpdatedPayload { node_id: from });
                        let line = format!(
                            "Radio linked to {}. Rank {}. The link lasts {} days. Send ?help for station commands, or a question for the AI. //op reaches the operator.",
                            row.name.trim(),
                            row.rank_level,
                            link_days
                        );
                        send_dm_chunks(&sink, &[line.as_str()], channel, from).await;
                    }
                    Err(e) => {
                        let _ = sink.send_dm(format!("Could not link radio: {e}"), true, channel, Some(from)).await;
                    }
                }
                return;
            }

            let cfg = load_config(app_data_dir.as_path());
            if let Some(reg) = try_parse_athreg1(trimmed) {
                let u = reg.u.trim();
                let c = reg.c.trim();
                let p = reg.p.trim();
                if u.is_empty() || c.is_empty() || p.is_empty() {
                    send_dm_chunks(&sink, &["Registration incomplete. Need u, c, and p in JSON."], channel, from).await;
                    return;
                }
                let keys = mesh_policy::pin_keys(from, "#signup");
                if let Some(secs) = mesh_policy::pins().lock().ok().and_then(|g| g.locked_for(&keys[..1], now_s)) {
                    let line = format!("Too many wrong attempts. Try again in {} minutes.", (secs + 59) / 60);
                    send_dm_chunks(&sink, &[line.as_str()], channel, from).await;
                    return;
                }
                match cfg.mesh_registration_admin_password.as_deref() {
                    None | Some("") => {
                        send_dm_chunks(&sink, &["Remote signup is off. The operator must add your radio in Athena."], channel, from).await;
                    }
                    Some(expected) if !constant_time_eq(p.as_bytes(), expected.as_bytes()) => {
                        if let Ok(mut g) = mesh_policy::pins().lock() {
                            g.fail(&keys[..1], now_s);
                        }
                        send_dm_chunks(&sink, &["Registration failed. The phrase does not match what the operator set."], channel, from).await;
                    }
                    Some(_) => {
                        let welcome_id = Uuid::new_v4().to_string();
                        let ins = db.lock().map_err(|e| e.to_string()).and_then(|g| {
                            g.mesh_profile_upsert(node_i64, u, c, 1)?;
                            g.mesh_link_set(node_i64, &public_key, now_s)?;
                            let _ = g.mesh_chat_insert(
                                &welcome_id,
                                node_i64,
                                "human",
                                "chat",
                                "Registered this radio from the mesh (remote signup).",
                                now_ms,
                            );
                            Ok(())
                        });
                        match ins {
                            Ok(()) => {
                                let _ = app.emit(MESH_NETWORK_CHAT_UPDATED, MeshNetworkChatUpdatedPayload { node_id: from });
                                send_dm_chunks(
                                    &sink,
                                    &[
                                        "You are registered. This station will remember your radio.",
                                        "Send messages normally. Default routing is AI. Use //op for human operator.",
                                        &format!("Your counter-sign (keep private): {}", c),
                                    ],
                                    channel,
                                    from,
                                )
                                .await;
                            }
                            Err(e) => {
                                let _ = sink.send_dm(format!("Could not save profile: {}", e), true, channel, Some(from)).await;
                            }
                        }
                    }
                }
                return;
            }

            let hint = match status {
                mesh_policy::LinkStatus::Expired => "Your radio link has expired. Link it again with REGISTER YourName PIN, or ask the operator to renew it.",
                mesh_policy::LinkStatus::KeyMismatch => "This radio is not the one linked to that account. Link it with: REGISTER YourName PIN",
                _ => "Unknown radio. To link it to your station account, reply: REGISTER YourName PIN",
            };
            let allowed = mesh_policy::hints().lock().map(|mut g| g.allow(&format!("reg:{from}"), now_s, mesh_policy::HINT_EVERY_SECS)).unwrap_or(false);
            if allowed {
                send_dm_chunks(&sink, &[hint], channel, from).await;
            }
            return;
        }

        let Some(profile) = profile_opt else {
            eprintln!("[mesh-router] missing mesh profile after registration branch");
            return;
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        if let Some(op_tail) = trimmed
            .strip_prefix("//op")
            .or_else(|| trimmed.strip_prefix("//OP"))
        {
            let body = op_tail.trim();
            let store = if body.is_empty() {
                "(operator message empty)"
            } else {
                body
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
                    "operator",
                    store,
                    now_ms,
                ) {
                    eprintln!("[mesh-router] insert operator message: {e}");
                    return;
                }
            }
            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );
            let preview: String = store.chars().take(120).collect();
            let _ = app.emit(
                MESH_OPERATOR_QUEUE_MESSAGE,
                MeshOperatorQueuePayload {
                    node_id: from,
                    preview,
                    kind: "op".to_string(),
                },
            );
            return;
        }

        // Station commands (`?inv rice`, `?runway`, `?checkin`): answered from station data,
        // with no AI model, so they work when the model is off or asleep.
        if let Some(query) = crate::field_ops::mesh_query::parse(trimmed) {
            let reply = {
                let g = match db.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let _ = g.mesh_chat_insert(
                    &Uuid::new_v4().to_string(),
                    node_i64,
                    "remote",
                    "chat",
                    trimmed,
                    now_ms,
                );
                let reply = crate::field_ops::mesh_query::answer(&g, node_i64, &query, now_ms / 1000);
                let _ = g.mesh_chat_insert(
                    &Uuid::new_v4().to_string(),
                    node_i64,
                    "ai",
                    "response",
                    &reply,
                    now_ms + 1,
                );
                reply
            };
            let _ = app.emit(
                MESH_NETWORK_CHAT_UPDATED,
                MeshNetworkChatUpdatedPayload { node_id: from },
            );
            let chunks = crate::mesh_ai::chunk_mesh_utf8(&reply, 190);
            let lines: Vec<&str> = chunks.iter().map(String::as_str).collect();
            send_dm_chunks(&sink, &lines, channel, from).await;
            return;
        }

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

        let prompt_text: &str = if let Some(after) = trimmed.strip_prefix("/ai") {
            let p = after.trim_start();
            if p.is_empty() {
                let _ = sink
                    .send_dm(
                        "Usage: /ai your question — or send any other message for the AI.".to_string(),
                        false,
                        channel,
                        Some(from),
                    )
                    .await;
                return;
            }
            p
        } else {
            if trimmed.is_empty() {
                return;
            }
            trimmed
        };

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
                prompt_text,
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
            let _ = sink.send_dm(msg, true, channel, Some(from)).await;
        }

        let rank_level = {
            let g = match db.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            g.personnel_fetch_by_name(profile.username.trim())
                .ok()
                .flatten()
                .map(|r| r.rank_level)
                .unwrap_or(1)
        };

        let request_id = Uuid::new_v4().to_string();
        let display_label = profile.username.clone();
        match crate::remote_queue::enqueue_remote_ai_job(crate::remote_queue::RemoteAiJob {
            request_id: request_id.clone(),
            node_id: from,
            channel,
            display_label,
            rank_level,
        }) {
            Ok(ack) => {
                if ack.jobs_ahead > 0 {
                    let _ = sink
                        .send_dm(
                            format!(
                                "Your request is queued ({} ahead). Stand by.",
                                ack.jobs_ahead
                            ),
                            false,
                            channel,
                            Some(from),
                        )
                        .await;
                }
            }
            Err(e) => {
                eprintln!("[mesh-router] remote queue: {e}");
                let _ = sink
                    .send_dm(format!("Queue error: {e}"), false, channel, Some(from))
                    .await;
            }
        }
    });
}
