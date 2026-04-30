//! Portable mode: if a `data` directory exists beside the app bundle (or beside the executable),
//! use it as the application data directory instead of the OS-specific Application Support path.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::Manager;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use walkdir::WalkDir;

use crate::config::{effective_embedding_folder, effective_reranker_folder, load_config, AppConfig};
use crate::db::Database;
use crate::map_manager_download;
use crate::{compute_watcher_folders, kiwix_downloads_dir, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OmnibusTier {
    Essentials,
    Recommended,
    Absolute,
}

fn parse_omnibus_tier(export_type: &str) -> Result<OmnibusTier, String> {
    let t = export_type.trim().to_ascii_lowercase();
    match t.as_str() {
        "essentials" => Ok(OmnibusTier::Essentials),
        "recommended" => Ok(OmnibusTier::Recommended),
        "absolute" => Ok(OmnibusTier::Absolute),
        // Back-compat with older UI naming.
        "complete" => Ok(OmnibusTier::Absolute),
        _ => Err("export_type must be \"essentials\", \"recommended\", or \"absolute\".".to_string()),
    }
}

fn approx_bbox_for_radius_miles(
    center_lon: f64,
    center_lat: f64,
    radius_miles: f64,
) -> (f64, f64, f64, f64) {
    // WGS84 mean Earth radius in statute miles.
    let earth_radius_miles = 3958.7613_f64;
    let dlat = (radius_miles / earth_radius_miles).to_degrees();
    let cos_lat = center_lat.to_radians().cos().abs().max(1e-6);
    let dlon = (radius_miles / earth_radius_miles).to_degrees() / cos_lat;
    (
        (center_lon - dlon).max(-180.0),
        (center_lat - dlat).max(-90.0),
        (center_lon + dlon).min(180.0),
        (center_lat + dlat).min(90.0),
    )
}

fn run_sidecar_blocking(
    app: &tauri::AppHandle,
    name: &str,
    args: &[&str],
    timeout_secs: u64,
) -> Result<(), String> {
    let cmd = app
        .shell()
        .sidecar(name)
        .map_err(|e| format!("Failed to resolve sidecar `{name}`: {e}"))?;
    let (mut rx, _child) = cmd
        .args(args)
        .spawn()
        .map_err(|e| format!("Failed to spawn `{name}`: {e}"))?;
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(timeout_secs) {
            return Err(format!("Sidecar `{name}` timed out after {timeout_secs}s."));
        }
        match tauri::async_runtime::block_on(rx.recv()) {
            Some(CommandEvent::Terminated(payload)) => {
                if payload.code == Some(0) {
                    return Ok(());
                }
                return Err(format!(
                    "Sidecar `{name}` failed (code {:?}, signal {:?}).",
                    payload.code, payload.signal
                ));
            }
            Some(_) => continue,
            None => return Err(format!("Sidecar `{name}` ended unexpectedly.")),
        }
    }
}

fn pick_best_local_pmtiles_source(app_data_dir: &Path) -> Option<PathBuf> {
    let dir = map_manager_download::offline_maps_directory(app_data_dir);
    if !dir.is_dir() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file()
                && p.extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .eq_ignore_ascii_case("pmtiles")
            {
                candidates.push(p);
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

fn ensure_fairlee_proximity_pmtiles(
    app: &tauri::AppHandle,
    dest_root: &Path,
    app_data_dir: &Path,
) -> Result<(), String> {
    const PROTOMAPS_SOURCE_URL: &str =
        "https://data.source.coop/protomaps/openstreetmap/tiles/v3.pmtiles";

    let center_lat = 43.908_f64;
    let center_lon = -72.143_f64;
    let radius_miles = 25.0_f64;
    let (min_lon, min_lat, max_lon, max_lat) =
        approx_bbox_for_radius_miles(center_lon, center_lat, radius_miles);

    let out_dir = dest_root.join("data").join("maps");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out = out_dir.join("fairlee_vt_25mi.pmtiles");

    let source = pick_best_local_pmtiles_source(app_data_dir)
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| PROTOMAPS_SOURCE_URL.to_string());

    let bbox = format!("--bbox={min_lon:.8},{min_lat:.8},{max_lon:.8},{max_lat:.8}");
    let out_str = out
        .to_str()
        .ok_or_else(|| "PMTiles output path contains invalid characters.".to_string())?;

    run_sidecar_blocking(
        app,
        "pmtiles",
        &["extract", source.as_str(), out_str, bbox.as_str()],
        600,
    )?;
    Ok(())
}

fn available_space_bytes_for_destination(dest_root: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let target = dest_root.canonicalize().unwrap_or_else(|_| dest_root.to_path_buf());
    let mut best: Option<(usize, u64)> = None;
    for d in disks.list() {
        let mp = d.mount_point();
        let mp = mp.canonicalize().unwrap_or_else(|_| mp.to_path_buf());
        if target.starts_with(&mp) {
            let len = mp.as_os_str().len();
            let avail = d.available_space();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, avail));
            }
        }
    }
    best.map(|(_, a)| a)
}

fn dir_size_bytes(p: &Path) -> u64 {
    if p.is_file() {
        return std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            total = total.saturating_add(dir_size_bytes(&e.path()));
        }
    }
    total
}

#[derive(Debug, serde::Deserialize)]
struct PayloadMetadata {
    #[serde(default)]
    priority: Option<i64>,
}

fn payload_priority_for_dir(dir: &Path) -> Option<i64> {
    let meta = dir.join("metadata.json");
    let raw = std::fs::read_to_string(meta).ok()?;
    let parsed: PayloadMetadata = serde_json::from_str(&raw).ok()?;
    parsed.priority
}

fn tier2_copy_payload_priority_fill(app: &tauri::AppHandle, dest_root: &Path) -> Result<(), String> {
    let payload_root = app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Could not resolve bundled payload dir: {e}"))?;
    if !payload_root.is_dir() {
        return Ok(());
    }

    let mut entries: Vec<(i64, PathBuf, u64)> = Vec::new();
    for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let src = e.path();
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("Linux") {
            continue;
        }
        if !src.is_dir() {
            continue;
        }
        let prio = payload_priority_for_dir(&src).unwrap_or(0);
        let sz = dir_size_bytes(&src);
        entries.push((prio, src, sz));
    }

    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let reserve_bytes = 500u64 * 1024u64 * 1024u64;
    for (prio, src, sz) in entries {
        let avail = available_space_bytes_for_destination(dest_root).unwrap_or(0);
        if avail <= reserve_bytes {
            break;
        }
        if avail.saturating_sub(sz) <= reserve_bytes {
            continue;
        }
        let name = src
            .file_name()
            .ok_or_else(|| "payload entry missing filename".to_string())?;
        let dst = dest_root.join(name);
        eprintln!(
            "[athena] usb export: tier2 copy payload pack prio={prio} size={} -> {}",
            sz,
            dst.display()
        );
        if dst.exists() {
            let _ = std::fs::remove_dir_all(&dst);
        }
        copy_dir_recursive_strict(&src, &dst)?;
    }

    Ok(())
}

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

    // Tier 1 essentials intentionally does not require models.
    if export_type != "essentials" {
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
    }

    if export_type == "complete" || export_type == "absolute" {
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

    let tier = parse_omnibus_tier(&export_type)?;
    if wipe_after && tier != OmnibusTier::Absolute {
        return Err("Scorched Earth wipe_after is only supported for export_type=\"absolute\".".to_string());
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

    eprintln!(
        "[athena] usb export ({tier:?}): copying application to {}",
        dest_root.display()
    );
    copy_application_to_destination(&dest_root)?;

    // Tier 1 baseline: copy the platform launchers (Windows/Linux).
    // Tier 3 ("Absolute") copies all payload children under the destination root.
    match tier {
        OmnibusTier::Absolute => {
            let payload_root = app
                .path()
                .resolve("payload", tauri::path::BaseDirectory::Resource)
                .ok();
            if let Some(payload_root) = payload_root {
                if payload_root.is_dir() {
                    for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
                        let e = entry.map_err(|e| e.to_string())?;
                        let src = e.path();
                        let dst = dest_root.join(e.file_name());
                        if dst.exists() {
                            let _ = if dst.is_dir() {
                                std::fs::remove_dir_all(&dst)
                            } else {
                                std::fs::remove_file(&dst)
                            };
                        }
                        if src.is_dir() {
                            copy_dir_recursive_strict(&src, &dst)?;
                        } else if src.is_file() {
                            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        }
        OmnibusTier::Essentials | OmnibusTier::Recommended => {
            copy_bundled_payload_branch_to_usb(&app, &dest_root, "Windows");
            copy_bundled_payload_branch_to_usb(&app, &dest_root, "Linux");
        }
    }

    // Tier 1 map: proximity-pruned extract for Fairlee, VT (25mi).
    ensure_fairlee_proximity_pmtiles(&app, &dest_root, &app_data_dir)?;

    let usb_data = dest_root.join("data");
    fs::create_dir_all(&usb_data).map_err(|e| e.to_string())?;

    // Essentials: copy local SQLite index (personnel, roster, library metadata) when present.
    if tier == OmnibusTier::Essentials {
        let db_src = app_data_dir.join("athena_index.db");
        if db_src.is_file() {
            let db_dst = usb_data.join("athena_index.db");
            eprintln!(
                "[athena] usb export: copying index database to {}",
                db_dst.display()
            );
            fs::copy(&db_src, &db_dst).map_err(|e| e.to_string())?;
        }
    }

    let models_root = usb_data.join("models");
    let emb_dst = models_root.join("embeddings").join(&emb_name);
    let rr_dst = models_root.join("rerankers").join(&rr_name);
    if let Some(p) = emb_dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    if let Some(p) = rr_dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }

    if tier != OmnibusTier::Essentials {
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
            "[athena] usb export: copying embedding model to {}",
            emb_dst.display()
        );
        copy_dir_recursive_strict(&emb_src, &emb_dst)?;
        eprintln!(
            "[athena] usb export: copying reranker model to {}",
            rr_dst.display()
        );
        copy_dir_recursive_strict(&rr_src, &rr_dst)?;
    }

    let kiwix_str = kiwix_downloads_dir(&app_data_dir)
        .to_string_lossy()
        .to_string();

    if tier == OmnibusTier::Recommended {
        // Tier 2: priority-based fill of payload packs until 500MB remains free.
        let _ = tier2_copy_payload_priority_fill(&app, &dest_root);
    }

    if tier == OmnibusTier::Absolute {
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
        let et_label = match tier {
            OmnibusTier::Essentials => "essentials",
            OmnibusTier::Recommended => "recommended",
            OmnibusTier::Absolute => "absolute",
        }
        .to_string();
        verify_usb_export(&dest_root, &et_label, &app_data_dir, &emb_name, &rr_name)?;
        execute_scorched_wipe(&*state, &app_data_dir, &cfg, &kiwix_str)?;
    }

    eprintln!("[athena] usb export: finished");
    Ok(())
}

#[cfg(test)]
mod fairlee_tests {
    use super::approx_bbox_for_radius_miles;

    #[test]
    fn fairlee_vt_25mi_bbox_contains_center() {
        let (min_lon, min_lat, max_lon, max_lat) =
            approx_bbox_for_radius_miles(-72.143_f64, 43.908_f64, 25.0_f64);
        assert!((-72.143_f64) >= min_lon && (-72.143_f64) <= max_lon, "lon range");
        assert!(43.908_f64 >= min_lat && 43.908_f64 <= max_lat, "lat range");
        assert!(max_lon > min_lon && max_lat > min_lat);
    }
}

