use tauri::{AppHandle, Manager};

/// Open Athena's unified downloads folder in the native file explorer.
#[tauri::command]
pub async fn reveal_downloads_folder(app_handle: AppHandle) -> Result<(), String> {
    use std::process::Command;

    let base = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    let path = base.join("downloads");

    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Fallback for other OSes: try opener integration when available.
        use tauri_plugin_opener::OpenerExt;
        app_handle
            .opener()
            .open_path(path.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsMigrationReport {
    pub moved_files: u64,
    pub moved_dirs: u64,
    pub skipped_existing: u64,
    pub errors: u64,
}

fn dedupe_dest(dest_dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    crate::library_ingest::dedupe_destination_path(dest_dir, filename)
}

fn move_path_best_effort(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Try rename first (fast). Fallback to copy+delete when crossing volumes.
    match std::fs::rename(src, dest) {
        Ok(_) => Ok(()),
        Err(_) => {
            // Fallback: copy + delete (works across volumes and avoids platform-specific error kinds).
            if src.is_dir() {
                for entry in walkdir::WalkDir::new(src)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let p = entry.path();
                    let rel = p.strip_prefix(src).unwrap_or(p);
                    let target = dest.join(rel);
                    if p.is_dir() {
                        std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                    } else if p.is_file() {
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        std::fs::copy(p, &target).map_err(|e| e.to_string())?;
                    }
                }
                std::fs::remove_dir_all(src).map_err(|e| e.to_string())?;
                Ok(())
            } else {
                std::fs::copy(src, dest).map_err(|e| e.to_string())?;
                std::fs::remove_file(src).map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }
}

fn migrate_one_root(
    old_root: &std::path::Path,
    new_root: &std::path::Path,
    report: &mut DownloadsMigrationReport,
) {
    if !old_root.exists() {
        return;
    }
    if old_root.is_file() {
        let fname = old_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        let dest = dedupe_dest(new_root, fname);
        if dest.exists() {
            report.skipped_existing += 1;
            return;
        }
        match move_path_best_effort(old_root, &dest) {
            Ok(_) => report.moved_files += 1,
            Err(_) => report.errors += 1,
        }
        return;
    }

    for entry in walkdir::WalkDir::new(old_root)
        .follow_links(false)
        .max_depth(64)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if p.is_dir() {
            continue;
        }
        if !p.is_file() {
            continue;
        }
        let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let dest = dedupe_dest(new_root, fname);
        if dest.exists() {
            report.skipped_existing += 1;
            continue;
        }
        match move_path_best_effort(p, &dest) {
            Ok(_) => report.moved_files += 1,
            Err(_) => report.errors += 1,
        }
    }

    // Clean up empty legacy dir tree if possible.
    let _ = std::fs::remove_dir_all(old_root);
    report.moved_dirs += 1;
}

/// One-time migration: move legacy download/staging folders into the unified `downloads/*` tree.
#[tauri::command]
pub async fn migrate_existing_downloads(app_handle: AppHandle) -> Result<DownloadsMigrationReport, String> {
    let base = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    crate::ensure_download_dirs(&base)?;

    let downloads = base.join("downloads");
    let packages = downloads.join("packages");
    let maps = downloads.join("maps");
    let documents = downloads.join("documents");

    let mut report = DownloadsMigrationReport {
        moved_files: 0,
        moved_dirs: 0,
        skipped_existing: 0,
        errors: 0,
    };

    // Legacy roots
    migrate_one_root(&base.join("library"), &packages, &mut report);
    migrate_one_root(&base.join("kiwix_downloads"), &packages, &mut report);
    migrate_one_root(&base.join("map_downloads"), &maps, &mut report);
    migrate_one_root(&base.join("harvest_staging"), &documents.join("harvest_staging"), &mut report);
    migrate_one_root(&base.join("Unsorted"), &documents.join("staging"), &mut report);
    migrate_one_root(&base.join("archive"), &documents.join("archive"), &mut report);

    Ok(report)
}

