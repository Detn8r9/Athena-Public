//! Persistent JSON snapshot of deployment download queue state.
//! Stored under the portable / AthenaData root at `.athena/deployment_queue_state.json`
//! so jobs survive process restarts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::mpsc;

/// Relative path from app / AthenaData root.
pub const STATE_FILE_RELATIVE: &str = ".athena/deployment_queue_state.json";

pub const STATE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentQueueJobRecord {
    pub job_id: String,
    pub source_url: String,
    pub display_name: String,
    /// Opaque lifecycle label (e.g. queued, running, completed, failed, paused).
    pub state: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    /// Why the last attempt failed or is being retried, in plain words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The original download request, kept so an unfinished file can be retried later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentQueuePersistence {
    pub format_version: u32,
    #[serde(default)]
    pub queue_active: bool,
    #[serde(default)]
    pub cancel_requested: bool,
    pub jobs: Vec<DeploymentQueueJobRecord>,
    /// Every file from the most recent run with how it ended (completed, failed, paused,
    /// interrupted), so the hub can show what still needs doing and offer a retry.
    #[serde(default)]
    pub last_session: Vec<DeploymentQueueJobRecord>,
    /// Unix seconds when the most recent run ended.
    #[serde(default)]
    pub last_session_ended_at: Option<i64>,
}

impl Default for DeploymentQueuePersistence {
    fn default() -> Self {
        Self {
            format_version: STATE_FORMAT_VERSION,
            queue_active: false,
            cancel_requested: false,
            jobs: Vec::new(),
            last_session: Vec::new(),
            last_session_ended_at: None,
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Moves the current run's rows into `last_session`. Rows that never finished are marked so the
/// hub can say what happened to them.
fn archive_jobs(p: &mut DeploymentQueuePersistence, unfinished_state: &str) {
    if p.jobs.is_empty() {
        return;
    }
    let mut rows = std::mem::take(&mut p.jobs);
    for r in &mut rows {
        if matches!(r.state.as_str(), "queued" | "running" | "retrying") {
            r.state = unfinished_state.to_string();
        }
    }
    p.last_session = rows;
    p.last_session_ended_at = Some(now_secs());
}

#[inline]
pub fn queue_state_path(root: &Path) -> PathBuf {
    root.join(STATE_FILE_RELATIVE)
}

pub fn load_queue_state(path: &Path) -> Result<DeploymentQueuePersistence, String> {
    if !path.is_file() {
        return Ok(DeploymentQueuePersistence::default());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {}", path.display(), e))
}

/// Atomic replace: write temp next to final, flush + sync, rename.
pub async fn save_queue_state(path: &Path, state: &DeploymentQueuePersistence) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    // Each write gets its own temp file. The background persister and an explicit `flush()` can
    // run at the same time; with one shared temp name, the second writer truncated the first
    // writer's file before it was renamed into place, leaving empty or missing state on disk.
    static WRITE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = WRITE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}.{seq}.tmp", std::process::id()));
    let data = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    use tokio::io::AsyncWriteExt;
    f.write_all(&data).await.map_err(|e| e.to_string())?;
    f.flush().await.map_err(|e| e.to_string())?;
    f.sync_all().await.map_err(|e| e.to_string())?;
    drop(f);
    if let Err(e) = tokio::fs::rename(&tmp, path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e.to_string());
    }
    Ok(())
}

/// In-memory queue snapshot with durable backing store on disk.
pub struct DeploymentQueueStateManager {
    path: PathBuf,
    inner: std::sync::Arc<Mutex<DeploymentQueuePersistence>>,
    persist_tx: mpsc::UnboundedSender<PersistMsg>,
}

#[derive(Debug, Clone, Copy)]
enum PersistMsg {
    Dirty,
    Flush,
}

impl DeploymentQueueStateManager {
    #[allow(dead_code)] // tests / alternate entry points
    pub fn open(root: &Path) -> Result<Self, String> {
        let path = queue_state_path(root);
        let initial = load_queue_state(&path)?;
        Ok(Self::new_with_state(path, initial))
    }

    /// Clears `queue_active` if it was left set (crash / force-kill) so a new session can start.
    pub fn open_clearing_stale_session(root: &Path) -> Result<Self, String> {
        let path = queue_state_path(root);
        let mut initial = load_queue_state(&path)?;
        if initial.queue_active {
            eprintln!(
                "[athena] deployment queue: clearing stale queueActive from interrupted session"
            );
            initial.queue_active = false;
            initial.cancel_requested = false;
            archive_jobs(&mut initial, "interrupted");
            tauri::async_runtime::block_on(async { save_queue_state(&path, &initial).await })?;
        } else if !initial.jobs.is_empty() {
            // Idle snapshots should not retain completed/cancelled job rows — they inflate hub totals ("ghost GiB").
            eprintln!(
                "[athena] deployment queue: clearing {} stale job row(s) from idle snapshot",
                initial.jobs.len()
            );
            archive_jobs(&mut initial, "interrupted");
            tauri::async_runtime::block_on(async { save_queue_state(&path, &initial).await })?;
        }
        Ok(Self::new_with_state(path, initial))
    }

    fn new_with_state(path: PathBuf, initial: DeploymentQueuePersistence) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<PersistMsg>();
        let inner = std::sync::Arc::new(Mutex::new(initial));
        let inner_for_worker = std::sync::Arc::clone(&inner);

        // Debounced persistence worker:
        // - Dirty => schedule write at now + 1s (coalesce)
        // - Flush => write ASAP
        let path_clone = path.clone();
        tauri::async_runtime::spawn(async move {
            let mut deadline: Option<tokio::time::Instant> = None;
            loop {
                if let Some(d) = deadline {
                    tokio::select! {
                        msg = rx.recv() => {
                            let Some(msg) = msg else { break; };
                            match msg {
                                PersistMsg::Dirty => deadline = Some(tokio::time::Instant::now() + Duration::from_millis(1000)),
                                PersistMsg::Flush => deadline = Some(tokio::time::Instant::now()),
                            }
                        }
                        _ = tokio::time::sleep_until(d) => {
                            // snapshot + persist
                            let state = inner_for_worker.lock().ok().map(|g| g.clone());
                            if let Some(state) = state {
                                let _ = save_queue_state(&path_clone, &state).await;
                            }
                            deadline = None;
                        }
                    }
                } else {
                    let Some(msg) = rx.recv().await else { break; };
                    deadline = Some(match msg {
                        PersistMsg::Dirty => tokio::time::Instant::now() + Duration::from_millis(1000),
                        PersistMsg::Flush => tokio::time::Instant::now(),
                    });
                }
            }
        });

        Self {
            path,
            inner,
            persist_tx: tx,
        }
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn snapshot(&self) -> Result<DeploymentQueuePersistence, String> {
        let g = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(g.clone())
    }

    /// Persists the current snapshot immediately (async-friendly).
    pub async fn flush(&self) -> Result<(), String> {
        let snap = self.snapshot()?;
        save_queue_state(&self.path, &snap).await
    }

    // NOTE: No `flush_blocking()` here. Blocking on the async runtime from runtime threads
    // can panic ("Cannot start a runtime from within a runtime"). Prefer scheduling a flush
    // via `update_and_persist` / `end_session`, or call `flush().await` from async contexts.

    #[allow(dead_code)]
    pub fn replace_and_persist(&self, state: DeploymentQueuePersistence) -> Result<(), String> {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        *g = state;
        let _ = self.persist_tx.send(PersistMsg::Flush);
        Ok(())
    }

    pub fn update_in_memory<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut DeploymentQueuePersistence),
    {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        f(&mut *g);
        let _ = self.persist_tx.send(PersistMsg::Dirty);
        Ok(())
    }

    pub fn update_and_persist<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut DeploymentQueuePersistence),
    {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        f(&mut *g);
        let _ = self.persist_tx.send(PersistMsg::Flush);
        Ok(())
    }

    pub fn try_begin_session(&self, jobs: Vec<DeploymentQueueJobRecord>) -> Result<(), String> {
        let mut g = self.inner.lock().map_err(|e| e.to_string())?;
        if g.queue_active {
            return Err("A deployment download is already in progress.".to_string());
        }
        g.queue_active = true;
        g.cancel_requested = false;
        g.jobs = jobs;
        let _ = self.persist_tx.send(PersistMsg::Flush);
        Ok(())
    }

    pub fn end_session(&self) -> Result<(), String> {
        self.update_and_persist(|p| {
            p.queue_active = false;
            p.cancel_requested = false;
            archive_jobs(p, "not_started");
        })
    }

    /// Forgets the most recent run's results.
    pub fn clear_last_session(&self) -> Result<(), String> {
        self.update_and_persist(|p| {
            p.last_session.clear();
            p.last_session_ended_at = None;
        })
    }

    pub fn set_cancel_requested(&self, v: bool) -> Result<(), String> {
        self.update_and_persist(|p| p.cancel_requested = v)
    }

    pub fn is_cancel_requested(&self) -> Result<bool, String> {
        let g = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(g.cancel_requested)
    }

    pub fn is_queue_active(&self) -> Result<bool, String> {
        let g = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(g.queue_active)
    }

    pub fn touch_job_by_display_name(
        &self,
        display_name: &str,
        f: impl FnOnce(&mut DeploymentQueueJobRecord),
    ) -> Result<(), String> {
        self.update_in_memory(|p| {
            if let Some(j) = p.jobs.iter_mut().find(|j| j.display_name == display_name) {
                f(j);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_yields_default() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("nope.json");
        let s = load_queue_state(&p).unwrap();
        assert_eq!(s, DeploymentQueuePersistence::default());
    }

    #[test]
    fn round_trip_save_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut state = DeploymentQueuePersistence::default();
        state.jobs.push(DeploymentQueueJobRecord {
            job_id: "a1".into(),
            source_url: "https://example.com/f".into(),
            display_name: "f.bin".into(),
            state: "queued".into(),
            bytes_downloaded: 0,
            total_bytes: Some(1024),
            message: None,
            job: None,
        });
        tauri::async_runtime::block_on(async { save_queue_state(&path, &state).await }).unwrap();
        let loaded = load_queue_state(&path).unwrap();
        assert_eq!(loaded, state);
    }

    #[test]
    fn open_clearing_stale_resets_queue_active() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = queue_state_path(root);
        let mut s = DeploymentQueuePersistence::default();
        s.queue_active = true;
        s.cancel_requested = true;
        tauri::async_runtime::block_on(async { save_queue_state(&path, &s).await }).unwrap();
        let m = DeploymentQueueStateManager::open_clearing_stale_session(root).unwrap();
        let snap = m.snapshot().unwrap();
        assert!(!snap.queue_active);
        assert!(!snap.cancel_requested);
    }

    #[test]
    fn ending_a_run_keeps_its_results_for_retry() {
        let dir = tempdir().unwrap();
        let m = DeploymentQueueStateManager::open(dir.path()).unwrap();
        let row = |name: &str, state: &str| DeploymentQueueJobRecord {
            job_id: name.into(),
            source_url: String::new(),
            display_name: name.into(),
            state: state.into(),
            bytes_downloaded: 0,
            total_bytes: None,
            message: None,
            job: None,
        };
        m.try_begin_session(vec![row("a", "completed"), row("b", "failed"), row("c", "queued")]).unwrap();
        m.end_session().unwrap();
        let snap = m.snapshot().unwrap();
        assert!(snap.jobs.is_empty(), "hub totals only count the running queue");
        let states: Vec<_> = snap.last_session.iter().map(|r| r.state.as_str()).collect();
        assert_eq!(states, ["completed", "failed", "not_started"]);
        assert!(snap.last_session_ended_at.is_some());
        m.clear_last_session().unwrap();
        assert!(m.snapshot().unwrap().last_session.is_empty());
    }

    #[test]
    fn manager_persists_across_open() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        {
            let m = DeploymentQueueStateManager::open(root).unwrap();
            m.update_and_persist(|s| {
                s.jobs.push(DeploymentQueueJobRecord {
                    job_id: "j".into(),
                    source_url: "magnet:?xt=urn:btih:abc".into(),
                    display_name: "t".into(),
                    state: "running".into(),
                    bytes_downloaded: 50,
                    total_bytes: None,
                    message: None,
                    job: None,
                });
            })
            .unwrap();
            tauri::async_runtime::block_on(async { m.flush().await }).unwrap();
        }
        let m2 = DeploymentQueueStateManager::open(root).unwrap();
        let snap = m2.snapshot().unwrap();
        assert_eq!(snap.jobs.len(), 1);
        assert_eq!(snap.jobs[0].job_id, "j");
    }
}
