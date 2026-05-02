//! US library-installer map selection: geocode pin, home state + nearby border states (Protomaps extracts).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::config::LibraryInstallerLocation;

const MAP_JSON: &str = include_str!("../../../resources/us_states_library_maps.json");
const PROTOMAPS_BASE: &str = "https://data.protomaps.com/extracts";
/// Include a bordering state if within this distance of the midpoint between home and neighbor centroids (meters).
const MIDPOINT_MAX_M: f64 = 155_000.0;
/// Include a bordering state if the user is this close to that state's centroid (cross-metro areas).
const NEIGHBOR_CENTROID_MAX_M: f64 = 125_000.0;

#[derive(Debug, Clone)]
pub struct MapOffer {
    pub display_name: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Dataset {
    states: HashMap<String, StateRow>,
    neighbors: HashMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct StateRow {
    slug: String,
    display: String,
    lat: f64,
    lon: f64,
}

fn dataset() -> &'static Dataset {
    static CACHE: OnceLock<Dataset> = OnceLock::new();
    CACHE.get_or_init(|| serde_json::from_str(MAP_JSON).expect("bundled us_states_library_maps.json"))
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371000.0_f64;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn resolve_state_codes(lat: f64, lon: f64, home: &str, data: &Dataset) -> Result<Vec<String>, String> {
    let home_u = home.trim().to_uppercase();
    let Some(hc) = data.states.get(&home_u) else {
        return Err(format!("Unknown US state/territory code: {home_u}"));
    };
    let mut out = vec![home_u.clone()];
    let neighbors = data.neighbors.get(&home_u).cloned().unwrap_or_default();
    for n_code in neighbors {
        let Some(nc) = data.states.get(&n_code) else {
            continue;
        };
        let mid_lat = (hc.lat + nc.lat) / 2.0;
        let mid_lon = (hc.lon + nc.lon) / 2.0;
        let d_mid = haversine_m(lat, lon, mid_lat, mid_lon);
        let d_nc = haversine_m(lat, lon, nc.lat, nc.lon);
        if d_mid <= MIDPOINT_MAX_M || d_nc <= NEIGHBOR_CENTROID_MAX_M {
            if !out.contains(&n_code) {
                out.push(n_code);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Build Protomaps `.pmtiles` URLs for the Satchel tier from a saved installer location.
pub fn satchel_map_offers(loc: &LibraryInstallerLocation) -> Result<Vec<MapOffer>, String> {
    let data = dataset();
    let codes = resolve_state_codes(loc.lat, loc.lon, &loc.state_code, data)?;
    // Protomaps has no small DC extract in this naming scheme; MD + VA (neighbors) cover the metro.
    let mut rows: Vec<(String, String)> = codes
        .into_iter()
        .filter(|c| c != "DC")
        .filter_map(|code| {
            let s = data.states.get(&code)?;
            Some((s.display.clone(), format!("{PROTOMAPS_BASE}/{}.pmtiles", s.slug)))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows
        .into_iter()
        .map(|(display_name, url)| MapOffer { display_name, url })
        .collect())
}

/// Offline default: Manhattan / NYC coordinates, NY home state (no network).
pub fn default_nyc_location() -> LibraryInstallerLocation {
    LibraryInstallerLocation {
        query: "New York, NY".to_string(),
        display_name: "New York City (default area)".to_string(),
        lat: 40.7127753,
        lon: -74.0059728,
        state_code: "NY".to_string(),
    }
}

/// Forward geocode a US place query via Nominatim (OpenStreetMap). Requires network.
pub async fn geocode_us_place(query: &str) -> Result<LibraryInstallerLocation, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Enter a city, address, or ZIP in the United States.".to_string());
    }
    let encoded = urlencoding::encode(q);
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={encoded}&format=json&limit=1&addressdetails=1&countrycodes=us"
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent("AthenaLibraryInstaller/1.0 (local-first education app)")
        .build()
        .map_err(|e| e.to_string())?;

    let text = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let first = v
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| "No results. Try a more specific US address or city.".to_string())?;

    let lat = parse_coord(&first["lat"]).ok_or_else(|| "Geocoder returned no latitude.".to_string())?;
    let lon = parse_coord(&first["lon"]).ok_or_else(|| "Geocoder returned no longitude.".to_string())?;

    let display_name = first["display_name"]
        .as_str()
        .unwrap_or(q)
        .to_string();

    let state_code = extract_state_code(first).ok_or_else(|| {
        "Could not determine a US state from that place. Try adding the state (e.g. \"Austin, TX\").".to_string()
    })?;

    let data = dataset();
    if data.states.get(&state_code).is_none() {
        return Err(format!("State {state_code} is not supported for regional map downloads."));
    }

    Ok(LibraryInstallerLocation {
        query: q.to_string(),
        display_name,
        lat,
        lon,
        state_code,
    })
}

fn parse_coord(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn extract_state_code(hit: &serde_json::Value) -> Option<String> {
    if let Some(iso) = hit["address"]["ISO3166-2-lvl4"].as_str() {
        let iso = iso.trim();
        if let Some(rest) = iso.strip_prefix("US-") {
            let code = rest.to_uppercase();
            if code.len() == 2 {
                return Some(code);
            }
        }
    }
    if let Some(st) = hit["address"]["state"].as_str() {
        return state_name_to_code(st);
    }
    None
}

fn state_name_to_code(name: &str) -> Option<String> {
    let n = name.trim().to_lowercase();
    match n.as_str() {
        "alabama" => Some("AL".into()),
        "alaska" => Some("AK".into()),
        "arizona" => Some("AZ".into()),
        "arkansas" => Some("AR".into()),
        "california" => Some("CA".into()),
        "colorado" => Some("CO".into()),
        "connecticut" => Some("CT".into()),
        "delaware" => Some("DE".into()),
        "florida" => Some("FL".into()),
        "georgia" => Some("GA".into()),
        "hawaii" => Some("HI".into()),
        "idaho" => Some("ID".into()),
        "illinois" => Some("IL".into()),
        "indiana" => Some("IN".into()),
        "iowa" => Some("IA".into()),
        "kansas" => Some("KS".into()),
        "kentucky" => Some("KY".into()),
        "louisiana" => Some("LA".into()),
        "maine" => Some("ME".into()),
        "maryland" => Some("MD".into()),
        "massachusetts" => Some("MA".into()),
        "michigan" => Some("MI".into()),
        "minnesota" => Some("MN".into()),
        "mississippi" => Some("MS".into()),
        "missouri" => Some("MO".into()),
        "montana" => Some("MT".into()),
        "nebraska" => Some("NE".into()),
        "nevada" => Some("NV".into()),
        "new hampshire" => Some("NH".into()),
        "new jersey" => Some("NJ".into()),
        "new mexico" => Some("NM".into()),
        "new york" => Some("NY".into()),
        "north carolina" => Some("NC".into()),
        "north dakota" => Some("ND".into()),
        "ohio" => Some("OH".into()),
        "oklahoma" => Some("OK".into()),
        "oregon" => Some("OR".into()),
        "pennsylvania" => Some("PA".into()),
        "rhode island" => Some("RI".into()),
        "south carolina" => Some("SC".into()),
        "south dakota" => Some("SD".into()),
        "tennessee" => Some("TN".into()),
        "texas" => Some("TX".into()),
        "utah" => Some("UT".into()),
        "vermont" => Some("VT".into()),
        "virginia" => Some("VA".into()),
        "washington" => Some("WA".into()),
        "west virginia" => Some("WV".into()),
        "wisconsin" => Some("WI".into()),
        "wyoming" => Some("WY".into()),
        "district of columbia" => Some("DC".into()),
        _ => None,
    }
}
