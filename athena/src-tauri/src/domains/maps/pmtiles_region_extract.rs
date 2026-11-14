//! Regional PMTiles extraction using the bundled Protomaps `pmtiles` sidecar.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::map_manager_download;

pub const PMTILES_REGION_EXTRACT_PROGRESS_EVENT: &str = "pmtiles_region_extract_progress";
pub const PMTILES_REGION_EXTRACT_COMPLETE_EVENT: &str = "pmtiles_region_extract_complete";
pub const PMTILES_REGION_EXTRACT_ERROR_EVENT: &str = "pmtiles_region_extract_error";

/// Older fixed Protomaps archive (2024, v3 layer schema). Used only if no recent daily build answers.
const PROTOMAPS_SOURCE_URL: &str =
    "https://data.source.coop/protomaps/openstreetmap/tiles/v3.pmtiles";
/// Protomaps daily planet builds (v4 layer schema, which ATHENA's street style draws). Each is
/// kept for about a week.
const PROTOMAPS_DAILY_BUILD_BASE: &str = "https://build.protomaps.com";

/// Daily build file names to try, newest first, starting the day before `today`.
pub fn daily_build_candidates(today: chrono::NaiveDate, days: i64) -> Vec<String> {
    (1..=days)
        .map(|d| format!("{PROTOMAPS_DAILY_BUILD_BASE}/{}.pmtiles", (today - chrono::Duration::days(d)).format("%Y%m%d")))
        .collect()
}

/// The newest daily build that answers, or the older fixed archive.
fn resolve_protomaps_source() -> String {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("ATHENA-station")
        .build()
    {
        Ok(c) => c,
        Err(_) => return PROTOMAPS_SOURCE_URL.to_string(),
    };
    for url in daily_build_candidates(chrono::Utc::now().date_naive(), 8) {
        if client.head(&url).send().map(|r| r.status().is_success()).unwrap_or(false) {
            return url;
        }
    }
    PROTOMAPS_SOURCE_URL.to_string()
}

/// WGS84 mean Earth radius in statute miles (aligned with `geographicBoundingBox.ts`).
const EARTH_MEAN_RADIUS_MILES: f64 = 3958.7613;
/// Maximum half-span along latitude or longitude (approximate miles) for a single extract — matches UI radius cap.
const MAX_EXTRACT_HALF_SPAN_MILES: f64 = 100.0;

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PmtilesExtractStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Serialize)]
pub struct PmtilesRegionExtractProgressPayload {
    pub job_id: String,
    pub line: String,
    pub stream: PmtilesExtractStream,
}

#[derive(Clone, Serialize)]
pub struct PmtilesRegionExtractCompletePayload {
    pub job_id: String,
    pub output_path: String,
}

#[derive(Clone, Serialize)]
pub struct PmtilesRegionExtractErrorPayload {
    pub job_id: String,
    pub message: String,
}

fn validate_bounding_box(
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<(), String> {
    if !min_lon.is_finite() || !min_lat.is_finite() || !max_lon.is_finite() || !max_lat.is_finite()
    {
        return Err("Bounding box coordinates must be finite numbers.".to_string());
    }
    if min_lon < -180.0 || max_lon > 180.0 {
        return Err("Longitude must stay within -180 to 180 degrees.".to_string());
    }
    if min_lat < -90.0 || max_lat > 90.0 {
        return Err("Latitude must stay within -90 to 90 degrees.".to_string());
    }
    if min_lon >= max_lon {
        return Err("Bounding box requires min_lon to be less than max_lon.".to_string());
    }
    if min_lat >= max_lat {
        return Err("Bounding box requires min_lat to be less than max_lat.".to_string());
    }
    Ok(())
}

/// Ensures the bbox does not exceed the same ~100 mi effective radius enforced in the frontend.
fn validate_bounding_box_extent_miles(
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<(), String> {
    let center_lat = (min_lat + max_lat) / 2.0;
    let center_lat_rad = center_lat.to_radians();
    let half_lat_deg = (max_lat - min_lat) / 2.0;
    let half_lon_deg = (max_lon - min_lon) / 2.0;
    let half_lat_miles = half_lat_deg.to_radians() * EARTH_MEAN_RADIUS_MILES;
    let cos_lat = center_lat_rad.cos().abs().max(1e-6);
    let half_lon_miles = half_lon_deg.to_radians() * EARTH_MEAN_RADIUS_MILES * cos_lat;
    if half_lat_miles > MAX_EXTRACT_HALF_SPAN_MILES {
        return Err(format!(
            "Extract region north–south extent (~{half_lat_miles:.0} mi) exceeds maximum (~{MAX_EXTRACT_HALF_SPAN_MILES:.0} mi). Reduce the area."
        ));
    }
    if half_lon_miles > MAX_EXTRACT_HALF_SPAN_MILES {
        return Err(format!(
            "Extract region east–west extent (~{half_lon_miles:.0} mi) exceeds maximum (~{MAX_EXTRACT_HALF_SPAN_MILES:.0} mi). Reduce the area."
        ));
    }
    Ok(())
}

fn make_job_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    format!("protomaps_region_{millis}_{pid}")
}

fn sanitize_job_id_for_filename(job_id: &str) -> Result<String, String> {
    let safe: String = job_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect();
    if safe.is_empty() {
        return Err("Job identifier could not be converted to a safe file name.".to_string());
    }
    Ok(safe)
}

fn reserve_output_path(app_data_dir: &Path, job_id: &str) -> Result<PathBuf, String> {
    map_manager_download::ensure_offline_maps_directory(app_data_dir)?;
    let safe_stem = sanitize_job_id_for_filename(job_id)?;
    let dir = map_manager_download::offline_maps_directory(app_data_dir);
    let path = dir.join(format!("{safe_stem}.pmtiles"));
    if path.exists() {
        return Err("An extract output file for this job already exists.".to_string());
    }
    Ok(path)
}

#[tauri::command]
pub async fn extract_pmtiles_region(
    app: AppHandle,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
    max_zoom: Option<u8>,
) -> Result<String, String> {
    validate_bounding_box(min_lon, min_lat, max_lon, max_lat)?;
    validate_bounding_box_extent_miles(min_lon, min_lat, max_lon, max_lat)?;
    let max_zoom = max_zoom.unwrap_or(15).clamp(6, 15);
    let source_url = tauri::async_runtime::spawn_blocking(resolve_protomaps_source).await.map_err(|e| e.to_string())?;

    let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
    let job_id = make_job_id();
    let output_path = reserve_output_path(&app_data_dir, &job_id)?;

    let output_path_string = output_path
        .to_str()
        .ok_or("Output path contains characters that cannot be passed to the pmtiles process.")?
        .to_string();

    let bbox_argument = format!("--bbox={min_lon:.8},{min_lat:.8},{max_lon:.8},{max_lat:.8}");
    let maxzoom_argument = format!("--maxzoom={max_zoom}");

    let app_handle = app.clone();
    let job_id_task = job_id.clone();
    let output_path_for_cleanup = output_path.clone();

    tauri::async_runtime::spawn(async move {
        let command = match app_handle.shell().sidecar("pmtiles") {
            Ok(sidecar_command) => sidecar_command.args([
                "extract",
                source_url.as_str(),
                output_path_string.as_str(),
                bbox_argument.as_str(),
                maxzoom_argument.as_str(),
            ]),
            Err(error) => {
                let _ = std::fs::remove_file(&output_path_for_cleanup);
                let _ = app_handle.emit(
                    PMTILES_REGION_EXTRACT_ERROR_EVENT,
                    PmtilesRegionExtractErrorPayload {
                        job_id: job_id_task.clone(),
                        message: format!("Failed to resolve the pmtiles sidecar: {error}"),
                    },
                );
                return;
            }
        };

        let (mut receiver, _child) = match command.spawn() {
            Ok(pair) => pair,
            Err(error) => {
                let _ = std::fs::remove_file(&output_path_for_cleanup);
                let _ = app_handle.emit(
                    PMTILES_REGION_EXTRACT_ERROR_EVENT,
                    PmtilesRegionExtractErrorPayload {
                        job_id: job_id_task.clone(),
                        message: format!("Failed to spawn pmtiles extract: {error}"),
                    },
                );
                return;
            }
        };

        while let Some(event) = receiver.recv().await {
            match event {
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes).trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    let _ = app_handle.emit(
                        PMTILES_REGION_EXTRACT_PROGRESS_EVENT,
                        PmtilesRegionExtractProgressPayload {
                            job_id: job_id_task.clone(),
                            line,
                            stream: PmtilesExtractStream::Stdout,
                        },
                    );
                }
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes).trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    let _ = app_handle.emit(
                        PMTILES_REGION_EXTRACT_PROGRESS_EVENT,
                        PmtilesRegionExtractProgressPayload {
                            job_id: job_id_task.clone(),
                            line,
                            stream: PmtilesExtractStream::Stderr,
                        },
                    );
                }
                CommandEvent::Error(message) => {
                    let _ = app_handle.emit(
                        PMTILES_REGION_EXTRACT_PROGRESS_EVENT,
                        PmtilesRegionExtractProgressPayload {
                            job_id: job_id_task.clone(),
                            line: message.clone(),
                            stream: PmtilesExtractStream::Stderr,
                        },
                    );
                }
                CommandEvent::Terminated(payload) => {
                    let success = payload.code == Some(0);
                    if success {
                        let _ = app_handle.emit(
                            PMTILES_REGION_EXTRACT_COMPLETE_EVENT,
                            PmtilesRegionExtractCompletePayload {
                                job_id: job_id_task.clone(),
                                output_path: output_path_string.clone(),
                            },
                        );
                    } else {
                        let _ = std::fs::remove_file(&output_path_for_cleanup);
                        let detail = match (payload.code, payload.signal) {
                            (Some(code), _) => format!("exit code {code}"),
                            (None, Some(signal)) => format!("terminated by signal {signal}"),
                            _ => "unknown failure".to_string(),
                        };
                        let _ = app_handle.emit(
                            PMTILES_REGION_EXTRACT_ERROR_EVENT,
                            PmtilesRegionExtractErrorPayload {
                                job_id: job_id_task.clone(),
                                message: format!("pmtiles extract failed ({detail})."),
                            },
                        );
                    }
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(job_id)
}

#[cfg(test)]
mod daily_build_tests {
    use super::*;

    #[test]
    fn daily_builds_are_tried_newest_first_from_yesterday() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert_eq!(
            daily_build_candidates(today, 2),
            vec!["https://build.protomaps.com/20261031.pmtiles", "https://build.protomaps.com/20261030.pmtiles"]
        );
    }
}
