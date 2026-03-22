//! Offline map manager: embedded mock catalog JSON for development and tests.
//! Later this will be replaced by fetching the same structure from a remote URL.

use serde::{Deserialize, Serialize};

/// One entry in the offline map catalog (PMTiles or MBTiles archives).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfflineMapCatalogEntry {
    pub id: String,
    pub name: String,
    pub region: String,
    /// Archive container format as in the catalog JSON: `"PMTiles"` or `"MBTiles"`.
    #[serde(rename = "type")]
    pub archive_type: String,
    pub size_mb: u64,
    pub download_url: String,
}

/// Hardcoded catalog JSON; structure and URLs match the agreed mock for download testing.
const MOCK_OFFLINE_MAP_CATALOG_JSON: &str = r#"[
  {
    "id": "global-osm-vector",
    "name": "Protomaps Global OpenStreetMap",
    "region": "Global",
    "type": "PMTiles",
    "size_mb": 115000,
    "download_url": "https://build.protomaps.com/20240923.pmtiles"
  },
  {
    "id": "noaa-northeast-coastal",
    "name": "NOAA Nautical Charts - US Northeast",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 450,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_Northeast.mbtiles"
  },
  {
    "id": "vt-nh-street-extract",
    "name": "Vermont & New Hampshire Street Map",
    "region": "North America",
    "type": "PMTiles",
    "size_mb": 120,
    "download_url": "https://example-survival-repo.com/maps/vt-nh-street.pmtiles"
  }
]"#;

/// Parses the embedded mock catalog. Returns an error only if the JSON is invalid.
pub fn catalog_from_embedded_mock_json() -> Result<Vec<OfflineMapCatalogEntry>, String> {
    serde_json::from_str(MOCK_OFFLINE_MAP_CATALOG_JSON).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_mock_parses_to_three_entries() {
        let entries = catalog_from_embedded_mock_json().expect("mock JSON must parse");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, "global-osm-vector");
        assert_eq!(entries[0].archive_type, "PMTiles");
        assert_eq!(entries[0].size_mb, 115000);
        assert_eq!(
            entries[0].download_url,
            "https://build.protomaps.com/20240923.pmtiles"
        );
        assert_eq!(entries[1].archive_type, "MBTiles");
        assert_eq!(entries[1].id, "noaa-northeast-coastal");
        assert_eq!(entries[2].id, "vt-nh-street-extract");
    }
}
