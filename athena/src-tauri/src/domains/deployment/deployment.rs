//! Resumable deployment downloads into the Athena data directory (portable or OS app data).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use sysinfo::Disks;
use tauri::Emitter;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::deployment_manifest_cache;
use crate::deployment_queue_state::{DeploymentQueueJobRecord, DeploymentQueueStateManager};
use crate::library_ingest;
use crate::portable;
use crate::storage;
use crate::watcher;

pub const DEPLOYMENT_DOWNLOAD_PROGRESS_EVENT: &str = "deployment_download_progress";
pub const DEPLOYMENT_DOWNLOAD_ERROR_EVENT: &str = "deployment_download_error";
pub const DEPLOYMENT_DOWNLOAD_FILE_COMPLETE_EVENT: &str = "deployment_download_file_complete";
pub const DEPLOYMENT_QUEUE_COMPLETE_EVENT: &str = "deployment_queue_complete";

const PROGRESS_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(250);

fn spawn_post_download_library_extract(app: AppHandle, athena_root: PathBuf, final_path: PathBuf) {
    if !library_ingest::is_library_archive(&final_path) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        match library_ingest::extract_archive_with_progress_async(
            app.clone(),
            final_path.clone(),
            athena_root,
            "archives".into(),
        )
        .await
        {
            Ok(_) => {
                let _ = app.emit(watcher::ARCHIVE_FS_CHANGED_EVENT, serde_json::json!({}));
            }
            Err(e) => {
                eprintln!(
                    "[athena] post-download archive extract failed for {}: {e}",
                    final_path.display()
                );
            }
        }
    });
}

// --- Path helpers / Athena root ------------------------------------------------

pub fn athena_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    portable::resolved_app_data_dir(app)
}

/// Joins `rel` to `root`, rejecting `..` and absolute paths.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let t = rel.trim();
    if t.is_empty() {
        return Err("Path is empty".to_string());
    }
    let p = Path::new(t);
    if p.is_absolute() {
        return Err("Path must be relative to the Athena data folder".to_string());
    }
    for c in p.components() {
        match c {
            std::path::Component::Normal(_) => {}
            std::path::Component::ParentDir => {
                return Err("Path must not contain '..'".to_string());
            }
            std::path::Component::RootDir
            | std::path::Component::CurDir
            | std::path::Component::Prefix(_) => {
                return Err("Invalid path".to_string());
            }
        }
    }
    Ok(root.join(p))
}

fn dest_dir_from_subdir(
    athena: &Path,
    dest_subdir: Option<String>,
) -> Result<PathBuf, String> {
    let Some(s) = dest_subdir else {
        return Ok(athena.to_path_buf());
    };
    let t = s.trim();
    if t.is_empty() {
        return Ok(athena.to_path_buf());
    }
    safe_join(athena, t)
}

fn mount_point_for_path(p: &Path) -> Result<String, String> {
    let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(usize, String)> = None;
    for d in disks.list() {
        let mp = d.mount_point();
        let mp = mp.canonicalize().unwrap_or_else(|_| mp.to_path_buf());
        if p.starts_with(&mp) {
            let len = mp.as_os_str().len();
            if best.as_ref().map(|(l, _)| *l < len).unwrap_or(true) {
                best = Some((len, mp.to_string_lossy().to_string()));
            }
        }
    }
    best.map(|(_, s)| s)
        .ok_or_else(|| "Could not determine a disk for the Athena data path.".to_string())
}

// --- Disk + scan API ----------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpaceReport {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub mount_point: String,
}

#[tauri::command]
pub fn deployment_check_disk_space(app: tauri::AppHandle) -> Result<DiskSpaceReport, String> {
    let athena = athena_data_dir(&app)?;
    let s = storage::check_available_storage(
        app,
        Some(athena.to_string_lossy().to_string()),
    )?;
    let mount_point = mount_point_for_path(&athena)?;
    Ok(DiskSpaceReport {
        available_bytes: s.available_bytes,
        total_bytes: s.total_bytes,
        mount_point,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedFile {
    /// Relative to Athena data root.
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialFileStatus {
    pub path: String,
    pub downloaded_bytes: u64,
    pub expected_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileScanReport {
    pub complete: Vec<String>,
    pub partial: Vec<PartialFileStatus>,
    pub missing: Vec<String>,
    pub oversized: Vec<PartialFileStatus>,
}

#[tauri::command]
pub fn deployment_scan_files(
    app: tauri::AppHandle,
    expected: Vec<ExpectedFile>,
) -> Result<FileScanReport, String> {
    use std::fs;
    let root = athena_data_dir(&app)?;
    let mut complete: Vec<String> = Vec::new();
    let mut partial: Vec<PartialFileStatus> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut oversized: Vec<PartialFileStatus> = Vec::new();

    for e in expected {
        let p = safe_join(&root, &e.path)?;
        let rel = e.path.replace('\\', "/");
        match fs::metadata(&p) {
            Ok(m) if m.is_file() => {
                let len = m.len();
                if len == e.size_bytes {
                    complete.push(rel);
                } else if len < e.size_bytes {
                    partial.push(PartialFileStatus {
                        path: rel,
                        downloaded_bytes: len,
                        expected_bytes: e.size_bytes,
                    });
                } else {
                    oversized.push(PartialFileStatus {
                        path: rel,
                        downloaded_bytes: len,
                        expected_bytes: e.size_bytes,
                    });
                }
            }
            Ok(m) if m.is_dir() => {
                return Err(format!("{} is a directory, not a file.", p.display()));
            }
            Ok(_) => {
                return Err(format!("{} is not a file.", p.display()));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                missing.push(rel);
            }
            Err(err) => {
                return Err(err.to_string());
            }
        }
    }

    Ok(FileScanReport {
        complete,
        partial,
        missing,
        oversized,
    })
}

// --- Power (keepawake) ref-count ----------------------------------------------

struct PowerState {
    handle: Option<keepawake::KeepAwake>,
    refs: u32,
}

static POWER: OnceLock<Mutex<PowerState>> = OnceLock::new();

fn power_state() -> &'static Mutex<PowerState> {
    POWER.get_or_init(|| {
        Mutex::new(PowerState {
            handle: None,
            refs: 0,
        })
    })
}

pub fn acquire_power_assertion(reason: &str) -> Result<(), String> {
    let mut g = power_state().lock().map_err(|e| e.to_string())?;
    g.refs = g.refs.saturating_add(1);
    if g.refs == 1 {
        let h = keepawake::Builder::default()
            .display(false)
            .idle(true)
            .sleep(true)
            .reason(reason)
            .app_name("ATHENA")
            .app_reverse_domain("com.athena.micro")
            .create()
            .map_err(|e: keepawake::Error| e.to_string())?;
        g.handle = Some(h);
    }
    Ok(())
}

pub fn release_power_assertion() {
    let Ok(mut g) = power_state().lock() else {
        return;
    };
    if g.refs == 0 {
        return;
    }
    g.refs = g.refs.saturating_sub(1);
    if g.refs == 0 {
        g.handle.take();
    }
}

// --- Download queue -----------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    pub url: String,
    pub file_name: String,
    pub expected_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgressPayload {
    file_name: String,
    bytes_downloaded: u64,
    total_bytes: u64,
    bytes_per_sec: f64,
    eta_seconds: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadErrorPayload {
    file_name: String,
    message: String,
    recoverable: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadFileCompletePayload {
    file_name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueCompletePayload {
    /// True when the entire queue completed without error or pause.
    ok: bool,
}

struct SpeedMeter {
    primed: bool,
    at: Instant,
    bytes: u64,
}

impl SpeedMeter {
    fn new() -> Self {
        Self {
            primed: false,
            at: Instant::now(),
            bytes: 0,
        }
    }

    fn sample(&mut self, downloaded: u64) -> Option<f64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.at).as_secs_f64();
        if !self.primed {
            self.primed = true;
            self.at = now;
            self.bytes = downloaded;
            return None;
        }
        if elapsed < 0.08 {
            return None;
        }
        let delta = downloaded.saturating_sub(self.bytes);
        let bps = (delta as f64 / elapsed).max(0.0);
        self.at = now;
        self.bytes = downloaded;
        Some(bps)
    }
}

fn recoverable_reqwest(e: &reqwest::Error) -> bool {
    if e.is_timeout() || e.is_connect() || e.is_request() {
        return true;
    }
    if e.is_body() {
        let mut s: &dyn std::error::Error = e;
        while let Some(src) = s.source() {
            if let Some(io) = src.downcast_ref::<std::io::Error>() {
                return matches!(
                    io.kind(),
                    ErrorKind::ConnectionReset
                        | ErrorKind::BrokenPipe
                        | ErrorKind::UnexpectedEof
                        | ErrorKind::TimedOut
                        | ErrorKind::Interrupted
                );
            }
            s = src;
        }
    }
    false
}

fn recoverable_io(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::ConnectionReset
            | ErrorKind::BrokenPipe
            | ErrorKind::UnexpectedEof
            | ErrorKind::TimedOut
            | ErrorKind::Interrupted
    )
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(30))
        .user_agent("AthenaDeployment/1.0")
        .redirect(reqwest::redirect::Policy::limited(16))
        .build()
        .map_err(|e| e.to_string())
}

fn emit_progress(
    app: &AppHandle,
    name: &str,
    bytes_downloaded: u64,
    total_bytes: u64,
    speed: &mut SpeedMeter,
) {
    let bps = speed.sample(bytes_downloaded).unwrap_or(0.0);
    let eta = if bps > 0.0 {
        let rem = (total_bytes as f64) - (bytes_downloaded as f64);
        if rem > 0.0 {
            Some((rem / bps) as u64)
        } else {
            Some(0)
        }
    } else {
        None
    };
    let _ = app.emit(
        DEPLOYMENT_DOWNLOAD_PROGRESS_EVENT,
        DownloadProgressPayload {
            file_name: name.to_string(),
            bytes_downloaded,
            total_bytes,
            bytes_per_sec: bps,
            eta_seconds: eta,
        },
    );
}

enum QueueError {
    Paused,
    Recoverable(String),
    Fatal(String),
}

/// Clears `DOWNLOAD_CANCEL` and attempts one GET; maps reqwest::Error to queue outcome.
fn map_send_error(e: reqwest::Error) -> QueueError {
    if recoverable_reqwest(&e) {
        QueueError::Recoverable(e.to_string())
    } else {
        QueueError::Fatal(e.to_string())
    }
}

fn header_accept_ranges_bytes(resp: &reqwest::Response) -> bool {
    resp.headers()
        .get("accept-ranges")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase().contains("bytes"))
        .unwrap_or(false)
}

fn header_content_length(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn header_etag(resp: &reqwest::Response) -> Option<String> {
    resp.headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn header_last_modified(resp: &reqwest::Response) -> Option<String> {
    resp.headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// HEAD (or GET `Range: bytes=0-0` if HEAD is not allowed) for `Content-Length`, weak validators, and `Accept-Ranges`.
async fn deployment_network_preflight(
    client: &reqwest::Client,
    url: &str,
    manifest_expected: u64,
    partial_on_disk: u64,
    athena_root: &Path,
) -> Result<(u64, bool), QueueError> {
    let mut resp = client.head(url).send().await.map_err(map_send_error)?;
    if resp.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
        drop(resp);
        resp = client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(map_send_error)?;
    }

    let status = resp.status();
    if status.is_client_error() || status.is_server_error() {
        return Err(QueueError::Fatal(format!(
            "Preflight HTTP {} for {}",
            status, url
        )));
    }
    if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
        return Err(QueueError::Fatal(format!(
            "Preflight unexpected HTTP {} for {}",
            status, url
        )));
    }

    let accept_ranges = header_accept_ranges_bytes(&resp);
    let content_length = header_content_length(&resp);
    let etag = header_etag(&resp);
    let last_modified = header_last_modified(&resp);
    let _drain = resp.bytes().await.map_err(map_send_error)?;

    let url_key = deployment_manifest_cache::normalize_url_key(url);
    let effective_bytes = if let Some(cl) = content_length {
        if cl > 0 && cl != manifest_expected {
            deployment_manifest_cache::persist_head_size_if_changed(
                athena_root,
                &url_key,
                manifest_expected,
                cl,
                etag,
                last_modified,
            )
            .await
            .map_err(QueueError::Fatal)?;
        }
        if cl > 0 {
            cl
        } else {
            manifest_expected
        }
    } else {
        manifest_expected
    };

    if partial_on_disk > 0 && !accept_ranges {
        return Err(QueueError::Fatal(format!(
            "Server does not advertise byte-range resume (Accept-Ranges: bytes) for this URL, but {} byte(s) are already on disk for a partial download. The partial file was preserved; remove it or use a mirror that supports HTTP Range requests. URL: {}",
            partial_on_disk, url
        )));
    }

    Ok((effective_bytes, accept_ranges))
}

async fn download_one_job(
    app: &AppHandle,
    client: &reqwest::Client,
    dest_dir: &Path,
    athena_root: &Path,
    mgr: &DeploymentQueueStateManager,
    job: &DownloadJob,
) -> Result<(), QueueError> {
    if job.url.trim().is_empty() {
        return Err(QueueError::Fatal("Empty URL".to_string()));
    }
    if job.file_name.trim().is_empty() {
        return Err(QueueError::Fatal("Empty file_name".to_string()));
    }

    let final_path = safe_join(dest_dir, &job.file_name)
        .map_err(QueueError::Fatal)?;
    if let Some(p) = final_path.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| QueueError::Fatal(e.to_string()))?;
    }

    // Existing on-disk size (before network preflight).
    let mut on_disk: u64 = 0;
    if let Ok(m) = tokio::fs::metadata(&final_path).await {
        on_disk = m.len();
    }

    if on_disk > job.expected_bytes {
        on_disk = 0;
        let _ = tokio::fs::File::create(&final_path)
            .await
            .map_err(|e| QueueError::Fatal(e.to_string()))?;
    }

    let (effective_bytes, _ranges_ok) = deployment_network_preflight(
        client,
        &job.url,
        job.expected_bytes,
        on_disk,
        athena_root,
    )
    .await?;

    let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
        j.total_bytes = Some(effective_bytes);
    });

    if on_disk > effective_bytes {
        on_disk = 0;
        let _ = tokio::fs::File::create(&final_path)
            .await
            .map_err(|e| QueueError::Fatal(e.to_string()))?;
    }

    if on_disk == effective_bytes {
        let mut s = SpeedMeter::new();
        s.sample(on_disk);
        emit_progress(
            app,
            &job.file_name,
            on_disk,
            effective_bytes,
            &mut s,
        );
        let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
            j.bytes_downloaded = on_disk;
            j.state = "completed".into();
        });
        let _ = app.emit(
            DEPLOYMENT_DOWNLOAD_FILE_COMPLETE_EVENT,
            DownloadFileCompletePayload {
                file_name: job.file_name.clone(),
            },
        );
        spawn_post_download_library_extract(app.clone(), athena_root.to_path_buf(), final_path.clone());
        return Ok(());
    }

    let resume_from = on_disk;

    {
        if mgr.is_cancel_requested().unwrap_or(true) {
            return Err(QueueError::Paused);
        }

        let mut request = client.get(&job.url);
        if resume_from > 0 {
            request = request.header(
                reqwest::header::RANGE,
                format!("bytes={}-", resume_from),
            );
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => return Err(map_send_error(e)),
        };

        if mgr.is_cancel_requested().unwrap_or(true) {
            return Err(QueueError::Paused);
        }

        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            return Err(QueueError::Fatal(format!(
                "HTTP {}: {}",
                status,
                job.file_name
            )));
        }

        if !status.is_success() {
            return Err(QueueError::Fatal(format!(
                "Unexpected HTTP status {}: {}",
                status, job.file_name
            )));
        }

        // Server answered with a full-body 200 while we hold a partial — do not truncate the file.
        if status == reqwest::StatusCode::OK && resume_from > 0 {
            return Err(QueueError::Fatal(format!(
                "Resume rejected for {}: server returned HTTP 200 (full file) instead of 206 Partial Content while resuming from byte {}. The partial file on disk was not overwritten. The server or CDN is not honoring byte-range requests for this resource.",
                job.file_name, resume_from
            )));
        }

        let (mut file, start_write_offset) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            let f = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&final_path)
                .await
                .map_err(|e| QueueError::Fatal(e.to_string()))?;
            (f, resume_from)
        } else {
            // Fresh download from byte 0
            let f = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&final_path)
                .await
                .map_err(|e| QueueError::Fatal(e.to_string()))?;
            (f, 0u64)
        };

        if start_write_offset > effective_bytes {
            return Err(QueueError::Fatal("Resume offset beyond expected size".to_string()));
        }

        let mut downloaded: u64 = start_write_offset;
        let mut speed = SpeedMeter::new();
        emit_progress(
            app,
            &job.file_name,
            downloaded,
            effective_bytes,
            &mut speed,
        );
        let mut last_emit = Instant::now();
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            if mgr.is_cancel_requested().unwrap_or(true) {
                drop(file);
                return Err(QueueError::Paused);
            }
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    drop(file);
                    return Err(map_send_error(e));
                }
            };
            if let Err(e) = file.write_all(&chunk).await {
                drop(file);
                return Err(if recoverable_io(&e) {
                    QueueError::Recoverable(e.to_string())
                } else {
                    QueueError::Fatal(e.to_string())
                });
            }
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > effective_bytes {
                drop(file);
                return Err(QueueError::Fatal(format!(
                    "Download exceeded expected size for {} ({}).",
                    job.file_name, effective_bytes
                )));
            }
            if last_emit.elapsed() >= PROGRESS_EMIT_MIN_INTERVAL {
                last_emit = Instant::now();
                let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                    j.bytes_downloaded = downloaded;
                });
                emit_progress(
                    app,
                    &job.file_name,
                    downloaded,
                    effective_bytes,
                    &mut speed,
                );
            }
        }

        file.flush().await.map_err(|e| {
            if recoverable_io(&e) {
                QueueError::Recoverable(e.to_string())
            } else {
                QueueError::Fatal(e.to_string())
            }
        })?;
        drop(file);

        if downloaded != effective_bytes {
            return Err(QueueError::Fatal(format!(
                "File size {} does not match expected {} bytes: {}",
                downloaded, effective_bytes, job.file_name
            )));
        }

        emit_progress(
            app,
            &job.file_name,
            effective_bytes,
            effective_bytes,
            &mut speed,
        );
        let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
            j.bytes_downloaded = effective_bytes;
            j.state = "completed".into();
        });
        let _ = app.emit(
            DEPLOYMENT_DOWNLOAD_FILE_COMPLETE_EVENT,
            DownloadFileCompletePayload {
                file_name: job.file_name.clone(),
            },
        );
        spawn_post_download_library_extract(app.clone(), athena_root.to_path_buf(), final_path.clone());
        return Ok(());
    }
}

async fn run_deployment_queue(
    app: AppHandle,
    mgr: Arc<DeploymentQueueStateManager>,
    athena_root: PathBuf,
    dest_dir: PathBuf,
    jobs: Vec<DownloadJob>,
) {
    struct EndClear {
        mgr: Arc<DeploymentQueueStateManager>,
    }
    impl Drop for EndClear {
        fn drop(&mut self) {
            release_power_assertion();
            let _ = self.mgr.end_session();
            // Persist queue end state without relying on debounce (best-effort).
            let _ = self.mgr.flush_blocking();
        }
    }
    let _clear = EndClear {
        mgr: Arc::clone(&mgr),
    };

    let client = match build_http_client() {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit(
                DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                DownloadErrorPayload {
                    file_name: String::new(),
                    message: e,
                    recoverable: false,
                },
            );
            let _ = app.emit(
                DEPLOYMENT_QUEUE_COMPLETE_EVENT,
                QueueCompletePayload { ok: false },
            );
            return;
        }
    };

    let mut all_ok = true;
    for job in &jobs {
        let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
            j.state = "running".into();
        });
        let r = download_one_job(
            &app,
            &client,
            &dest_dir,
            &athena_root,
            mgr.as_ref(),
            job,
        )
        .await;
        match r {
            Ok(()) => continue,
            Err(QueueError::Paused) => {
                let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                    j.state = "paused".into();
                });
                let _ = app.emit(
                    DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                    DownloadErrorPayload {
                        file_name: job.file_name.clone(),
                        message: "paused".to_string(),
                        recoverable: true,
                    },
                );
                all_ok = false;
                break;
            }
            Err(QueueError::Recoverable(msg)) => {
                let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                    j.state = "interrupted".into();
                });
                let _ = app.emit(
                    DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                    DownloadErrorPayload {
                        file_name: job.file_name.clone(),
                        message: msg,
                        recoverable: true,
                    },
                );
                all_ok = false;
                break;
            }
            Err(QueueError::Fatal(msg)) => {
                let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                    j.state = "failed".into();
                });
                let _ = app.emit(
                    DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                    DownloadErrorPayload {
                        file_name: job.file_name.clone(),
                        message: msg,
                        recoverable: false,
                    },
                );
                all_ok = false;
                break;
            }
        }
    }

    let _ = app.emit(
        DEPLOYMENT_QUEUE_COMPLETE_EVENT,
        QueueCompletePayload { ok: all_ok },
    );
}

/// Starts a sequential resumable download into `dest_subdir` (or Athena root when `None` / empty).
/// Returns immediately; progress and errors are emitted to the frontend.
#[tauri::command]
pub async fn deployment_start_downloads(
    app: tauri::AppHandle,
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
    jobs: Vec<DownloadJob>,
    dest_subdir: Option<String>,
) -> Result<(), String> {
    if jobs.is_empty() {
        return Ok(());
    }
    for j in &jobs {
        if j.url.trim().is_empty() {
            return Err("URL is required for every download job.".to_string());
        }
        if j.file_name.trim().is_empty() {
            return Err("file_name is required for every download job.".to_string());
        }
    }

    let records: Vec<DeploymentQueueJobRecord> = jobs
        .iter()
        .map(|j| DeploymentQueueJobRecord {
            job_id: Uuid::new_v4().to_string(),
            source_url: j.url.clone(),
            display_name: j.file_name.clone(),
            state: "queued".into(),
            bytes_downloaded: 0,
            total_bytes: Some(j.expected_bytes),
        })
        .collect();

    queue.try_begin_session(records)?;
    // Durable snapshot at session start (avoid relying on debounce).
    queue.flush().await?;

    if let Err(e) = acquire_power_assertion("ATHENA download") {
        let _ = queue.end_session();
        return Err(e);
    }

    let athena = athena_data_dir(&app)?;
    let dest_dir = dest_dir_from_subdir(&athena, dest_subdir)?;
    if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
        let _ = queue.end_session();
        release_power_assertion();
        return Err(e.to_string());
    }

    let mgr = Arc::clone(&queue);
    let app2 = app.clone();
    let athena2 = athena.clone();
    tauri::async_runtime::spawn(async move {
        run_deployment_queue(app2, mgr, athena2, dest_dir, jobs).await
    });

    Ok(())
}

/// Requests cooperative pause: in-flight work stops after the current network chunk and leaves partials on disk.
#[tauri::command]
pub fn deployment_pause_downloads(
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
) -> Result<(), String> {
    if queue.is_queue_active()? {
        queue.set_cancel_requested(true)?;
    }
    Ok(())
}

#[tauri::command]
pub fn deployment_is_running(
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
) -> Result<bool, String> {
    queue.is_queue_active()
}

/// Canonical queue + session flags for the React deployment UIs (call on mount).
#[tauri::command]
pub fn deployment_get_queue_snapshot(
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
) -> Result<crate::deployment_queue_state::DeploymentQueuePersistence, String> {
    queue.snapshot()
}

#[tauri::command]
pub async fn deployment_open_athena_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = athena_data_dir(&app)?;
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::safe_join;
    use tempfile::tempdir;

    #[test]
    fn safe_join_rejects_parent() {
        let t = tempdir().unwrap();
        let r = safe_join(t.path(), "foo/../bar");
        assert!(r.is_err());
    }

    #[test]
    fn safe_join_ok() {
        let t = tempdir().unwrap();
        let p = safe_join(t.path(), "a/b.txt").unwrap();
        assert!(p.to_string_lossy().ends_with("a/b.txt"));
    }
}
