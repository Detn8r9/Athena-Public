//! Build and query offline place / address index from an OSM PBF extract (per routing region).

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

/// One searchable place, address or point of interest, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct GeocodeFeature {
    pub id: String,
    pub class: &'static str,
    pub name: String,
    pub alts: String,
    pub address: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal: Option<String>,
    pub country: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub importance: f64,
}

/// What a tagged OSM object is in the index, before its position is known. `None`: not indexed.
fn classify(tags: &osmpbfreader::objects::Tags, is_way: bool) -> Option<(&'static str, String, Option<String>, f64)> {
    let name = tags.get("name").map(|s| s.to_string()).filter(|s| !s.is_empty());
    if let (Some(place), Some(nm)) = (tags.get("place"), name.as_ref()) {
        return Some(("place", nm.clone(), None, place_importance(place.as_str())));
    }
    if tags.get("addr:housenumber").is_some() && tags.get("addr:street").is_some() {
        let addr = build_address_str(tags)?;
        let primary = name.clone().unwrap_or_else(|| addr.clone());
        return Some(("address", primary, Some(addr), if is_way { 72.0 } else { 75.0 }));
    }
    let nm = name?;
    if is_way {
        let is_trail = tags
            .get("highway")
            .is_some_and(|v| matches!(v.as_str(), "path" | "footway" | "track" | "bridleway" | "cycleway"));
        if !has_interesting_poi_key(tags) && !is_trail {
            return None;
        }
        Some(if is_trail { ("trail", nm, None, 36.0) } else { ("poi", nm, None, 38.0) })
    } else if has_interesting_poi_key(tags) {
        Some(("poi", nm, None, 42.0))
    } else {
        None
    }
}

fn make_feature(
    prefix: &str,
    tags: &osmpbfreader::objects::Tags,
    (class, name, address, importance): (&'static str, String, Option<String>, f64),
    lat: f64,
    lon: f64,
) -> GeocodeFeature {
    let get = |k: &str| tags.get(k).map(|s| s.to_string());
    let p = match (class, prefix) {
        ("address", "n") => "addr_n",
        ("address", _) => "addr_w",
        ("poi", "n") => "poi_n",
        ("place", "n") => "n",
        ("place", _) => "w",
        _ => "way",
    };
    GeocodeFeature {
        id: format!("{p}_{}", uuid::Uuid::new_v4()),
        class,
        name,
        alts: collect_alt_names(tags),
        address,
        city: get("addr:city"),
        state: get("addr:state"),
        postal: get("addr:postcode"),
        country: get("addr:country"),
        lat,
        lon,
        importance,
    }
}

/// Read an extract into index entries without touching the database, so the station stays usable
/// while a large extract is read. Two passes, keeping positions only for nodes of indexed ways
/// (not every node in the extract).
pub fn collect_features_from_pbf(pbf_path: &Path) -> Result<Vec<GeocodeFeature>, String> {
    if !pbf_path.is_file() {
        return Err(format!("PBF not found: {}", pbf_path.display()));
    }
    // Pass 1: indexed ways and the nodes they use.
    struct PendingWay {
        nodes: Vec<i64>,
        tags: osmpbfreader::objects::Tags,
        kind: (&'static str, String, Option<String>, f64),
    }
    let mut ways: Vec<PendingWay> = Vec::new();
    let f = std::fs::File::open(pbf_path).map_err(|e| e.to_string())?;
    for obj in OsmPbfReader::new(f).iter() {
        if let OsmObj::Way(w) = obj.map_err(|e| e.to_string())? {
            if ways.len() >= MAX_FEATURES_PER_REGION {
                break;
            }
            if let Some(kind) = classify(&w.tags, true) {
                ways.push(PendingWay { nodes: w.nodes.iter().map(|NodeId(id)| *id).collect(), tags: w.tags, kind });
            }
        }
    }
    let mut needed: Vec<i64> = ways.iter().flat_map(|w| w.nodes.iter().copied()).collect();
    needed.sort_unstable();
    needed.dedup();
    let mut coords: Vec<(f64, f64)> = vec![(f64::NAN, f64::NAN); needed.len()];

    // Pass 2: node features, and positions for the ways' nodes.
    let mut out: Vec<GeocodeFeature> = Vec::new();
    let f = std::fs::File::open(pbf_path).map_err(|e| e.to_string())?;
    for obj in OsmPbfReader::new(f).iter() {
        let OsmObj::Node(n) = obj.map_err(|e| e.to_string())? else { continue };
        let NodeId(nid) = n.id;
        if let Ok(k) = needed.binary_search(&nid) {
            coords[k] = (n.lat(), n.lon());
        }
        if out.len() < MAX_FEATURES_PER_REGION {
            if let Some(kind) = classify(&n.tags, false) {
                out.push(make_feature("n", &n.tags, kind, n.lat(), n.lon()));
            }
        }
    }
    for w in ways {
        if out.len() >= MAX_FEATURES_PER_REGION {
            break;
        }
        let pts: Vec<(f64, f64)> = w
            .nodes
            .iter()
            .filter_map(|id| needed.binary_search(id).ok().map(|k| coords[k]))
            .filter(|p| !p.0.is_nan())
            .collect();
        if pts.is_empty() {
            continue;
        }
        let lat = pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64;
        let lon = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
        out.push(make_feature("w", &w.tags, w.kind, lat, lon));
    }
    Ok(out)
}

/// Replace a region's index entries with `features` (the database lock is held only here).
pub fn store_features(db: &Database, region_id: &str, pbf_path: &Path, features: &[GeocodeFeature]) -> Result<GeocodeBuildStats, String> {
    let rid = region_id.trim();
    if rid.is_empty() {
        return Err("region_id required".to_string());
    }
    // One transaction: hundreds of thousands of rows in seconds rather than many minutes.
    db.exec_batch("BEGIN")?;
    let result = store_features_in_transaction(db, rid, features);
    match result {
        Ok(()) => db.exec_batch("COMMIT")?,
        Err(e) => {
            let _ = db.exec_batch("ROLLBACK");
            return Err(e);
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    db.set_routing_region_geocode_built(rid, &now)?;
    Ok(GeocodeBuildStats { region_id: rid.to_string(), features_inserted: features.len(), pbf_path: pbf_path.display().to_string() })
}

fn store_features_in_transaction(db: &Database, rid: &str, features: &[GeocodeFeature]) -> Result<(), String> {
    db.geocode_delete_for_region(rid)?;
    for f in features {
        do_insert(
            db,
            &f.id,
            rid,
            f.class,
            Some(f.name.as_str()),
            &f.alts,
            f.address.as_deref(),
            f.city.as_deref(),
            f.state.as_deref(),
            f.postal.as_deref(),
            f.country.as_deref(),
            f.lat,
            f.lon,
            f.importance,
        )?;
    }
    Ok(())
}

/// Read an extract and store its index. Prefer [`collect_features_from_pbf`] then
/// [`store_features`] when holding a shared database lock, so the read happens outside it.
pub fn build_index_from_pbf(db: &Database, region_id: &str, pbf_path: &Path) -> Result<GeocodeBuildStats, String> {
    if region_id.trim().is_empty() {
        return Err("region_id required".to_string());
    }
    let features = collect_features_from_pbf(pbf_path)?;
    store_features(db, region_id, pbf_path, &features)
}
