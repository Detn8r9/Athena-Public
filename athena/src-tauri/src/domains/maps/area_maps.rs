//! "Maps for my area": one action that starts every download a station needs around its location.
//! - Street map (roads, names, buildings, house numbers, places) up to 100 miles out.
//! - Road and address data for the regions within reach: turn-by-turn directions and address
//!   lookup, built once the extracts arrive.
//! - Aerial imagery (USGS in the US, free) close in, at house-level detail.
//!
//! Each part runs as its own background job with its own progress events, as when started by hand.

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Degrees of latitude and longitude for `miles` at `lat`.
pub fn half_span_degrees(lat: f64, miles: f64) -> (f64, f64) {
    let dlat = miles / 69.0;
    let dlon = miles / (69.172 * lat.to_radians().cos().abs().max(0.05));
    (dlat, dlon)
}

/// West, south, east, north around a point.
pub fn bbox_around(lat: f64, lon: f64, miles: f64) -> (f64, f64, f64, f64) {
    let (dlat, dlon) = half_span_degrees(lat, miles);
    (lon - dlon, (lat - dlat).max(-85.0), lon + dlon, (lat + dlat).min(85.0))
}

/// Largest imagery radius (up to `wanted`) whose tiles fit one download job.
pub fn imagery_radius_that_fits(lat: f64, lon: f64, wanted: f64, min_zoom: u8, max_zoom: u8) -> f64 {
    let mut r = wanted;
    while r > 1.0 {
        let (w, s, e, n) = bbox_around(lat, lon, r);
        if crate::satellite_region_download::enumerate_tiles(w, s, e, n, min_zoom, max_zoom).len()
            <= crate::satellite_region_download::SATELLITE_MAX_TILES_PER_JOB
        {
            return r;
        }
        r -= 1.0;
    }
    1.0
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaMapsStarted {
    pub lat: f64,
    pub lon: f64,
    pub street_map_job: Option<String>,
    pub imagery_job: Option<String>,
    pub imagery_radius_miles: f64,
    pub road_data_started: bool,
    pub problems: Vec<String>,
}

/// The station's position: the saved map location, else the alerts position.
fn station_position(app: &AppHandle) -> Option<(f64, f64)> {
    let dir = crate::portable::resolved_app_data_dir(app).ok()?;
    let cfg = crate::config::load_config(&dir);
    if let (Some(lat), Some(lon)) = (cfg.last_known_gps_lat, cfg.last_known_gps_lng) {
        return Some((lat, lon));
    }
    let state = app.try_state::<crate::AppState>()?;
    let db = state.db.lock().ok()?;
    let s = crate::vault::load_settings(&db);
    s.alert_lat.zip(s.alert_lon)
}

/// Start every map download for the area around the station.
#[tauri::command]
pub async fn maps_download_my_area(
    app: AppHandle,
    session: tauri::State<'_, crate::auth::AuthSession>,
    street_radius_miles: Option<f64>,
    imagery_radius_miles: Option<f64>,
    road_radius_miles: Option<f64>,
) -> Result<AreaMapsStarted, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let (lat, lon) = station_position(&app).ok_or(
        "Set the station's location first: Vault > Home Assistant > Use Home Assistant's location, or set it on the map.",
    )?;
    let street = street_radius_miles.unwrap_or(99.0).clamp(5.0, 99.0);
    let roads = road_radius_miles.unwrap_or(100.0).clamp(10.0, 300.0);
    let mut problems = Vec::new();

    let (w, s, e, n) = bbox_around(lat, lon, street);
    let street_map_job = match crate::pmtiles_region_extract::extract_pmtiles_region(app.clone(), w, s, e, n, Some(15)).await {
        Ok(j) => Some(j),
        Err(e) => {
            problems.push(format!("Street map: {e}"));
            None
        }
    };

    let imagery = imagery_radius_that_fits(lat, lon, imagery_radius_miles.unwrap_or(15.0).clamp(1.0, 40.0), 10, 16);
    let (w, s, e, n) = bbox_around(lat, lon, imagery);
    let imagery_job = match crate::satellite_region_download::start_satellite_region_download(
        app.clone(),
        w,
        s,
        e,
        n,
        Some(10),
        Some(16),
        Some("Home area".to_string()),
        None,
    )
    .await
    {
        Ok(j) => Some(j),
        Err(e) => {
            problems.push(format!("Aerial imagery: {e}"));
            None
        }
    };

    // Road and address data: download, index and build on a worker thread (progress events
    // `geofabrik-download-progress` and `road-router-progress`).
    let app2 = app.clone();
    std::thread::spawn(move || {
        let state = app2.state::<crate::AppState>();
        if let Err(e) = crate::geofabrik_downloader::download_regions(&app2, &state, lat, lon, roads * 1.609_344, Some(8), Some(true)) {
            use tauri::Emitter;
            let _ = app2.emit(
                "geofabrik-download-progress",
                serde_json::json!({ "phase": "error", "message": format!("Road data: {e}"), "done": 0, "total": 0 }),
            );
        }
    });

    Ok(AreaMapsStarted { lat, lon, street_map_job, imagery_job, imagery_radius_miles: imagery, road_data_started: true, problems })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_are_sized_in_miles() {
        let (w, s, e, n) = bbox_around(44.08, -103.23, 100.0);
        assert!(((n - s) * 69.0 / 2.0 - 100.0).abs() < 0.5);
        assert!(((e - w) / 2.0 * 69.172 * 44.08f64.to_radians().cos() - 100.0).abs() < 0.5);
    }

    #[test]
    fn imagery_shrinks_to_fit_one_job() {
        let r = imagery_radius_that_fits(44.08, -103.23, 40.0, 10, 16);
        assert!(r < 40.0 && r >= 10.0, "{r}");
        assert_eq!(imagery_radius_that_fits(44.08, -103.23, 5.0, 10, 16), 5.0);
    }
}
