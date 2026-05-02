export type MapBasemap = "online_osm" | "offline_osm" | "offline_topo" | "offline_imported" | "offline_archive_raster";

/** Output of `convert_coordinates` (serde default field names from Rust). */
export interface CoordinateConversionResult {
  decimal_degrees: string;
  decimal_degrees_lat: string;
  decimal_degrees_lng: string;
  utm: string;
  mgrs: string;
}
