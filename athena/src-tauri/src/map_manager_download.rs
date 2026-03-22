//! Streaming downloads for offline PMTiles/MBTiles archives into `app_data_dir/maps/`.

use futures_util::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

/// Emitted periodically while bytes are written. `total_bytes` is `None` if the server omitted Content-Length.
pub const MAP_DOWNLOAD_PROGRESS_EVENT: &str = "map_download_progress";

/// Emitted when the file is fully written and renamed into place.
pub const MAP_DOWNLOAD_COMPLETE_EVENT: &str = "map_download_complete";

/// Emitted when the download or disk write fails after starting.
pub const MAP_DOWNLOAD_ERROR_EVENT: &str = "map_download_error";

const MAPS_SUBDIR: &str = "maps";
const PROGRESS_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Clone, Serialize)]
pub struct MapDownloadProgressPayload {
    pub map_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct MapDownloadCompletePayload {
    pub map_id: String,
    pub path: String,
}

#[derive(Clone, Serialize)]
pub struct MapDownloadErrorPayload {
    pub map_id: String,
    pub message: String,
}

pub fn offline_maps_directory(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(MAPS_SUBDIR)
}

pub fn ensure_offline_maps_directory(app_data_dir: &Path) -> Result<(), String> {
    let dir = offline_maps_directory(app_data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())
}

fn sanitize_map_id_for_filename(map_id: &str) -> Result<String, String> {
    let s: String = map_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect();
    if s.is_empty() {
        return Err("Map id is empty or contains no safe filename characters.".to_string());
    }
    Ok(s)
}

fn archive_extension_from_url(download_url: &str) -> Result<&'static str, String> {
    let parsed = reqwest::Url::parse(download_url).map_err(|e| e.to_string())?;
    let path = parsed.path().to_lowercase();
    if path.ends_with(".pmtiles") {
        Ok("pmtiles")
    } else if path.ends_with(".mbtiles") {
        Ok("mbtiles")
    } else {
        Err("URL path must end with .pmtiles or .mbtiles.".to_string())
    }
}

/// Returns `(final_path, partial_path)` for the download.
pub fn destination_paths(
    app_data_dir: &Path,
    map_id: &str,
    download_url: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let safe = sanitize_map_id_for_filename(map_id)?;
    let ext = archive_extension_from_url(download_url)?;
    let dir = offline_maps_directory(app_data_dir);
    let final_path = dir.join(format!("{safe}.{ext}"));
    let part_path = dir.join(format!("{safe}.{ext}.part"));
    Ok((final_path, part_path))
}

/// Lists map ids that have a completed `.pmtiles` or `.mbtiles` file in `maps/`.
pub fn list_downloaded_offline_map_ids(app_data_dir: &Path) -> Result<Vec<String>, String> {
    let dir = offline_maps_directory(app_data_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with(".part") {
            continue;
        }
        let lower = name.to_lowercase();
        const PMT: &str = ".pmtiles";
        const MBT: &str = ".mbtiles";
        if lower.ends_with(PMT) && name.len() > PMT.len() {
            out.push(name[..name.len() - PMT.len()].to_string());
        } else if lower.ends_with(MBT) && name.len() > MBT.len() {
            out.push(name[..name.len() - MBT.len()].to_string());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub async fn stream_download_to_file(
    app: AppHandle,
    map_id: String,
    download_url: String,
    final_path: PathBuf,
    part_path: PathBuf,
) -> Result<(), String> {
    if final_path.exists() {
        return Err("File already exists.".to_string());
    }
    let _ = std::fs::remove_file(&part_path);

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(30))
        .user_agent("AthenaMapManager/1.0")
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

    let _ = app.emit(
        MAP_DOWNLOAD_PROGRESS_EVENT,
        MapDownloadProgressPayload {
            map_id: map_id.clone(),
            bytes_downloaded: 0,
            total_bytes,
        },
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

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| e.to_string())?;
        let n = chunk.len() as u64;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded = downloaded.saturating_add(n);

        if last_emit.elapsed() >= PROGRESS_EMIT_MIN_INTERVAL {
            last_emit = Instant::now();
            let _ = app.emit(
                MAP_DOWNLOAD_PROGRESS_EVENT,
                MapDownloadProgressPayload {
                    map_id: map_id.clone(),
                    bytes_downloaded: downloaded,
                    total_bytes,
                },
            );
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    let _ = app.emit(
        MAP_DOWNLOAD_PROGRESS_EVENT,
        MapDownloadProgressPayload {
            map_id: map_id.clone(),
            bytes_downloaded: downloaded,
            total_bytes,
        },
    );

    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Removes `*.part` files in `maps/` older than 24 hours (interrupted downloads from prior sessions).
pub fn cleanup_stale_part_files(app_data_dir: &std::path::Path) -> Result<(), String> {
    cleanup_stale_part_files_at(app_data_dir, std::time::SystemTime::now())
}

fn cleanup_stale_part_files_at(app_data_dir: &std::path::Path, now: std::time::SystemTime) -> Result<(), String> {
    let dir = offline_maps_directory(app_data_dir);
    if !dir.is_dir() {
        return Ok(());
    }
    let max_age = std::time::Duration::from_secs(24 * 60 * 60);
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".part") {
            continue;
        }
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = match meta.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if now.duration_since(modified).unwrap_or(std::time::Duration::ZERO) > max_age {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cleanup_stale_part_files_at, offline_maps_directory};
    use std::time::{Duration, SystemTime};
    use tempfile::tempdir;

    /// Phase 4: `.part` file with mtime ~25h before the cleanup `now` is removed.
    #[test]
    fn cleanup_stale_part_files_removes_old_part_downloads() {
        let root = tempdir().expect("temp dir");
        let maps = offline_maps_directory(root.path());
        std::fs::create_dir_all(&maps).expect("maps dir");
        let path = maps.join("dummy.pmtiles.part");
        std::fs::write(&path, b"x").expect("write part");

        let file_mtime = SystemTime::now();
        let now = file_mtime + Duration::from_secs(25 * 3600 + 10);
        cleanup_stale_part_files_at(root.path(), now).expect("cleanup");
        assert!(!path.exists(), "stale .part file should be deleted");
    }

    /// Phase 4: `.part` file newer than 24h relative to `now` is kept.
    #[test]
    fn cleanup_stale_part_files_keeps_recent_part_downloads() {
        let root = tempdir().expect("temp dir");
        let maps = offline_maps_directory(root.path());
        std::fs::create_dir_all(&maps).expect("maps dir");
        let path = maps.join("fresh.pmtiles.part");
        std::fs::write(&path, b"x").expect("write part");

        let now = SystemTime::now() + Duration::from_secs(3600);
        cleanup_stale_part_files_at(root.path(), now).expect("cleanup");
        assert!(path.exists(), "recent .part file should be retained");
    }
}
