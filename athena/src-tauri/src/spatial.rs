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
    if let Some(at) = trimmed.find('@') {
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
        assert!(super::user_query_suggests_spatial_context("Where is the nearest water?"));
        assert!(super::user_query_suggests_spatial_context("How far to the map marker?"));
        assert!(super::user_query_suggests_spatial_context("Distance to base"));
        assert!(super::user_query_suggests_spatial_context("Show tactical overview"));
        assert!(!super::user_query_suggests_spatial_context("Summarize yesterday's notes."));
    }
}
