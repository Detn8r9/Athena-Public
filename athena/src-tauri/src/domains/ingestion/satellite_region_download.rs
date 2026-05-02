use rusqlite::{params, Connection};
use serde::Serialize;
use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::load_config;
use crate::map_manager_download;

pub const SATELLITE_REGION_DOWNLOAD_PROGRESS_EVENT: &str = "satellite_region_download_progress";
pub const SATELLITE_REGION_DOWNLOAD_COMPLETE_EVENT: &str = "satellite_region_download_complete";
pub const SATELLITE_REGION_DOWNLOAD_ERROR_EVENT: &str = "satellite_region_download_error";

const SATELLITE_TILESET_URL_TEMPLATE: &str =
    "https://api.maptiler.com/tiles/satellite-v2/{z}/{x}/{y}.jpg?key={key}";
const SATELLITE_TILE_TIMEOUT_SECS: u64 = 30;
const SATELLITE_MAX_TILES_PER_JOB: usize = 30_000;
const EARTH_MEAN_RADIUS_MILES: f64 = 3958.7613;
const MAX_EXTRACT_HALF_SPAN_MILES: f64 = 100.0;

#[derive(Clone, Serialize)]
pub struct SatelliteRegionDownloadProgressPayload {
    pub job_id: String,
    pub map_id: String,
    pub downloaded_tiles: u64,
    pub total_tiles: u64,
}

#[derive(Clone, Serialize)]
pub struct SatelliteRegionDownloadCompletePayload {
    pub job_id: String,
    pub map_id: String,
    pub path: String,
}

#[derive(Clone, Serialize)]
pub struct SatelliteRegionDownloadErrorPayload {
    pub job_id: String,
    pub map_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
struct TileCoord {
    z: u8,
    x: u32,
    y: u32,
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
    if min_lat < -85.051_128_78 || max_lat > 85.051_128_78 {
        return Err(
            "Latitude must stay within Web Mercator limits (-85.0511 to 85.0511).".to_string(),
        );
    }
    if min_lon >= max_lon {
        return Err("Bounding box requires min_lon to be less than max_lon.".to_string());
    }
    if min_lat >= max_lat {
        return Err("Bounding box requires min_lat to be less than max_lat.".to_string());
    }
    Ok(())
}

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
            "Extract region north-south extent (~{half_lat_miles:.0} mi) exceeds maximum (~{MAX_EXTRACT_HALF_SPAN_MILES:.0} mi). Reduce the area."
        ));
    }
    if half_lon_miles > MAX_EXTRACT_HALF_SPAN_MILES {
        return Err(format!(
            "Extract region east-west extent (~{half_lon_miles:.0} mi) exceeds maximum (~{MAX_EXTRACT_HALF_SPAN_MILES:.0} mi). Reduce the area."
        ));
    }
    Ok(())
}

fn lon_to_tile_x(lon_deg: f64, z: u8) -> u32 {
    let n = (1u64 << z) as f64;
    let x = ((lon_deg + 180.0) / 360.0 * n).floor();
    x.max(0.0).min(n - 1.0) as u32
}

fn lat_to_tile_y(lat_deg: f64, z: u8) -> u32 {
    let lat_rad = lat_deg.to_radians();
    let n = (1u64 << z) as f64;
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * n).floor();
    y.max(0.0).min(n - 1.0) as u32
}

fn enumerate_tiles(
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
    min_zoom: u8,
    max_zoom: u8,
) -> Vec<TileCoord> {
    let mut out = Vec::new();
    for z in min_zoom..=max_zoom {
        let x_min = lon_to_tile_x(min_lon, z);
        let x_max = lon_to_tile_x(max_lon, z);
        let y_min = lat_to_tile_y(max_lat, z);
        let y_max = lat_to_tile_y(min_lat, z);
        for x in x_min..=x_max {
            for y in y_min..=y_max {
                out.push(TileCoord { z, x, y });
            }
        }
    }
    out
}

fn sanitize_for_filename(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(48)
        .collect()
}

fn create_map_id(location_label: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let slug = sanitize_for_filename(location_label);
    if slug.is_empty() {
        format!("satellite_region_{millis}")
    } else {
        format!("satellite_{slug}_{millis}")
    }
}

fn create_mbtiles_schema(
    connection: &Connection,
    name: &str,
    min_zoom: u8,
    max_zoom: u8,
    bounds: &str,
) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE metadata (name text, value text);
             CREATE TABLE tiles (zoom_level integer, tile_column integer, tile_row integer, tile_data blob);
             CREATE UNIQUE INDEX tile_index on tiles (zoom_level, tile_column, tile_row);",
        )
        .map_err(|e| e.to_string())?;
    for (k, v) in [
        ("name", name.to_string()),
        ("description", "Athena satellite imagery region".to_string()),
        ("type", "baselayer".to_string()),
        ("version", "1".to_string()),
        ("format", "jpg".to_string()),
        ("minzoom", min_zoom.to_string()),
        ("maxzoom", max_zoom.to_string()),
        ("bounds", bounds.to_string()),
    ] {
        connection
            .execute(
                "INSERT INTO metadata (name, value) VALUES (?1, ?2)",
                params![k, v],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_satellite_region_download(
    app: AppHandle,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
    min_zoom: Option<u8>,
    max_zoom: Option<u8>,
    location_label: Option<String>,
) -> Result<String, String> {
    validate_bounding_box(min_lon, min_lat, max_lon, max_lat)?;
    validate_bounding_box_extent_miles(min_lon, min_lat, max_lon, max_lat)?;
    let min_z = min_zoom.unwrap_or(10);
    let max_z = max_zoom.unwrap_or(15);
    if min_z > max_z {
        return Err("Minimum zoom must be less than or equal to maximum zoom.".to_string());
    }
    if max_z > 18 {
        return Err("Maximum zoom above 18 is not allowed for this workflow.".to_string());
    }

    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let api_key = config
        .satellite_api_key
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("ATHENA_SATELLITE_API_KEY").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
        .ok_or_else(|| {
            "Satellite API key is not configured. Paste your key in Settings > API keys > MapTiler satellite imagery key, or set ATHENA_SATELLITE_API_KEY.".to_string()
        })?;
    if api_key.trim().is_empty() {
        return Err("Satellite API key is empty. Add it in Settings > Location.".to_string());
    }

    let tiles = enumerate_tiles(min_lon, min_lat, max_lon, max_lat, min_z, max_z);
    if tiles.is_empty() {
        return Err("No tiles found for the selected region and zoom range.".to_string());
    }
    if tiles.len() > SATELLITE_MAX_TILES_PER_JOB {
        return Err(format!(
            "Requested region is too large at current zoom levels ({} tiles). Reduce radius or max zoom.",
            tiles.len()
        ));
    }

    map_manager_download::ensure_offline_maps_directory(&app_data_dir)?;
    let map_id = create_map_id(location_label.as_deref().unwrap_or("region"));
    let maps_dir = map_manager_download::offline_maps_directory(&app_data_dir);
    let final_path = maps_dir.join(format!("{map_id}.mbtiles"));
    let part_path = maps_dir.join(format!("{map_id}.mbtiles.part"));
    if final_path.exists() {
        return Err(
            "A satellite archive with this generated id already exists; retry the download."
                .to_string(),
        );
    }
    let _ = std::fs::remove_file(&part_path);
    let bounds = format!("{min_lon:.6},{min_lat:.6},{max_lon:.6},{max_lat:.6}");
    let display_name = format!(
        "Satellite region ({})",
        location_label.unwrap_or_else(|| "saved location".to_string())
    );
    let job_id = format!(
        "satellite_job_{}_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        std::process::id()
    );

    let app_handle = app.clone();
    let job_id_task = job_id.clone();
    let map_id_task = map_id.clone();

    static SAT_DL_SEM: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let sem = SAT_DL_SEM.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)));
    let sem = Arc::clone(sem);

    tauri::async_runtime::spawn(async move {
        let _permit = match sem.acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };

        let result = (|| -> Result<(), String> {
            let mut connection = Connection::open(&part_path).map_err(|e| e.to_string())?;
            // Optimize for bulk tile ingest (MBTiles part file).
            connection
                .execute_batch(
                    "PRAGMA journal_mode=WAL;
                     PRAGMA synchronous=NORMAL;",
                )
                .map_err(|e| e.to_string())?;
            create_mbtiles_schema(&connection, &display_name, min_z, max_z, &bounds)?;
            let client = reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(30))
                .timeout(Duration::from_secs(SATELLITE_TILE_TIMEOUT_SECS))
                .user_agent("AthenaSatelliteDownloader/1.0")
                .build()
                .map_err(|e: reqwest::Error| e.to_string())?;

            let total_tiles = tiles.len() as u64;
            let mut downloaded_tiles = 0u64;
            let mut inserted_tiles = 0u64;
            let mut since_commit: u64 = 0;

            let _ = app_handle.emit(
                SATELLITE_REGION_DOWNLOAD_PROGRESS_EVENT,
                SatelliteRegionDownloadProgressPayload {
                    job_id: job_id_task.clone(),
                    map_id: map_id_task.clone(),
                    downloaded_tiles,
                    total_tiles,
                },
            );

            let mut tx = connection
                .transaction()
                .map_err(|e| e.to_string())?;
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO tiles (zoom_level, tile_column, tile_row, tile_data) VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(|e| e.to_string())?;

            for tile in tiles {
                let url = SATELLITE_TILESET_URL_TEMPLATE
                    .replace("{z}", &tile.z.to_string())
                    .replace("{x}", &tile.x.to_string())
                    .replace("{y}", &tile.y.to_string())
                    .replace("{key}", &api_key);
                let response = client
                    .get(url)
                    .send()
                    .map_err(|e: reqwest::Error| e.to_string())?;
                if response.status().is_success() {
                    let bytes = response
                        .bytes()
                        .map_err(|e: reqwest::Error| e.to_string())?;
                    if !bytes.is_empty() {
                        let nrows = 1i64
                            .checked_shl(tile.z as u32)
                            .ok_or_else(|| format!("Zoom {} is too large for tile-row conversion.", tile.z))?;
                        let tile_row_tms = nrows - 1 - tile.y as i64;
                        stmt.execute(params![
                            tile.z as i64,
                            tile.x as i64,
                            tile_row_tms,
                            bytes.as_ref()
                        ])
                        .map_err(|e| e.to_string())?;
                        inserted_tiles += 1;
                        since_commit += 1;
                        if since_commit >= 500 {
                            drop(stmt);
                            tx.commit().map_err(|e| e.to_string())?;
                            tx = connection.transaction().map_err(|e| e.to_string())?;
                            stmt = tx
                                .prepare(
                                    "INSERT OR REPLACE INTO tiles (zoom_level, tile_column, tile_row, tile_data) VALUES (?1, ?2, ?3, ?4)",
                                )
                                .map_err(|e| e.to_string())?;
                            since_commit = 0;
                        }
                    }
                } else if response.status().as_u16() != 404 {
                    return Err(format!(
                        "Satellite tile provider returned HTTP {}. Check key limits or provider status.",
                        response.status().as_u16()
                    ));
                }
                downloaded_tiles += 1;
                if downloaded_tiles % 25 == 0 || downloaded_tiles == total_tiles {
                    let _ = app_handle.emit(
                        SATELLITE_REGION_DOWNLOAD_PROGRESS_EVENT,
                        SatelliteRegionDownloadProgressPayload {
                            job_id: job_id_task.clone(),
                            map_id: map_id_task.clone(),
                            downloaded_tiles,
                            total_tiles,
                        },
                    );
                }
            }

            drop(stmt);
            tx.commit().map_err(|e| e.to_string())?;

            if inserted_tiles == 0 {
                return Err("No satellite tiles were downloaded for this region. Try a different area or zoom range.".to_string());
            }

            std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                let path_string = final_path.to_string_lossy().to_string();
                let _ = app_handle.emit(
                    SATELLITE_REGION_DOWNLOAD_COMPLETE_EVENT,
                    SatelliteRegionDownloadCompletePayload {
                        job_id: job_id_task.clone(),
                        map_id: map_id_task.clone(),
                        path: path_string.clone(),
                    },
                );
                let _ = app_handle.emit(
                    map_manager_download::MAP_DOWNLOAD_COMPLETE_EVENT,
                    map_manager_download::MapDownloadCompletePayload {
                        map_id: map_id_task.clone(),
                        path: path_string,
                    },
                );
            }
            Err(message) => {
                let _ = std::fs::remove_file(&part_path);
                let _ = app_handle.emit(
                    SATELLITE_REGION_DOWNLOAD_ERROR_EVENT,
                    SatelliteRegionDownloadErrorPayload {
                        job_id: job_id_task.clone(),
                        map_id: map_id_task.clone(),
                        message: message.clone(),
                    },
                );
                let _ = app_handle.emit(
                    map_manager_download::MAP_DOWNLOAD_ERROR_EVENT,
                    map_manager_download::MapDownloadErrorPayload {
                        map_id: map_id_task.clone(),
                        message,
                    },
                );
            }
        }
    });

    Ok(job_id)
}
