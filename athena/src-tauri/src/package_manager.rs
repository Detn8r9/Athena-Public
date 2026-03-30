use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

pub const PACKAGE_DOWNLOAD_PROGRESS_EVENT: &str = "package_download_progress";
pub const PACKAGE_DOWNLOAD_COMPLETE_EVENT: &str = "package_download_complete";
pub const PACKAGE_DOWNLOAD_ERROR_EVENT: &str = "package_download_error";
pub const PACKAGE_DOWNLOAD_CANCELLED_EVENT: &str = "package_download_cancelled";

/// Returned from [`stream_download_to_file`] when the user cancels; matched in `download_package`.
pub const PACKAGE_DOWNLOAD_CANCELLED_MSG: &str = "Cancelled.";

const PROGRESS_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(160);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryManifest {
    pub tiers: Vec<LibraryTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTier {
    pub name: String,
    pub description: String,
    pub files: Vec<LibraryFile>,
    /// Optional human-facing tier total (e.g. "~1.5 GB") when rounded display differs from raw byte sum.
    #[serde(default)]
    pub display_total_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryFile {
    pub name: String,
    pub category: String,
    pub url: String,
    pub destination: String,
    /// Approximate download size when known (from manifest); used for UI totals only.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// When `category` is Academy, optional curriculum grouping for the archive UI.
    #[serde(default)]
    pub academy_age_group_id: Option<String>,
    #[serde(default)]
    pub academy_subject_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: String,
}

/// Emitted to the frontend with camelCase JSON keys (matches ChatConsoleView / LibraryInstaller).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDownloadProgressPayload {
    pub manifest_type: String,
    pub download_id: String,
    #[serde(rename = "downloadedBytes")]
    pub bytes_downloaded: u64,
    #[serde(rename = "totalBytes")]
    pub total_bytes: Option<u64>,
    /// Instantaneous throughput since the previous progress emit (when available).
    #[serde(rename = "bytesPerSecond")]
    pub bytes_per_second: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDownloadCompletePayload {
    pub manifest_type: String,
    pub download_id: String,
    pub path: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDownloadErrorPayload {
    pub manifest_type: String,
    pub download_id: String,
    pub error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDownloadCancelledPayload {
    pub manifest_type: String,
    pub download_id: String,
}

struct SpeedMeter {
    at: Instant,
    bytes: u64,
    primed: bool,
}

impl SpeedMeter {
    fn new() -> Self {
        Self {
            at: Instant::now(),
            bytes: 0,
            primed: false,
        }
    }

    /// Updates anchors and returns bytes/sec since last sample (or `None` for the first sample).
    fn sample(&mut self, downloaded: u64) -> Option<u64> {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
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
        let bps = (delta as f64 / elapsed) as u64;
        self.at = now;
        self.bytes = downloaded;
        Some(bps)
    }
}

fn emit_download_progress(
    app: &AppHandle,
    manifest_type: &str,
    download_id: &str,
    downloaded: u64,
    total_bytes: Option<u64>,
    speed: &mut SpeedMeter,
) {
    let bytes_per_second = speed.sample(downloaded);
    let _ = app.emit(
        PACKAGE_DOWNLOAD_PROGRESS_EVENT,
        PackageDownloadProgressPayload {
            manifest_type: manifest_type.to_string(),
            download_id: download_id.to_string(),
            bytes_downloaded: downloaded,
            total_bytes,
            bytes_per_second,
        },
    );
}

fn sanitize_filename(input: &str) -> Result<String, String> {
    let s: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(200)
        .collect();
    if s.is_empty() {
        return Err("Filename is empty or contains no safe characters.".to_string());
    }
    Ok(s)
}

pub fn load_library_manifest_bundled() -> Result<LibraryManifest, String> {
    let raw = include_str!("../resources/library_manifest.json");
    serde_json::from_str::<LibraryManifest>(raw).map_err(|e| e.to_string())
}

/// Merges regional Protomaps state extracts into "The Satchel" based on a saved US pin.
pub fn load_library_manifest_with_location(
    loc: Option<&crate::config::LibraryInstallerLocation>,
) -> Result<LibraryManifest, String> {
    let mut m = load_library_manifest_bundled()?;
    for tier in &mut m.tiers {
        if tier.name != "The Satchel" {
            continue;
        }
        tier.files.retain(|f| f.category != "Maps");
        if let Some(loc) = loc {
            let offers = crate::library_installer_location::satchel_map_offers(loc)?;
            for o in offers {
                tier.files.push(LibraryFile {
                    name: format!("{} Emergency Map", o.display_name),
                    category: "Maps".into(),
                    url: o.url,
                    destination: "library/maps/".into(),
                    size_bytes: None,
                    academy_age_group_id: None,
                    academy_subject_id: None,
                });
            }
        }
    }
    Ok(m)
}

pub fn load_model_manifest_bundled() -> Result<ModelManifest, String> {
    let raw = include_str!("../resources/model_manifest.json");
    serde_json::from_str::<ModelManifest>(raw).map_err(|e| e.to_string())
}

pub fn destination_paths(dest_dir: &Path, file_name: &str) -> Result<(PathBuf, PathBuf), String> {
    let safe = sanitize_filename(file_name)?;
    let final_path = dest_dir.join(&safe);
    let part_path = dest_dir.join(format!("{safe}.part"));
    Ok((final_path, part_path))
}

pub async fn stream_download_to_file(
    app: AppHandle,
    manifest_type: String,
    download_id: String,
    download_url: String,
    final_path: PathBuf,
    part_path: PathBuf,
    cancel: CancellationToken,
) -> Result<(), String> {
    // If the user re-runs "Install tier" or a file was copied in manually, skip the
    // network fetch and treat it as success so multi-file tiers can continue.
    if final_path.exists() {
        match std::fs::metadata(&final_path) {
            Ok(meta) if meta.is_file() && meta.len() > 0 => {
                let len = meta.len();
                let mut speed = SpeedMeter::new();
                emit_download_progress(
                    &app,
                    &manifest_type,
                    &download_id,
                    len,
                    Some(len),
                    &mut speed,
                );
                return Ok(());
            }
            Ok(meta) if meta.is_file() && meta.len() == 0 => {
                let _ = std::fs::remove_file(&final_path);
            }
            Ok(_) => {
                return Err(
                    "Destination exists but is not a regular file; remove it or pick another folder."
                        .to_string(),
                );
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = std::fs::remove_file(&part_path);

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(30))
        .user_agent("AthenaPackageManager/1.0")
        .redirect(reqwest::redirect::Policy::limited(16))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let total_bytes = response.content_length();
    let mut speed = SpeedMeter::new();
    emit_download_progress(
        &app,
        &manifest_type,
        &download_id,
        0,
        total_bytes,
        &mut speed,
    );

    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&part_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                drop(file);
                let _ = std::fs::remove_file(&part_path);
                return Err(PACKAGE_DOWNLOAD_CANCELLED_MSG.to_string());
            }
            item = stream.next() => {
                match item {
                    None => break,
                    Some(Err(e)) => {
                        drop(file);
                        return Err(e.to_string());
                    }
                    Some(Ok(chunk)) => {
                        let n = chunk.len() as u64;
                        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                        downloaded = downloaded.saturating_add(n);

                        if last_emit.elapsed() >= PROGRESS_EMIT_MIN_INTERVAL {
                            last_emit = Instant::now();
                            emit_download_progress(
                                &app,
                                &manifest_type,
                                &download_id,
                                downloaded,
                                total_bytes,
                                &mut speed,
                            );
                        }
                    }
                }
            }
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    emit_download_progress(
        &app,
        &manifest_type,
        &download_id,
        downloaded,
        total_bytes,
        &mut speed,
    );

    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
}
