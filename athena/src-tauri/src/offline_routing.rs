//! Offline routing — placeholder until GraphHopper/OSRM sidecar graphs are configured.
//! Returns a straight-line GeoJSON path between two WGS84 points for map display.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RouteGeoJson {
    pub geojson: serde_json::Value,
    pub mode: &'static str,
}

/// Great-circle approximation as a single-segment LineString (not turn-by-turn).
pub fn straight_line_route_geojson(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> RouteGeoJson {
    let gj = serde_json::json!({
        "type": "Feature",
        "properties": { "kind": "straight_line_placeholder" },
        "geometry": {
            "type": "LineString",
            "coordinates": [ [lon1, lat1], [lon2, lat2] ]
        }
    });
    RouteGeoJson {
        geojson: gj,
        mode: "straight_line",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line_geojson_has_two_coords() {
        let r = straight_line_route_geojson(0.0, 0.0, 1.0, 1.0);
        let coords = r.geojson["geometry"]["coordinates"].as_array().expect("coords");
        assert_eq!(coords.len(), 2);
    }
}
