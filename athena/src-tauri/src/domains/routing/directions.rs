//! Resolve from/to text to coordinates and compute routes (Valhalla or straight-line fallback).

use serde::Serialize;
use crate::db::{Database, GeocodeMatch};
use crate::offline_geocoder;
use crate::portable;
use crate::valhalla_service::{valhalla_route_impl, ValhallaRouteArgs, ValhallaRouteResult};
use crate::AppState;

/// Top two results with importance within this ratio (second/first) require disambiguation.
const AMBIGUITY_CLOSE_RATIO: f64 = 0.92;

fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let la1 = lat1.to_radians();
    let la2 = lat2.to_radians();
    let a =
        (d_lat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    6371000.0 * c
}

fn norm_q(s: &str) -> String {
    s.trim().to_lowercase()
}

fn search_markers_local(
    db: &Database,
    q: &str,
) -> Result<Vec<GeocodeMatch>, String> {
    let nq = norm_q(q);
    if nq.is_empty() {
        return Ok(vec![]);
    }
    let ms = db.list_map_markers()?;
    let mut out: Vec<GeocodeMatch> = Vec::new();
    for m in ms {
        if norm_q(&m.title).contains(&nq) || norm_q(&m.description).contains(&nq) {
            out.push(GeocodeMatch {
                id: m.id.clone(),
                region_id: "marker".to_string(),
                feature_class: "marker".to_string(),
                label: m.title,
                lat: m.lat,
                lon: m.lng,
                importance: 1000.0,
            });
        }
    }
    out.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
    out.truncate(5);
    Ok(out)
}

fn search_pois_local(db: &Database, q: &str) -> Result<Vec<GeocodeMatch>, String> {
    let nq = norm_q(q);
    if nq.is_empty() {
        return Ok(vec![]);
    }
    let ps = db.list_tactical_points_of_interest()?;
    let mut out: Vec<GeocodeMatch> = Vec::new();
    for p in ps {
        if norm_q(&p.name).contains(&nq) || norm_q(&p.notes).contains(&nq) {
            out.push(GeocodeMatch {
                id: p.id.clone(),
                region_id: "poi".to_string(),
                feature_class: "poi".to_string(),
                label: p.name,
                lat: p.lat,
                lon: p.lon,
                importance: 900.0,
            });
        }
    }
    out.truncate(5);
    Ok(out)
}

/// Markers → POIs → offline geocode FTS.
fn resolve_endpoint(
    db: &Database,
    text: &str,
    active_region: Option<&str>,
) -> Result<Vec<GeocodeMatch>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(vec![]);
    }
    let m = search_markers_local(db, t)?;
    if !m.is_empty() {
        return Ok(m);
    }
    let p = search_pois_local(db, t)?;
    if !p.is_empty() {
        return Ok(p);
    }
    let fts = match offline_geocoder::user_query_to_fts_match(t) {
        Some(f) => f,
        None => return Ok(vec![]),
    };
    let mut hits = db.geocode_search_fts(&fts, active_region, 8)?;
    if hits.is_empty() {
        hits = db.geocode_search_fts(&fts, None, 8)?;
    }
    Ok(hits)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EndpointResolution {
    pub query: String,
    pub selected: Option<GeocodeMatch>,
    pub candidates: Vec<GeocodeMatch>,
    pub ambiguous: bool,
}

fn pick_endpoint(res: Vec<GeocodeMatch>, query: &str) -> EndpointResolution {
    if res.is_empty() {
        return EndpointResolution {
            query: query.to_string(),
            selected: None,
            candidates: vec![],
            ambiguous: false,
        };
    }
    if res.len() == 1 {
        let a = res[0].clone();
        return EndpointResolution {
            query: query.to_string(),
            selected: Some(a.clone()),
            candidates: vec![a],
            ambiguous: false,
        };
    }
    let first = res[0].importance.max(0.01);
    let second = res[1].importance;
    let amb = second / first >= AMBIGUITY_CLOSE_RATIO;
    EndpointResolution {
        query: query.to_string(),
        selected: if amb { None } else { Some(res[0].clone()) },
        candidates: res,
        ambiguous: amb,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectionsResolveResult {
    pub ambiguous: bool,
    pub from: Option<EndpointResolution>,
    pub to: Option<EndpointResolution>,
    pub route: Option<ValhallaRouteResult>,
    pub haversine_meters: f64,
    pub from_label: Option<String>,
    pub to_label: Option<String>,
    pub error: Option<String>,
}

/// Plan-level directions: markers → POIs → offline geocode → `valhalla_route`.
pub fn directions_resolve_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    from: String,
    to: String,
    mode: String,
) -> Result<DirectionsResolveResult, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let cfg = crate::config::load_config(&app_data);
    let active = cfg
        .active_routing_region_id
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let from_res = resolve_endpoint(&db, &from, active)?;
    let to_res = resolve_endpoint(&db, &to, active)?;
    drop(db);

    let from_ep = pick_endpoint(from_res, &from);
    let to_ep = pick_endpoint(to_res, &to);
    if from_ep.ambiguous || to_ep.ambiguous {
        return Ok(DirectionsResolveResult {
            ambiguous: true,
            from: Some(from_ep),
            to: Some(to_ep),
            route: None,
            haversine_meters: 0.0,
            from_label: None,
            to_label: None,
            error: None,
        });
    }
    let Some(fs) = from_ep.selected.clone() else {
        return Ok(DirectionsResolveResult {
            ambiguous: false,
            from: Some(from_ep),
            to: Some(to_ep),
            route: None,
            haversine_meters: 0.0,
            from_label: None,
            to_label: None,
            error: Some(
                "Could not resolve start location. Register a routing region, build the geocode index from PBF, or add a map marker."
                    .to_string(),
            ),
        });
    };
    let Some(ts) = to_ep.selected.clone() else {
        return Ok(DirectionsResolveResult {
            ambiguous: false,
            from: Some(from_ep),
            to: Some(to_ep),
            route: None,
            haversine_meters: 0.0,
            from_label: None,
            to_label: None,
            error: Some(
                "Could not resolve destination. Register a routing region, build the geocode index from PBF, or add a map marker."
                    .to_string(),
            ),
        });
    };
    let h = haversine_meters(fs.lat, fs.lon, ts.lat, ts.lon);
    let args = ValhallaRouteArgs {
        lat1: fs.lat,
        lon1: fs.lon,
        lat2: ts.lat,
        lon2: ts.lon,
        mode: mode.clone(),
    };
    let enemy_territories = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.list_territories_by_status("enemy")?
    };
    let route = valhalla_route_impl(app, state, args, enemy_territories).ok();
    Ok(DirectionsResolveResult {
        ambiguous: false,
        from: Some(from_ep),
        to: Some(to_ep),
        route,
        haversine_meters: h,
        from_label: Some(fs.label.clone()),
        to_label: Some(ts.label.clone()),
        error: None,
    })
}
