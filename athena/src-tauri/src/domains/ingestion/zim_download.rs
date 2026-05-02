//! Stream-based ZIM downloads into `payload/ZIM_Archives` (async, bounded memory).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::kiwix_serve;
use crate::package_manager;
use crate::AppState;

pub const ZIM_DOWNLOAD_PROGRESS_EVENT: &str = "zim_download_progress";
pub const ZIM_DOWNLOAD_COMPLETE_EVENT: &str = "zim_download_complete";
pub const ZIM_DOWNLOAD_ERROR_EVENT: &str = "zim_download_error";

const PROGRESS_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(160);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZimDownloadProgressPayload {
    pub download_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZimDownloadCompletePayload {
    pub download_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZimDownloadErrorPayload {
    pub download_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadZimArgs {
    pub url: String,
    pub file_name: String,
    pub download_id: String,
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

    fn sample(&mut self, downloaded: u64) -> Option<u64> {
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
        let bps = (delta as f64 / elapsed) as u64;
        self.at = now;
        self.bytes = downloaded;
        Some(bps)
    }
}

fn emit_progress(
    app: &AppHandle,
    download_id: &str,
    downloaded: u64,
    total_bytes: Option<u64>,
    speed: &mut SpeedMeter,
) {
    let _ = speed.sample(downloaded);
    let percent = total_bytes.map(|t| {
        if t == 0 {
            0.0
        } else {
            ((downloaded as f64 / t as f64) * 100.0).min(100.0)
        }
    });
    let _ = app.emit(
        ZIM_DOWNLOAD_PROGRESS_EVENT,
        ZimDownloadProgressPayload {
            download_id: download_id.to_string(),
            bytes_downloaded: downloaded,
            total_bytes,
            percent,
        },
    );
}

async fn stream_zim_to_file(
    app: AppHandle,
    download_id: String,
    download_url: String,
    final_path: PathBuf,
    part_path: PathBuf,
    cancel: CancellationToken,
) -> Result<(), String> {
    if final_path.exists() {
        match std::fs::metadata(&final_path) {
            Ok(meta) if meta.is_file() && meta.len() > 0 => {
                let len = meta.len();
                let mut speed = SpeedMeter::new();
                emit_progress(&app, &download_id, len, Some(len), &mut speed);
                return Ok(());
            }
            Ok(meta) if meta.is_file() && meta.len() == 0 => {
                let _ = std::fs::remove_file(&final_path);
            }
            Ok(_) => {
                return Err(
                    "Destination exists but is not a regular file; remove it or pick another name."
                        .to_string(),
                );
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = std::fs::remove_file(&part_path);

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(300))
        .user_agent("AthenaZimDownloader/1.0")
        .redirect(reqwest::redirect::Policy::limited(32))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let total_bytes = response.content_length();
    let mut speed = SpeedMeter::new();
    emit_progress(&app, &download_id, 0, total_bytes, &mut speed);

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
                return Err("Cancelled.".to_string());
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
                            emit_progress(&app, &download_id, downloaded, total_bytes, &mut speed);
                        }
                    }
                }
            }
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    emit_progress(
        &app,
        &download_id,
        downloaded,
        total_bytes,
        &mut speed,
    );

    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
}

struct ZimDownloadEntryGuard {
    map: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, CancellationToken>>>,
    key: String,
}

impl Drop for ZimDownloadEntryGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = self.map.lock() {
            g.remove(&self.key);
        }
    }
}

#[tauri::command]
pub async fn download_zim(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    args: DownloadZimArgs,
) -> Result<(), String> {
    let url = args.url.trim().to_string();
    if url.is_empty() {
        return Err("url is required.".to_string());
    }
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("URL must use http or https.".to_string());
    }

    let file_name = args.file_name.trim().to_string();
    if file_name.is_empty() {
        return Err("file_name is required.".to_string());
    }
    let download_id = args.download_id.trim().to_string();
    if download_id.is_empty() {
        return Err("download_id is required.".to_string());
    }

    let dest_dir = kiwix_serve::zim_archives_dir(&app)?;
    let (final_path, part_path) = package_manager::destination_paths(&dest_dir, &file_name)?;

    let cancel = CancellationToken::new();
    let run_cancel = cancel.clone();
    {
        let mut active = state
            .active_zim_downloads
            .lock()
            .map_err(|_| "ZIM download state lock poisoned.".to_string())?;
        if active.contains_key(&download_id) {
            return Err("This download is already in progress.".to_string());
        }
        active.insert(download_id.clone(), cancel);
    }

    let app_handle = app.clone();
    let active_map = std::sync::Arc::clone(&state.active_zim_downloads);
    let id_for_task = download_id.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = ZimDownloadEntryGuard {
            map: active_map,
            key: id_for_task.clone(),
        };
        let outcome = stream_zim_to_file(
            app_handle.clone(),
            id_for_task.clone(),
            url,
            final_path.clone(),
            part_path,
            run_cancel,
        )
        .await;

        match outcome {
            Ok(()) => {
                kiwix_serve::restart_after_library_change(&app_handle);
                let path_string = final_path.to_string_lossy().to_string();
                let _ = app_handle.emit(
                    ZIM_DOWNLOAD_COMPLETE_EVENT,
                    ZimDownloadCompletePayload {
                        download_id: id_for_task,
                        path: path_string,
                    },
                );
            }
            Err(ref msg) if msg == "Cancelled." => {}
            Err(msg) => {
                let _ = app_handle.emit(
                    ZIM_DOWNLOAD_ERROR_EVENT,
                    ZimDownloadErrorPayload {
                        download_id: id_for_task,
                        message: msg,
                    },
                );
            }
        }
    });

    Ok(())
}
