//! Portable mode: if a `data` directory exists beside the app bundle (or beside the executable),
//! use it as the application data directory instead of the OS-specific Application Support path.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::Manager;
use walkdir::WalkDir;

use crate::config::{effective_embedding_folder, effective_reranker_folder, load_config, AppConfig};
use crate::db::Database;
use crate::{compute_watcher_folders, kiwix_downloads_dir, AppState};

static PORTABLE_APP_DATA_OVERRIDE: OnceLock<Option<PathBuf>> = OnceLock::new();

fn macos_app_bundle_walking_up(exe: &Path) -> Option<PathBuf> {
    let mut cur = exe.parent()?;
    loop {
        if cur.extension().map_or(false, |e| e == "app") {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
}

fn detect_portable_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);

    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            if let Some(bundle_parent) = bundle.parent() {
                let sibling = bundle_parent.join("data");
                if sibling.is_dir() {
                    return Some(sibling);
                }
            }
            let inside_contents = bundle.join("Contents").join("data");
            if inside_contents.is_dir() {
                return Some(inside_contents);
            }
        }
    }

    let exe_dir = exe.parent()?;
    let beside_exe = exe_dir.join("data");
    if beside_exe.is_dir() {
        return Some(beside_exe);
    }

    None
}

pub fn init_portable_app_data_override() {
    let detected = detect_portable_data_dir();
    if let Some(ref p) = detected {
        eprintln!("[athena] portable mode: app data directory {}", p.display());
    }
    let _ = PORTABLE_APP_DATA_OVERRIDE.set(detected);
}

pub fn resolved_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(Some(p)) = PORTABLE_APP_DATA_OVERRIDE.get() {
        return Ok(p.clone());
    }
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Resolve a configured library / watch root: relative paths are anchored at `app_data_dir`
/// (including portable `data` beside the executable) so the same config works across machines.
pub fn resolve_library_watch_root_path(raw: &str, app_data_dir: &Path) -> PathBuf {
    let t = raw.trim();
    if t.is_empty() {
        return PathBuf::from(t);
    }
    let p = Path::new(t);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        app_data_dir.join(p)
    }
}

/// Recursive copy; returns the first error (used so export can abort before wipe).
fn copy_dir_recursive_strict(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("Missing source: {}", src.display()));
    }
    for entry in WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(src).map_err(|e| e.to_string())?;
        let target = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else if path.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(path, &target).map_err(|e| {
                format!(
                    "copy {} -> {}: {}",
                    path.display(),
                    target.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

fn tree_file_bytes(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut sum = 0u64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        if entry.path().is_file() {
            sum += entry.metadata().map_err(|e| e.to_string())?.len();
        }
    }
    Ok(sum)
}

fn close_db_for_disk_wipe(db: &std::sync::Arc<std::sync::Mutex<Database>>) -> Result<(), String> {
    let mut guard = db.lock().map_err(|e| e.to_string())?;
    let _old = std::mem::replace(&mut *guard, Database::new_in_memory_placeholder()?);
    drop(_old);
    Ok(())
}

fn remove_sqlite_sidecar_files(db_path: &Path) {
    let base = db_path.to_string_lossy().to_string();
    for suf in ["-wal", "-shm", "-journal"] {
        let p = format!("{base}{suf}");
        if let Err(e) = fs::remove_file(&p) {
            eprintln!("[athena] scorched earth: remove {p}: {e}");
        }
    }
}

fn wipe_directory_contents(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn verify_usb_export(
    dest_root: &Path,
    export_type: &str,
    app_data_dir: &Path,
    emb_name: &str,
    rr_name: &str,
) -> Result<(), String> {
    let usb_data = dest_root.join("data");
    let emb_src = app_data_dir.join("models").join("embeddings").join(emb_name);
    let rr_src = app_data_dir.join("models").join("rerankers").join(rr_name);
    let emb_dst = usb_data.join("models").join("embeddings").join(emb_name);
    let rr_dst = usb_data.join("models").join("rerankers").join(rr_name);

    if !emb_dst.is_dir() {
        return Err(format!(
            "Verification failed: missing embedding tree on USB: {}",
            emb_dst.display()
        ));
    }
    if !rr_dst.is_dir() {
        return Err(format!(
            "Verification failed: missing reranker tree on USB: {}",
            rr_dst.display()
        ));
    }

    let s_emb = tree_file_bytes(&emb_src)?;
    let d_emb = tree_file_bytes(&emb_dst)?;
    if s_emb != d_emb {
        return Err(format!(
            "Verification failed: embedding byte count mismatch (local {s_emb}, USB {d_emb})."
        ));
    }
    let s_rr = tree_file_bytes(&rr_src)?;
    let d_rr = tree_file_bytes(&rr_dst)?;
    if s_rr != d_rr {
        return Err(format!(
            "Verification failed: reranker byte count mismatch (local {s_rr}, USB {d_rr})."
        ));
    }

    if export_type == "complete" {
        let db_src = app_data_dir.join("athena_index.db");
        let db_dst = usb_data.join("athena_index.db");
        if db_src.is_file() {
            if !db_dst.is_file() {
                return Err(format!(
                    "Verification failed: database missing on USB: {}",
                    db_dst.display()
                ));
            }
            let m1 = fs::metadata(&db_src).map_err(|e| e.to_string())?.len();
            let m2 = fs::metadata(&db_dst).map_err(|e| e.to_string())?.len();
            if m1 != m2 {
                return Err(format!(
                    "Verification failed: database size mismatch (local {m1} B, USB {m2} B)."
                ));
            }
        }
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            let name = bundle
                .file_name()
                .ok_or_else(|| "Verification failed: bundle name.".to_string())?;
            let out = dest_root.join("Mac").join(name);
            if !out.is_dir() {
                return Err(format!(
                    "Verification failed: application bundle missing on USB: {}",
                    out.display()
                ));
            }
        } else {
            let out = dest_root.join("ATHENA");
            if !out.is_dir() {
                return Err(format!(
                    "Verification failed: install folder missing on USB: {}",
                    out.display()
                ));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let out = dest_root.join("ATHENA");
        if !out.is_dir() {
            return Err(format!(
                "Verification failed: install folder missing on USB: {}",
                out.display()
            ));
        }
    }

    Ok(())
}

fn execute_scorched_wipe(
    state: &AppState,
    app_data_dir: &Path,
    cfg: &AppConfig,
    kiwix_str: &str,
) -> Result<(), String> {
    let folders = {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, app_data_dir, cfg)?
    };

    close_db_for_disk_wipe(&state.db)?;

    let db_path = app_data_dir.join("athena_index.db");
    remove_sqlite_sidecar_files(&db_path);

    let app_data_canon = fs::canonicalize(app_data_dir).ok();

    for folder in folders {
        if folder == kiwix_str {
            continue;
        }
        let p = Path::new(&folder);
        if !p.is_dir() {
            continue;
        }
        if let Some(ref ad) = app_data_canon {
            if let Ok(c) = fs::canonicalize(p) {
                if c.starts_with(ad) {
                    eprintln!(
                        "[athena] scorched earth: skipping watch path inside app data (removed with app data): {}",
                        folder
                    );
                    continue;
                }
            }
        }
        eprintln!("[athena] scorched earth: wiping watch folder contents: {}", folder);
        wipe_directory_contents(p)?;
    }

    eprintln!(
        "[athena] scorched earth: removing app data directory {}",
        app_data_dir.display()
    );
    if app_data_dir.is_dir() {
        fs::remove_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    }

    eprintln!("[athena] scorched earth: finished");
    Ok(())
}

fn write_usb_readme(dest_root: &Path) -> Result<(), String> {
    let text = r#"ATHENA USB drive — how to use this drive

Open the folder that matches your computer's operating system.

On a Mac, open the "Mac" folder and run ATHENA from the application inside.

On Windows, open the "Windows" folder, then open the subfolder that matches your system type. Most modern computers use the "64-bit" folder. Older PCs may use the "32-bit" folder. Newer tablet-style devices with ARM processors should try the "ARM" folder. Run ATHENA.exe from the folder that matches your machine.

On Linux, open the "Linux" folder. Most desktops and laptops use "64-bit". ARM-based devices (including many tablets and single-board computers) should use the "ARM" folder. Run the ATHENA AppImage from the matching folder.

All intelligence, indexes, models, and application data for this portable install are stored in the "data" folder at the root of this drive. Keep the "data" folder together with these launchers.
"#;
    let p = dest_root.join("README.txt");
    fs::write(&p, text).map_err(|e| format!("Failed to write {}: {e}", p.display()))?;
    eprintln!("[athena] usb export: wrote {}", p.display());
    Ok(())
}

/// Copies `payload/<branch>` from app resources to `<dest_root>/<branch>` (e.g. Windows, Linux).
/// Warn-only on missing resources so dev builds without a full payload still export.
fn copy_bundled_payload_branch_to_usb(app: &tauri::AppHandle, dest_root: &Path, branch: &str) {
    let rel = format!("payload/{branch}");
    match app.path().resolve(&rel, tauri::path::BaseDirectory::Resource) {
        Ok(src) if src.is_dir() => {
            let dst = dest_root.join(branch);
            if dst.exists() {
                let _ = if dst.is_dir() {
                    fs::remove_dir_all(&dst)
                } else {
                    fs::remove_file(&dst)
                };
            }
            match copy_dir_recursive_strict(&src, &dst) {
                Ok(()) => eprintln!(
                    "[athena] usb export: copied bundled {} -> {}",
                    rel,
                    dst.display()
                ),
                Err(e) => eprintln!(
                    "[athena] usb export: warning: could not copy {rel} to USB: {e}"
                ),
            }
        }
        Ok(_) => eprintln!(
            "[athena] usb export: warning: {rel} is not a directory in resources (skipping)"
        ),
        Err(e) => eprintln!(
            "[athena] usb export: warning: could not resolve {rel} in app resources: {e} (skipping)"
        ),
    }
}

#[cfg(target_os = "macos")]
fn copy_macos_app_bundle(src: &Path, dest_root: &Path) -> Result<(), String> {
    let mac_dir = dest_root.join("Mac");
    fs::create_dir_all(&mac_dir).map_err(|e| e.to_string())?;
    let name = src
        .file_name()
        .ok_or_else(|| "Invalid bundle path.".to_string())?;
    let out = mac_dir.join(name);
    if out.exists() {
        eprintln!(
            "[athena] usb export: replacing existing {}",
            out.display()
        );
        if out.is_dir() {
            fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&out).map_err(|e| e.to_string())?;
        }
    }
    let status = std::process::Command::new("/usr/bin/ditto")
        .arg(src.as_os_str())
        .arg(out.as_os_str())
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "ditto failed copying bundle (status {status})"
        ));
    }
    Ok(())
}

fn copy_non_macos_install_folder(src: &Path, dest_root: &Path) -> Result<(), String> {
    let out = dest_root.join("ATHENA");
    if out.exists() {
        eprintln!("[athena] usb export: replacing existing {}", out.display());
        fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    copy_dir_recursive_strict(src, &out)?;
    Ok(())
}

fn copy_application_to_destination(dest_root: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);

    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            return copy_macos_app_bundle(&bundle, dest_root);
        }
    }

    let install_dir = exe
        .parent()
        .ok_or_else(|| "Could not resolve executable directory.".to_string())?;
    copy_non_macos_install_folder(install_dir, dest_root)
}

fn sanitize_documents_subdir(raw: &str) -> String {
    let s: String = raw
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0'))
        .collect();
    let t = s.trim();
    if t.is_empty() {
        "library".to_string()
    } else {
        t.chars().take(120).collect()
    }
}

#[tauri::command]
pub fn export_athena_to_usb(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    destination_path: String,
    export_type: String,
    wipe_after: bool,
    format_first: bool,
    device_path: Option<String>,
) -> Result<(), String> {
    let mut dest_root = PathBuf::from(destination_path.trim());

    #[cfg(target_os = "macos")]
    if format_first {
        let dev = device_path
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "device_path is required when format_first is true (select a USB drive)."
                    .to_string()
            })?;
        dest_root = crate::usb::macos_erase_disk_exfat_and_resolve_mount(dev)?;
    }
    #[cfg(not(target_os = "macos"))]
    if format_first {
        let _ = device_path;
        return Err("Drive formatting is only supported on macOS.".to_string());
    }

    if dest_root.as_os_str().is_empty() {
        return Err("Destination path is empty.".to_string());
    }
    if !dest_root.is_dir() {
        return Err("Destination is not a directory.".to_string());
    }

    fs::create_dir_all(dest_root.join("data")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest_root.join("models")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest_root.join("documents")).map_err(|e| e.to_string())?;

    let et = export_type.trim().to_ascii_lowercase();
    if et != "essentials" && et != "complete" {
        return Err("export_type must be \"essentials\" or \"complete\".".to_string());
    }

    let app_data_dir = resolved_app_data_dir(&app)?;
    let cfg = load_config(&app_data_dir);
    let emb_name = effective_embedding_folder(&cfg);
    let rr_name = effective_reranker_folder(&cfg);
    let emb_src = app_data_dir
        .join("models")
        .join("embeddings")
        .join(&emb_name);
    let rr_src = app_data_dir
        .join("models")
        .join("rerankers")
        .join(&rr_name);

    if !emb_src.is_dir() {
        return Err(format!(
            "Embedding model folder not found: {}",
            emb_src.display()
        ));
    }
    if !rr_src.is_dir() {
        return Err(format!(
            "Reranker model folder not found: {}",
            rr_src.display()
        ));
    }

    eprintln!(
        "[athena] usb export ({et}): copying application to {}",
        dest_root.display()
    );
    copy_application_to_destination(&dest_root)?;
    copy_bundled_payload_branch_to_usb(&app, &dest_root, "Windows");
    copy_bundled_payload_branch_to_usb(&app, &dest_root, "Linux");

    let usb_data = dest_root.join("data");
    fs::create_dir_all(&usb_data).map_err(|e| e.to_string())?;
    let models_root = usb_data.join("models");
    let emb_dst = models_root.join("embeddings").join(&emb_name);
    let rr_dst = models_root.join("rerankers").join(&rr_name);
    if let Some(p) = emb_dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    if let Some(p) = rr_dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }

    eprintln!(
        "[athena] usb export: copying embedding model to {}",
        emb_dst.display()
    );
    copy_dir_recursive_strict(&emb_src, &emb_dst)?;
    eprintln!(
        "[athena] usb export: copying reranker model to {}",
        rr_dst.display()
    );
    copy_dir_recursive_strict(&rr_src, &rr_dst)?;

    let kiwix_str = kiwix_downloads_dir(&app_data_dir)
        .to_string_lossy()
        .to_string();

    if et == "complete" {
        let db_src = app_data_dir.join("athena_index.db");
        if wipe_after && !db_src.is_file() {
            return Err(
                "Complete export with scorched earth requires a local athena_index.db.".to_string(),
            );
        }
        if !db_src.is_file() {
            eprintln!(
                "[athena] usb export: database missing at {}",
                db_src.display()
            );
        } else {
            let db_dst = usb_data.join("athena_index.db");
            eprintln!(
                "[athena] usb export: copying database to {}",
                db_dst.display()
            );
            fs::copy(&db_src, &db_dst).map_err(|e| e.to_string())?;
        }

        let folders = {
            let g = state.db.lock().map_err(|e| e.to_string())?;
            compute_watcher_folders(&*g, &app_data_dir, &cfg)?
        };
        let docs_root = dest_root.join("documents");
        fs::create_dir_all(&docs_root).map_err(|e| e.to_string())?;

        let mut used_names = std::collections::HashSet::<String>::new();
        for folder in &folders {
            if folder == &kiwix_str {
                continue;
            }
            let p = Path::new(folder);
            if !p.is_dir() {
                eprintln!(
                    "[athena] usb export: skip non-directory watch path: {}",
                    folder
                );
                continue;
            }
            let base = p
                .file_name()
                .and_then(|s| s.to_str())
                .map(sanitize_documents_subdir)
                .unwrap_or_else(|| "library".to_string());
            let mut name = base.clone();
            let mut n = 0u32;
            while used_names.contains(&name) {
                n += 1;
                name = format!("{base}_{n}");
            }
            used_names.insert(name.clone());
            let dst = docs_root.join(&name);
            eprintln!(
                "[athena] usb export: copying library folder {} -> {}",
                folder,
                dst.display()
            );
            fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
            copy_dir_recursive_strict(p, &dst)?;
        }
    }

    write_usb_readme(&dest_root)?;

    if wipe_after {
        verify_usb_export(&dest_root, &et, &app_data_dir, &emb_name, &rr_name)?;
        execute_scorched_wipe(&*state, &app_data_dir, &cfg, &kiwix_str)?;
    }

    eprintln!("[athena] usb export: finished");
    Ok(())
}
