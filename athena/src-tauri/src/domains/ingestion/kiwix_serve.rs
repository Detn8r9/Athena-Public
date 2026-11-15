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
/// kiwix-serve answers under this path so the LAN server can pass it through unchanged
/// (`http://station:8787/kiwix/...`): phones and tablets cannot reach 127.0.0.1:8080.
pub const KIWIX_URL_ROOT: &str = "/kiwix";

/// Base URL of the local kiwix-serve, for the station's own requests.
pub fn local_base_url() -> String {
    format!("http://127.0.0.1:{KIWIX_SERVE_PORT}{KIWIX_URL_ROOT}")
}

/// Archives kiwix-serve refused to open (cut-off download, damaged file), with the size they had.
/// One bad file makes kiwix-serve exit, so it is left out until the file changes.
fn refused_zims() -> &'static Mutex<std::collections::HashMap<String, u64>> {
    static R: std::sync::OnceLock<Mutex<std::collections::HashMap<String, u64>>> = std::sync::OnceLock::new();
    R.get_or_init(Default::default)
}

fn file_len(p: &str) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// The archive named in kiwix-serve's "cannot open" message, if this line is one.
pub(crate) fn refused_zim_in_line(line: &str) -> Option<String> {
    let rest = line.split("Unable to add the ZIM file '").nth(1)?;
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

/// Archives kiwix-serve refused (and that have not changed since), by path.
pub fn refused_list() -> std::collections::HashSet<String> {
    refused_zims()
        .lock()
        .map(|r| r.iter().filter(|(p, len)| file_len(p) == **len).map(|(p, _)| p.clone()).collect())
        .unwrap_or_default()
}

/// Archives that kiwix-serve can be given, leaving out ones it refused before (unless changed)
/// and second copies of the same file in another folder (same archive, read twice at start).
pub(crate) fn servable_zims(all: Vec<String>) -> Vec<String> {
    let all = first_copy_of_each(all);
    let Ok(mut refused) = refused_zims().lock() else { return all };
    refused.retain(|p, len| file_len(p) == *len && *len > 0);
    all.into_iter().filter(|p| !refused.contains_key(p)).collect()
}

fn zim_file_name(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_ascii_lowercase()
}

/// Keeps the first path for each archive file name.
pub(crate) fn first_copy_of_each(all: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    all.into_iter().filter(|p| seen.insert(zim_file_name(p))).collect()
}

/// How long a freshly started kiwix-serve may take to open its archives before the heartbeat
/// treats it as stuck. A large library on a spinning disk can need several minutes.
const STARTUP_ALLOWANCE: Duration = Duration::from_secs(20 * 60);

/// Which kiwix-serve start is current, whether it is still running, and when it started.
struct Run {
    generation: u64,
    alive: bool,
    started: std::time::Instant,
}

fn current_run() -> &'static Mutex<Option<Run>> {
    static R: std::sync::OnceLock<Mutex<Option<Run>>> = std::sync::OnceLock::new();
    R.get_or_init(Default::default)
}

/// True while the current kiwix-serve is running and still inside its start-up allowance.
pub fn still_starting() -> bool {
    current_run()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|r| r.alive && r.started.elapsed() < STARTUP_ALLOWANCE))
        .unwrap_or(false)
}

/// The first search after start reads the full-text indexes from disk and can take half a
/// minute on a large library. Run one in the background so a member's first question doesn't.
fn warm_up_search() {
    std::thread::spawn(|| {
        for _ in 0..600 {
            if kiwix_http_reachable() {
                let url = format!("{}/search?pattern=water&format=xml&pageLength=1&books.filter.lang=eng", local_base_url());
                let _ = ureq::get(&url).timeout(Duration::from_secs(120)).call();
                return;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

pub struct KiwixServeState(pub Mutex<Option<CommandChild>>);

impl Default for KiwixServeState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

/// Roots under AthenaData where offline `.zim` files are expected (recursive scan per directory).
pub fn zim_discovery_roots(athena_root: &Path) -> Vec<PathBuf> {
    let paths = crate::athena_paths::AthenaPaths::new(athena_root);
    let mut roots = vec![kiwix_downloads_dir(athena_root)];
    // Files downloaded before the download folder was moved stay where they were.
    let default_packages = paths.default_downloads_dir().join("packages");
    if !roots.contains(&default_packages) {
        roots.push(default_packages);
    }
    roots.extend([
        paths.user_library_dir(),
        athena_root.join("library"), // legacy dual-read
        athena_root.join("ZIM_Archives"),
    ]);
    roots
}

/// Primary ZIM install directory under AthenaData (same tree [`zim_discovery_roots`] scans first).
pub fn zim_archives_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let athena = portable::resolved_app_data_dir(app).map_err(|e| e.to_string())?;
    let p = kiwix_downloads_dir(&athena);
    std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Canonical path as a plain string. On Windows `canonicalize` adds a `\\?\` prefix that some
/// programs (kiwix-serve among them) do not understand; a normal drive path is equivalent.
fn plain_path(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => s,
    }
}

pub(crate) fn collect_zim_paths_from_roots(roots: &[PathBuf]) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for root in roots {
        if root.is_file() {
            if root
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
            {
                if let Ok(can) = std::fs::canonicalize(root) {
                    seen.insert(plain_path(&can));
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
                seen.insert(plain_path(&can));
            }
        }
    }
    seen.into_iter().collect()
}

pub fn kiwix_http_reachable() -> bool {
    let url = format!("{}/", local_base_url());
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
    spawn(app, false)
}

#[cfg(not(mobile))]
fn spawn(app: &AppHandle, force: bool) -> Result<(), String> {
    let Some(state) = app.try_state::<KiwixServeState>() else {
        return Ok(());
    };
    let athena = portable::resolved_app_data_dir(app).map_err(|e| e.to_string())?;
    let roots = zim_discovery_roots(&athena);
    let zims = servable_zims(collect_zim_paths_from_roots(&roots));
    let zim_count = zims.len();
    let fingerprint = library_fingerprint(&zims);
    // Same archives as the running kiwix-serve: keep it. Restarting would throw away the minutes
    // it spends opening a large library, and ingestion asks for a reload for every archive.
    let running = current_run().lock().ok().and_then(|g| g.as_ref().map(|r| r.alive)).unwrap_or(false);
    if !force && running && !zims.is_empty() && served_fingerprint().lock().map(|f| *f == fingerprint).unwrap_or(false) {
        return Ok(());
    }
    if let Ok(mut f) = served_fingerprint().lock() {
        *f = fingerprint;
    }
    if let Ok(mut served) = served_zims().lock() {
        *served = zims.iter().cloned().collect();
    }
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
        format!("--urlRootLocation={KIWIX_URL_ROOT}"),
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
            let generation = {
                let mut g = current_run().lock().map_err(|_| "kiwix run lock poisoned".to_string())?;
                let next = g.as_ref().map(|r| r.generation + 1).unwrap_or(1);
                *g = Some(Run { generation: next, alive: true, started: std::time::Instant::now() });
                next
            };
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_shell::process::CommandEvent;
                let mut refused_now = false;
                while let Some(ev) = rx.recv().await {
                    match ev {
                        CommandEvent::Stderr(bytes) | CommandEvent::Stdout(bytes) => {
                            let line = String::from_utf8_lossy(&bytes);
                            if let Some(bad) = refused_zim_in_line(&line) {
                                eprintln!("[athena] kiwix-serve: cannot open {bad}; serving the other archives without it.");
                                let key = std::fs::canonicalize(&bad)
                                    .map(|p| plain_path(&p))
                                    .unwrap_or(bad);
                                let len = file_len(&key);
                                if let Ok(mut r) = refused_zims().lock() {
                                    r.insert(key, len);
                                }
                                refused_now = true;
                            }
                        }
                        CommandEvent::Terminated(_) => {
                            if let Ok(mut g) = current_run().lock() {
                                if let Some(r) = g.as_mut().filter(|r| r.generation == generation) {
                                    r.alive = false;
                                }
                            }
                            if refused_now {
                                if let Err(e) = spawn_or_update(&app2) {
                                    eprintln!("[athena] kiwix-serve: restart without refused archives failed: {e}");
                                }
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            });
            warm_up_search();
            let mut guard = state.0.lock().map_err(|_| "KiwixServeState lock poisoned.".to_string())?;
            *guard = Some(child);
            eprintln!(
                "[athena] kiwix-serve: started on http://127.0.0.1:{} with {} ZIM file(s); large libraries take a few minutes to open.",
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

/// Paths and sizes of the archives handed to the running kiwix-serve.
fn served_fingerprint() -> &'static Mutex<Vec<(String, u64)>> {
    static F: std::sync::OnceLock<Mutex<Vec<(String, u64)>>> = std::sync::OnceLock::new();
    F.get_or_init(Default::default)
}

pub(crate) fn library_fingerprint(zims: &[String]) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = zims.iter().map(|p| (p.clone(), file_len(p))).collect();
    v.sort();
    v
}

fn served_zims() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static SERVED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    SERVED.get_or_init(Default::default)
}

/// A `.zim` appeared in a watched folder (copied in by hand). Reload the sidecar unless it is
/// already serving that file, so the archive shows up in Academy without restarting ATHENA.
pub fn serve_new_zim(app: &AppHandle, path: &Path) {
    let key = plain_path(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    if served_zims().lock().map(|s| s.contains(&key)).unwrap_or(false) {
        return;
    }
    restart_after_library_change(app);
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
    let zims = servable_zims(collect_zim_paths_from_roots(&roots));
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
    // Still opening its archives: restarting now would throw that work away and start over,
    // which on a big library repeats forever.
    if still_starting() {
        return;
    }

    eprintln!("[athena] kiwix-serve: not running or not answering; restarting it.");
    if let Err(e) = spawn(app, true) {
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

#[cfg(test)]
mod refused_tests {
    use super::*;

    #[test]
    fn each_archive_is_served_once() {
        let v = first_copy_of_each(vec![
            r"T:\ZIM\ifixit_en.zim".to_string(),
            r"T:\packages\ifixit_en.zim".to_string(),
            "/a/osewiki.zim".to_string(),
        ]);
        assert_eq!(v, vec![r"T:\ZIM\ifixit_en.zim".to_string(), "/a/osewiki.zim".to_string()]);
    }

    #[test]
    fn windows_paths_lose_the_verbatim_prefix() {
        assert_eq!(plain_path(Path::new(r"\\?\T:\A\b.zim")), r"T:\A\b.zim");
        assert_eq!(plain_path(Path::new("/x/b.zim")), "/x/b.zim");
    }

    #[test]
    fn reads_the_archive_kiwix_serve_refused() {
        let line = "Unable to add the ZIM file 'T:\\AthenaArchive\\packages\\gutenberg.zim' to the internal library.";
        assert_eq!(refused_zim_in_line(line).as_deref(), Some("T:\\AthenaArchive\\packages\\gutenberg.zim"));
        assert_eq!(refused_zim_in_line("The Kiwix server is running"), None);
    }
}
