//! FIFO queue for remote mesh AI requests, with priority insertion for rank 4–5 personnel.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::Emitter;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::db::Database;
use crate::mesh_ai::chunk_mesh_utf8;
use crate::mesh_listener::MeshRadioControl;

pub const REMOTE_AI_QUEUE_UPDATED: &str = "athena:remote-ai-queue-updated";

const MESH_NETWORK_CHAT_UPDATED: &str = "athena:mesh-network-chat-updated";

#[derive(Clone, Debug)]
pub struct RemoteAiJob {
    pub request_id: String,
    pub node_id: u32,
    pub channel: u32,
    pub display_label: String,
    pub rank_level: i32,
}

impl RemoteAiJob {
    fn priority(&self) -> bool {
        self.rank_level >= 4
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteQueueWaitingEntry {
    pub position: usize,
    pub request_id: String,
    pub node_id: u32,
    pub display_label: String,
    pub rank_level: i32,
    pub priority: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteQueueCurrentEntry {
    pub request_id: String,
    pub node_id: u32,
    pub display_label: String,
    pub rank_level: i32,
    pub priority: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteQueueStatusDto {
    pub pending_count: usize,
    pub waiting: Vec<RemoteQueueWaitingEntry>,
    pub current: Option<RemoteQueueCurrentEntry>,
    pub active_transcript: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueAck {
    pub request_id: String,
    pub jobs_ahead: usize,
}

struct QueueInner {
    pending: VecDeque<RemoteAiJob>,
    current: Option<RemoteAiJob>,
    active_transcript: String,
}

struct RemoteQueueCtx {
    inner: Mutex<QueueInner>,
    notify: Notify,
    app: tauri::AppHandle,
    app_data_dir: PathBuf,
    db: Arc<Mutex<Database>>,
    mesh: MeshRadioControl,
}

static REMOTE_CTX: OnceLock<Arc<RemoteQueueCtx>> = OnceLock::new();

fn emit_queue_update(ctx: &RemoteQueueCtx) {
    match get_queue_status_from(ctx) {
        Ok(payload) => {
            let _ = ctx.app.emit(REMOTE_AI_QUEUE_UPDATED, &payload);
        }
        Err(e) => eprintln!("[remote-queue] emit status failed: {e}"),
    }
}

fn get_queue_status_from(ctx: &RemoteQueueCtx) -> Result<RemoteQueueStatusDto, String> {
    let g = ctx.inner.lock().map_err(|e| e.to_string())?;
    let waiting: Vec<RemoteQueueWaitingEntry> = g
        .pending
        .iter()
        .enumerate()
        .map(|(i, j)| RemoteQueueWaitingEntry {
            position: i + 1,
            request_id: j.request_id.clone(),
            node_id: j.node_id,
            display_label: j.display_label.clone(),
            rank_level: j.rank_level,
            priority: j.priority(),
        })
        .collect();
    let current = g.current.as_ref().map(|j| RemoteQueueCurrentEntry {
        request_id: j.request_id.clone(),
        node_id: j.node_id,
        display_label: j.display_label.clone(),
        rank_level: j.rank_level,
        priority: j.priority(),
    });
    Ok(RemoteQueueStatusDto {
        pending_count: g.pending.len(),
        waiting,
        current,
        active_transcript: g.active_transcript.clone(),
    })
}

/// Start the single consumer loop and store global context. Call once at app setup after mesh spawn.
pub fn spawn_remote_ai_queue_worker(
    app: tauri::AppHandle,
    app_data_dir: PathBuf,
    db: Arc<Mutex<Database>>,
    mesh: MeshRadioControl,
) {
    let ctx = Arc::new(RemoteQueueCtx {
        inner: Mutex::new(QueueInner {
            pending: VecDeque::new(),
            current: None,
            active_transcript: String::new(),
        }),
        notify: Notify::new(),
        app,
        app_data_dir,
        db,
        mesh,
    });
    if REMOTE_CTX.set(Arc::clone(&ctx)).is_err() {
        eprintln!("[remote-queue] spawn_remote_ai_queue_worker called twice; ignoring");
        return;
    }
    tauri::async_runtime::spawn(run_worker_loop(ctx));
}

fn ctx() -> Result<&'static Arc<RemoteQueueCtx>, String> {
    REMOTE_CTX.get().ok_or_else(|| "Remote AI queue is not initialized.".to_string())
}

pub fn enqueue_remote_ai_job(job: RemoteAiJob) -> Result<EnqueueAck, String> {
    let c = ctx()?;
    let jobs_ahead = {
        let mut g = c.inner.lock().map_err(|e| e.to_string())?;
        let insert_idx = if job.priority() {
            g.pending
                .iter()
                .position(|j| !j.priority())
                .unwrap_or(g.pending.len())
        } else {
            g.pending.len()
        };
        let ahead = insert_idx + usize::from(g.current.is_some());
        if job.priority() {
            g.pending.insert(insert_idx, job.clone());
        } else {
            g.pending.push_back(job.clone());
        }
        ahead
    };
    c.notify.notify_one();
    emit_queue_update(c);
    Ok(EnqueueAck {
        request_id: job.request_id,
        jobs_ahead,
    })
}

#[tauri::command]
pub fn get_queue_status() -> Result<RemoteQueueStatusDto, String> {
    get_queue_status_from(ctx()?)
}

#[tauri::command]
pub fn enqueue_request(node_id: u32, channel: u32) -> Result<EnqueueAck, String> {
    let job = RemoteAiJob {
        request_id: Uuid::new_v4().to_string(),
        node_id,
        channel,
        display_label: format!("node {}", node_id),
        rank_level: 1,
    };
    enqueue_remote_ai_job(job)
}

async fn run_worker_loop(ctx: Arc<RemoteQueueCtx>) {
    loop {
        ctx.notify.notified().await;
        loop {
            let job = {
                let mut g = match ctx.inner.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                match g.pending.pop_front() {
                    Some(j) => j,
                    None => break,
                }
            };
            {
                if let Ok(mut g) = ctx.inner.lock() {
                    g.current = Some(job.clone());
                    g.active_transcript.clear();
                }
            }
            emit_queue_update(&ctx);
            process_one_job(&ctx, job).await;
            if let Ok(mut g) = ctx.inner.lock() {
                g.current = None;
                g.active_transcript.clear();
            }
            emit_queue_update(&ctx);
        }
    }
}

#[derive(Clone, Serialize)]
struct MeshNetworkChatUpdatedPayload {
    node_id: u32,
}

async fn process_one_job(ctx: &RemoteQueueCtx, job: RemoteAiJob) {
    let db = Arc::clone(&ctx.db);
    let app_dir = ctx.app_data_dir.clone();
    let node_i64 = i64::from(job.node_id);
    let reply_result = tokio::task::spawn_blocking(move || {
        crate::mesh_ai::generate_mesh_slash_ai_reply(&db, app_dir.as_path(), node_i64)
    })
    .await;

    let reply_text = match reply_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            eprintln!("[remote-queue] inference: {e}");
            let _ = ctx
                .mesh
                .send_text_message(format!("Error: {e}"), false, job.channel, Some(job.node_id))
                .await;
            return;
        }
        Err(e) => {
            eprintln!("[remote-queue] join: {e}");
            return;
        }
    };

    if reply_text.is_empty() {
        let _ = ctx
            .mesh
            .send_text_message(
                "Error: model returned an empty reply.".to_string(),
                false,
                job.channel,
                Some(job.node_id),
            )
            .await;
        return;
    }

    {
        if let Ok(mut g) = ctx.inner.lock() {
            g.active_transcript = reply_text.clone();
        }
    }
    emit_queue_update(ctx);

    let resp_id = Uuid::new_v4().to_string();
    let resp_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if let Ok(g) = ctx.db.lock() {
        if let Err(e) = g.mesh_chat_insert(
            &resp_id,
            node_i64,
            "ai",
            "response",
            &reply_text,
            resp_now,
        ) {
            eprintln!("[remote-queue] insert ai response: {e}");
        }
    }

    let _ = ctx.app.emit(
        MESH_NETWORK_CHAT_UPDATED,
        MeshNetworkChatUpdatedPayload {
            node_id: job.node_id,
        },
    );

    let chunks = chunk_mesh_utf8(&reply_text, 180);
    let n = chunks.len().max(1);
    for (i, part) in chunks.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let msg = format!("[{}/{}] {}", i + 1, n, part);
        let last = i + 1 == n;
        if ctx
            .mesh
            .send_text_message(msg, last, job.channel, Some(job.node_id))
            .await
            .is_err()
        {
            break;
        }
    }
}
