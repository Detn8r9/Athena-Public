//! Import raster PMTiles v3 archives into Athena's XYZ `z/x/y.png` layout for Leaflet.
//! Vector (MVT) archives are rejected — use a vector-capable client if you need those.

use image::ImageFormat;
use pmtiles2::util::{decompress_all, zxy};
use pmtiles2::{Compression, PMTiles, TileType};
use std::fs::{self, File};
use std::io::Cursor;
use std::path::Path;

fn raster_to_png_bytes(tile_type: TileType, uncompressed: &[u8]) -> Result<Vec<u8>, String> {
    match tile_type {
        TileType::Png => Ok(uncompressed.to_vec()),
        TileType::Jpeg | TileType::WebP | TileType::AVIF => {
            let img = image::load_from_memory(uncompressed)
                .map_err(|e| format!("Decode tile image: {e}"))?;
            let mut cursor = Cursor::new(Vec::new());
            img.write_to(&mut cursor, ImageFormat::Png)
                .map_err(|e| format!("Encode PNG: {e}"))?;
            Ok(cursor.into_inner())
        }
        TileType::Mvt | TileType::Unknown => {
            Err("Unsupported tile encoding for raster import.".to_string())
        }
    }
}

/// Extract raster tiles from a PMTiles file into `dest_root/z/x/y.png` (Web Mercator XYZ).
pub fn import_pmtiles_to_xyz(
    pmtiles_path: &Path,
    dest_root: &Path,
    max_tiles: usize,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(u64, u8, u8), String> {
    let file = File::open(pmtiles_path).map_err(|e| e.to_string())?;
    let mut pm = PMTiles::from_reader(file).map_err(|e| e.to_string())?;

    if matches!(pm.tile_type, TileType::Mvt) {
        return Err(
            "This PMTiles file uses vector tiles (Mapbox Vector Tiles). Athena only imports \
             **raster** basemaps (PNG / JPEG / WebP / AVIF). Export or obtain a raster PMTiles archive."
                .to_string(),
        );
    }
    if matches!(pm.tile_type, TileType::Unknown) {
        return Err("PMTiles tile type is unknown or unsupported.".to_string());
    }

    let n = pm.num_tiles();
    if n == 0 {
        return Err("PMTiles archive contains no tiles.".to_string());
    }
    if n > max_tiles {
        return Err(format!(
            "PMTiles contains {n} tiles; max per import is {max_tiles}. Use a smaller archive or clip the source."
        ));
    }

    if matches!(pm.tile_compression, Compression::Unknown) {
        return Err("PMTiles tile compression is unknown; cannot decode.".to_string());
    }

    let min_z = pm.min_zoom;
    let max_z = pm.max_zoom;
    let total = n as u64;

    fs::create_dir_all(dest_root).map_err(|e| e.to_string())?;

    let ids: Vec<u64> = pm.tile_ids().into_iter().copied().collect();
    let mut done = 0u64;

    for tid in ids {
        let (z, x, y) = zxy(tid).map_err(|e| format!("Invalid PMTiles tile id {tid}: {e}"))?;
        let z_i32 = i32::from(z);
        let x_i32: i32 = x
            .try_into()
            .map_err(|_| "Tile x out of range".to_string())?;
        let y_i32: i32 = y
            .try_into()
            .map_err(|_| "Tile y out of range".to_string())?;

        let Some(raw) = pm
            .get_tile_by_id(tid)
            .map_err(|e| format!("Read tile {tid}: {e}"))?
        else {
            continue;
        };

        let uncompressed = decompress_all(pm.tile_compression, &raw)
            .map_err(|e| format!("Decompress tile {tid}: {e}"))?;
        let png = raster_to_png_bytes(pm.tile_type, &uncompressed)?;

        let dir = dest_root.join(z_i32.to_string()).join(x_i32.to_string());
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join(format!("{y_i32}.png")), png).map_err(|e| e.to_string())?;

        done += 1;
        if done % 20 == 0 || done == total {
            on_progress(done, total.max(1));
        }
    }

    on_progress(done, total.max(1));
    Ok((done, min_z, max_z))
}
