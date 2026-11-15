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

fn spawn_post_download_library_extract(app: AppHandle, _athena_root: PathBuf, final_path: PathBuf) {
    if !library_ingest::is_library_archive(&final_path) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        // Downloaded archives: extract next to the archive file, then delete the archive to save space.
        match library_ingest::extract_archive_in_place_with_progress_async(app.clone(), final_path.clone()).await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&final_path).await;
                let _ = app.emit(watcher::ARCHIVE_FS_CHANGED_EVENT, serde_json::json!({}));
            }
            Err(e) => {
                eprintln!(
                    "[athena] post-download archive extract failed for {}: {e}",
                    final_path.display()
                );
            }
        };
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
    let paths = crate::athena_paths::AthenaPaths::new(athena);
    let Some(s) = dest_subdir else {
        // Default: all Data Acquisition HTTP downloads land in managed downloads/packages.
        return Ok(paths.user_downloads_dir().join("packages"));
    };
    let t = s.trim();
    if t.is_empty() {
        return Ok(paths.user_downloads_dir().join("packages"));
    }
    safe_join(athena, t)
}

fn categorize_download_dest(athena_root: &Path, job_file_name: &str) -> Result<PathBuf, String> {
    let rel = job_file_name.trim().replace('\\', "/");
    let p = Path::new(&rel);
    let fname = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid file name.".to_string())?;
    let lower = rel.to_ascii_lowercase();
    let paths = crate::athena_paths::AthenaPaths::new(athena_root);
    let user = paths.user_dir();

    // Preserve `models/<subdir>/…` under AthenaData/user/models/
    if lower.starts_with("models/") {
        let rest = &rel[7..];
        if rest.is_empty() || rest.contains("..") {
            return Err("Invalid models path.".to_string());
        }
        return Ok(user.join("models").join(rest));
    }

    let folder = if lower.starts_with("library/maps/")
        || lower.contains("/maps/")
        || lower.starts_with("maps/")
    {
        user.join("maps")
    } else if lower.starts_with("academy/") {
        user.join("library").join("academy")
    } else if lower.starts_with("library/") || lower.contains("/library/") {
        paths.user_downloads_dir().join("packages")
    } else {
        paths.user_downloads_dir().join("documents")
    };
    Ok(folder.join(fname))
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
    // Measure the drive the downloads actually go to.
    let athena = crate::athena_paths::AthenaPaths::new(athena_data_dir(&app)?).user_downloads_dir();
    let _ = std::fs::create_dir_all(&athena);
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
pub struct FileLocation {
    pub path: String,
    /// Where the file is on the station.
    pub location: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileScanReport {
    pub complete: Vec<String>,
    pub partial: Vec<PartialFileStatus>,
    pub missing: Vec<String>,
    pub oversized: Vec<PartialFileStatus>,
    /// Where each complete (and partial) file was found.
    #[serde(default)]
    pub locations: Vec<FileLocation>,
}

#[derive(Debug, Clone, PartialEq)]
enum Found {
    Complete(PathBuf),
    Partial(u64, PathBuf),
    Oversized(u64, PathBuf),
    Missing,
}

fn len_of(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// Finds a catalog file wherever ATHENA keeps it: where downloads of its kind go now (the chosen
/// download folder), its partial download, the old place under the data folder, and, for
/// archives, any folder the library reader serves (an archive copied in by hand counts).
fn locate_expected(
    dest: &Path,
    legacy: &Path,
    size: u64,
    elsewhere: &mut dyn FnMut(&str) -> Option<(PathBuf, u64)>,
) -> Found {
    for p in [dest, legacy] {
        if len_of(p) == Some(size) {
            return Found::Complete(p.to_path_buf());
        }
    }
    if let Some(name) = dest.file_name().and_then(|n| n.to_str()) {
        if let Some((p, len)) = elsewhere(name) {
            if len == size {
                return Found::Complete(p);
            }
        }
    }
    let part = partial_download_path(dest);
    if let Some(len) = len_of(&part) {
        return if len <= size { Found::Partial(len, part) } else { Found::Oversized(len, part) };
    }
    for p in [dest, legacy] {
        match len_of(p) {
            Some(len) if len < size => return Found::Partial(len, p.to_path_buf()),
            Some(len) if len > size => return Found::Oversized(len, p.to_path_buf()),
            _ => {}
        }
    }
    Found::Missing
}

#[tauri::command]
pub fn deployment_scan_files(
    app: tauri::AppHandle,
    expected: Vec<ExpectedFile>,
) -> Result<FileScanReport, String> {
    let root = athena_data_dir(&app)?;
    let mut complete: Vec<String> = Vec::new();
    let mut partial: Vec<PartialFileStatus> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut oversized: Vec<PartialFileStatus> = Vec::new();
    let mut locations: Vec<FileLocation> = Vec::new();

    // Archives in any served folder, by file name (built only if needed).
    let mut by_name: Option<std::collections::HashMap<String, (PathBuf, u64)>> = None;
    let mut elsewhere = |name: &str| -> Option<(PathBuf, u64)> {
        if !name.to_ascii_lowercase().ends_with(".zim") {
            return None;
        }
        let map = by_name.get_or_insert_with(|| {
            let roots = crate::kiwix_serve::zim_discovery_roots(&root);
            crate::kiwix_serve::collect_zim_paths_from_roots(&roots)
                .into_iter()
                .filter_map(|p| {
                    let pb = PathBuf::from(&p);
                    let n = pb.file_name()?.to_str()?.to_ascii_lowercase();
                    let len = len_of(&pb)?;
                    Some((n, (pb, len)))
                })
                .collect()
        });
        map.get(&name.to_ascii_lowercase()).cloned()
    };

    for e in expected {
        let rel = e.path.replace('\\', "/");
        let legacy = safe_join(&root, &e.path)?;
        let dest = categorize_download_dest(&root, &e.path).unwrap_or_else(|_| legacy.clone());
        match locate_expected(&dest, &legacy, e.size_bytes, &mut elsewhere) {
            Found::Complete(p) => {
                locations.push(FileLocation { path: rel.clone(), location: p.display().to_string() });
                complete.push(rel);
            }
            Found::Partial(len, p) => {
                locations.push(FileLocation { path: rel.clone(), location: p.display().to_string() });
                partial.push(PartialFileStatus { path: rel, downloaded_bytes: len, expected_bytes: e.size_bytes });
            }
            Found::Oversized(len, _) => {
                oversized.push(PartialFileStatus { path: rel, downloaded_bytes: len, expected_bytes: e.size_bytes });
            }
            Found::Missing => missing.push(rel),
        }
    }

    Ok(FileScanReport {
        complete,
        partial,
        missing,
        oversized,
        locations,
    })
}

#[cfg(test)]
mod scan_tests {
    use super::*;

    #[test]
    fn files_are_found_where_they_really_are() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("packages").join("a.zim");
        let legacy = dir.path().join("library").join("a.zim");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let mut none = |_: &str| None;

        assert_eq!(locate_expected(&dest, &legacy, 10, &mut none), Found::Missing);

        // A partial download.
        std::fs::write(partial_download_path(&dest), b"12345").unwrap();
        assert_eq!(locate_expected(&dest, &legacy, 10, &mut none), Found::Partial(5, partial_download_path(&dest)));

        // Copied in by hand elsewhere (e.g. the ZIM archive folder) counts as installed.
        let other = dir.path().join("zim").join("a.zim");
        let mut found = |n: &str| (n == "a.zim").then(|| (other.clone(), 10));
        assert_eq!(locate_expected(&dest, &legacy, 10, &mut found), Found::Complete(other.clone()));

        // Complete in the download folder.
        std::fs::write(&dest, b"0123456789").unwrap();
        assert_eq!(locate_expected(&dest, &legacy, 10, &mut none), Found::Complete(dest.clone()));
    }
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
    /// Optional extra URLs to try (same file on mirrors). Used when the primary URL is stale or blocked.
    #[serde(default)]
    pub alternate_urls: Vec<String>,
    /// Optional library source package label for downstream ingestion + cataloging (e.g., "iFixit", "Wikipedia").
    #[serde(default)]
    pub source_package: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSizeProbeRow {
    pub file_name: String,
    pub bytes: u64,
    /// `"network"` when a preflight succeeded; `"manifest"` when all URLs failed and the manifest size is used.
    pub source: String,
    pub resolved_url: Option<String>,
    pub tried_urls: Vec<String>,
}

#[derive(Debug, Clone)]
struct PreflightResult {
    effective_bytes: u64,
    /// Final URL after redirects; use this for the streaming GET.
    final_url: String,
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
        // A body that stops partway (connection closed, proxy reset, HTTP/2 stream error) leaves
        // a resumable partial on disk, so it is always worth another try.
        return true;
    }
    e.is_decode()
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

/// Where a file's bytes collect until the download is complete.
fn partial_download_path(final_path: &Path) -> PathBuf {
    let mut name = final_path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    final_path.with_file_name(name)
}

/// Server answers worth waiting out and trying again (busy, rate limited, mirror hiccup).
fn http_status_is_temporary(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
}

/// How many times a dropped connection is retried before the file is marked failed, and the wait
/// before each try. A long archive can take hours; mirrors drop connections now and then.
const RETRY_DELAYS_SECS: [u64; 6] = [5, 15, 30, 60, 120, 300];

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

#[derive(Debug)]
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

/// Total resource size from `Content-Range: bytes 0-0/12345` (used when `Content-Length` is only the chunk size).
fn header_content_range_total(resp: &reqwest::Response) -> Option<u64> {
    let val = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())?;
    let slash = val.rfind('/')?;
    val[slash + 1..].trim().parse::<u64>().ok()
}

fn kiwix_mirror_candidate(url: &str) -> Option<String> {
    const PRIMARY: &str = "https://download.kiwix.org";
    let u = url.trim();
    if !u.to_ascii_lowercase().starts_with(PRIMARY) {
        return None;
    }
    Some(u.replacen(PRIMARY, "https://mirror.download.kiwix.org", 1))
}

fn job_candidate_urls(job: &DownloadJob) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        let t = s.trim().to_string();
        if t.is_empty() {
            return;
        }
        if out.iter().any(|x| x == &t) {
            return;
        }
        out.push(t);
    };

    push(job.url.clone());
    for a in &job.alternate_urls {
        push(a.clone());
    }
    if let Some(m) = kiwix_mirror_candidate(&job.url) {
        push(m);
    }
    out
}

fn preflight_status_needs_range_probe(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::METHOD_NOT_ALLOWED
        || status == reqwest::StatusCode::NOT_FOUND
        || status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::NOT_IMPLEMENTED
}

/// HEAD (or GET `Range: bytes=0-0` when HEAD is missing / blocked) for size, weak validators, and `Accept-Ranges`.
async fn deployment_network_preflight(
    client: &reqwest::Client,
    url: &str,
    manifest_expected: u64,
    partial_on_disk: u64,
    athena_root: &Path,
) -> Result<PreflightResult, QueueError> {
    let mut resp = client.head(url).send().await.map_err(map_send_error)?;
    let mut status = resp.status();

    if preflight_status_needs_range_probe(status) {
        drop(resp);
        resp = client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(map_send_error)?;
        status = resp.status();
    }

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
    let range_total = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        header_content_range_total(&resp)
    } else {
        None
    };
    let etag = header_etag(&resp);
    let last_modified = header_last_modified(&resp);
    let final_url = resp.url().as_str().to_string();
    let _drain = resp.bytes().await.map_err(map_send_error)?;

    let url_key = deployment_manifest_cache::normalize_url_key(&final_url);
    let effective_bytes = if let Some(rt) = range_total.filter(|&n| n > 0) {
        if rt != manifest_expected {
            deployment_manifest_cache::persist_head_size_if_changed(
                athena_root,
                &url_key,
                manifest_expected,
                rt,
                etag.clone(),
                last_modified.clone(),
            )
            .await
            .map_err(QueueError::Fatal)?;
        }
        rt
    } else if let Some(cl) = content_length.filter(|&n| n > 0) {
        if cl != manifest_expected {
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
        cl
    } else {
        manifest_expected
    };

    if partial_on_disk > 0 && !accept_ranges {
        return Err(QueueError::Fatal(format!(
            "Server does not advertise byte-range resume (Accept-Ranges: bytes) for this URL, but {} byte(s) are already on disk for a partial download. The partial file was preserved; remove it or use a mirror that supports HTTP Range requests. URL: {}",
            partial_on_disk, url
        )));
    }

    Ok(PreflightResult {
        effective_bytes,
        final_url,
    })
}

async fn preflight_first_working_url(
    client: &reqwest::Client,
    job: &DownloadJob,
    partial_on_disk: u64,
    athena_root: &Path,
) -> Result<PreflightResult, QueueError> {
    let candidates = job_candidate_urls(job);
    if candidates.is_empty() {
        return Err(QueueError::Fatal("No download URLs to try.".to_string()));
    }
    let mut last: Option<QueueError> = None;
    for u in &candidates {
        match deployment_network_preflight(
            client,
            u,
            job.expected_bytes,
            partial_on_disk,
            athena_root,
        )
        .await
        {
            Ok(p) => return Ok(p),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| {
        QueueError::Fatal("Preflight failed for all candidate URLs.".to_string())
    }))
}

async fn download_one_job(
    app: &AppHandle,
    client: &reqwest::Client,
    _dest_dir: &Path,
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

    // Route each job into AthenaData/user/… by category (models/chat, maps, packages, …).
    let final_path = categorize_download_dest(athena_root, &job.file_name)
        .map_err(QueueError::Fatal)?;
    if let Some(p) = final_path.parent() {
        tokio::fs::create_dir_all(p)
            .await
            .map_err(|e| QueueError::Fatal(e.to_string()))?;
    }

    // Bytes arrive in `<name>.part` and the file takes its real name only once complete, so a cut
    // download is never mistaken for a finished archive (and never served or indexed).
    let part_path = partial_download_path(&final_path);
    let part_len = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
    let final_len = tokio::fs::metadata(&final_path).await.ok().map(|m| m.len());

    // The manifest size can be stale, so nothing on disk is truncated until the server has said
    // how big the file really is.
    let pf = preflight_first_working_url(client, job, part_len, athena_root).await?;
    let effective_bytes = pf.effective_bytes;
    let resolved_url = pf.final_url;

    let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
        j.total_bytes = Some(effective_bytes);
        j.source_url = resolved_url.clone();
    });

    let mut on_disk = part_len;
    match final_len {
        Some(n) if n == effective_bytes => {
            // Already complete under its real name; any stray partial is leftover.
            let _ = tokio::fs::remove_file(&part_path).await;
            on_disk = effective_bytes;
        }
        Some(n) if n < effective_bytes && part_len == 0 && n > 0 => {
            // Older builds wrote straight to the real name: an interrupted file looks complete.
            // Move it aside and resume it.
            tokio::fs::rename(&final_path, &part_path)
                .await
                .map_err(|e| QueueError::Fatal(format!("Could not move partial {}: {e}", final_path.display())))?;
            on_disk = n;
        }
        _ => {}
    }

    if on_disk > effective_bytes {
        on_disk = 0;
        let _ = tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| QueueError::Fatal(e.to_string()))?;
    }

    if on_disk == effective_bytes && final_len != Some(effective_bytes) {
        // The partial already holds every byte: just finish it.
        tokio::fs::rename(&part_path, &final_path)
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

        let mut request = client.get(&resolved_url);
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
            let msg = format!("The server answered {} for {}", status, job.file_name);
            return Err(if http_status_is_temporary(status) {
                QueueError::Recoverable(msg)
            } else {
                QueueError::Fatal(msg)
            });
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
                .open(&part_path)
                .await
                .map_err(|e| QueueError::Fatal(e.to_string()))?;
            (f, resume_from)
        } else {
            // Fresh download from byte 0
            let f = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&part_path)
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
            // The connection ended early; the partial stays and the next attempt resumes it.
            return Err(QueueError::Recoverable(format!(
                "The connection closed after {} of {} bytes: {}",
                downloaded, effective_bytes, job.file_name
            )));
        }
        tokio::fs::rename(&part_path, &final_path)
            .await
            .map_err(|e| QueueError::Fatal(format!("Could not finish {}: {e}", final_path.display())))?;

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
    ingestion_queue: crate::background_ingestion::IngestionQueue,
    db: Arc<Mutex<crate::db::Database>>,
) {
    struct EndClear {
        mgr: Arc<DeploymentQueueStateManager>,
    }
    impl Drop for EndClear {
        fn drop(&mut self) {
            release_power_assertion();
            let _ = self.mgr.end_session();
            // `end_session()` already schedules an immediate persist via the background worker.
            // Avoid `flush_blocking()` here: this Drop runs on a Tokio runtime thread and
            // `block_on` will panic ("runtime within a runtime").
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
    'jobs: for job in &jobs {
        let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
            j.state = "running".into();
            j.message = None;
        });
        let mut attempt = 0usize;
        loop {
            let r = download_one_job(&app, &client, &dest_dir, &athena_root, mgr.as_ref(), job).await;
            let (msg, recoverable) = match r {
                Ok(()) => {
                    let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                        j.state = "completed".into();
                        j.message = None;
                    });
                    // Trigger background ingestion as soon as the file is complete.
                    // The file was written where its kind belongs, not under `dest_dir`.
                    let final_path = categorize_download_dest(&athena_root, &job.file_name)
                        .unwrap_or_else(|_| safe_join(&dest_dir, &job.file_name).unwrap_or_else(|_| dest_dir.join(&job.file_name)));
                    ingestion_queue.enqueue_completed_download(&app, &db, final_path, job.source_package.clone());
                    continue 'jobs;
                }
                Err(QueueError::Paused) => {
                    let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                        j.state = "paused".into();
                    });
                    let _ = app.emit(
                        DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                        DownloadErrorPayload { file_name: job.file_name.clone(), message: "paused".to_string(), recoverable: true },
                    );
                    all_ok = false;
                    // Everything after this file waits for the next run.
                    let _ = mgr.update_in_memory(|p| {
                        for j in &mut p.jobs {
                            if j.state == "queued" {
                                j.state = "paused".into();
                            }
                        }
                    });
                    break 'jobs;
                }
                Err(QueueError::Recoverable(m)) => (m, true),
                Err(QueueError::Fatal(m)) => (m, false),
            };
            if recoverable && attempt < RETRY_DELAYS_SECS.len() {
                let wait = RETRY_DELAYS_SECS[attempt];
                attempt += 1;
                let note = format!("Connection dropped; trying again in {wait}s (attempt {attempt} of {}). {msg}", RETRY_DELAYS_SECS.len());
                let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                    j.state = "retrying".into();
                    j.message = Some(note.clone());
                });
                let _ = app.emit(
                    DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                    DownloadErrorPayload { file_name: job.file_name.clone(), message: note, recoverable: true },
                );
                for _ in 0..wait {
                    if mgr.is_cancel_requested().unwrap_or(false) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                    j.state = "running".into();
                });
                continue;
            }
            // Give up on this file and move on: one bad mirror must not strand the rest of the queue.
            let _ = mgr.touch_job_by_display_name(&job.file_name, |j| {
                j.state = "failed".into();
                j.message = Some(msg.clone());
            });
            let _ = app.emit(
                DEPLOYMENT_DOWNLOAD_ERROR_EVENT,
                DownloadErrorPayload { file_name: job.file_name.clone(), message: msg, recoverable: false },
            );
            all_ok = false;
            continue 'jobs;
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
    state: tauri::State<'_, crate::AppState>,
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
            message: None,
            job: serde_json::to_value(j).ok(),
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
    // Unified managed downloads root (layout v2: user/downloads).
    let dest_dir = crate::athena_paths::AthenaPaths::new(&athena).user_downloads_dir();
    // Optional override still supported for advanced callers.
    let dest_dir = if dest_subdir.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        dest_dir
    } else {
        dest_dir_from_subdir(&athena, dest_subdir)?
    };
    if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
        let _ = queue.end_session();
        release_power_assertion();
        return Err(e.to_string());
    }

    let mgr = Arc::clone(&queue);
    let app2 = app.clone();
    let athena2 = athena.clone();
    let ingestion_queue = state.ingestion_queue.clone();
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn(async move {
        run_deployment_queue(app2, mgr, athena2, dest_dir, jobs, ingestion_queue, db).await
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

/// Starts again every file from the last run that did not finish (failed, paused, interrupted
/// or never reached). Partial files resume where they stopped.
#[tauri::command]
pub async fn deployment_retry_unfinished(
    app: tauri::AppHandle,
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<usize, String> {
    let jobs = unfinished_jobs(&queue.snapshot()?.last_session);
    if jobs.is_empty() {
        return Err("Nothing from the last run is left to download.".to_string());
    }
    let n = jobs.len();
    deployment_start_downloads(app, queue, state, jobs, None).await?;
    Ok(n)
}

/// Forgets the last run's results (the list under "Last run" in Downloads).
#[tauri::command]
pub fn deployment_clear_last_session(
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
) -> Result<(), String> {
    queue.clear_last_session()
}

fn unfinished_jobs(rows: &[DeploymentQueueJobRecord]) -> Vec<DownloadJob> {
    rows.iter()
        .filter(|r| r.state != "completed")
        .filter_map(|r| r.job.clone())
        .filter_map(|v| serde_json::from_value::<DownloadJob>(v).ok())
        .collect()
}

/// Canonical queue + session flags for the React deployment UIs (call on mount).
#[tauri::command]
pub fn deployment_get_queue_snapshot(
    queue: tauri::State<'_, Arc<DeploymentQueueStateManager>>,
) -> Result<crate::deployment_queue_state::DeploymentQueuePersistence, String> {
    queue.snapshot()
}

/// Lightweight remote size discovery for UI ETA (HEAD / tiny ranged GET). Falls back to manifest bytes on failure.
#[tauri::command]
pub async fn deployment_probe_remote_sizes(
    app: tauri::AppHandle,
    jobs: Vec<DownloadJob>,
) -> Result<Vec<RemoteSizeProbeRow>, String> {
    if jobs.is_empty() {
        return Ok(Vec::new());
    }
    let client = build_http_client()?;
    let athena_root = athena_data_dir(&app)?;
    let mut rows: Vec<RemoteSizeProbeRow> = Vec::with_capacity(jobs.len());
    for job in jobs {
        if job.file_name.trim().is_empty() {
            continue;
        }
        eprintln!(
            "[probe_remote_sizes] job file_name={} expected_bytes={} url={}",
            job.file_name.trim(),
            job.expected_bytes,
            job.url.trim()
        );
        if job.url.trim().to_ascii_lowercase().starts_with("magnet:") {
            rows.push(RemoteSizeProbeRow {
                file_name: job.file_name.clone(),
                bytes: job.expected_bytes,
                source: "manifest".into(),
                resolved_url: None,
                tried_urls: vec![job.url.clone()],
            });
            continue;
        }
        let candidates = job_candidate_urls(&job);
        if !job.alternate_urls.is_empty() {
            eprintln!(
                "[probe_remote_sizes] job has {} alternate_urls",
                job.alternate_urls.len()
            );
        }
        let mut tried: Vec<String> = Vec::new();
        let mut resolved: Option<String> = None;
        let mut ok_bytes: Option<u64> = None;
        for u in &candidates {
            tried.push(u.clone());
            eprintln!("[probe_remote_sizes] try {}", u);
            match deployment_network_preflight(
                &client,
                u,
                job.expected_bytes,
                0,
                &athena_root,
            )
            .await
            {
                Ok(p) => {
                    eprintln!(
                        "[probe_remote_sizes] ok final_url={} bytes={}",
                        p.final_url, p.effective_bytes
                    );
                    resolved = Some(p.final_url);
                    ok_bytes = Some(p.effective_bytes);
                    break;
                }
                Err(e) => {
                    eprintln!("[probe_remote_sizes] fail {} err={:?}", u, e);
                    continue;
                }
            }
        }
        if let Some(bytes) = ok_bytes {
            rows.push(RemoteSizeProbeRow {
                file_name: job.file_name.clone(),
                bytes,
                source: "network".into(),
                resolved_url: resolved,
                tried_urls: tried,
            });
        } else {
            eprintln!(
                "[probe_remote_sizes] no usable url; falling back to manifest bytes for {}",
                job.file_name.trim()
            );
            rows.push(RemoteSizeProbeRow {
                file_name: job.file_name.clone(),
                bytes: job.expected_bytes,
                source: "manifest".into(),
                resolved_url: None,
                tried_urls: tried,
            });
        }
    }
    Ok(rows)
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
    use super::*;
    #[test]
    fn partial_files_get_their_own_name() {
        let p = std::path::Path::new("/x/packages/gutenberg_en_all.zim");
        assert_eq!(partial_download_path(p), std::path::Path::new("/x/packages/gutenberg_en_all.zim.part"));
    }

    #[test]
    fn only_unfinished_files_are_retried() {
        let job = |n: &str| serde_json::json!({"url": format!("https://e.org/{n}"), "fileName": n, "expectedBytes": 10});
        let row = |n: &str, st: &str, with_job: bool| crate::deployment_queue_state::DeploymentQueueJobRecord {
            job_id: n.into(),
            source_url: String::new(),
            display_name: n.into(),
            state: st.into(),
            bytes_downloaded: 0,
            total_bytes: None,
            message: None,
            job: with_job.then(|| job(n)),
        };
        let got = unfinished_jobs(&[
            row("a", "completed", true),
            row("b", "failed", true),
            row("c", "not_started", true),
            row("d", "failed", false),
        ]);
        let names: Vec<_> = got.iter().map(|j| j.file_name.as_str()).collect();
        assert_eq!(names, ["b", "c"]);
    }

    #[test]
    fn temporary_server_errors_are_retried() {
        assert!(http_status_is_temporary(reqwest::StatusCode::SERVICE_UNAVAILABLE));
        assert!(http_status_is_temporary(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(!http_status_is_temporary(reqwest::StatusCode::NOT_FOUND));
    }

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
