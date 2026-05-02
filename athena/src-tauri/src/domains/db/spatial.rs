//! Geodesic distance and tactical map context for spatial retrieval-augmented generation.

use crate::db::{Database, TacticalMeshNodeLocation, TacticalPointOfInterest};

/// Mean Earth radius in statute miles (WGS84 approximation for short-range tactical use).
const EARTH_RADIUS_MILES: f64 = 3958.7613;

/// Maximum number of nearby tactical items to include in generated context (between 5 and 10 inclusive when enough data exists).
const NEARBY_SPATIAL_LIMIT: usize = 10;

/// Returns great-circle distance in statute miles between two WGS84 coordinates using the haversine formula.
pub fn haversine_distance_miles(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let dphi = (lat2 - lat1).to_radians();
    let dlambda = (lon2 - lon1).to_radians();
    let sin_dphi = (dphi / 2.0).sin();
    let sin_dlambda = (dlambda / 2.0).sin();
    let h = sin_dphi * sin_dphi + phi1.cos() * phi2.cos() * sin_dlambda * sin_dlambda;
    let c = 2.0 * h.sqrt().atan2((1.0 - h).max(0.0).sqrt());
    EARTH_RADIUS_MILES * c
}

/// Returns true when the latest user message likely asks for map-relative, distance, or location-based reasoning.
pub fn user_query_suggests_spatial_context(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    if lower.contains("mesh node") || lower.contains("mesh_node") {
        return true;
    }
    const KEYWORDS: &[&str] = &[
        "where",
        "nearest",
        "closest",
        "map",
        "water",
        "distance",
        "how far",
        "nearby",
        "mile",
        "miles",
        "coordinate",
        "coordinates",
        "latitude",
        "longitude",
        "direction",
        "route",
        "tactical",
    ];
    KEYWORDS.iter().any(|keyword| lower.contains(keyword))
}

#[derive(Debug, Clone)]
enum SpatialNearbyKind {
    PointOfInterest {
        category_label: String,
        display_name: String,
    },
    MeshNode {
        node_id: String,
        label: String,
    },
}

#[derive(Debug, Clone)]
struct SpatialNearbyRow {
    distance_miles: f64,
    kind: SpatialNearbyKind,
}

fn tactical_point_category_label(notes: &str) -> String {
    let trimmed = notes.trim();
    if let Some(rest) = trimmed.strip_prefix("@icon:") {
        let icon: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if icon.is_empty() {
            "Custom pin".to_string()
        } else {
            format!("Pin ({icon})")
        }
    } else if let Some(at) = trimmed.find('@') {
        let after = &trimmed[at + 1..];
        let token: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        match token.to_lowercase().as_str() {
            "power" => "Power".to_string(),
            "water" => "Water".to_string(),
            "food" => "Food".to_string(),
            "equipment" => "Equipment".to_string(),
            _ => "Point of interest".to_string(),
        }
    } else {
        "Point of interest".to_string()
    }
}

fn collect_and_rank_nearby(
    user_lat: f64,
    user_lon: f64,
    points: Vec<TacticalPointOfInterest>,
    nodes: Vec<TacticalMeshNodeLocation>,
) -> Vec<SpatialNearbyRow> {
    let mut rows: Vec<SpatialNearbyRow> = Vec::new();

    for point in points {
        let distance_miles = haversine_distance_miles(user_lat, user_lon, point.lat, point.lon);
        rows.push(SpatialNearbyRow {
            distance_miles,
            kind: SpatialNearbyKind::PointOfInterest {
                category_label: tactical_point_category_label(&point.notes),
                display_name: point.name.clone(),
            },
        });
    }

    for node in nodes {
        let distance_miles = haversine_distance_miles(user_lat, user_lon, node.lat, node.lon);
        rows.push(SpatialNearbyRow {
            distance_miles,
            kind: SpatialNearbyKind::MeshNode {
                node_id: node.node_id.clone(),
                label: node.label.clone(),
            },
        });
    }

    rows.sort_by(|a, b| {
        a.distance_miles
            .partial_cmp(&b.distance_miles)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let take = rows.len().min(NEARBY_SPATIAL_LIMIT);
    rows.truncate(take);
    rows
}

/// Builds a dense text block listing the closest tactical points of interest and mesh nodes relative to the user.
/// The `query_text` argument matches the user prompt supplied to chat; ranking uses distance only and does not inspect the text in this step.
pub fn build_spatial_context(
    db: &Database,
    user_lat: f64,
    user_lon: f64,
    query_text: &str,
) -> Result<String, String> {
    let _ = query_text;
    if !user_lat.is_finite() || !user_lon.is_finite() {
        return Err("User latitude and longitude must be finite numbers.".to_string());
    }

    let points = db.list_tactical_points_of_interest()?;
    let nodes = db.list_tactical_mesh_node_locations()?;

    let header = format!(
        "System Spatial Context: User location is [{:.6}, {:.6}].",
        user_lat, user_lon
    );

    if points.is_empty() && nodes.is_empty() {
        return Ok(format!(
            "{} No tactical points or mesh nodes are stored in the database.",
            header
        ));
    }

    let ranked = collect_and_rank_nearby(user_lat, user_lon, points, nodes);

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("{} Nearby assets:", header));

    for (index, row) in ranked.iter().enumerate() {
        let n = index + 1;
        let miles_label = format!("{:.2}", row.distance_miles);
        match &row.kind {
            SpatialNearbyKind::PointOfInterest {
                category_label,
                display_name,
            } => {
                lines.push(format!(
                    "{}. [{}] '{}' at {} miles.",
                    n, category_label, display_name, miles_label
                ));
            }
            SpatialNearbyKind::MeshNode { node_id, label } => {
                let label_trimmed = label.trim();
                if label_trimmed.is_empty() {
                    lines.push(format!(
                        "{}. Mesh Node '{}' at {} miles.",
                        n, node_id, miles_label
                    ));
                } else {
                    lines.push(format!(
                        "{}. Mesh Node '{}' ({}) at {} miles.",
                        n, node_id, label_trimmed, miles_label
                    ));
                }
            }
        }
    }

    Ok(lines.join(" "))
}

/// Ray-cast point-in-polygon for one closed ring of `[lng, lat]`.
fn point_in_ring_lng_lat(lng: f64, lat: f64, ring: &[[f64; 2]]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let xi = ring[i][0];
        let yi = ring[i][1];
        let xj = ring[j][0];
        let yj = ring[j][1];
        let intersect =
            (yi > lat) != (yj > lat) && lng < (xj - xi) * (lat - yi) / (yj - yi + f64::EPSILON) + xi;
        if intersect {
            inside = !inside;
        }
    }
    inside
}

/// Standard ray-casting point-in-polygon for a GeoJSON `Polygon`.
///
/// - `point` is `[lng, lat]`
/// - `polygon_geojson` is a parsed GeoJSON object like `{ "type": "Polygon", "coordinates": [...] }`
/// - Only the **outer ring** is considered (holes are out of scope).
pub fn point_in_polygon(point: [f64; 2], polygon_geojson: &serde_json::Value) -> bool {
    point_in_polygon_geojson(point[0], point[1], polygon_geojson)
}

/// Returns true if `[lng, lat]` lies in the outer ring of a GeoJSON `Polygon` value.
pub fn point_in_polygon_geojson(lng: f64, lat: f64, geometry: &serde_json::Value) -> bool {
    let typ = geometry.get("type").and_then(|t| t.as_str());
    if typ != Some("Polygon") {
        return false;
    }

    // Critical: safely navigate nested GeoJSON arrays:
    // geometry["coordinates"][0] => outer ring => [[lng, lat], ...]
    let outer_ring = geometry
        .get("coordinates")
        .and_then(|c| c.as_array())
        .and_then(|rings| rings.get(0))
        .and_then(|ring| ring.as_array());

    let Some(outer) = outer_ring else {
        return false;
    };

    let mut ring: Vec<[f64; 2]> = Vec::with_capacity(outer.len());
    for p in outer {
        let plng = p
            .as_array()
            .and_then(|a| a.get(0))
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite());
        let plat = p
            .as_array()
            .and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite());
        let (Some(plng), Some(plat)) = (plng, plat) else {
            return false;
        };
        ring.push([plng, plat]);
    }

    point_in_ring_lng_lat(lng, lat, &ring)
}

/// Territories (non-deleted) that contain the given point, for map-assistant / spatial awareness.
pub fn get_spatial_context(
    db: &Database,
    lng: f64,
    lat: f64,
) -> Result<serde_json::Value, String> {
    if !(lng.is_finite() && lat.is_finite()) {
        return Ok(serde_json::json!({ "inside_territories": [] }));
    }
    let territories = db.list_territories()?;
    let mut inside: Vec<serde_json::Value> = Vec::new();
    for t in territories {
        let g = match serde_json::from_str::<serde_json::Value>(&t.geometry_json) {
            Ok(v) => v,
            Err(_) => continue, // skip malformed geometry_json; never panic
        };
        if point_in_polygon([lng, lat], &g) {
            inside.push(serde_json::json!({
                "name": t.name,
                "status": t.status,
            }));
        }
    }
    Ok(serde_json::json!({ "inside_territories": inside }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_same_point_is_zero() {
        let d = haversine_distance_miles(40.7128, -74.006, 40.7128, -74.006);
        assert!(d < 0.0001, "expected ~0, got {}", d);
    }

    #[test]
    fn haversine_one_degree_latitude_is_about_sixty_nine_miles() {
        let d = haversine_distance_miles(0.0, 0.0, 1.0, 0.0);
        assert!(
            d > 68.5 && d < 69.5,
            "expected ~69 miles for 1 degree latitude, got {}",
            d
        );
    }

    #[test]
    fn haversine_new_york_to_philadelphia_is_plausible() {
        let d = haversine_distance_miles(40.7128, -74.006, 39.9526, -75.1652);
        assert!(
            d > 80.0 && d < 100.0,
            "expected roughly 85–95 miles, got {}",
            d
        );
    }

    #[test]
    fn spatial_keywords_detect_common_phrases() {
        assert!(super::user_query_suggests_spatial_context(
            "Where is the nearest water?"
        ));
        assert!(super::user_query_suggests_spatial_context(
            "How far to the map marker?"
        ));
        assert!(super::user_query_suggests_spatial_context(
            "Distance to base"
        ));
        assert!(super::user_query_suggests_spatial_context(
            "Show tactical overview"
        ));
        assert!(!super::user_query_suggests_spatial_context(
            "Summarize yesterday's notes."
        ));
    }
}
