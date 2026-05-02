//! Build and query offline place / address index from an OSM PBF extract (per routing region).

use std::collections::HashMap;
use std::path::Path;

use osmpbfreader::{objects::OsmObj, NodeId, OsmPbfReader};

use crate::db::Database;

const MAX_FEATURES_PER_REGION: usize = 500_000;

/// Stats returned after building an index from PBF.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeocodeBuildStats {
    pub region_id: String,
    pub features_inserted: usize,
    pub pbf_path: String,
}

/// Turn a user search string into an FTS5 `MATCH` expression (`"token"*` AND ...).
pub fn user_query_to_fts_match(q: &str) -> Option<String> {
    let parts: Vec<String> = q
        .split_whitespace()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.replace('\"', ""))
        .filter(|s| !s.is_empty())
        .map(|s| {
            let t = s
                .chars()
                .filter(|c| !c.is_control())
                .collect::<String>();
            if t.is_empty() {
                return None;
            }
            Some(format!("\"{}\"*", t))
        })
        .filter_map(|x| x)
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" AND "))
}

fn place_importance(place: &str) -> f64 {
    match place {
        "city" | "metropolis" => 100.0,
        "town" => 85.0,
        "village" => 70.0,
        "hamlet" | "isolated_dwelling" => 55.0,
        "suburb" | "neighbourhood" | "quarter" => 50.0,
        "locality" => 45.0,
        "lake" | "reservoir" => 62.0,
        "island" => 48.0,
        "farm" => 35.0,
        _ => 40.0,
    }
}

fn has_interesting_poi_key(tags: &osmpbfreader::objects::Tags) -> bool {
    tags.get("amenity").is_some()
        || tags.get("tourism").is_some()
        || tags.get("leisure").is_some()
        || tags.get("shop").is_some()
        || tags.get("office").is_some()
        || tags.get("man_made").is_some()
        || tags.get("historic").is_some()
        || tags.get("aeroway").is_some()
        || tags.get("railway").is_some()
        || tags
            .get("natural")
            .map(|v| {
                let s = v.as_str();
                s == "water" || s == "peak" || s == "wood"
            })
            .unwrap_or(false)
        || tags
            .get("waterway")
            .map(|v| {
                let s = v.as_str();
                s == "river" || s == "stream" || s == "canal"
            })
            .unwrap_or(false)
        || tags
            .get("highway")
            .map(|v| {
                let s = v.as_str();
                matches!(
                    s,
                    "path" | "footway" | "track" | "bridleway" | "cycleway" | "steps"
                )
            })
            .unwrap_or(false)
}

fn collect_alt_names(tags: &osmpbfreader::objects::Tags) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (k, v) in tags.iter() {
        if k == "name" {
            continue;
        }
        if k.starts_with("name:") || k == "alt_name" || k == "old_name" {
            if !v.is_empty() {
                parts.push(v.to_string());
            }
        }
    }
    parts.join(" ")
}

fn build_address_str(tags: &osmpbfreader::objects::Tags) -> Option<String> {
    let num = tags.get("addr:housenumber")?;
    let street = tags.get("addr:street")?;
    let mut s = format!("{num} {street}");
    if let Some(c) = tags.get("addr:city") {
        s.push_str(", ");
        s.push_str(c.as_str());
    }
    if let Some(st) = tags.get("addr:state") {
        s.push(' ');
        s.push_str(st.as_str());
    }
    if let Some(z) = tags.get("addr:postcode") {
        s.push(' ');
        s.push_str(z.as_str());
    }
    Some(s)
}

fn do_insert(
    db: &Database,
    id: &str,
    region_id: &str,
    feature_class: &str,
    primary_name: Option<&str>,
    alts: &str,
    address: Option<&str>,
    city: Option<&str>,
    state: Option<&str>,
    postal: Option<&str>,
    country: Option<&str>,
    lat: f64,
    lon: f64,
    imp: f64,
) -> Result<(), String> {
    db.geocode_insert(
        id,
        region_id,
        feature_class,
        primary_name,
        if alts.is_empty() { None } else { Some(alts) },
        address,
        city,
        state,
        postal,
        country,
        lat,
        lon,
        imp,
    )
}

/// Stream PBF twice: node coordinates, then features with names / addresses.
pub fn build_index_from_pbf(
    db: &Database,
    region_id: &str,
    pbf_path: &Path,
) -> Result<GeocodeBuildStats, String> {
    let rid = region_id.trim();
    if rid.is_empty() {
        return Err("region_id required".to_string());
    }
    if !pbf_path.is_file() {
        return Err(format!("PBF not found: {}", pbf_path.display()));
    }

    db.geocode_delete_for_region(rid)?;

    // Pass 1: all node positions
    let f = std::fs::File::open(pbf_path).map_err(|e| e.to_string())?;
    let mut reader = OsmPbfReader::new(f);
    let mut nodes: HashMap<i64, (f64, f64)> = HashMap::new();
    for obj in reader.iter() {
        let obj = obj.map_err(|e| e.to_string())?;
        if let OsmObj::Node(n) = obj {
            let NodeId(nid) = n.id;
            nodes.insert(nid, (n.lat(), n.lon()));
        }
    }

    // Pass 2: extract features
    let f2 = std::fs::File::open(pbf_path).map_err(|e| e.to_string())?;
    let mut reader2 = OsmPbfReader::new(f2);
    let mut inserted = 0usize;

    for obj in reader2.iter() {
        if inserted >= MAX_FEATURES_PER_REGION {
            break;
        }
        let obj = obj.map_err(|e| e.to_string())?;
        match obj {
            OsmObj::Node(n) => {
                let tags = &n.tags;
                let lat = n.lat();
                let lon = n.lon();
                let alts = collect_alt_names(tags);

                if let (Some(place), Some(nm)) = (tags.get("place"), tags.get("name")) {
                    if nm.is_empty() {
                        continue;
                    }
                    let id = format!("n_{}", uuid::Uuid::new_v4());
                    let imp = place_importance(place.as_str());
                    let city = tags.get("addr:city").map(|s| s.to_string());
                    let state = tags.get("addr:state").map(|s| s.to_string());
                    let postal = tags.get("addr:postcode").map(|s| s.to_string());
                    let country = tags.get("addr:country").map(|s| s.to_string());
                    do_insert(
                        db,
                        &id,
                        rid,
                        "place",
                        Some(nm.as_str()),
                        &alts,
                        None,
                        city.as_deref(),
                        state.as_deref(),
                        postal.as_deref(),
                        country.as_deref(),
                        lat,
                        lon,
                        imp,
                    )?;
                    inserted += 1;
                    continue;
                }

                if tags.get("addr:housenumber").is_some() && tags.get("addr:street").is_some() {
                    let Some(addr) = build_address_str(tags) else {
                        continue;
                    };
                    let primary = tags
                        .get("name")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| addr.clone());
                    let id = format!("addr_n_{}", uuid::Uuid::new_v4());
                    let city = tags.get("addr:city").map(|s| s.to_string());
                    let state = tags.get("addr:state").map(|s| s.to_string());
                    let postal = tags.get("addr:postcode").map(|s| s.to_string());
                    let country = tags.get("addr:country").map(|s| s.to_string());
                    do_insert(
                        db,
                        &id,
                        rid,
                        "address",
                        Some(primary.as_str()),
                        &alts,
                        Some(addr.as_str()),
                        city.as_deref(),
                        state.as_deref(),
                        postal.as_deref(),
                        country.as_deref(),
                        lat,
                        lon,
                        75.0,
                    )?;
                    inserted += 1;
                    continue;
                }

                let Some(nm) = tags.get("name") else {
                    continue;
                };
                if nm.is_empty() || !has_interesting_poi_key(tags) {
                    continue;
                }
                let id = format!("poi_n_{}", uuid::Uuid::new_v4());
                let city = tags.get("addr:city").map(|s| s.to_string());
                let state = tags.get("addr:state").map(|s| s.to_string());
                let postal = tags.get("addr:postcode").map(|s| s.to_string());
                let country = tags.get("addr:country").map(|s| s.to_string());
                do_insert(
                    db,
                    &id,
                    rid,
                    "poi",
                    Some(nm.as_str()),
                    &alts,
                    None,
                    city.as_deref(),
                    state.as_deref(),
                    postal.as_deref(),
                    country.as_deref(),
                    lat,
                    lon,
                    42.0,
                )?;
                inserted += 1;
            }
            OsmObj::Way(w) => {
                let tags = &w.tags;
                let Some(nm) = tags.get("name") else {
                    continue;
                };
                if nm.is_empty() {
                    continue;
                }
                let alts = collect_alt_names(tags);
                let mut sum_lat = 0.0f64;
                let mut sum_lon = 0.0f64;
                let mut cnt = 0.0f64;
                for nid in &w.nodes {
                    let NodeId(id) = *nid;
                    if let Some((la, lo)) = nodes.get(&id) {
                        sum_lat += *la;
                        sum_lon += *lo;
                        cnt += 1.0;
                    }
                }
                if cnt < 1.0 {
                    continue;
                }
                let lat = sum_lat / cnt;
                let lon = sum_lon / cnt;

                if let Some(place) = tags.get("place") {
                    let id = format!("w_{}", uuid::Uuid::new_v4());
                    let imp = place_importance(place.as_str());
                    let city = tags.get("addr:city").map(|s| s.to_string());
                    let state = tags.get("addr:state").map(|s| s.to_string());
                    let postal = tags.get("addr:postcode").map(|s| s.to_string());
                    let country = tags.get("addr:country").map(|s| s.to_string());
                    do_insert(
                        db,
                        &id,
                        rid,
                        "place",
                        Some(nm.as_str()),
                        &alts,
                        None,
                        city.as_deref(),
                        state.as_deref(),
                        postal.as_deref(),
                        country.as_deref(),
                        lat,
                        lon,
                        imp,
                    )?;
                    inserted += 1;
                    continue;
                }

                if tags.get("addr:housenumber").is_some() && tags.get("addr:street").is_some() {
                    let Some(addr) = build_address_str(tags) else {
                        continue;
                    };
                    let primary = tags
                        .get("name")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| addr.clone());
                    let id = format!("addr_w_{}", uuid::Uuid::new_v4());
                    let city = tags.get("addr:city").map(|s| s.to_string());
                    let state = tags.get("addr:state").map(|s| s.to_string());
                    let postal = tags.get("addr:postcode").map(|s| s.to_string());
                    let country = tags.get("addr:country").map(|s| s.to_string());
                    do_insert(
                        db,
                        &id,
                        rid,
                        "address",
                        Some(primary.as_str()),
                        &alts,
                        Some(addr.as_str()),
                        city.as_deref(),
                        state.as_deref(),
                        postal.as_deref(),
                        country.as_deref(),
                        lat,
                        lon,
                        72.0,
                    )?;
                    inserted += 1;
                    continue;
                }

                let is_trail = tags
                    .get("highway")
                    .map(|v| {
                        let s = v.as_str();
                        matches!(s, "path" | "footway" | "track" | "bridleway" | "cycleway")
                    })
                    .unwrap_or(false);
                if !has_interesting_poi_key(tags) && !is_trail {
                    continue;
                }
                let class = if is_trail { "trail" } else { "poi" };
                let id = format!("way_{}", uuid::Uuid::new_v4());
                let city = tags.get("addr:city").map(|s| s.to_string());
                let state = tags.get("addr:state").map(|s| s.to_string());
                let postal = tags.get("addr:postcode").map(|s| s.to_string());
                let country = tags.get("addr:country").map(|s| s.to_string());
                do_insert(
                    db,
                    &id,
                    rid,
                    class,
                    Some(nm.as_str()),
                    &alts,
                    None,
                    city.as_deref(),
                    state.as_deref(),
                    postal.as_deref(),
                    country.as_deref(),
                    lat,
                    lon,
                    if is_trail { 36.0 } else { 38.0 },
                )?;
                inserted += 1;
            }
            OsmObj::Relation(_) => {}
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    db.set_routing_region_geocode_built(rid, &now)?;

    Ok(GeocodeBuildStats {
        region_id: rid.to_string(),
        features_inserted: inserted,
        pbf_path: pbf_path.display().to_string(),
    })
}
