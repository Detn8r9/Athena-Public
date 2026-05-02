//! LAN P2P replication: manifest diff vs local SQLite, streaming downloads from peer `/mesh/download`,
//! integrity checks on `.part` files, bounded concurrency, post-replication ingest, and sync history events.

use std::fs::Metadata;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::db::{Database, MeshManifestRow};
use crate::mesh_sync_api::mesh_manifest_url_is_lan_safe;

static POWER_SAVE_MODE: std::sync::OnceLock<Arc<std::sync::atomic::AtomicBool>> = std::sync::OnceLock::new();

pub fn set_power_save_flag(flag: Arc<std::sync::atomic::AtomicBool>) {
    let _ = POWER_SAVE_MODE.set(flag);
}

fn power_save_active() -> bool {
    POWER_SAVE_MODE
        .get()
        .map(|f| f.load(Ordering::Relaxed))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMeshManifest {
    pub node_id: u32,
    pub generated_at_rfc3339: String,
    pub entry_count: usize,
    pub entries: Vec<MeshManifestRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2pMissingEntry {
    pub path: String,
    pub file_type: String,
    pub remote_size_bytes: u64,
    pub local_size_bytes: Option<u64>,
    /// Peer `indexed_files.modified_at` (unix seconds) when present; used for latest-wins.
    #[serde(default)]
    pub remote_modified_at: Option<i64>,
    pub category: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2pManifestDiffResult {
    pub missing: Vec<P2pMissingEntry>,
    pub remote_node_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2pReplicationProgressPayload {
    pub phase: String,
    pub files_done: u32,
    pub files_total: u32,
    pub current_path: String,
    pub bytes_written_this_file: u64,
    pub total_bytes_this_file: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2pSyncHistoryEntry {
    pub status: String,
    pub path: String,
    pub peer_base_url: String,
    pub detail: String,
    pub unix_ms: u64,
}

pub const MESH_P2P_REPLICATION_PROGRESS_EVENT: &str = "athena:mesh-p2p-replication-progress";
pub const MESH_P2P_SYNC_HISTORY_EVENT: &str = "athena:mesh-p2p-sync-history";

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

fn emit_sync_history(app: &AppHandle, peer_base_url: &str, status: &str, path: &str, detail: &str) {
    let _ = app.emit(
        MESH_P2P_SYNC_HISTORY_EVENT,
        &P2pSyncHistoryEntry {
            status: status.to_string(),
            path: path.to_string(),
            peer_base_url: peer_base_url.to_string(),
            detail: detail.to_string(),
            unix_ms: now_unix_ms(),
        },
    );
}

fn classify_entry(path: &str, file_type: &str) -> &'static str {
    let p = path.to_ascii_lowercase();
    let t = file_type.to_ascii_lowercase();
    if p.ends_with(".zim") || t.contains("zim") {
        return "zim";
    }
    if p.ends_with(".mbtiles")
        || p.ends_with(".pmtiles")
        || t.contains("mbtiles")
        || t.contains("pmtiles")
        || t.contains("map")
    {
        return "map";
    }
    if t.contains("pdf")
        || t.contains("text")
        || t.contains("markdown")
        || t.contains("chunk")
        || p.ends_with(".pdf")
        || p.ends_with(".md")
        || p.ends_with(".txt")
    {
        return "document";
    }
    "other"
}

fn should_post_ingest(job: &P2pMissingEntry) -> bool {
    let p = job.path.to_ascii_lowercase();
    let t = job.file_type.to_ascii_lowercase();
    p.ends_with(".zim")
        || t.contains("zim")
        || p.ends_with(".pdf")
        || t.contains("pdf")
}

/// Compare remote manifest JSON to local `indexed_files` / chunk coverage (heavy work in `spawn_blocking`).
pub async fn compute_manifest_diff(
    db: Arc<std::sync::Mutex<Database>>,
    remote_json: &str,
) -> Result<P2pManifestDiffResult, String> {
    let remote: RemoteMeshManifest =
        serde_json::from_str(remote_json).map_err(|e| format!("manifest JSON: {e}"))?;
    let entries = remote.entries.clone();
    let remote_node = remote.node_id;
    tokio::task::spawn_blocking(move || {
        let db = db.lock().map_err(|e| e.to_string())?;
        let mut missing = Vec::new();
        for row in entries {
            let cat = classify_entry(&row.path, &row.file_type).to_string();
            let local_size = db.indexed_file_size_by_path(&row.path)?;
            let local_mtime = db.indexed_file_mtime_by_path(&row.path)?;
            let remote_mtime = row.modified_at;

            let mut reason = String::new();
            let need_file = match local_size {
                None => {
                    reason = "missing_index_row".to_string();
                    true
                }
                Some(sz) if sz < row.size_bytes => {
                    reason = format!("size_mismatch_local_{sz}");
                    true
                }
                Some(_) => false,
            };

            // Latest wins: if the local index is strictly newer than the peer row, do not pull the file.
            if need_file {
                if let (Some(lm), Some(rm)) = (local_mtime, remote_mtime) {
                    if lm > rm {
                        continue;
                    }
                }
            }

            let mut need_vectors = false;
            if !need_file && (cat == "document" || cat == "other") {
                let n = db.chunk_embedding_count_for_file_path(&row.path)?;
                if n == 0 && row.size_bytes > 0 {
                    need_vectors = true;
                    if reason.is_empty() {
                        reason = "missing_vector_chunks".to_string();
                    }
                }
            }

            if need_file {
                missing.push(P2pMissingEntry {
                    path: row.path.clone(),
                    file_type: row.file_type.clone(),
                    remote_size_bytes: row.size_bytes,
                    local_size_bytes: local_size,
                    remote_modified_at: remote_mtime,
                    category: cat.clone(),
                    reason: if reason.is_empty() {
                        "missing_file".to_string()
                    } else {
                        reason
                    },
                });
            } else if need_vectors {
                missing.push(P2pMissingEntry {
                    path: row.path.clone(),
                    file_type: row.file_type.clone(),
                    remote_size_bytes: row.size_bytes,
                    local_size_bytes: local_size,
                    remote_modified_at: remote_mtime,
                    category: "vector".to_string(),
                    reason: "missing_vector_chunks".to_string(),
                });
            }
        }
        Ok(P2pManifestDiffResult {
            missing,
            remote_node_id: remote_node,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn build_download_url(peer_base: &str, rel_path: &str) -> Result<String, String> {
    let base = peer_base.trim_end_matches('/');
    let enc = urlencoding::encode(rel_path);
    Ok(format!("{base}/mesh/download?p={enc}"))
}

/// Skip P2P download when an on-disk file already matches the manifest and is not strictly older than the peer.
/// Uses size when mtimes are missing or equal so replication does not loop on identical payloads.
fn skip_replicate_reason_local_vs_manifest(meta: &Metadata, job: &P2pMissingEntry) -> Option<&'static str> {
    if !meta.is_file() {
        return None;
    }
    if meta.len() != job.remote_size_bytes {
        return None;
    }

    let local_mtime_secs = meta
        .modified()
        .ok()
        .and_then(|st| st.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    match (local_mtime_secs, job.remote_modified_at) {
        (Some(lm), Some(rm)) if lm > rm => Some("local file newer than peer manifest (latest wins)."),
        (Some(lm), Some(rm)) if lm < rm => None,
        (Some(lm), Some(rm)) if lm == rm => Some("local file matches peer size and modified time (latest wins)."),
        _ => Some("local file size matches manifest; mtime tie or unavailable — treating as current."),
    }
}

fn emit_throttled(
    app: &AppHandle,
    last_emit: &mut Instant,
    payload: P2pReplicationProgressPayload,
    min_interval: Duration,
) {
    if last_emit.elapsed() >= min_interval {
        let _ = app.emit(MESH_P2P_REPLICATION_PROGRESS_EVENT, &payload);
        *last_emit = Instant::now();
    }
}

async fn replicate_one_file(
    app: AppHandle,
    db: Arc<std::sync::Mutex<Database>>,
    client: reqwest::Client,
    peer_base_url: String,
    athena_data_root: PathBuf,
    job: P2pMissingEntry,
    files_total: u32,
    files_completed: Arc<AtomicU32>,
) -> Result<(), String> {
    let dest = crate::mesh_sync_api::safe_path_under_athena_root(&athena_data_root, &job.path)
        .map_err(|e| format!("{}: {e}", job.path))?;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Latest wins on disk (covers rows missing from the index but file already present).
    if let Ok(meta) = tokio::fs::metadata(&dest).await {
        if let Some(detail) = skip_replicate_reason_local_vs_manifest(&meta, &job) {
            emit_sync_history(&app, &peer_base_url, "skipped", &job.path, detail);
            let done = files_completed.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit(
                MESH_P2P_REPLICATION_PROGRESS_EVENT,
                &P2pReplicationProgressPayload {
                    phase: "file_complete".to_string(),
                    files_done: done,
                    files_total,
                    current_path: job.path.clone(),
                    bytes_written_this_file: meta.len(),
                    total_bytes_this_file: job.remote_size_bytes,
                },
            );
            return Ok(());
        }
    }

    let parent = dest
        .parent()
        .ok_or_else(|| format!("{}: missing parent directory", job.path))?;
    let fname = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let part_path = parent.join(format!(".{fname}.{}.p2p-part", Uuid::new_v4()));

    let _ = tokio::fs::remove_file(&part_path).await;

    let url = build_download_url(&peer_base_url, &job.path)?;
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if !mesh_manifest_url_is_lan_safe(&parsed) {
        return Err("Peer download URL failed LAN safety check.".to_string());
    }

    let mut resp = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let total_len = resp.content_length().unwrap_or(0);
    let mut out = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| format!("create {}: {e}", part_path.display()))?;

    let mut written: u64 = 0;
    let mut last_throttle = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);

    let done_before = files_completed.load(Ordering::SeqCst);
    let _ = app.emit(
        MESH_P2P_REPLICATION_PROGRESS_EVENT,
        &P2pReplicationProgressPayload {
            phase: "file_start".to_string(),
            files_done: done_before,
            files_total,
            current_path: job.path.clone(),
            bytes_written_this_file: 0,
            total_bytes_this_file: total_len.max(job.remote_size_bytes),
        },
    );

    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
        written = written.saturating_add(chunk.len() as u64);
        emit_throttled(
            &app,
            &mut last_throttle,
            P2pReplicationProgressPayload {
                phase: "downloading".to_string(),
                files_done: done_before,
                files_total,
                current_path: job.path.clone(),
                bytes_written_this_file: written,
                total_bytes_this_file: total_len.max(job.remote_size_bytes),
            },
            Duration::from_millis(220),
        );
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);

    if written != job.remote_size_bytes {
        let _ = tokio::fs::remove_file(&part_path).await;
        let msg = format!(
            "size mismatch: wrote {written} bytes, manifest expects {} (integrity check failed).",
            job.remote_size_bytes
        );
        return Err(msg);
    }

    if dest.exists() {
        tokio::fs::remove_file(&dest).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::rename(&part_path, &dest)
        .await
        .map_err(|e| format!("finalize {}: {e}", dest.display()))?;

    let ran_ingest = should_post_ingest(&job);
    if ran_ingest {
        let db2 = Arc::clone(&db);
        let app2 = app.clone();
        let dest_pb = dest.clone();
        let root2 = athena_data_root.clone();
        tokio::task::spawn_blocking(move || {
            crate::watcher::try_process_file(&db2, &dest_pb, &root2, &app2, None);
        })
        .await
        .map_err(|e| format!("post-replication ingest task: {e}"))?;
    }

    let done = files_completed.fetch_add(1, Ordering::SeqCst) + 1;
    emit_sync_history(
        &app,
        &peer_base_url,
        "ok",
        &job.path,
        if ran_ingest {
            "replicated, size verified, ZIM/PDF ingest completed."
        } else {
            "replicated and size verified."
        },
    );
    let _ = app.emit(
        MESH_P2P_REPLICATION_PROGRESS_EVENT,
        &P2pReplicationProgressPayload {
            phase: "file_complete".to_string(),
            files_done: done,
            files_total,
            current_path: job.path.clone(),
            bytes_written_this_file: written,
            total_bytes_this_file: job.remote_size_bytes,
        },
    );

    Ok(())
}

/// Stream each missing **file** from the peer into AthenaData (max **2** concurrent downloads per run).
pub async fn replicate_missing_files(
    app: AppHandle,
    db: Arc<std::sync::Mutex<Database>>,
    peer_base_url: String,
    athena_data_root: PathBuf,
    entries: Vec<P2pMissingEntry>,
) -> Result<(), String> {
    if power_save_active() {
        return Err("Power Save Mode is active — refusing to start new P2P replication jobs.".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let client = Arc::new(client);

    let file_jobs: Vec<_> = entries
        .into_iter()
        .filter(|e| e.category != "vector")
        .collect();
    let total = file_jobs.len() as u32;
    let files_completed = Arc::new(AtomicU32::new(0));
    let peer_hist = peer_base_url.clone();
    // Cap concurrent P2P downloads per replication run (field laptop / LAN friendly).
    let download_slots = Arc::new(Semaphore::new(2));

    let _ = app.emit(
        MESH_P2P_REPLICATION_PROGRESS_EVENT,
        &P2pReplicationProgressPayload {
            phase: "starting".to_string(),
            files_done: 0,
            files_total: total,
            current_path: String::new(),
            bytes_written_this_file: 0,
            total_bytes_this_file: 0,
        },
    );

    let errors: Vec<String> = stream::iter(file_jobs)
        .map(|job| {
            let app = app.clone();
            let db = Arc::clone(&db);
            let client = Arc::clone(&client);
            let peer = peer_base_url.clone();
            let root = athena_data_root.clone();
            let fc = Arc::clone(&files_completed);
            let peer_hist = peer_hist.clone();
            let slots = Arc::clone(&download_slots);
            async move {
                if power_save_active() {
                    emit_sync_history(
                        &app,
                        &peer_hist,
                        "skipped",
                        &job.path,
                        "Power Save Mode active — skipping P2P job.",
                    );
                    let done = fc.fetch_add(1, Ordering::SeqCst) + 1;
                    let _ = app.emit(
                        MESH_P2P_REPLICATION_PROGRESS_EVENT,
                        &P2pReplicationProgressPayload {
                            phase: "file_complete".to_string(),
                            files_done: done,
                            files_total: total,
                            current_path: job.path.clone(),
                            bytes_written_this_file: 0,
                            total_bytes_this_file: job.remote_size_bytes,
                        },
                    );
                    return None;
                }
                let _permit = slots
                    .acquire_owned()
                    .await
                    .expect("P2P download semaphore");
                let path = job.path.clone();
                match replicate_one_file(app.clone(), db, (*client).clone(), peer, root, job, total, fc).await {
                    Ok(()) => None,
                    Err(e) => {
                        emit_sync_history(&app, &peer_hist, "error", &path, &e);
                        Some(e)
                    }
                }
            }
        })
        .buffer_unordered(8)
        .collect::<Vec<Option<String>>>()
        .await
        .into_iter()
        .flatten()
        .collect();

    let _ = app.emit(
        MESH_P2P_REPLICATION_PROGRESS_EVENT,
        &P2pReplicationProgressPayload {
            phase: "complete".to_string(),
            files_done: total,
            files_total: total,
            current_path: String::new(),
            bytes_written_this_file: 0,
            total_bytes_this_file: 0,
        },
    );

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Fetch manifest from `manifest_url`, diff against local DB, return missing rows.
pub async fn fetch_and_diff_manifest(
    db: Arc<std::sync::Mutex<Database>>,
    manifest_url: &str,
) -> Result<P2pManifestDiffResult, String> {
    let parsed = reqwest::Url::parse(manifest_url.trim()).map_err(|e| e.to_string())?;
    if !mesh_manifest_url_is_lan_safe(&parsed) {
        return Err("manifest_url must be a safe LAN http URL.".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let txt = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    compute_manifest_diff(db, &txt).await
}