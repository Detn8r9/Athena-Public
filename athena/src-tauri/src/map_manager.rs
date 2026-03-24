use std::io::Write;

use chrono::Utc;
use geoconvert::{LatLon, Mgrs, UtmUps};
use serde::Serialize;
use uuid::Uuid;

use crate::db::{MapDrawing, MapMarker};
use crate::radio_manager::RadioState;
use crate::AppState;

/// MGRS precision 5 yields a 10-digit grid (5 easting + 5 northing digits within the 100 km square).
const MGRS_GRID_PRECISION: i32 = 5;

#[derive(Debug, Clone, Serialize)]
pub struct CoordinateConversionResult {
    /// Single-line summary, e.g. `40.748333° N, 73.985278° W`
    pub decimal_degrees: String,
    pub decimal_degrees_lat: String,
    pub decimal_degrees_lng: String,
    pub utm: String,
    /// Full MGRS string at 1 m (10-digit grid) precision.
    pub mgrs: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatLngResult {
    pub lat: f64,
    pub lng: f64,
}

fn lat_lon_from_inputs(lat: f64, lng: f64) -> Result<LatLon, String> {
    if !lat.is_finite() || !lng.is_finite() {
        return Err("Latitude and longitude must be finite numbers.".to_string());
    }
    LatLon::create(lat, lng).map_err(|e| format!("Invalid coordinates: {e}"))
}

#[tauri::command]
pub async fn convert_coordinates(lat: f64, lng: f64) -> Result<CoordinateConversionResult, String> {
    let ll = lat_lon_from_inputs(lat, lng)?;
    let utm = UtmUps::from_latlon(&ll);
    let mgrs = Mgrs::from_latlon(&ll, MGRS_GRID_PRECISION);
    let lat_v = ll.latitude();
    let lng_v = ll.longitude();
    let hemi_n = if lat_v >= 0.0 { "N" } else { "S" };
    let hemi_e = if lng_v >= 0.0 { "E" } else { "W" };
    let decimal_degrees = format!(
        "{:.6}° {}, {:.6}° {}",
        lat_v.abs(),
        hemi_n,
        lng_v.abs(),
        hemi_e
    );
    let decimal_degrees_lat = format!("{:.6}° {}", lat_v.abs(), hemi_n);
    let decimal_degrees_lng = format!("{:.6}° {}", lng_v.abs(), hemi_e);
    Ok(CoordinateConversionResult {
        decimal_degrees,
        decimal_degrees_lat,
        decimal_degrees_lng,
        utm: utm.to_string(),
        mgrs: mgrs.to_string(),
    })
}

#[tauri::command]
pub async fn convert_mgrs_to_lat_lng(mgrs: String) -> Result<LatLngResult, String> {
    let trimmed = mgrs.trim();
    if trimmed.is_empty() {
        return Err("MGRS string is required.".to_string());
    }
    let parsed = Mgrs::parse_str(trimmed).map_err(|e| format!("Invalid MGRS: {e}"))?;
    let ll = parsed.to_latlon();
    let lat = ll.latitude();
    let lng = ll.longitude();
    if !lat.is_finite() || !lng.is_finite() {
        return Err("Converted coordinates are not finite.".to_string());
    }
    Ok(LatLngResult { lat, lng })
}

#[tauri::command]
pub async fn get_map_markers(state: tauri::State<'_, AppState>) -> Result<Vec<MapMarker>, String> {
    state.db.lock().map_err(|e| e.to_string())?.list_map_markers()
}

#[tauri::command]
pub async fn add_map_marker(
    state: tauri::State<'_, AppState>,
    lat: f64,
    lng: f64,
    title: String,
    description: String,
    category: String,
) -> Result<MapMarker, String> {
    if !lat.is_finite() || !lng.is_finite() {
        return Err("Latitude and longitude must be finite numbers.".to_string());
    }
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Marker title is required.".to_string());
    }
    let category = normalize_marker_category(&category);
    let marker = MapMarker {
        id: Uuid::new_v4().to_string(),
        lat,
        lng,
        title,
        description: description.trim().to_string(),
        created_at: Utc::now().to_rfc3339(),
        category,
    };
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .insert_map_marker(&marker)?;
    Ok(marker)
}

fn normalize_marker_category(raw: &str) -> String {
    let t = raw.trim().to_lowercase();
    match t.as_str() {
        "hazard" | "water" | "cache" | "med" => t,
        "" => "hazard".to_string(),
        _ => t,
    }
}

#[tauri::command]
pub async fn delete_map_marker(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("Marker id is required.".to_string());
    }
    state.db.lock().map_err(|e| e.to_string())?.delete_map_marker(&id)
}

#[tauri::command]
pub async fn save_drawing(
    state: tauri::State<'_, AppState>,
    feature_type: String,
    geometry_json: String,
    style_json: String,
    id: Option<String>,
) -> Result<MapDrawing, String> {
    let feature_type = feature_type.trim().to_string();
    if feature_type.is_empty() {
        return Err("feature_type is required.".to_string());
    }
    if feature_type != "polyline" && feature_type != "polygon" {
        return Err("feature_type must be 'polyline' or 'polygon'.".to_string());
    }
    let geometry_json = geometry_json.trim().to_string();
    if geometry_json.is_empty() {
        return Err("geometry_json is required.".to_string());
    }
    serde_json::from_str::<serde_json::Value>(&geometry_json).map_err(|e| format!("geometry_json must be valid JSON: {e}"))?;
    let style_json = style_json.trim().to_string();
    if style_json.is_empty() {
        return Err("style_json is required.".to_string());
    }
    serde_json::from_str::<serde_json::Value>(&style_json).map_err(|e| format!("style_json must be valid JSON: {e}"))?;
    let id = id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let drawing = MapDrawing {
        id,
        feature_type,
        geometry_json,
        style_json,
        created_at: Utc::now().to_rfc3339(),
    };
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .insert_map_drawing(&drawing)?;
    Ok(drawing)
}

#[tauri::command]
pub async fn get_drawings(state: tauri::State<'_, AppState>) -> Result<Vec<MapDrawing>, String> {
    state.db.lock().map_err(|e| e.to_string())?.list_map_drawings()
}

#[tauri::command]
pub async fn delete_drawing(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("Drawing id is required.".to_string());
    }
    state.db.lock().map_err(|e| e.to_string())?.delete_map_drawing(&id)
}

#[tauri::command]
pub async fn get_spatial_context(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let markers = state.db.lock().map_err(|e| e.to_string())?.list_map_markers()?;
    if markers.is_empty() {
        return Ok(String::new());
    }
    let mut lines: Vec<String> = Vec::with_capacity(markers.len());
    for marker in markers {
        let title = marker.title.trim();
        let desc = marker.description.trim();
        let cat = marker.category.trim();
        let line = if desc.is_empty() {
            format!(
                "Marker: {} [{}] at Lat {:.6}, Lng {:.6}. Description: (none).",
                title, cat, marker.lat, marker.lng
            )
        } else {
            format!(
                "Marker: {} [{}] at Lat {:.6}, Lng {:.6}. Description: {}.",
                title, cat, marker.lat, marker.lng, desc
            )
        };
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

#[tauri::command]
pub async fn broadcast_map_marker(
    id: String,
    state: tauri::State<'_, AppState>,
    radio_state: tauri::State<'_, RadioState>,
) -> Result<(), String> {
    let marker_id = id.trim().to_string();
    if marker_id.is_empty() {
        return Err("Marker id is required.".to_string());
    }
    let marker = state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .get_map_marker_by_id(&marker_id)?
        .ok_or_else(|| "Map marker not found.".to_string())?;
    let encoded_json = serde_json::to_string(&marker).map_err(|e| e.to_string())?;
    let encoded = format!("ATK:{encoded_json}");

    let mut guard = radio_state
        .tx_port
        .lock()
        .map_err(|e| format!("Radio TX state lock failed: {e}"))?;
    let Some(port) = guard.as_mut() else {
        return Err("No radio port connected.".to_string());
    };
    port.write_all(encoded.as_bytes())
        .map_err(|e| format!("Failed to write marker payload: {e}"))?;
    port.write_all(b"\n")
        .map_err(|e| format!("Failed to write marker payload newline: {e}"))?;
    port.flush()
        .map_err(|e| format!("Failed to flush marker payload: {e}"))?;
    Ok(())
}
