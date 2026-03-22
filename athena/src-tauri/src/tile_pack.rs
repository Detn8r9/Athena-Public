//! Offline raster tiles (OpenStreetMap / OpenTopoMap) for Leaflet + Tauri `convertFileSrc`.
//! Follow OSM tile usage policy: cache on disk, identifiable User-Agent, moderate rate.

use crate::mbtiles_io;
use crate::pmtiles_io;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::Emitter;
use tauri::Manager;

const PACKS_FILENAME: &str = "athena_tile_packs.json";
pub const TILE_PACKS_SUBDIR: &str = "tile_packs";

const MAX_TILES_PER_JOB: usize = 8_000;
const OSM_DELAY_MS: u64 = 75;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OfflineBasemapPaths {
    pub osm: Option<String>,
    pub opentopo: Option<String>,
    /// Latest pack imported from MBTiles (XYZ folder).
    pub imported: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TilePackEstimate {
    pub count: usize,
    pub max_tiles: usize,
    pub within_limit: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TilePackInfo {
    pub id: String,
    pub name: String,
    /// `osm` or `opentopo`
    pub source: String,
    pub root_path: String,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub center_lat: f64,
    pub center_lon: f64,
    pub radius_miles: f64,
}

fn packs_path(app_data: &Path) -> PathBuf {
    app_data.join(PACKS_FILENAME)
}

fn load_packs(app_data: &Path) -> Vec<TilePackInfo> {
    let path = packs_path(app_data);
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(p) = serde_json::from_str::<Vec<TilePackInfo>>(&data) {
                return p;
            }
        }
    }
    Vec::new()
}

fn save_packs(app_data: &Path, packs: &[TilePackInfo]) -> Result<(), String> {
    let path = packs_path(app_data);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(packs).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())
}

pub fn list_tile_packs(app_data: &Path) -> Vec<TilePackInfo> {
    load_packs(app_data)
}

/// Latest pack path per category (last in list wins).
pub fn offline_basemap_paths(app_data: &Path) -> OfflineBasemapPaths {
    let packs = load_packs(app_data);
    let mut osm: Option<String> = None;
    let mut topo: Option<String> = None;
    let mut imported: Option<String> = None;
    for p in packs {
        match p.source.as_str() {
            "opentopo" => topo = Some(p.root_path),
            "mbtiles" | "pmtiles" => imported = Some(p.root_path),
            _ => osm = Some(p.root_path),
        }
    }
    OfflineBasemapPaths {
        osm,
        opentopo: topo,
        imported,
    }
}

pub fn estimate_tile_download(
    center_lat: f64,
    center_lon: f64,
    radius_miles: f64,
    zoom_min: u8,
    zoom_max: u8,
) -> TilePackEstimate {
    let (nw_lat, nw_lon) = offset_miles(center_lat, center_lon, radius_miles, -radius_miles);
    let (se_lat, se_lon) = offset_miles(center_lat, center_lon, -radius_miles, radius_miles);
    let min_lat = se_lat.min(nw_lat);
    let max_lat = se_lat.max(nw_lat);
    let min_lon = nw_lon.min(se_lon);
    let max_lon = nw_lon.max(se_lon);
    let zmin = zoom_min.min(zoom_max);
    let zmax = zoom_max.max(zoom_min);
    let n = count_tiles(min_lat, max_lat, min_lon, max_lon, zmin, zmax);
    TilePackEstimate {
        count: n,
        max_tiles: MAX_TILES_PER_JOB,
        within_limit: n <= MAX_TILES_PER_JOB,
    }
}

pub fn remove_tile_pack(app_data: &Path, id: &str) -> Result<(), String> {
    let mut packs = load_packs(app_data);
    let removed = packs.iter().find(|p| p.id == id).cloned();
    packs.retain(|p| p.id != id);
    save_packs(app_data, &packs)?;
    if let Some(p) = removed {
        let path = PathBuf::from(&p.root_path);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn tile_xy(lat_deg: f64, lon_deg: f64, zoom: u8) -> (i32, i32) {
    let n = (1u32 << zoom) as f64;
    let x = ((lon_deg + 180.0) / 360.0 * n).floor() as i32;
    let lat_rad = lat_deg.to_radians();
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n).floor() as i32;
    (x, y)
}

fn bbox_tiles(min_lat: f64, max_lat: f64, min_lon: f64, max_lon: f64, zoom: u8) -> Vec<(i32, i32)> {
    let (x1, y1) = tile_xy(max_lat, min_lon, zoom);
    let (x2, y2) = tile_xy(min_lat, max_lon, zoom);
    let (min_x, max_x) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
    let (min_y, max_y) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
    let mut out = Vec::new();
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            out.push((x, y));
        }
    }
    out
}

fn offset_miles(lat: f64, lon: f64, miles_north: f64, miles_east: f64) -> (f64, f64) {
    let dlat = miles_north / 69.0;
    let dlon = miles_east / (69.0 * lat.to_radians().cos().max(0.02));
    (lat + dlat, lon + dlon)
}

/// Nominatim search — respect usage policy (single-threaded, UA).
pub fn geocode_nominatim(query: &str) -> Result<(f64, f64), String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Empty address".to_string());
    }
    let enc = urlencoding::encode(q);
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1",
        enc
    );
    let resp = ureq::get(&url)
        .set(
            "User-Agent",
            "AthenaMicro/1.0 (offline maps; https://github.com/athena-micro)",
        )
        .timeout(std::time::Duration::from_secs(12))
        .call()
        .map_err(|e| e.to_string())?;
    if resp.status() != 200 {
        return Err(format!("Geocode HTTP {}", resp.status()));
    }
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let arr = v.as_array().ok_or_else(|| "No geocode results".to_string())?;
    let first = arr.first().ok_or_else(|| "No geocode results".to_string())?;
    fn parse_coord(v: &serde_json::Value) -> Option<f64> {
        v.as_f64().or_else(|| v.as_str()?.parse().ok())
    }
    let lat = parse_coord(&first["lat"]).ok_or_else(|| "Bad lat".to_string())?;
    let lon = parse_coord(&first["lon"]).ok_or_else(|| "Bad lon".to_string())?;
    Ok((lat, lon))
}

fn tile_url(source: &str, z: i32, x: i32, y: i32) -> String {
    match source {
        "opentopo" => format!("https://a.tile.opentopomap.org/{z}/{x}/{y}.png"),
        _ => format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
    }
}

fn count_tiles(
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
    zoom_min: u8,
    zoom_max: u8,
) -> usize {
    let mut n = 0usize;
    for z in zoom_min..=zoom_max {
        n += bbox_tiles(min_lat, max_lat, min_lon, max_lon, z).len();
    }
    n
}

pub fn download_tile_pack_background(
    app: tauri::AppHandle,
    progress_event: &'static str,
    done_event: &'static str,
    error_event: &'static str,
    pack_id: String,
    name: String,
    source: String,
    center_lat: f64,
    center_lon: f64,
    radius_miles: f64,
    zoom_min: u8,
    zoom_max: u8,
) {
    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let root = app_data.join(TILE_PACKS_SUBDIR).join(&pack_id);
            fs::create_dir_all(&root).map_err(|e| e.to_string())?;

            let (nw_lat, nw_lon) = offset_miles(center_lat, center_lon, radius_miles, -radius_miles);
            let (se_lat, se_lon) = offset_miles(center_lat, center_lon, -radius_miles, radius_miles);
            let min_lat = se_lat.min(nw_lat);
            let max_lat = se_lat.max(nw_lat);
            let min_lon = nw_lon.min(se_lon);
            let max_lon = nw_lon.max(se_lon);

            let zmin = zoom_min.min(zoom_max);
            let zmax = zoom_max.max(zoom_min);
            let n = count_tiles(min_lat, max_lat, min_lon, max_lon, zmin, zmax);
            if n > MAX_TILES_PER_JOB {
                return Err(format!(
                    "Area too large at these zoom levels (~{n} tiles; max {MAX_TILES_PER_JOB}). Shrink radius or zoom range."
                ));
            }

            let mut tiles: Vec<(u8, i32, i32)> = Vec::new();
            for z in zmin..=zmax {
                for (x, y) in bbox_tiles(min_lat, max_lat, min_lon, max_lon, z) {
                    tiles.push((z, x, y));
                }
            }
            let total = tiles.len() as u64;
            let agent = "AthenaMicro/1.0 (local offline cache; +https://github.com/athena-micro)";
            let mut done = 0u64;

            for (z, x, y) in tiles {
                let dir = root.join(z.to_string()).join(x.to_string());
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let path = dir.join(format!("{y}.png"));
                if !path.exists() {
                    let url = tile_url(&source, z as i32, x, y);
                    let resp = ureq::get(&url)
                        .set("User-Agent", agent)
                        .timeout(std::time::Duration::from_secs(25))
                        .call()
                        .map_err(|e| e.to_string())?;
                    if resp.status() != 200 {
                        return Err(format!("Tile fetch HTTP {} for {}", resp.status(), url));
                    }
                    let mut body = Vec::new();
                    let mut reader = resp.into_reader().take(2 * 1024 * 1024);
                    reader
                        .read_to_end(&mut body)
                        .map_err(|e| e.to_string())?;
                    fs::write(&path, body).map_err(|e| e.to_string())?;
                    std::thread::sleep(std::time::Duration::from_millis(OSM_DELAY_MS));
                }
                done += 1;
                let payload = serde_json::json!({
                    "id": format!("tiles:{pack_id}"),
                    "downloaded_bytes": done,
                    "total_bytes": total,
                });
                let _ = app.emit(progress_event, payload);
            }

            let root_str = root.to_string_lossy().to_string();
            let mut packs = load_packs(&app_data);
            packs.push(TilePackInfo {
                id: pack_id.clone(),
                name,
                source: source.clone(),
                root_path: root_str,
                min_zoom: zmin,
                max_zoom: zmax,
                center_lat,
                center_lon,
                radius_miles,
            });
            save_packs(&app_data, &packs)?;

            let _ = app.emit(
                done_event,
                serde_json::json!({ "id": format!("tiles:{pack_id}"), "path": pack_id }),
            );
            Ok(())
        };

        if let Err(e) = run() {
            let _ = app.emit(
                error_event,
                serde_json::json!({ "id": format!("tiles:{pack_id}"), "error": e }),
            );
        }
    });
}

/// Background: export an existing XYZ pack folder to a single `.mbtiles` file.
pub fn export_pack_to_mbtiles_background(
    app: tauri::AppHandle,
    progress_event: &'static str,
    done_event: &'static str,
    error_event: &'static str,
    pack_id: String,
    dest_mbtiles_path: String,
) {
    let progress_id = format!("mbtiles_export:{pack_id}");
    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let packs = load_packs(&app_data);
            let pack = packs
                .iter()
                .find(|p| p.id == pack_id)
                .ok_or_else(|| "Tile pack not found.".to_string())?;
            let root = PathBuf::from(&pack.root_path);
            if !root.is_dir() {
                return Err("Pack folder is missing on disk.".to_string());
            }
            let attr = format!(
                "Athena export · {} · Respect original tile licenses (OSM ODbL / OpenTopoMap).",
                pack.name
            );
            mbtiles_io::export_xyz_to_mbtiles(
                &root,
                Path::new(&dest_mbtiles_path),
                &pack.name,
                &attr,
                |done, total| {
                    let _ = app.emit(
                        progress_event,
                        serde_json::json!({
                            "id": progress_id,
                            "downloaded_bytes": done,
                            "total_bytes": total,
                        }),
                    );
                },
            )?;
            let _ = app.emit(
                done_event,
                serde_json::json!({ "id": progress_id, "path": dest_mbtiles_path }),
            );
            Ok(())
        };
        if let Err(e) = run() {
            let _ = app.emit(
                error_event,
                serde_json::json!({ "id": progress_id, "error": e }),
            );
        }
    });
}

/// Background: import MBTiles → XYZ folder + register pack (`source`: mbtiles).
pub fn import_mbtiles_background(
    app: tauri::AppHandle,
    progress_event: &'static str,
    done_event: &'static str,
    error_event: &'static str,
    job_id: String,
    src_mbtiles_path: String,
    display_name: String,
) {
    let progress_id = format!("import:{job_id}");
    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let dest_root = app_data.join(TILE_PACKS_SUBDIR).join(&job_id);
            if dest_root.exists() {
                fs::remove_dir_all(&dest_root).map_err(|e| e.to_string())?;
            }
            let (count, min_z, max_z) = mbtiles_io::import_mbtiles_to_xyz(
                Path::new(&src_mbtiles_path),
                &dest_root,
                |done, total| {
                    let _ = app.emit(
                        progress_event,
                        serde_json::json!({
                            "id": progress_id,
                            "downloaded_bytes": done,
                            "total_bytes": total,
                        }),
                    );
                },
            )?;
            if count == 0 {
                return Err("MBTiles file contained no tiles.".to_string());
            }
            let root_str = dest_root.to_string_lossy().to_string();
            let mut packs = load_packs(&app_data);
            packs.push(TilePackInfo {
                id: job_id.clone(),
                name: display_name.clone(),
                source: "mbtiles".to_string(),
                root_path: root_str,
                min_zoom: min_z,
                max_zoom: max_z,
                center_lat: 0.0,
                center_lon: 0.0,
                radius_miles: 0.0,
            });
            save_packs(&app_data, &packs)?;
            let _ = app.emit(
                done_event,
                serde_json::json!({ "id": progress_id, "path": job_id }),
            );
            Ok(())
        };
        if let Err(e) = run() {
            let _ = app.emit(
                error_event,
                serde_json::json!({ "id": progress_id, "error": e }),
            );
        }
    });
}

/// Background: import raster PMTiles → XYZ folder + register pack (`source`: pmtiles).
pub fn import_pmtiles_background(
    app: tauri::AppHandle,
    progress_event: &'static str,
    done_event: &'static str,
    error_event: &'static str,
    job_id: String,
    src_pmtiles_path: String,
    display_name: String,
) {
    let progress_id = format!("import:{job_id}");
    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let dest_root = app_data.join(TILE_PACKS_SUBDIR).join(&job_id);
            if dest_root.exists() {
                fs::remove_dir_all(&dest_root).map_err(|e| e.to_string())?;
            }
            let (count, min_z, max_z) = pmtiles_io::import_pmtiles_to_xyz(
                Path::new(&src_pmtiles_path),
                &dest_root,
                MAX_TILES_PER_JOB,
                |done, total| {
                    let _ = app.emit(
                        progress_event,
                        serde_json::json!({
                            "id": progress_id,
                            "downloaded_bytes": done,
                            "total_bytes": total,
                        }),
                    );
                },
            )?;
            if count == 0 {
                return Err("PMTiles import produced no tiles.".to_string());
            }
            let root_str = dest_root.to_string_lossy().to_string();
            let mut packs = load_packs(&app_data);
            packs.push(TilePackInfo {
                id: job_id.clone(),
                name: display_name.clone(),
                source: "pmtiles".to_string(),
                root_path: root_str,
                min_zoom: min_z,
                max_zoom: max_z,
                center_lat: 0.0,
                center_lon: 0.0,
                radius_miles: 0.0,
            });
            save_packs(&app_data, &packs)?;
            let _ = app.emit(
                done_event,
                serde_json::json!({ "id": progress_id, "path": job_id }),
            );
            Ok(())
        };
        if let Err(e) = run() {
            let _ = app.emit(
                error_event,
                serde_json::json!({ "id": progress_id, "error": e }),
            );
        }
    });
}
