//! MBTiles 1.3 read/write for interchange (XYZ slippy map on disk ↔ SQLite MBTiles).
//! Tile rows in MBTiles are TMS; our on-disk layout is OSM XYZ (y from north).

use rusqlite::OptionalExtension;
use rusqlite::{params, Connection, OpenFlags};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Read-only MBTiles connection with a generous busy timeout (tile protocol + concurrent ingest).
pub fn open_mbtiles_readonly(path: &Path) -> Result<Connection, String> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    c.busy_timeout(Duration::from_millis(8000))
        .map_err(|e| e.to_string())?;
    Ok(c)
}

/// Count .png tiles under z/x/y.png
pub fn count_png_tiles(root: &Path) -> Result<u64, String> {
    let mut n = 0u64;
    for z_res in fs::read_dir(root).map_err(|e| e.to_string())? {
        let z_path = z_res.map_err(|e| e.to_string())?.path();
        if !z_path.is_dir() {
            continue;
        }
        for x_res in fs::read_dir(&z_path).map_err(|e| e.to_string())? {
            let x_path = x_res.map_err(|e| e.to_string())?.path();
            if !x_path.is_dir() {
                continue;
            }
            for y_res in fs::read_dir(&x_path).map_err(|e| e.to_string())? {
                let y_path = y_res.map_err(|e| e.to_string())?.path();
                if y_path.is_file()
                    && y_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("png"))
                        .unwrap_or(false)
                {
                    n += 1;
                }
            }
        }
    }
    Ok(n)
}

/// Export Athena tile pack folder (z/x/y.png XYZ) to MBTiles at `mbtiles_out`.
pub fn export_xyz_to_mbtiles(
    root: &Path,
    mbtiles_out: &Path,
    display_name: &str,
    attribution: &str,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<u64, String> {
    if mbtiles_out.exists() {
        fs::remove_file(mbtiles_out).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(mbtiles_out).map_err(|e| e.to_string())?;
    let _ = conn.busy_timeout(Duration::from_millis(8000));
    conn.execute_batch(
        "CREATE TABLE metadata (name text, value text);
         CREATE TABLE tiles (zoom_level integer, tile_column integer, tile_row integer, tile_data blob);
         CREATE UNIQUE INDEX tile_index on tiles (zoom_level, tile_column, tile_row);",
    )
    .map_err(|e| e.to_string())?;

    for (k, v) in [
        ("name", display_name),
        ("description", attribution),
        ("type", "baselayer"),
        ("format", "png"),
    ] {
        conn.execute(
            "INSERT INTO metadata (name, value) VALUES (?1, ?2)",
            params![k, v],
        )
        .map_err(|e| e.to_string())?;
    }

    let total = count_png_tiles(root)?;
    let mut done = 0u64;

    for z_res in fs::read_dir(root).map_err(|e| e.to_string())? {
        let z_path = z_res.map_err(|e| e.to_string())?.path();
        if !z_path.is_dir() {
            continue;
        }
        let z: i32 = z_path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("Bad z directory {:?}", z_path))?;

        for x_res in fs::read_dir(&z_path).map_err(|e| e.to_string())? {
            let x_path = x_res.map_err(|e| e.to_string())?.path();
            if !x_path.is_dir() {
                continue;
            }
            let x: i32 = x_path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("Bad x directory {:?}", x_path))?;

            for y_res in fs::read_dir(&x_path).map_err(|e| e.to_string())? {
                let y_path = y_res.map_err(|e| e.to_string())?.path();
                if !y_path.is_file() {
                    continue;
                }
                if y_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("png"))
                    != Some(true)
                {
                    continue;
                }
                let y_xyz: i32 = y_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| format!("Bad tile file {:?}", y_path))?
                    .parse()
                    .map_err(|_| format!("Bad tile file {:?}", y_path))?;
                let nrows = 1i32
                    .checked_shl(z as u32)
                    .ok_or_else(|| format!("Zoom {z} too large"))?;
                let tile_row = nrows - 1 - y_xyz;
                let data = fs::read(&y_path).map_err(|e| e.to_string())?;
                conn.execute(
                    "INSERT OR REPLACE INTO tiles (zoom_level, tile_column, tile_row, tile_data) VALUES (?1, ?2, ?3, ?4)",
                    params![z, x, tile_row, data],
                )
                .map_err(|e| e.to_string())?;
                done += 1;
                if done % 50 == 0 || done == total {
                    on_progress(done, total);
                }
            }
        }
    }
    on_progress(done, total.max(1));
    Ok(done)
}

/// Import MBTiles into z/x/y.png under `dest_root` (XYZ).
pub fn import_mbtiles_to_xyz(
    mbtiles_path: &Path,
    dest_root: &Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(u64, u8, u8), String> {
    fs::create_dir_all(dest_root).map_err(|e| e.to_string())?;
    let conn = open_mbtiles_readonly(mbtiles_path)?;

    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM tiles", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if total == 0 {
        return Err("MBTiles has no tiles.".to_string());
    }

    let (min_z, max_z): (i32, i32) = conn
        .query_row(
            "SELECT MIN(zoom_level), MAX(zoom_level) FROM tiles",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT zoom_level, tile_column, tile_row, tile_data FROM tiles")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i32>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, i32>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut done = 0u64;
    for row in rows {
        let (z, x, tile_row, data) = row.map_err(|e| e.to_string())?;
        let nrows = 1i32
            .checked_shl(z as u32)
            .ok_or_else(|| format!("Zoom {z} too large"))?;
        let y_xyz = nrows - 1 - tile_row;
        let dir = dest_root.join(z.to_string()).join(x.to_string());
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join(format!("{y_xyz}.png")), data).map_err(|e| e.to_string())?;
        done += 1;
        if done % 50 == 0 || done == total {
            on_progress(done, total.max(1));
        }
    }
    Ok((done, min_z.clamp(0, 255) as u8, max_z.clamp(0, 255) as u8))
}

/// Reads one raster tile from an MBTiles file using XYZ (slippy map) indices.
/// MBTiles stores `tile_row` in TMS order; this function converts from `y_xyz`.
pub fn read_mbtiles_raster_tile_xyz(
    mbtiles_path: &Path,
    z: u32,
    x: u32,
    y_xyz: u32,
) -> Result<Option<Vec<u8>>, String> {
    let z_i = i32::try_from(z).map_err(|_| "Zoom level is out of range.".to_string())?;
    let x_i = i32::try_from(x).map_err(|_| "Tile column is out of range.".to_string())?;
    let y_i = i32::try_from(y_xyz).map_err(|_| "Tile row is out of range.".to_string())?;
    let nrows = 1i32
        .checked_shl(z)
        .ok_or_else(|| "Zoom level is too large for tile row conversion.".to_string())?;
    let tile_row = nrows
        .checked_sub(1)
        .and_then(|v| v.checked_sub(y_i))
        .ok_or_else(|| "Tile row conversion overflowed.".to_string())?;

    let connection = open_mbtiles_readonly(mbtiles_path)?;

    let blob: Option<Vec<u8>> = connection
        .query_row(
            "SELECT tile_data FROM tiles WHERE zoom_level = ?1 AND tile_column = ?2 AND tile_row = ?3",
            params![z_i, x_i, tile_row],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(blob)
}

/// Returns a MIME type for raster tiles based on the MBTiles `metadata.format` value.
pub fn read_mbtiles_raster_format_mime(mbtiles_path: &Path) -> Result<&'static str, String> {
    let connection = open_mbtiles_readonly(mbtiles_path)?;
    let format_value: Option<String> = connection
        .query_row(
            "SELECT value FROM metadata WHERE name = 'format' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let normalized = format_value
        .as_deref()
        .unwrap_or("png")
        .trim()
        .to_ascii_lowercase();

    Ok(match normalized.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => {
            if normalized.contains("png") {
                "image/png"
            } else if normalized.contains("jpeg") || normalized.contains("jpg") {
                "image/jpeg"
            } else if normalized.contains("webp") {
                "image/webp"
            } else {
                return Err(format!(
                    "MBTiles format metadata \"{normalized}\" is not a supported raster format for direct display."
                ));
            }
        }
    })
}

/// Returns the native zoom range present in an MBTiles archive (`tiles` table), as `(min_zoom, max_zoom)`.
pub fn read_mbtiles_zoom_range(mbtiles_path: &Path) -> Result<Option<(u8, u8)>, String> {
    let connection = open_mbtiles_readonly(mbtiles_path)?;
    let row: Option<(i64, i64)> = connection
        .query_row(
            "SELECT MIN(zoom_level), MAX(zoom_level) FROM tiles",
            [],
            |r| {
                let min_z: Option<i64> = r.get(0)?;
                let max_z: Option<i64> = r.get(1)?;
                Ok(match (min_z, max_z) {
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => None,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();

    let Some((min_i, max_i)) = row else {
        return Ok(None);
    };
    if min_i < 0 || max_i < 0 || min_i > max_i {
        return Ok(None);
    }
    if min_i > u8::MAX as i64 || max_i > u8::MAX as i64 {
        return Ok(None);
    }
    Ok(Some((min_i as u8, max_i as u8)))
}

fn parse_metadata_bounds(raw_bounds: &str) -> Option<(f64, f64, f64, f64)> {
    let parts: Vec<&str> = raw_bounds.split(',').map(|part| part.trim()).collect();
    if parts.len() != 4 {
        return None;
    }
    let parse = |s: &str| s.parse::<f64>().ok();
    let (Some(min_lon), Some(min_lat), Some(max_lon), Some(max_lat)) = (
        parse(parts[0]),
        parse(parts[1]),
        parse(parts[2]),
        parse(parts[3]),
    ) else {
        return None;
    };
    if !min_lon.is_finite() || !min_lat.is_finite() || !max_lon.is_finite() || !max_lat.is_finite()
    {
        return None;
    }
    if min_lon >= max_lon || min_lat >= max_lat {
        return None;
    }
    Some((min_lon, min_lat, max_lon, max_lat))
}

fn tile_x_to_lon(x: u64, z: u8) -> Option<f64> {
    let n = 1u64.checked_shl(z as u32)? as f64;
    Some((x as f64 / n) * 360.0 - 180.0)
}

fn tile_y_to_lat(y: u64, z: u8) -> Option<f64> {
    let n = 1u64.checked_shl(z as u32)? as f64;
    let mercator = std::f64::consts::PI * (1.0 - (2.0 * y as f64) / n);
    Some(mercator.sinh().atan().to_degrees())
}

/// Returns geographic bounds from actual tile coverage across all zoom levels (`tiles` table), as `(min_lon, min_lat, max_lon, max_lat)`.
/// Falls back to MBTiles metadata (`metadata.bounds`) when tile-derived bounds are unavailable.
pub fn read_mbtiles_bounds(mbtiles_path: &Path) -> Result<Option<(f64, f64, f64, f64)>, String> {
    let connection = open_mbtiles_readonly(mbtiles_path)?;

    let mut statement = connection
        .prepare(
            "SELECT zoom_level, MIN(tile_column), MAX(tile_column), MIN(tile_row), MAX(tile_row)
             FROM tiles
             GROUP BY zoom_level",
        )
        .map_err(|e| e.to_string())?;
    let zoom_rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut union_min_lon: Option<f64> = None;
    let mut union_min_lat: Option<f64> = None;
    let mut union_max_lon: Option<f64> = None;
    let mut union_max_lat: Option<f64> = None;

    for row in zoom_rows {
        let (z_i, x_min_i, x_max_i, y_tms_min_i, y_tms_max_i) = row.map_err(|e| e.to_string())?;
        if z_i < 0
            || z_i > u8::MAX as i64
            || x_min_i < 0
            || x_max_i < x_min_i
            || y_tms_min_i < 0
            || y_tms_max_i < y_tms_min_i
        {
            continue;
        }
        let z = z_i as u8;
        let Some(n) = 1u64.checked_shl(z as u32) else {
            continue;
        };
        let x_min = x_min_i as u64;
        let x_max = x_max_i as u64;
        let y_tms_min = y_tms_min_i as u64;
        let y_tms_max = y_tms_max_i as u64;
        if x_max >= n || y_tms_max >= n {
            continue;
        }

        // MBTiles tile_row is TMS; convert top/bottom edges to XYZ boundaries.
        let y_xyz_top = (n - 1).saturating_sub(y_tms_max);
        let y_xyz_bottom = (n - 1).saturating_sub(y_tms_min);
        let west_lon = tile_x_to_lon(x_min, z);
        let east_lon = tile_x_to_lon(x_max.saturating_add(1), z);
        let north_lat = tile_y_to_lat(y_xyz_top, z);
        let south_lat = tile_y_to_lat(y_xyz_bottom.saturating_add(1), z);
        let (Some(west_lon), Some(south_lat), Some(east_lon), Some(north_lat)) =
            (west_lon, south_lat, east_lon, north_lat)
        else {
            continue;
        };
        if !(west_lon < east_lon && south_lat < north_lat) {
            continue;
        }
        union_min_lon = Some(union_min_lon.map_or(west_lon, |v| v.min(west_lon)));
        union_min_lat = Some(union_min_lat.map_or(south_lat, |v| v.min(south_lat)));
        union_max_lon = Some(union_max_lon.map_or(east_lon, |v| v.max(east_lon)));
        union_max_lat = Some(union_max_lat.map_or(north_lat, |v| v.max(north_lat)));
    }

    if let (Some(min_lon), Some(min_lat), Some(max_lon), Some(max_lat)) =
        (union_min_lon, union_min_lat, union_max_lon, union_max_lat)
    {
        if min_lon < max_lon && min_lat < max_lat {
            return Ok(Some((min_lon, min_lat, max_lon, max_lat)));
        }
    }

    let raw_bounds: Option<String> = connection
        .query_row(
            "SELECT value FROM metadata WHERE name='bounds' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw_bounds.as_deref().and_then(parse_metadata_bounds))
}
