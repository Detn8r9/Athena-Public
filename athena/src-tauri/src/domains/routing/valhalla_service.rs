//! Valhalla HTTP routing (optional). Falls back to straight-line when the service is not running.
//! Expects a local `valhalla` HTTP service; tile build is tracked in `routing_regions` + app config.

use serde::Serialize;
use tauri::State;
use ureq::Agent;

use crate::auth::{authenticated_user, AuthSession};
use crate::config::{load_config, save_config};
use crate::db::{RouteBlockageRow, TerritoryRow};
use crate::offline_routing;
use crate::portable;
use crate::AppState;
use tauri::AppHandle;

const DEFAULT_VALHALLA_PORT: u16 = 8002;

fn valhalla_base(app: &AppHandle) -> Result<String, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let cfg = load_config(&app_data);
    let port = cfg.valhalla_port.unwrap_or(DEFAULT_VALHALLA_PORT);
    Ok(format!("http://127.0.0.1:{port}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValhallaStatus {
    pub reachable: bool,
    pub message: String,
    pub active_routing_region_id: Option<String>,
}

#[tauri::command]
pub fn valhalla_get_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ValhallaStatus, String> {
    let _ = state;
    let base = valhalla_base(&app)?;
    let url = format!("{base}/status");
    let agent = Agent::new();
    let res = agent.get(&url).timeout(std::time::Duration::from_secs(2)).call();
    let app_data = portable::resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data);
    let active = cfg.active_routing_region_id.clone();
    match res {
        Ok(r) if (200..300).contains(&r.status()) => Ok(ValhallaStatus {
            reachable: true,
            message: "valhalla_service responded".to_string(),
            active_routing_region_id: active,
        }),
        Ok(r) => Ok(ValhallaStatus {
            reachable: false,
            message: format!("HTTP {}", r.status()),
            active_routing_region_id: active,
        }),
        Err(e) => Ok(ValhallaStatus {
            reachable: false,
            message: format!("{e}"),
            active_routing_region_id: active,
        }),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValhallaRouteArgs {
    pub lat1: f64,
    pub lon1: f64,
    pub lat2: f64,
    pub lon2: f64,
    /// `walk`, `drive`, `bike`, `offroad` — mapped to Valhalla costing or placeholder.
    pub mode: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValhallaRouteResult {
    pub geojson: serde_json::Value,
    pub mode: String,
    pub source: String,
    /// meters (approximate) when from straight-line fallback
    pub distance_meters: Option<f64>,
    pub duration_seconds: Option<f64>,
    /// Turn-by-turn from Valhalla (first leg) when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuvers: Option<serde_json::Value>,
}

/// Build a minimal Valhalla `/route` JSON body.
fn valhalla_route_request_body(
    args: &ValhallaRouteArgs,
    blockages: &[RouteBlockageRow],
    enemy_territories: Vec<TerritoryRow>,
) -> serde_json::Value {
    let mode = args.mode.to_lowercase();
    let (costing, costing_options) = match mode.as_str() {
        "walk" => ("pedestrian", None),
        "offroad" => (
            "pedestrian",
            Some(serde_json::json!({
                "pedestrian": { "use_tracks": 1.0, "use_hills": 0.5, "use_lit": 0.0 }
            })),
        ),
        "bike" | "bicycle" => ("bicycle", None),
        _ => ("auto", None),
    };
    let mut avoid_polys: Vec<serde_json::Value> = blockages
        .iter()
        .filter(|b| b.status == "active")
        .filter_map(|b| serde_json::from_str(&b.geometry_json).ok())
        .collect();
    let enemy_polys: Vec<serde_json::Value> = enemy_territories
        .into_iter()
        .filter_map(|t| serde_json::from_str(&t.geometry_json).ok())
        .collect();
    avoid_polys.extend(enemy_polys);
    let mut body = serde_json::json!({
        "locations": [
            { "lat": args.lat1, "lon": args.lon1 },
            { "lat": args.lat2, "lon": args.lon2 }
        ],
        "costing": costing,
        "directions_options": { "units": "kilometers" }
    });
    if let Some(opts) = costing_options {
        body["costing_options"] = opts;
    }
    if !avoid_polys.is_empty() {
        body["exclude_polygons"] = serde_json::Value::Array(avoid_polys);
    }
    body
}

fn valhalla_trip_to_feature_geojson(trip: &serde_json::Value) -> Option<serde_json::Value> {
    let leg0 = trip.get("legs")?.as_array()?.first()?.clone();
    let shape = leg0.get("shape")?.as_str()?;
    let coords = polyline::decode_polyline(shape.to_string(), 6).ok()?;
    if coords.is_empty() {
        return None;
    }
    // Decoded pairs are [lat, lon] (common polyline convention); GeoJSON needs [lon, lat].
    let line: Vec<Vec<f64>> = coords.iter().map(|p| vec![p[1], p[0]]).collect();
    Some(serde_json::json!({
        "type": "Feature",
        "properties": { "kind": "valhalla_route" },
        "geometry": { "type": "LineString", "coordinates": line }
    }))
}

pub fn valhalla_route_impl(
    app: &AppHandle,
    state: &AppState,
    args: ValhallaRouteArgs,
    enemy_territories: Vec<TerritoryRow>,
) -> Result<ValhallaRouteResult, String> {
    if !(args.lat1.is_finite()
        && args.lon1.is_finite()
        && args.lat2.is_finite()
        && args.lon2.is_finite())
    {
        return Err("Coordinates must be finite.".to_string());
    }
    let blockages = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let mut rows = db.list_route_blockages_routing()?;
        rows.retain(|b| b.status == "active");
        let mode = args.mode.to_lowercase();
        let blockages: Vec<_> = rows
            .into_iter()
            .filter(|b| {
                let m = b.modes_affected.to_lowercase();
                m.contains(&mode) || m.split(',').any(|x| x.trim() == &mode)
            })
            .collect();
        blockages
    };
    let base = valhalla_base(app)?;
    let body = valhalla_route_request_body(&args, &blockages, enemy_territories);
    let url = format!("{base}/route");
    let agent = Agent::new();
    let res = agent
        .post(&url)
        .set("Content-Type", "application/json")
        .send_json(&body);
    match res {
        Ok(r) if (200..300).contains(&r.status()) => {
            let v: serde_json::Value = r
                .into_json()
                .map_err(|e| format!("Valhalla JSON: {e}"))?;
            let trip = v.get("trip").cloned().unwrap_or(serde_json::Value::Null);
            let dist_m = trip
                .get("summary")
                .and_then(|s| s.get("length"))
                .and_then(|x| x.as_f64())
                .map(|km| km * 1000.0);
            let dur_s = trip
                .get("summary")
                .and_then(|s| s.get("time"))
                .and_then(|x| x.as_f64());
            let maneuvers = trip
                .get("legs")
                .and_then(|l| l.as_array())
                .and_then(|a| a.first())
                .and_then(|leg| leg.get("maneuvers"))
                .cloned();
            let geojson = valhalla_trip_to_feature_geojson(&trip)
                .unwrap_or_else(|| trip.clone());
            Ok(ValhallaRouteResult {
                geojson,
                mode: args.mode,
                source: "valhalla".to_string(),
                distance_meters: dist_m,
                duration_seconds: dur_s,
                maneuvers,
            })
        }
        _ => {
            let fallback = offline_routing::straight_line_route_geojson(
                args.lat1, args.lon1, args.lat2, args.lon2,
            );
            let d_lat = (args.lat2 - args.lat1).to_radians();
            let d_lon = (args.lon2 - args.lon1).to_radians();
            let lat1 = args.lat1.to_radians();
            let lat2 = args.lat2.to_radians();
            let a =
                (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon / 2.0).sin().powi(2);
            let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
            let m = 6371000.0 * c;
            Ok(ValhallaRouteResult {
                geojson: fallback.geojson,
                mode: args.mode,
                source: "straight_line_fallback".to_string(),
                distance_meters: Some(m),
                duration_seconds: None,
                maneuvers: None,
            })
        }
    }
}

#[tauri::command]
pub fn valhalla_route(
    app: AppHandle,
    state: State<'_, AppState>,
    args: ValhallaRouteArgs,
) -> Result<ValhallaRouteResult, String> {
    let enemy_territories = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.list_territories_by_status("enemy")?
    };
    valhalla_route_impl(&app, &state, args, enemy_territories)
}

/// Offline-first route between two coordinates: local Valhalla when available, otherwise straight-line GeoJSON.
/// Same engine as [`valhalla_route`], exposed for map UI with flat coordinate arguments.
#[tauri::command]
pub fn map_offline_route_geojson(
    app: AppHandle,
    state: State<'_, AppState>,
    lat1: f64,
    lon1: f64,
    lat2: f64,
    lon2: f64,
    mode: Option<String>,
) -> Result<ValhallaRouteResult, String> {
    let mode_str = mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("drive")
        .to_string();
    let args = ValhallaRouteArgs {
        lat1,
        lon1,
        lat2,
        lon2,
        mode: mode_str,
    };
    let enemy_territories = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.list_territories_by_status("enemy")?
    };
    valhalla_route_impl(&app, &state, args, enemy_territories)
}

#[tauri::command]
pub fn valhalla_list_regions(state: State<'_, AppState>) -> Result<Vec<crate::db::RoutingRegionRow>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_routing_regions()
}

#[tauri::command]
pub fn valhalla_set_active_region(
    app: AppHandle,
    region_id: Option<String>,
) -> Result<(), String> {
    let app_data = portable::resolved_app_data_dir(&app)?;
    let mut cfg = load_config(&app_data);
    let rid = region_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    cfg.active_routing_region_id = rid;
    save_config(&app_data, &cfg)
}

#[tauri::command]
pub fn valhalla_register_routing_region(
    app: AppHandle,
    state: State<'_, AppState>,
    session: State<'_, AuthSession>,
    name: String,
    tiles_path: String,
    bbox_json: String,
    build_status: String,
) -> Result<crate::db::RoutingRegionRow, String> {
    let _u = authenticated_user(&session)?;
    let n = name.trim();
    if n.is_empty() {
        return Err("name required".to_string());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let row = crate::db::RoutingRegionRow {
        id: id.clone(),
        name: n.to_string(),
        bbox_json: if bbox_json.trim().is_empty() {
            "{}".to_string()
        } else {
            bbox_json
        },
        source_pbf_path: None,
        tiles_path: tiles_path.trim().to_string(),
        build_status: if build_status.trim().is_empty() {
            "ready".to_string()
        } else {
            build_status.trim().to_string()
        },
        modes_built: "auto,pedestrian,bicycle".to_string(),
        size_bytes: 0,
        created_at: now,
        geocode_built_at: None,
    };
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.upsert_routing_region(&row)?;
    }
    let _ = app;
    Ok(row)
}
