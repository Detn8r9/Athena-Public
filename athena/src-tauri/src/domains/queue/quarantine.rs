//! Quarantine: move corrupted or unreadable files out of the library so the pipeline never crashes.

use std::path::{Path, PathBuf};

const QUARANTINE_DIR: &str = "quarantine";

/// Path to the quarantine directory under app data. Caller must create it if needed.
pub fn quarantine_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(QUARANTINE_DIR)
}

/// Ensure the quarantine directory exists.
pub fn ensure_quarantine_dir(app_data_dir: &Path) -> Result<PathBuf, String> {
    let q = quarantine_dir(app_data_dir);
    std::fs::create_dir_all(&q).map_err(|e| e.to_string())?;
    Ok(q)
}

/// Sanitize a file name for use in quarantine (remove path components, replace unsafe chars).
fn safe_quarantine_name(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Move a file into the quarantine directory. Returns the destination path inside quarantine.
/// Uses a unique name (prefixes with timestamp) to avoid overwriting.
pub fn move_to_quarantine(source: &Path, app_data_dir: &Path) -> Result<PathBuf, String> {
    let quarantine = ensure_quarantine_dir(app_data_dir)?;
    if !source.is_file() {
        return Err("source is not a file".to_string());
    }
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let safe = safe_quarantine_name(name);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut dest = quarantine.join(format!("{}_{}", ts, safe));
    let mut n = 0u32;
    while dest.exists() {
        n += 1;
        let stem = Path::new(&safe)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(&safe)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        dest = if ext.is_empty() {
            quarantine.join(format!("{}_{}_{}", ts, stem, n))
        } else {
            quarantine.join(format!("{}_{}_{}.{}", ts, stem, n, ext))
        };
    }
    std::fs::rename(source, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}
