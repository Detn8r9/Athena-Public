// Map catalog (Kiwix + future sources), downloaded maps list, and POI store.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const POIS_FILENAME: &str = "athena_pois.json";
const DOWNLOADED_MAPS_FILENAME: &str = "athena_downloaded_maps.json";

/// One point of interest the user has saved (e.g. "Homebase").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Poi {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub notes: String,
}

/// Entry in the map catalog (available to download).
#[derive(Debug, Clone, Serialize)]
pub struct MapCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub region: String,
    pub style: String,
    pub url: String,
    pub size_bytes: u64,
    pub size_label: String,
    /// When true, this map is suggested for the user's location.
    pub for_user_region: bool,
}

/// A downloaded map (path and metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedMap {
    pub path: String,
    pub name: String,
    pub region: String,
    pub style: String,
}

fn format_size(bytes: u64) -> String {
    const GB: u64 = 1_000_000_000;
    const MB: u64 = 1_000_000;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    }
}

/// Kiwix regional **slippy-map** ZIMs (`maps_en_*` under `…/zim/maps/`).
///
/// As of early 2026 the official mirror’s `zim/maps/` index is **empty** and filenames like
/// `maps_en_north-america_2026-03.zim` return **404**, so in-app “Download” would fail instantly.
/// Keep this list empty until OpenZIM publishes files again; users can still add `.zim` files via
/// Library / Kiwix Library and use **Offline raster basemaps** for the in-app Leaflet map.
fn kiwix_map_entries() -> Vec<(String, String, u64)> {
    vec![]
}

/// Infer which catalog regions might match the user's address (simple keyword match).
fn regions_for_address(address: &str) -> Vec<String> {
    let a = address.to_lowercase();
    let mut out = Vec::new();
    if a.contains("usa") || a.contains("united states") || a.contains("canada") || a.contains("mexico") || a.contains(" north america") {
        out.push("North America".to_string());
    }
    if a.contains("europe") || a.contains("uk ") || a.contains(" germany") || a.contains(" france") || a.contains(" spain") || a.contains(" italy") {
        out.push("Europe".to_string());
    }
    if a.contains("australia") || a.contains("oceania") || a.contains(" new zealand") {
        out.push("Australia & Oceania".to_string());
    }
    if a.contains("asia") || a.contains(" japan") || a.contains(" china") || a.contains(" korea") {
        out.push("Asia".to_string());
    }
    if a.contains("india") {
        out.push("India".to_string());
    }
    if a.contains("russia") {
        out.push("Russia".to_string());
    }
    if a.contains("ukraine") {
        out.push("Ukraine".to_string());
    }
    if a.contains("africa") || a.contains(" south africa") || a.contains(" nigeria") {
        out.push("Africa".to_string());
    }
    if a.contains("south america") || a.contains(" brazil") || a.contains(" argentina") {
        out.push("South America".to_string());
    }
    if a.contains("central america") || a.contains(" guatemala") || a.contains(" costa rica") {
        out.push("Central America".to_string());
    }
    if out.is_empty() {
        out.push("North America".to_string());
        out.push("Europe".to_string());
    }
    out
}

/// Smallest Kiwix maps ZIM (by file size) whose region matches the user's address heuristics.
/// Use this to recommend one download that covers the user without grabbing a larger continent pack.
pub fn smallest_kiwix_map_for_address(address: &str) -> Option<MapCatalogEntry> {
    let address = address.trim();
    if address.is_empty() {
        return None;
    }
    let user_regions = regions_for_address(address);
    let mut best: Option<(usize, String, String, u64)> = None;
    for (i, (region, url, size_bytes)) in kiwix_map_entries().into_iter().enumerate() {
        if !user_regions.iter().any(|r| r == &region) {
            continue;
        }
        match &best {
            None => best = Some((i, region, url, size_bytes)),
            Some((_, _, _, best_size)) if size_bytes < *best_size => {
                best = Some((i, region, url, size_bytes));
            }
            _ => {}
        }
    }
    let (i, region, url, size_bytes) = best?;
    let id = url.split('/').last().unwrap_or("").trim_end_matches(".zim").to_string();
    Some(MapCatalogEntry {
        id: format!("kiwix_{}_{}", i, id),
        name: format!("{} (OpenStreetMap)", region),
        description: format!("Offline map of {} for viewing in the Map tab.", region),
        region: region.clone(),
        style: "standard".to_string(),
        url,
        size_bytes,
        size_label: format_size(size_bytes),
        for_user_region: true,
    })
}

pub fn get_map_catalog(
    _app_data_dir: &Path,
    location_enabled: bool,
    user_address: Option<&str>,
) -> Vec<MapCatalogEntry> {
    let user_regions = if location_enabled {
        user_address.map(|a| regions_for_address(a)).unwrap_or_default()
    } else {
        Vec::new()
    };

    kiwix_map_entries()
        .into_iter()
        .enumerate()
        .map(|(i, (region, url, size_bytes))| {
            let id = url.split('/').last().unwrap_or("").trim_end_matches(".zim").to_string();
            let for_user_region = user_regions.iter().any(|r| r == &region);
            MapCatalogEntry {
                id: format!("kiwix_{}_{}", i, id),
                name: format!("{} (OpenStreetMap)", region),
                description: format!("Offline map of {} for viewing in the Map tab.", region),
                region: region.clone(),
                style: "standard".to_string(),
                url,
                size_bytes,
                size_label: format_size(size_bytes),
                for_user_region,
            }
        })
        .collect()
}

// --- POI store ---

pub fn pois_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join(POIS_FILENAME)
}

fn load_pois(app_data_dir: &Path) -> Vec<Poi> {
    let path = pois_path(app_data_dir);
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(pois) = serde_json::from_str::<Vec<Poi>>(&data) {
                return pois;
            }
        }
    }
    Vec::new()
}

fn save_pois(app_data_dir: &Path, pois: &[Poi]) -> Result<(), String> {
    let path = pois_path(app_data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(pois).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_pois(app_data_dir: &Path) -> Vec<Poi> {
    load_pois(app_data_dir)
}

pub fn add_poi(app_data_dir: &Path, name: String, lat: f64, lon: f64, notes: String) -> Result<Poi, String> {
    let mut pois = load_pois(app_data_dir);
    let id = format!("poi_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
    let poi = Poi { id: id.clone(), name, lat, lon, notes };
    pois.push(poi.clone());
    save_pois(app_data_dir, &pois)?;
    Ok(poi)
}

pub fn remove_poi(app_data_dir: &Path, id: &str) -> Result<(), String> {
    let mut pois = load_pois(app_data_dir);
    pois.retain(|p| p.id != id);
    save_pois(app_data_dir, &pois)?;
    Ok(())
}

pub fn update_poi(app_data_dir: &Path, id: &str, name: Option<String>, lat: Option<f64>, lon: Option<f64>, notes: Option<String>) -> Result<Poi, String> {
    let mut pois = load_pois(app_data_dir);
    let idx = pois.iter().position(|p| p.id == id).ok_or("POI not found")?;
    if let Some(n) = name {
        pois[idx].name = n;
    }
    if let Some(l) = lat {
        pois[idx].lat = l;
    }
    if let Some(l) = lon {
        pois[idx].lon = l;
    }
    if let Some(n) = notes {
        pois[idx].notes = n;
    }
    let out = pois[idx].clone();
    save_pois(app_data_dir, &pois)?;
    Ok(out)
}

// --- Downloaded maps store ---

pub fn map_downloads_dir(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("map_downloads")
}

fn downloaded_maps_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join(DOWNLOADED_MAPS_FILENAME)
}

fn load_downloaded_maps(app_data_dir: &Path) -> Vec<DownloadedMap> {
    let path = downloaded_maps_path(app_data_dir);
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(maps) = serde_json::from_str::<Vec<DownloadedMap>>(&data) {
                return maps;
            }
        }
    }
    Vec::new()
}

fn save_downloaded_maps(app_data_dir: &Path, maps: &[DownloadedMap]) -> Result<(), String> {
    let path = downloaded_maps_path(app_data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(maps).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_downloaded_maps(app_data_dir: &Path) -> Vec<DownloadedMap> {
    load_downloaded_maps(app_data_dir)
}

pub fn add_downloaded_map(app_data_dir: &Path, path: String, name: String, region: String, style: String) -> Result<(), String> {
    let mut maps = load_downloaded_maps(app_data_dir);
    if maps.iter().any(|m| m.path == path) {
        return Ok(());
    }
    maps.push(DownloadedMap { path, name, region, style });
    save_downloaded_maps(app_data_dir, &maps)?;
    Ok(())
}

pub fn remove_downloaded_map(app_data_dir: &Path, path: &str) -> Result<(), String> {
    let mut maps = load_downloaded_maps(app_data_dir);
    maps.retain(|m| m.path != path);
    save_downloaded_maps(app_data_dir, &maps)?;
    Ok(())
}
