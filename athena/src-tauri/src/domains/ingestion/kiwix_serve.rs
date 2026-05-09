//! Bundled `kiwix-serve` sidecar: serves `.zim` archives discovered under the AthenaData root
//! (`kiwix_downloads/`, `library/`, `ZIM_Archives/`) on 127.0.0.1:8080.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;
use walkdir::WalkDir;

use crate::kiwix_downloads_dir;
use crate::portable;

pub const KIWIX_SERVE_PORT: u16 = 8080;

pub struct KiwixServeState(pub Mutex<Option<CommandChild>>);

impl Default for KiwixServeState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

/// Roots under AthenaData where offline `.zim` files are expected (recursive scan per directory).
pub fn zim_discovery_roots(athena_root: &Path) -> Vec<PathBuf> {
    vec![
        kiwix_downloads_dir(athena_root),
        athena_root.join("library"),
        athena_root.join("ZIM_Archives"),
    ]
}

/// Primary ZIM install directory under AthenaData (same tree [`zim_discovery_roots`] scans first).
pub fn zim_archives_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let athena = portable::resolved_app_data_dir(app).map_err(|e| e.to_string())?;
    let p = kiwix_downloads_dir(&athena);
    std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn collect_zim_paths_from_roots(roots: &[PathBuf]) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for root in roots {
        if root.is_file() {
            if root
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
            {
                if let Ok(can) = std::fs::canonicalize(root) {
                    seen.insert(can.to_string_lossy().to_string());
                }
            }
            continue;
        }
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(64)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            if !p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
            {
                continue;
            }
            if let Ok(can) = std::fs::canonicalize(p) {
                seen.insert(can.to_string_lossy().to_string());
            }
        }
    }
    seen.into_iter().collect()
}

fn kiwix_http_reachable() -> bool {
    let url = format!("http://127.0.0.1:{}/", KIWIX_SERVE_PORT);
    ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .is_ok()
}

#[cfg(not(mobile))]
fn kill_stored_child(state: &KiwixServeState) {
    let Ok(mut guard) = state.0.lock() else {
        return;
    };
    if let Some(child) = guard.take() {
        if let Err(e) = child.kill() {
            eprintln!("[athena] kiwix-serve: failed to stop sidecar: {e}");
        }
    }
}

/// Spawn or restart `kiwix-serve` with every `.zim` under AthenaData discovery roots.
#[cfg(not(mobile))]
pub fn spawn_or_update(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<KiwixServeState>() else {
        return Ok(());
    };
    let athena = portable::resolved_app_data_dir(app).map_err(|e| e.to_string())?;
    let roots = zim_discovery_roots(&athena);
    let zims = collect_zim_paths_from_roots(&roots);
    let zim_count = zims.len();
    kill_stored_child(&state);

    if zims.is_empty() {
        eprintln!(
            "[athena] kiwix-serve: no .zim under AthenaData ({}) — server not started.",
            athena.display()
        );
        return Ok(());
    }

    let mut args: Vec<String> = vec![
        format!("--port={}", KIWIX_SERVE_PORT),
        "--address=127.0.0.1".to_string(),
        "-k".to_string(),
    ];
    args.extend(zims);

    let sidecar = match app.shell().sidecar("kiwix-serve") {
        Ok(s) => s,
        Err(e) => {
            // `kiwix-serve` is optional (GPL-3). If it isn't bundled/configured, don't fail downloads/ingestion.
            eprintln!(
                "[athena] kiwix-serve: sidecar not available; skipping ZIM HTTP server spawn: {e}"
            );
            return Ok(());
        }
    };
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match sidecar.args(arg_refs).spawn() {
        Ok((mut rx, child)) => {
            tauri::async_runtime::spawn(async move {
                while rx.recv().await.is_some() {}
            });
            let mut guard = state.0.lock().map_err(|_| "KiwixServeState lock poisoned.".to_string())?;
            *guard = Some(child);
            eprintln!(
                "[athena] kiwix-serve: listening on http://127.0.0.1:{} ({} ZIM file(s) from AthenaData).",
                KIWIX_SERVE_PORT, zim_count
            );
            Ok(())
        }
        Err(e) => Err(format!("kiwix-serve spawn failed: {e}")),
    }
}

#[cfg(mobile)]
pub fn spawn_or_update(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}

/// After a successful download, reload the sidecar so new archives appear in the web UI.
pub fn restart_after_library_change(app: &AppHandle) {
    if let Err(e) = spawn_or_update(app) {
        eprintln!("[athena] kiwix-serve: restart after ZIM change failed: {e}");
    }
}

/// Heartbeat: if we have ZIMs but the HTTP front is down, restart the sidecar.
#[cfg(not(mobile))]
pub fn supervise_or_restart(app: &AppHandle) {
    let Ok(athena) = portable::resolved_app_data_dir(app) else {
        return;
    };
    let roots = zim_discovery_roots(&athena);
    let zims = collect_zim_paths_from_roots(&roots);
    let Some(state) = app.try_state::<KiwixServeState>() else {
        return;
    };

    if zims.is_empty() {
        kill_stored_child(&state);
        return;
    }

    if kiwix_http_reachable() {
        return;
    }

    eprintln!("[athena] kiwix-serve: HTTP not reachable; restarting sidecar.");
    if let Err(e) = spawn_or_update(app) {
        eprintln!("[athena] kiwix-serve: supervise restart failed: {e}");
    }
}

#[cfg(mobile)]
pub fn supervise_or_restart(_app: &AppHandle) {}

#[cfg(not(mobile))]
pub fn kill_if_running(app: &AppHandle) {
    let Some(state) = app.try_state::<KiwixServeState>() else {
        return;
    };
    kill_stored_child(&state);
}

#[cfg(mobile)]
pub fn kill_if_running(_app: &AppHandle) {}
