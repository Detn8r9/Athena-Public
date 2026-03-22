//! MBTiles 1.3 read/write for interchange (XYZ slippy map on disk ↔ SQLite MBTiles).
//! Tile rows in MBTiles are TMS; our on-disk layout is OSM XYZ (y from north).

use rusqlite::OptionalExtension;
use rusqlite::{params, Connection, OpenFlags};
use std::fs;
use std::path::Path;

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
                if y_path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("png")) != Some(true) {
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
    let conn = Connection::open_with_flags(mbtiles_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

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
    Ok((
        done,
        min_z.clamp(0, 255) as u8,
        max_z.clamp(0, 255) as u8,
    ))
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
        .checked_shl(z as u32)
        .ok_or_else(|| "Zoom level is too large for tile row conversion.".to_string())?;
    let tile_row = nrows
        .checked_sub(1)
        .and_then(|v| v.checked_sub(y_i))
        .ok_or_else(|| "Tile row conversion overflowed.".to_string())?;

    let connection = Connection::open_with_flags(mbtiles_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

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
    let connection = Connection::open_with_flags(mbtiles_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
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

