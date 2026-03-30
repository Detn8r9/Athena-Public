//! LAN distributed inference: registry, session inject queue, chunking, master/worker jobs.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::inference::is_openai_compatible_backend;
use crate::lan_identity::lan_local_node_id;
use crate::lan_packets::{LanPacket, LAN_PROTOCOL_VERSION};
use crate::ollama::{self, is_ollama_running, ChatMessage, ModelInfo, ModelOptions};

pub const STREAM_CHUNK_EVENT: &str = "athena:stream-chunk";
pub const STREAM_DONE_EVENT: &str = "athena:stream-done";
pub const STREAM_ERROR_EVENT: &str = "athena:stream-error";

pub fn cluster_min_total_chars() -> usize {
    std::env::var("ATHENA_CLUSTER_MIN_CHARS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12000)
}

pub fn cluster_max_chunk_chars() -> usize {
    std::env::var("ATHENA_CLUSTER_CHUNK_CHARS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6000)
}

/// Outbound packets queued for a single peer connection (wakes duplex writer).
pub struct InjectQueue {
    inner: Mutex<VecDeque<LanPacket>>,
    notify: tokio::sync::Notify,
}

impl InjectQueue {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            notify: tokio::sync::Notify::new(),
        }
    }

    pub fn push(&self, p: LanPacket) {
        self.inner.lock().unwrap().push_back(p);
        self.notify.notify_one();
    }

    pub async fn pop(&self) -> LanPacket {
        loop {
            if let Some(p) = self.inner.lock().unwrap().pop_front() {
                return p;
            }
            self.notify.notified().await;
        }
    }
}

pub struct SessionHub {
    inner: Mutex<HashMap<u32, std::sync::Weak<InjectQueue>>>,
}

impl SessionHub {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, node_id: u32, q: &std::sync::Arc<InjectQueue>) {
        self.inner
            .lock()
            .unwrap()
            .insert(node_id, std::sync::Arc::downgrade(q));
    }

    pub fn unregister(&self, node_id: u32) {
        self.inner.lock().unwrap().remove(&node_id);
    }

    pub fn inject(&self, node_id: u32, p: LanPacket) -> Result<(), String> {
        let g = self.inner.lock().unwrap();
        let Some(w) = g.get(&node_id) else {
            return Err(format!("no active LAN session for node {node_id}"));
        };
        let Some(q) = w.upgrade() else {
            return Err(format!("stale LAN session for node {node_id}"));
        };
        q.push(p);
        Ok(())
    }
}

type ChunkReplySender = std::sync::mpsc::SyncSender<Result<String, String>>;

pub struct JobPending {
    inner: Mutex<HashMap<(Uuid, u32), ChunkReplySender>>,
}

impl JobPending {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(
        &self,
        job: Uuid,
        chunk: u32,
    ) -> std::sync::mpsc::Receiver<Result<String, String>> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.inner.lock().unwrap().insert((job, chunk), tx);
        rx
    }

    pub fn complete(&self, job: Uuid, chunk: u32, result: Result<String, String>) {
        if let Some(tx) = self.inner.lock().unwrap().remove(&(job, chunk)) {
            let _ = tx.send(result);
        }
    }
}

#[derive(Clone, Serialize)]
pub struct LanClusterPeerView {
    pub node_id: u32,
    pub ram_gb: u64,
    pub cpu_cores: usize,
    pub cpu_tier: String,
    pub ollama_models: Vec<String>,
    pub ollama_ok: bool,
    pub updated_at_ms: u64,
}

#[derive(Clone)]
struct PeerRecord {
    caps: LanClusterPeerView,
}

pub struct ClusterGlobals {
    pub hub: SessionHub,
    pub pending: JobPending,
    registry: RwLock<HashMap<u32, PeerRecord>>,
    rr: AtomicUsize,
}

impl ClusterGlobals {
    fn new() -> Self {
        Self {
            hub: SessionHub::new(),
            pending: JobPending::new(),
            registry: RwLock::new(HashMap::new()),
            rr: AtomicUsize::new(0),
        }
    }
}

pub static CLUSTER: LazyLock<ClusterGlobals> = LazyLock::new(ClusterGlobals::new);

pub fn complete_job_result(job: Uuid, chunk: u32, result: Result<String, String>) {
    CLUSTER.pending.complete(job, chunk, result);
}

pub fn merge_capabilities_from_packet(pkt: &LanPacket) {
    let LanPacket::CapabilitiesAnnounce {
        node_id,
        ram_gb,
        cpu_cores,
        cpu_tier,
        ollama_models,
        ollama_ok,
        updated_at_ms,
        ..
    } = pkt
    else {
        return;
    };
    let view = LanClusterPeerView {
        node_id: *node_id,
        ram_gb: *ram_gb,
        cpu_cores: *cpu_cores,
        cpu_tier: cpu_tier.clone(),
        ollama_models: ollama_models.clone(),
        ollama_ok: *ollama_ok,
        updated_at_ms: *updated_at_ms,
    };
    let mut g = CLUSTER.registry.write().unwrap();
    g.insert(*node_id, PeerRecord { caps: view });
}

pub fn list_cluster_peers_for_ui() -> Vec<LanClusterPeerView> {
    CLUSTER
        .registry
        .read()
        .unwrap()
        .values()
        .map(|r| r.caps.clone())
        .filter(|p| p.node_id != lan_local_node_id())
        .collect()
}

fn workers_with_model(model: &str) -> Vec<u32> {
    let me = lan_local_node_id();
    let g = CLUSTER.registry.read().unwrap();
    let mut out: Vec<u32> = g
        .values()
        .filter(|r| {
            r.caps.node_id != me
                && r.caps.ollama_ok
                && r.caps
                    .ollama_models
                    .iter()
                    .any(|m| m == model || m.trim() == model.trim())
        })
        .map(|r| r.caps.node_id)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn pick_worker_round_robin(candidates: &[u32]) -> Option<u32> {
    if candidates.is_empty() {
        return None;
    }
    let i = CLUSTER.rr.fetch_add(1, Ordering::Relaxed);
    Some(candidates[i % candidates.len()])
}

pub fn build_capabilities_packet() -> Result<LanPacket, String> {
    let specs = ollama::get_system_specs();
    let models = if is_ollama_running() {
        ollama::get_available_models().unwrap_or_default()
    } else {
        Vec::new()
    };
    let names: Vec<String> = models.into_iter().map(|m| m.name).collect();
    Ok(LanPacket::CapabilitiesAnnounce {
        node_id: lan_local_node_id(),
        protocol_version: LAN_PROTOCOL_VERSION,
        ram_gb: specs.ram_gb,
        cpu_cores: specs.cpu_cores,
        cpu_tier: specs.recommended_tier,
        gpu_vram_mb: None,
        ollama_models: names,
        ollama_ok: is_ollama_running(),
        updated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    })
}

/// Split only on paragraph boundaries (`\n\n`), then newlines, then sentence ends; last resort word boundary.
pub fn split_prompt_for_cluster(text: &str, max_chars: usize) -> Vec<String> {
    let t = text.replace("\r\n", "\n").replace('\r', "\n");
    if t.is_empty() {
        return vec![String::new()];
    }
    if t.len() <= max_chars {
        return vec![t];
    }
    let paras: Vec<&str> = t.split("\n\n").collect();
    let mut units: Vec<String> = Vec::new();
    for p in paras {
        let p = p.trim_end_matches('\n');
        if p.is_empty() {
            continue;
        }
        if p.len() <= max_chars {
            units.push(p.to_string());
        } else {
            units.extend(split_by_newlines_then_sentences(p, max_chars));
        }
    }
    if units.is_empty() {
        return split_by_newlines_then_sentences(&t, max_chars);
    }
    merge_units_to_limit(units, max_chars)
}

fn merge_units_to_limit(units: Vec<String>, max_chars: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for u in units {
        if u.len() > max_chars {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.extend(split_by_newlines_then_sentences(&u, max_chars));
            continue;
        }
        if cur.is_empty() {
            cur.push_str(&u);
            continue;
        }
        if cur.len() + 2 + u.len() <= max_chars {
            cur.push_str("\n\n");
            cur.push_str(&u);
        } else {
            out.push(std::mem::take(&mut cur));
            cur.push_str(&u);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn split_by_newlines_then_sentences(s: &str, max_chars: usize) -> Vec<String> {
    if s.len() <= max_chars {
        return vec![s.to_string()];
    }
    let lines: Vec<&str> = s.split('\n').collect();
    let mut pieces: Vec<String> = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.len() <= max_chars {
            pieces.push(line.to_string());
        } else {
            pieces.extend(split_sentences(line, max_chars));
        }
    }
    merge_units_to_limit(pieces, max_chars)
}

fn split_sentences(s: &str, max_chars: usize) -> Vec<String> {
    if s.len() <= max_chars {
        return vec![s.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        let ch = s[i..].chars().next().unwrap();
        let cl = ch.len_utf8();
        if (ch == '.' || ch == '?' || ch == '!') && i + cl < s.len() && s[i + cl..].starts_with(' ')
        {
            let end = i + cl;
            let piece = s[start..end].trim();
            if !piece.is_empty() {
                if piece.len() <= max_chars {
                    out.push(piece.to_string());
                } else {
                    out.extend(split_at_word_boundary(piece, max_chars));
                }
            }
            start = end + 1;
            while start < s.len() && s.as_bytes()[start] == b' ' {
                start += 1;
            }
            i = start;
            continue;
        }
        i += cl;
    }
    if start < s.len() {
        let piece = s[start..].trim();
        if !piece.is_empty() {
            if piece.len() <= max_chars {
                out.push(piece.to_string());
            } else {
                out.extend(split_at_word_boundary(piece, max_chars));
            }
        }
    }
    if out.is_empty() {
        split_at_word_boundary(s, max_chars)
    } else {
        out
    }
}

fn split_at_word_boundary(s: &str, max_chars: usize) -> Vec<String> {
    if s.len() <= max_chars {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        if rest.len() <= max_chars {
            out.push(rest.to_string());
            break;
        }
        let slice = &rest[..max_chars];
        if let Some(pos) = slice.rfind(' ') {
            let (a, b) = rest.split_at(pos);
            let a = a.trim();
            if !a.is_empty() {
                out.push(a.to_string());
            }
            rest = b.trim_start();
        } else {
            out.push(rest[..max_chars].to_string());
            rest = &rest[max_chars..];
        }
    }
    out
}

fn messages_concat_for_chunking(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn should_attempt_cluster(
    config: &AppConfig,
    ollama_models: &[ModelInfo],
    messages: &[ChatMessage],
    model: &str,
) -> bool {
    if is_openai_compatible_backend(config) {
        return false;
    }
    if !is_ollama_running() || ollama_models.is_empty() {
        return false;
    }
    if !cluster_eligible_now(model) {
        return false;
    }
    let total: usize = messages.iter().map(|m| m.content.len()).sum();
    if total < cluster_min_total_chars() {
        return false;
    }
    true
}

pub fn cluster_eligible_now(model: &str) -> bool {
    !workers_with_model(model).is_empty()
}

pub fn execute_worker_ai_job_blocking(offer: &LanPacket) -> LanPacket {
    let LanPacket::AiJobOffer {
        job_id,
        model,
        chunk_index,
        chunk_total,
        chunk_text,
        system_hint,
        temperature,
        num_ctx,
    } = offer
    else {
        return LanPacket::AiJobResult {
            job_id: String::new(),
            chunk_index: 0,
            ok: false,
            partial_text: String::new(),
            error: Some("not a job offer".to_string()),
        };
    };
    if !is_ollama_running() {
        return LanPacket::AiJobResult {
            job_id: job_id.clone(),
            chunk_index: *chunk_index,
            ok: false,
            partial_text: String::new(),
            error: Some("ollama not running".to_string()),
        };
    }
    let models = ollama::get_available_models().unwrap_or_default();
    if !models.iter().any(|m| m.name == *model) {
        return LanPacket::AiJobResult {
            job_id: job_id.clone(),
            chunk_index: *chunk_index,
            ok: false,
            partial_text: String::new(),
            error: Some(format!("model {} not available", model)),
        };
    }
    let default_hint = format!(
        "You are worker part {} of {}. Answer using ONLY the user excerpt below. Be concise and factual.",
        chunk_index.saturating_add(1),
        (*chunk_total).max(1u32)
    );
    let sys = system_hint
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(default_hint);
    let msgs = vec![
        ChatMessage {
            role: "system".to_string(),
            content: sys,
        },
        ChatMessage {
            role: "user".to_string(),
            content: chunk_text.clone(),
        },
    ];
    let opts = ModelOptions {
        temperature: *temperature,
        num_ctx: *num_ctx,
    };
    match ollama::chat(model, msgs, Some(opts)) {
        Ok(text) => LanPacket::AiJobResult {
            job_id: job_id.clone(),
            chunk_index: *chunk_index,
            ok: true,
            partial_text: text,
            error: None,
        },
        Err(e) => LanPacket::AiJobResult {
            job_id: job_id.clone(),
            chunk_index: *chunk_index,
            ok: false,
            partial_text: String::new(),
            error: Some(e),
        },
    }
}

pub async fn run_cluster_stream(
    app: AppHandle,
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<ModelOptions>,
) -> Result<(), String> {
    let max_chunk = cluster_max_chunk_chars();
    let combined = messages_concat_for_chunking(&messages);
    let chunks = split_prompt_for_cluster(&combined, max_chunk);
    if chunks.is_empty() {
        return Err("cluster: no text to process".to_string());
    }
    let workers = workers_with_model(&model);
    if workers.is_empty() {
        return Err("cluster: no peers with this model".to_string());
    }
    let job = Uuid::new_v4();
    let chunk_total = chunks.len() as u32;
    let temp = options.map(|o| o.temperature).unwrap_or(0.7);
    let nctx = options.map(|o| o.num_ctx).unwrap_or(4096);
    let mut partials: Vec<(u32, String)> = Vec::new();

    for (i, chunk_text) in chunks.iter().enumerate() {
        let idx = i as u32;
        let worker =
            pick_worker_round_robin(&workers).ok_or_else(|| "cluster: no worker".to_string())?;
        let rx = CLUSTER.pending.register(job, idx);
        let offer = LanPacket::AiJobOffer {
            job_id: job.to_string(),
            model: model.clone(),
            chunk_index: idx,
            chunk_total,
            chunk_text: chunk_text.clone(),
            system_hint: None,
            temperature: temp,
            num_ctx: nctx,
        };
        CLUSTER.hub.inject(worker, offer)?;
        let recv_res =
            tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(120)))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|_| "cluster: worker timeout".to_string())?;
        match recv_res {
            Ok(s) => partials.push((idx, s)),
            Err(e) => return Err(e),
        }
    }

    partials.sort_by_key(|p| p.0);
    let drafts: String = partials
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let ctx_trim: String = combined.chars().take(12000).collect();
    let synth_system = "You integrate partial worker analyses into one coherent answer for the user. Do not mention workers, steps, or drafts. Resolve contradictions conservatively. Stay factual.".to_string();
    let synth_user = format!(
        "User-visible conversation (truncated if long):\n{}\n\nHidden worker drafts (rough, may overlap):\n{}\n\nWrite the single final reply the user should see.",
        ctx_trim, drafts
    );
    let synth_messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: synth_system,
        },
        ChatMessage {
            role: "user".to_string(),
            content: synth_user,
        },
    ];

    let app_emit = app.clone();
    let app_err = app.clone();
    let model_s = model.clone();
    let synth_opts = options;
    let stream_res = tokio::task::spawn_blocking(move || {
        let mut on_chunk = |c: &str| {
            let _ = app_emit.emit(STREAM_CHUNK_EVENT, c);
        };
        let on_done = || {
            let _ = app_emit.emit(STREAM_DONE_EVENT, ());
        };
        ollama::chat_stream(&model_s, synth_messages, &mut on_chunk, on_done, synth_opts)
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(e) = stream_res {
        let _ = app_err.emit(STREAM_ERROR_EVENT, e);
    }
    Ok(())
}
