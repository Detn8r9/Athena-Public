// Kiwix ZIM: discover local .zim files by recursive directory scan (no in-app catalog or download).
// Also provides a generic download_zim helper used by map downloads.

use serde::Serialize;
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;

fn format_size(bytes: u64) -> String {
    const GB: u64 = 1_000_000_000;
    const MB: u64 = 1_000_000;
    const KB: u64 = 1_000;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredZim {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub size_label: String,
}

/// Recursively scan `root` for files with a `.zim` extension. Skips directories that cause
/// permission or I/O errors (logs and continues) so the app does not crash on restricted folders.
pub fn scan_kiwix_zim_files(root: &Path) -> Result<Vec<DiscoveredZim>, String> {
    if !root.is_dir() {
        return Err(format!(
            "Library path is not a directory: {}",
            root.display()
        ));
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        match entry {
            Ok(e) => {
                let path = e.path();
                if !path.is_file() {
                    continue;
                }
                let is_zim = path
                    .extension()
                    .map_or(false, |ext| ext.eq_ignore_ascii_case("zim"));
                if !is_zim {
                    continue;
                }
                let path_str = path.to_string_lossy().to_string();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                out.push(DiscoveredZim {
                    path: path_str,
                    name,
                    size_bytes,
                    size_label: format_size(size_bytes),
                });
            }
            Err(err) => {
                if err.path().is_some() {
                    eprintln!("[Kiwix] Skipping path (permission or I/O error): {}", err);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Downloads a ZIM (or any file) from `url` into `dest_path`, calling `on_progress(downloaded_bytes, total_bytes)` periodically.
/// Used by map downloads; total_bytes is None if Content-Length is missing.
pub fn download_zim(
    url: &str,
    dest_path: &Path,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let response = ureq::get(url)
        .set(
            "User-Agent",
            "AthenaMicro/1.0 (offline map ZIM; https://github.com/athena-micro)",
        )
        .timeout(std::time::Duration::from_secs(3600))
        .call()
        .map_err(|e| {
            let hint = if e.to_string().contains("404") {
                " (file may be missing from the mirror — check library.kiwix.org for current map ZIMs.)"
            } else {
                ""
            };
            format!("{e}{hint}")
        })?;
    let total: Option<u64> = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok());
    // So the UI shows activity before the first 256 KiB chunk (and if the server sends no Content-Length).
    on_progress(0, total);
    let parent = dest_path.parent().ok_or("Invalid destination path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(dest_path).map_err(|e| e.to_string())?;
    let mut reader = response.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if downloaded - last_emit >= 256 * 1024 {
            last_emit = downloaded;
            on_progress(downloaded, total);
        }
    }
    on_progress(downloaded, total);
    Ok(())
}
