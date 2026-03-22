//! Offline map manager: embedded catalog JSON for production.
//! Later this may be replaced by fetching the same structure from a remote URL.

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

/// Embedded production catalog JSON (PMTiles global vector + NOAA regional MBTiles).
const MOCK_OFFLINE_MAP_CATALOG_JSON: &str = r#"[
  {
    "id": "global_osm_vector",
    "name": "Protomaps Global OpenStreetMap",
    "region": "Global",
    "type": "PMTiles",
    "size_mb": 117000,
    "download_url": "https://data.source.coop/protomaps/openstreetmap/tiles/v3.pmtiles"
  },
  {
    "id": "noaa_northeast",
    "name": "NOAA Nautical Charts US Northeast",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 350,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_Northeast.mbtiles"
  },
  {
    "id": "noaa_midatlantic",
    "name": "NOAA Nautical Charts US Mid Atlantic",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 200,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_MidAtlantic.mbtiles"
  },
  {
    "id": "noaa_southeast",
    "name": "NOAA Nautical Charts US Southeast",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 150,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_Southeast.mbtiles"
  },
  {
    "id": "noaa_gulfcoast",
    "name": "NOAA Nautical Charts US Gulf Coast",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 300,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_GulfCoast.mbtiles"
  },
  {
    "id": "noaa_westcoast",
    "name": "NOAA Nautical Charts US West Coast",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 250,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_WestCoast.mbtiles"
  },
  {
    "id": "noaa_greatlakes",
    "name": "NOAA Nautical Charts US Great Lakes",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 180,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_GreatLakes.mbtiles"
  },
  {
    "id": "noaa_alaska",
    "name": "NOAA Nautical Charts Alaska",
    "region": "North America",
    "type": "MBTiles",
    "size_mb": 850,
    "download_url": "https://distribution.charts.noaa.gov/ncds/latest/US_Alaska.mbtiles"
  }
]"#;

/// Parses the embedded catalog. Returns an error only if the JSON is invalid.
pub fn catalog_from_embedded_mock_json() -> Result<Vec<OfflineMapCatalogEntry>, String> {
    serde_json::from_str(MOCK_OFFLINE_MAP_CATALOG_JSON).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_parses_to_eight_entries() {
        let entries = catalog_from_embedded_mock_json().expect("catalog JSON must parse");
        assert_eq!(entries.len(), 8);

        assert_eq!(entries[0].id, "global_osm_vector");
        assert_eq!(entries[0].archive_type, "PMTiles");
        assert_eq!(entries[0].size_mb, 117000);
        assert_eq!(
            entries[0].download_url,
            "https://data.source.coop/protomaps/openstreetmap/tiles/v3.pmtiles"
        );

        assert_eq!(entries[1].id, "noaa_northeast");
        assert_eq!(entries[1].archive_type, "MBTiles");

        assert_eq!(entries[7].id, "noaa_alaska");
        assert_eq!(entries[7].archive_type, "MBTiles");
        assert_eq!(entries[7].size_mb, 850);
        assert_eq!(
            entries[7].download_url,
            "https://distribution.charts.noaa.gov/ncds/latest/US_Alaska.mbtiles"
        );
    }
}
