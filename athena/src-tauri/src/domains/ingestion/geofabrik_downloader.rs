//! Geofabrik region download helper (online at setup time).
//!
//! Goal: given a user location + radius, download all Geofabrik regions that intersect that radius
//! (using a bbox approximation from the region polygons), store them under app_data, and register
//! them as routing regions. Geocode can then be built from the downloaded PBF(s) for offline search.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use serde::Serialize;
use tauri::Emitter;

use crate::db;
use crate::portable;
use crate::AppState;

const GEOFABRIK_INDEX_URL: &str = "https://download.geofabrik.de/index-v1.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeofabrikDownloadProgressEvent {
    pub phase: String,
    pub message: String,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone)]
struct RegionCandidate {
    geofabrik_id: String,
    name: String,
    pbf_url: String,
    min_distance_km_to_bbox: f64,
    bbox: (f64, f64, f64, f64),
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let la1 = lat1.to_radians();
    let la2 = lat2.to_radians();
    let a = (d_lat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    6371.0 * c
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Compute bbox (min_lon, min_lat, max_lon, max_lat) from GeoJSON geometry coordinates.
fn compute_bbox_from_geometry(geom: &serde_json::Value) -> Option<(f64, f64, f64, f64)> {
    let coords = geom.get("coordinates")?;
    let mut min_lon = f64::INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut max_lat = f64::NEG_INFINITY;

    fn walk(v: &serde_json::Value, min_lon: &mut f64, min_lat: &mut f64, max_lon: &mut f64, max_lat: &mut f64) {
        match v {
            serde_json::Value::Array(a) => {
                // Leaf candidate: [lon, lat]
                if a.len() == 2 && a[0].is_number() && a[1].is_number() {
                    let lon = a[0].as_f64().unwrap_or(f64::NAN);
                    let lat = a[1].as_f64().unwrap_or(f64::NAN);
                    if lon.is_finite() && lat.is_finite() {
                        *min_lon = (*min_lon).min(lon);
                        *min_lat = (*min_lat).min(lat);
                        *max_lon = (*max_lon).max(lon);
                        *max_lat = (*max_lat).max(lat);
                    }
                    return;
                }
                for x in a {
                    walk(x, min_lon, min_lat, max_lon, max_lat);
                }
            }
            _ => {}
        }
    }

    walk(coords, &mut min_lon, &mut min_lat, &mut max_lon, &mut max_lat);
    if min_lon.is_finite() && min_lat.is_finite() && max_lon.is_finite() && max_lat.is_finite() {
        Some((min_lon, min_lat, max_lon, max_lat))
    } else {
        None
    }
}

/// Minimum distance in km from point to bbox (0 if inside).
fn min_distance_km_to_bbox(lat: f64, lon: f64, bbox: (f64, f64, f64, f64)) -> f64 {
    let (min_lon, min_lat, max_lon, max_lat) = bbox;
    let clat = clamp(lat, min_lat, max_lat);
    let clon = clamp(lon, min_lon, max_lon);
    haversine_km(lat, lon, clat, clon)
}

fn fetch_json(url: &str) -> Result<serde_json::Value, String> {
    let agent = ureq::Agent::new();
    let resp = agent
        .get(url)
        .timeout(std::time::Duration::from_secs(25))
        .call()
        .map_err(|e| format!("Geofabrik index fetch failed: {e}"))?;
    resp.into_json()
        .map_err(|e| format!("Geofabrik index JSON parse failed: {e}"))
}

fn pick_candidates(lat: f64, lon: f64, radius_km: f64, index: &serde_json::Value) -> Result<Vec<RegionCandidate>, String> {
    let feats = index
        .get("features")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Geofabrik index missing features".to_string())?;

    // Only the smallest regions: a region that others name as their parent (a country, a
    // continent, "US Midwest") would download the same roads again, and can be many gigabytes.
    let parents: std::collections::HashSet<&str> = feats
        .iter()
        .filter_map(|f| f.get("properties")?.get("parent")?.as_str())
        .collect();
    let mut out: Vec<RegionCandidate> = Vec::new();
    for f in feats {
        let props = match f.get("properties") {
            Some(p) => p,
            None => continue,
        };
        let id = props.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
        if id.is_empty() || parents.contains(id) {
            continue;
        }
        let name = props.get("name").and_then(|v| v.as_str()).unwrap_or(id).to_string();
        let pbf = props
            .get("urls")
            .and_then(|u| u.get("pbf"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if pbf.is_empty() {
            continue;
        }
        let geom = match f.get("geometry") {
            Some(g) => g,
            None => continue,
        };
        let bbox = match compute_bbox_from_geometry(geom) {
            Some(b) => b,
            None => continue,
        };
        let d = min_distance_km_to_bbox(lat, lon, bbox);
        if d <= radius_km {
            out.push(RegionCandidate {
                geofabrik_id: id.to_string(),
                name,
                pbf_url: pbf,
                min_distance_km_to_bbox: d,
                bbox,
            });
        }
    }
    // Drop groupings such as "US" or "US Midwest" that wrap other nearby regions: their states
    // are downloaded on their own.
    let contains = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| a != b && a.0 <= b.0 && a.1 <= b.1 && a.2 >= b.2 && a.3 >= b.3;
    let boxes: Vec<(f64, f64, f64, f64)> = out.iter().map(|c| c.bbox).collect();
    out.retain(|c| !boxes.iter().any(|b| contains(c.bbox, *b)));
    out.sort_by(|a, b| a.min_distance_km_to_bbox.partial_cmp(&b.min_distance_km_to_bbox).unwrap());
    Ok(out)
}

fn download_to_path(url: &str, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let agent = ureq::Agent::new();
    let mut resp = agent
        .get(url)
        .timeout(std::time::Duration::from_secs(6 * 3600)) // whole download, large states are GBs
        .call()
        .map_err(|e| format!("Download failed: {e}"))?
        .into_reader();

    let tmp = out_path.with_extension("part");
    let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 1024 * 256];
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    f.sync_all().ok();
    fs::rename(&tmp, out_path).map_err(|e| e.to_string())
}

fn safe_region_id_prefix(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    if out.is_empty() {
        "geofabrik".to_string()
    } else {
        out
    }
}

/// Every downloaded extract (`.osm.pbf`) of the station's routing regions.
pub fn all_region_pbfs(state: &AppState) -> Result<Vec<std::path::PathBuf>, String> {
    let dbh = state.db.lock().map_err(|e| e.to_string())?;
    Ok(dbh
        .list_routing_regions()?
        .into_iter()
        .filter_map(|r| r.source_pbf_path)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_file())
        .collect())
}

/// Download + register regions within radius. Optionally build geocode index for each downloaded PBF.
#[tauri::command]
pub fn geofabrik_download_regions_within_radius(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session: tauri::State<'_, crate::auth::AuthSession>,
    lat: f64,
    lon: f64,
    radius_km: f64,
    max_regions: Option<u32>,
    build_geocode: Option<bool>,
) -> Result<Vec<db::RoutingRegionRow>, String> {
    let _u = crate::auth::authenticated_user(&session)?;
    download_regions(&app, &state, lat, lon, radius_km, max_regions, build_geocode)
}

/// Download the regions within `radius_km`, index their addresses and build the road data.
pub fn download_regions(
    app: &tauri::AppHandle,
    state: &AppState,
    lat: f64,
    lon: f64,
    radius_km: f64,
    max_regions: Option<u32>,
    build_geocode: Option<bool>,
) -> Result<Vec<db::RoutingRegionRow>, String> {
    if !lat.is_finite() || !lon.is_finite() {
        return Err("lat/lon must be finite".to_string());
    }
    let r = radius_km.clamp(1.0, 2500.0);
    let want_geocode = build_geocode.unwrap_or(true);
    let limit = max_regions.unwrap_or(12).clamp(1, 200) as usize;

    let app_data_dir = portable::resolved_app_data_dir(app)?;
    let regions_root = app_data_dir.join("routing_regions");
    fs::create_dir_all(&regions_root).map_err(|e| e.to_string())?;

    let emit = |phase: &str, message: String, done: u32, total: u32| {
        let _ = app.emit(
            "geofabrik-download-progress",
            GeofabrikDownloadProgressEvent {
                phase: phase.to_string(),
                message,
                done,
                total,
            },
        );
    };

    emit("fetch_index", "Fetching Geofabrik index…".to_string(), 0, 0);
    let index = fetch_json(GEOFABRIK_INDEX_URL)?;
    emit("select_regions", format!("Selecting regions within {r:.1} km…"), 0, 0);
    let mut candidates = pick_candidates(lat, lon, r, &index)?;
    if candidates.is_empty() {
        return Err("No Geofabrik regions intersect that radius (unexpected). Try a larger radius or check network.".to_string());
    }
    if candidates.len() > limit {
        candidates.truncate(limit);
    }

    let total = candidates.len() as u32;
    emit("download_start", format!("Downloading {total} region(s)…"), 0, total);

    let mut created: Vec<db::RoutingRegionRow> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let done = i as u32;
        emit(
            "download_one",
            format!("Downloading {} ({}/{})…", c.name, done + 1, total),
            done,
            total,
        );
        let region_id = format!(
            "geofabrik_{}_{}",
            safe_region_id_prefix(&c.geofabrik_id),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let region_dir = regions_root.join(&region_id);
        fs::create_dir_all(&region_dir).map_err(|e| e.to_string())?;

        let pbf_path = region_dir.join("source.osm.pbf");
        if !pbf_path.exists() {
            download_to_path(&c.pbf_url, &pbf_path)?;
        }
        let tiles_dir = region_dir.join("valhalla_tiles");
        fs::create_dir_all(&tiles_dir).map_err(|e| e.to_string())?;

        // Register region in DB so UI can build geocode / set active.
        let now = chrono::Utc::now().to_rfc3339();
        let row = db::RoutingRegionRow {
            id: region_id.clone(),
            name: c.name.clone(),
            bbox_json: "{}".to_string(),
            source_pbf_path: Some(pbf_path.display().to_string()),
            tiles_path: tiles_dir.display().to_string(),
            build_status: "downloaded".to_string(),
            modes_built: "auto,pedestrian,bicycle".to_string(),
            size_bytes: 0,
            created_at: now,
            geocode_built_at: None,
        };
        {
            let dbh = state.db.lock().map_err(|e| e.to_string())?;
            dbh.upsert_routing_region(&row)?;
        }

        if want_geocode {
            emit(
                "build_geocode",
                format!("Building offline geocoder for {}…", c.name),
                done,
                total,
            );
            // Read the extract first; hold the database only while storing.
            let features = crate::offline_geocoder::collect_features_from_pbf(&pbf_path)?;
            let dbh = state.db.lock().map_err(|e| e.to_string())?;
            let _stats = crate::offline_geocoder::store_features(&dbh, &region_id, &pbf_path, &features)?;
        }

        created.push(row);
        emit(
            "done_one",
            format!("Ready: {}", c.name),
            done + 1,
            total,
        );
    }

    // Turn-by-turn directions: rebuild the road data from every downloaded extract.
    emit("build_roads", "Building road data for directions…".to_string(), total, total);
    let pbfs = all_region_pbfs(state)?;
    if let Err(e) = crate::road_router::rebuild(&app_data_dir, &pbfs, &|m| emit("build_roads", m, total, total)) {
        emit("build_roads", format!("Road data was not built: {e}"), total, total);
    }

    emit("complete", format!("Downloaded {total} region(s)."), total, total);
    Ok(created)
}


#[cfg(test)]
mod pick_tests {
    use super::*;

    #[test]
    fn only_the_smallest_regions_are_picked() {
        let sq = |w: f64, s: f64, e: f64, n: f64| serde_json::json!({ "type": "Polygon", "coordinates": [[[w, s], [e, s], [e, n], [w, n], [w, s]]] });
        let f = |id: &str, parent: Option<&str>, g: serde_json::Value| serde_json::json!({
            "properties": { "id": id, "name": id, "parent": parent, "urls": { "pbf": format!("https://x/{id}.osm.pbf") } },
            "geometry": g
        });
        let index = serde_json::json!({ "features": [
            f("north-america", None, sq(-170.0, 5.0, -50.0, 85.0)),
            f("us", Some("north-america"), sq(-125.0, 24.0, -66.0, 50.0)),
            f("us-midwest", Some("north-america"), sq(-105.0, 36.0, -80.0, 50.0)),
            f("us/south-dakota", Some("north-america"), sq(-104.1, 42.5, -96.4, 46.0)),
            f("us/wyoming", Some("north-america"), sq(-111.1, 41.0, -104.0, 45.0)),
            f("us/florida", Some("north-america"), sq(-87.6, 24.5, -80.0, 31.0)),
        ]});
        let ids: Vec<String> = pick_candidates(44.08, -103.23, 160.0, &index).unwrap().into_iter().map(|c| c.geofabrik_id).collect();
        assert_eq!(ids, vec!["us/south-dakota", "us/wyoming"]);
    }
}
