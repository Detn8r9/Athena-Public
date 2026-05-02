//! Free-space queries for the local app data path (or an arbitrary path).

use serde::Serialize;
use std::path::{Path, PathBuf};
use sysinfo::Disks;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableStorage {
    pub available_bytes: u64,
    pub total_bytes: u64,
    /// Target path (canonicalized when possible) used to pick a mount.
    pub path: String,
}

/// Returns available and total space on the disk that contains `path` (or app data if None / empty).
#[tauri::command]
pub fn check_available_storage(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Result<AvailableStorage, String> {
    let target: PathBuf = match path.as_deref() {
        None | Some("") => app.path().app_data_dir().map_err(|e| e.to_string())?,
        Some(s) => Path::new(s).to_path_buf(),
    };

    let mut p = target.clone();
    if !p.exists() {
        p = p
            .parent()
            .map(|x| x.to_path_buf())
            .unwrap_or_else(|| target);
    }
    if p.is_file() {
        p = p.parent().map(|x| x.to_path_buf()).unwrap_or(p);
    }
    let p = p.canonicalize().unwrap_or(p);

    let path_str = p.to_string_lossy().to_string();

    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(usize, &sysinfo::Disk)> = None;

    for d in disks.list() {
        let mp = d.mount_point();
        let mp = mp.canonicalize().unwrap_or_else(|_| mp.to_path_buf());
        if p.starts_with(&mp) {
            let len = mp.as_os_str().len();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, d));
            }
        }
    }

    let disk = best
        .map(|(_, d)| d)
        .ok_or_else(|| format!("Could not find a disk for path: {}", path_str))?;

    Ok(AvailableStorage {
        available_bytes: disk.available_space(),
        total_bytes: disk.total_space(),
        path: path_str,
    })
}
