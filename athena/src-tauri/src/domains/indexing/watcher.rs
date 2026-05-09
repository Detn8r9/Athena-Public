use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

use crate::config::load_config;
use crate::db::Database;
use crate::indexer;
use crate::library_ingest;
use crate::quarantine;
use crate::vector_index;
use crate::{compute_watcher_folders, kiwix_downloads_dir};

pub const FILE_QUARANTINED_EVENT: &str = "athena:file-quarantined";
pub const ARCHIVE_FS_CHANGED_EVENT: &str = "athena:archive-fs-changed";
pub const INGESTION_STATUS_EVENT: &str = "athena:ingestion-status";

pub fn emit_ingestion_status(app: &tauri::AppHandle, is_ingesting: bool, filename: Option<&str>) {
    let _ = app.emit(
        INGESTION_STATUS_EVENT,
        serde_json::json!({
            "isIngesting": is_ingesting,
            "filename": filename,
        }),
    );
}

/// Emits `isIngesting: false` on drop so the UI never sticks on loading after success, error, or early return.
pub struct IngestionActivityGuard<'a> {
    app: &'a tauri::AppHandle,
}

impl<'a> IngestionActivityGuard<'a> {
    pub fn begin(app: &'a tauri::AppHandle, filename: Option<&str>) -> Self {
        emit_ingestion_status(app, true, filename);
        Self { app }
    }
}

impl<'a> Drop for IngestionActivityGuard<'a> {
    fn drop(&mut self) {
        emit_ingestion_status(self.app, false, None);
    }
}

pub enum WatchCommand {
    SetFolders(Vec<String>),
}

const DEBOUNCE_MS: u64 = 600;
/// Process at most this many file paths before yielding so the OS can schedule other work.
const WATCHER_BATCH_SIZE: usize = 10;
const FILE_STABLE_MS: u64 = 600;
const FILE_STABLE_MAX_WAIT_MS: u64 = 30_000;
/// Ignore further notify events for the same path while ingesting or within this window after completion.
const INGEST_EVENT_COOLDOWN: Duration = Duration::from_secs(2);

/// Coalesces duplicate watcher notifications (Create/Modify bursts) to avoid overlapping DB work.
#[derive(Default)]
struct IngestDebounceState {
    in_flight: HashSet<PathBuf>,
    last_done: HashMap<PathBuf, std::time::Instant>,
}

impl IngestDebounceState {
    fn should_skip(&mut self, path: &PathBuf) -> bool {
        if self.in_flight.contains(path) {
            return true;
        }
        if let Some(t) = self.last_done.get(path) {
            if t.elapsed() < INGEST_EVENT_COOLDOWN {
                return true;
            }
        }
        false
    }

    fn begin(&mut self, path: PathBuf) {
        self.in_flight.insert(path);
    }

    fn finish(&mut self, path: &PathBuf) {
        self.in_flight.remove(path);
        self.last_done
            .insert(path.clone(), std::time::Instant::now());
        self.last_done.retain(|_, t| {
            t.elapsed() < INGEST_EVENT_COOLDOWN + Duration::from_secs(10)
        });
    }
}

struct IngestDebounceGuard<'a> {
    state: &'a mut IngestDebounceState,
    path: PathBuf,
}

impl Drop for IngestDebounceGuard<'_> {
    fn drop(&mut self) {
        self.state.finish(&self.path);
    }
}

fn file_signature(path: &PathBuf) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    let len = meta.len();
    let modified = meta.modified().ok()?;
    Some((len, modified))
}

fn wait_for_stable_file(path: &PathBuf) -> bool {
    let start = std::time::Instant::now();
    loop {
        let Some(sig_a) = file_signature(path) else {
            return false;
        };
        std::thread::sleep(Duration::from_millis(FILE_STABLE_MS));
        let Some(sig_b) = file_signature(path) else {
            return false;
        };
        if sig_a == sig_b {
            return true;
        }
        if start.elapsed() >= Duration::from_millis(FILE_STABLE_MAX_WAIT_MS) {
            return false;
        }
    }
}

fn flush_pending_events_map(
    pending_events: &mut HashMap<PathBuf, EventKind>,
    db: &Arc<Mutex<Database>>,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
    ingest_debounce: &mut IngestDebounceState,
) -> bool {
    let had_work = !pending_events.is_empty();
    let keys: Vec<PathBuf> = pending_events.keys().cloned().collect();
    for chunk in keys.chunks(WATCHER_BATCH_SIZE) {
        for path in chunk {
            if let Some(kind) = pending_events.remove(path) {
                process_event_path(
                    db,
                    path,
                    &kind,
                    app_data_dir,
                    app,
                    ingest_debounce,
                );
            }
        }
        thread::yield_now();
    }
    if had_work {
        let _ = app.emit(ARCHIVE_FS_CHANGED_EVENT, serde_json::json!({}));
    }
    had_work
}

/// Stable key for the built-in Kiwix downloads directory (no `archive_watch_roots` row).
const KIWIX_BUILTIN_ROOT_KEY: &str = "__athena_builtin_kiwix_downloads__";

fn stored_root_key_for_scan_folder(
    resolved_folder_str: &str,
    app_data_dir: &Path,
    db: &Database,
) -> String {
    let root_pb = PathBuf::from(resolved_folder_str);
    let kiwix = kiwix_downloads_dir(app_data_dir);
    if let (Ok(a), Ok(b)) = (root_pb.canonicalize(), kiwix.canonicalize()) {
        if a == b {
            return KIWIX_BUILTIN_ROOT_KEY.to_string();
        }
    }
    match db.archive_watch_root_stored_path_for_resolved(&root_pb) {
        Ok(Some(s)) => s.trim().replace('\\', "/"),
        _ => resolved_folder_str.trim().replace('\\', "/"),
    }
}

pub(crate) fn resolve_ingest_context(
    db: &Database,
    app_data_dir: &Path,
    file_path: &Path,
) -> Option<(String, PathBuf)> {
    let config = load_config(app_data_dir);
    let folders = compute_watcher_folders(db, app_data_dir, &config).ok()?;
    let file = file_path.canonicalize().ok()?;
    for folder in folders {
        let root = PathBuf::from(&folder);
        let root_canon = root.canonicalize().unwrap_or(root.clone());
        if file.starts_with(&root_canon) {
            let key = stored_root_key_for_scan_folder(&folder, app_data_dir, db);
            return Some((key, root_canon));
        }
    }
    None
}

fn purge_ghost_index_entries(db: &Arc<Mutex<Database>>, root: &str, seen_abs: &HashSet<String>) {
    let guard = match db.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let indexed = match guard.list_indexed_paths_for_root(root) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[athena] ghost index cleanup: list paths: {e}");
            return;
        }
    };
    let mut zim_cleared: HashSet<String> = HashSet::new();
    for db_path in indexed {
        if let Some(idx) = db_path.rfind("::") {
            let zim_base = &db_path[..idx];
            if zim_cleared.contains(zim_base) {
                continue;
            }
            let on_disk = Path::new(zim_base).exists();
            let was_seen = seen_abs.contains(zim_base);
            if was_seen || on_disk {
                continue;
            }
            if let Err(e) = guard.delete_by_path_prefix(zim_base) {
                eprintln!("[athena] ghost index cleanup: zim prefix {zim_base}: {e}");
            } else {
                zim_cleared.insert(zim_base.to_string());
            }
            continue;
        }
        if seen_abs.contains(&db_path) {
            continue;
        }
        if Path::new(&db_path).exists() {
            continue;
        }
        if let Err(e) = guard.remove_file_from_index(&db_path) {
            eprintln!("[athena] ghost index cleanup: {db_path}: {e}");
        }
    }
}

fn emit_quarantine(app: &tauri::AppHandle, path: &str, reason: &str, quarantine_path: &str) {
    let _ = app.emit(
        FILE_QUARANTINED_EVENT,
        serde_json::json!({
            "path": path,
            "reason": reason,
            "quarantine_path": quarantine_path,
        }),
    );
}

/// Try to index one file. On panic or extraction failure, move to quarantine and emit. Never panics.
/// Does not hold `state.db` while waiting for file stability or while reading files from disk.
/// Index a single library file (ZIM, PDF, etc.); used by the watch pipeline and mesh P2P post-replication ingest.
///
/// Concurrent mesh downloads may finish together and schedule several `spawn_blocking` ingests; they serialize on
/// `AppState.db` (outer `Mutex`). The underlying SQLite connection also uses WAL, `busy_timeout`, and bounded retries
/// on `SQLITE_BUSY` for ZIM clears and `upsert_file` so a transient lock does not drop a completed replication ingest.
pub(crate) fn try_process_file(
    db: &Arc<Mutex<Database>>,
    path: &PathBuf,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
    explicit_ctx: Option<(&str, &Path)>,
) {
    if !path.is_file() {
        return;
    }
    if !indexer::is_supported_path(path) {
        return;
    }

    // Stability gate: delay indexing until file size and modified time have remained unchanged
    // for at least 600ms (common for downloads that finalize via rename/move).
    if !wait_for_stable_file(path) {
        return;
    }

    let ingest_path = match library_ingest::copy_watch_file_to_unsorted_if_external(path, app_data_dir) {
        Ok(Some(p)) => p,
        Ok(None) => path.clone(),
        Err(e) => {
            eprintln!(
                "[athena] watch-folder copy to Unsorted failed ({}): {e}",
                path.display()
            );
            path.clone()
        }
    };

    let path_str = ingest_path.to_string_lossy();
    let ingest_display_name = ingest_path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty());

    if ingest_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
    {
        let _ingest_guard = IngestionActivityGuard::begin(app, ingest_display_name);
        let result = catch_unwind(AssertUnwindSafe(|| indexer::index_zim_file(&ingest_path)));
        match result {
            Ok(vec) => {
                if vec.is_empty() {
                    if let Ok(qpath) = quarantine::move_to_quarantine(&ingest_path, app_data_dir) {
                        emit_quarantine(
                            app,
                            &path_str,
                            "ZIM file could not be read or had no text entries.",
                            &qpath.to_string_lossy(),
                        );
                    }
                    return;
                }
                let Ok(guard) = db.lock() else {
                    return;
                };
                let db_ref = &*guard;
                let _ = db_ref.delete_by_path_prefix(&path_str);
                for data in vec {
                    let _ = db_ref.upsert_file(
                        &data.path,
                        &data.file_type,
                        &data.name,
                        Some(data.size),
                        data.modified_at,
                        data.text_content.as_deref(),
                        data.metadata_json.as_deref(),
                        Some(data.display_title.as_str()),
                        data.page_count.map(|n| n as i64),
                        data.relative_path.as_deref(),
                    );
                }
            }
            Err(_) => {
                if let Ok(qpath) = quarantine::move_to_quarantine(&ingest_path, app_data_dir) {
                    emit_quarantine(
                        app,
                        &path_str,
                        "ZIM file caused an error while indexing.",
                        &qpath.to_string_lossy(),
                    );
                }
            }
        }
        return;
    }

    if library_ingest::is_library_archive(&ingest_path) {
        let _ingest_guard = IngestionActivityGuard::begin(app, ingest_display_name);
        match library_ingest::extract_archive_with_progress(app, &ingest_path, app_data_dir, "archives") {
            Ok(_) => {
                let _ = app.emit(ARCHIVE_FS_CHANGED_EVENT, serde_json::json!({}));
            }
            Err(e) => {
                eprintln!(
                    "[athena] library archive extract failed ({}): {e}",
                    ingest_path.display()
                );
            }
        }
        return;
    }

    let ctx: Option<(String, PathBuf)> = if let Some((k, r)) = explicit_ctx {
        Some((k.to_string(), r.to_path_buf()))
    } else {
        let guard = match db.lock() {
            Ok(g) => g,
            Err(e) => {
                eprintln!(
                    "[athena] Fatal Ingestion Error: db.lock() failed (poisoned mutex?): {:?}",
                    e
                );
                return;
            }
        };
        resolve_ingest_context(&*guard, app_data_dir, &ingest_path)
    };
    let ctx_tuple = ctx.as_ref().map(|(k, r)| (k.as_str(), r.as_path()));

    let skip = {
        let guard = match db.lock() {
            Ok(g) => g,
            Err(e) => {
                eprintln!(
                    "[athena] Fatal Ingestion Error: db.lock() failed (poisoned mutex?): {:?}",
                    e
                );
                return;
            }
        };
        let portable_src = if ingest_path != *path {
            Some(path.as_path())
        } else {
            None
        };
        indexer::skip_if_up_to_date(&ingest_path, &*guard, ctx_tuple, portable_src)
    };
    if skip.is_some() {
        // Backfill RAG metadata in case this file was indexed before `rag_documents` tracking existed.
        // If it also has no embeddings yet, try to rebuild embeddings from stored `indexed_files.text_content`
        // (so the file becomes usable for semantic RAG without re-reading from disk).
        if let Ok(guard) = db.lock() {
            let db_ref = &*guard;
            let emb_n = db_ref
                .chunk_embedding_count_for_file_path(&path_str)
                .unwrap_or(0);
            if emb_n <= 0 {
                if let Ok(Some((file_type, title, text))) =
                    db_ref.indexed_file_rag_backfill_seed(&path_str)
                {
                    let t = text.trim();
                    if !t.is_empty() {
                        let _ = vector_index::index_document(db_ref, &path_str, &title, None, t);
                        let _ = db_ref.upsert_rag_document_for_file_path(&path_str, &file_type);
                    }
                }
            } else {
                let _ = db_ref.upsert_rag_document_for_file_path(&path_str, "");
            }
        }
        if let Err(e) = library_ingest::move_unsorted_to_archive(&ingest_path, app_data_dir) {
            eprintln!(
                "[athena] ingest: archive move skipped ({}): {e}",
                ingest_path.display()
            );
        }
        return;
    }

    let _ingest_guard = IngestionActivityGuard::begin(app, ingest_display_name);
    let result = catch_unwind(AssertUnwindSafe(|| indexer::index_file(&ingest_path)));
    match result {
        Ok(Some(mut data)) => {
            let fill_src: &std::path::Path = if ingest_path != *path {
                path
            } else {
                &ingest_path
            };
            indexer::fill_relative_path_key(&mut data, ctx_tuple, fill_src);
            tracing::info!("index_file returned Some; acquiring DB lock");
            let guard = match db.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!(error = ?e, "Fatal Ingestion Error: db.lock() failed (poisoned mutex?)");
                    return;
                }
            };
            let db_ref = &*guard;
            tracing::info!("calling upsert_file");
            if let Err(e) = db_ref.upsert_file(
                &data.path,
                &data.file_type,
                &data.name,
                Some(data.size),
                data.modified_at,
                data.text_content.as_deref(),
                data.metadata_json.as_deref(),
                Some(data.display_title.as_str()),
                data.page_count.map(|n| n as i64),
                data.relative_path.as_deref(),
            ) {
                tracing::error!(error = %e, "Fatal Ingestion Error: upsert_file failed");
                return;
            }
            tracing::info!(
                text_content_is_some = data.text_content.is_some(),
                trimmed_empty = data.text_content.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true),
                "upsert_file completed"
            );
            if let Some(ref text) = data.text_content {
                if !text.trim().is_empty() {
                    tracing::info!("calling vector_index::index_document");
                    match vector_index::index_document(
                        db_ref,
                        &path_str,
                        &data.display_title,
                        data.index_pages.as_deref(),
                        text,
                    ) {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::error!(error = %e, "Fatal Ingestion Error: vector index");
                        }
                    }
                    // Ensure this file appears in RAG documents list immediately.
                    let _ = db_ref.upsert_rag_document_for_file_path(&path_str, &data.file_type);
                } else {
                    tracing::info!("skipped embeddings: text empty after trim");
                }
            } else {
                tracing::info!("skipped embeddings: no text_content");
            }

            if let Err(e) = library_ingest::move_unsorted_to_archive(&ingest_path, app_data_dir) {
                eprintln!(
                    "[athena] ingest: archive move failed after success ({}): {e}",
                    ingest_path.display()
                );
            }
        }
        Ok(None) => {
            if let Ok(qpath) = quarantine::move_to_quarantine(&ingest_path, app_data_dir) {
                emit_quarantine(
                    app,
                    &path_str,
                    "File could not be read or format is corrupted.",
                    &qpath.to_string_lossy(),
                );
            }
        }
        Err(_) => {
            if let Ok(qpath) = quarantine::move_to_quarantine(&ingest_path, app_data_dir) {
                emit_quarantine(
                    app,
                    &path_str,
                    "File caused an error while indexing.",
                    &qpath.to_string_lossy(),
                );
            }
        }
    }
}

fn initial_scan(
    db: &Arc<Mutex<Database>>,
    root: &str,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
) {
    let root_path = PathBuf::from(root);
    if !root_path.is_dir() {
        return;
    }
    let stored_key = {
        let guard = match db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        stored_root_key_for_scan_folder(root, app_data_dir, &*guard)
    };
    let resolved_root = root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.clone());
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if path.is_file() {
            paths.push(path);
        }
    }
    let mut seen_abs: HashSet<String> = HashSet::new();
    for chunk in paths.chunks(WATCHER_BATCH_SIZE) {
        for path in chunk {
            let abs = path
                .canonicalize()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            seen_abs.insert(abs);
            try_process_file(
                db,
                path,
                app_data_dir,
                app,
                Some((stored_key.as_str(), resolved_root.as_path())),
            );
        }
        thread::yield_now();
    }
    purge_ghost_index_entries(db, root, &seen_abs);
}

fn process_event_path(
    db: &Arc<Mutex<Database>>,
    path: &PathBuf,
    kind: &EventKind,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
    ingest_debounce: &mut IngestDebounceState,
) {
    if path.is_dir() {
        if matches!(kind, EventKind::Remove(_)) {
            let path_str = path.to_string_lossy();
            let Ok(guard) = db.lock() else {
                return;
            };
            let db_ref = &*guard;
            let _ = db_ref.delete_by_path(&path_str);
            let _ = db_ref.delete_embeddings_by_path_prefix(&path_str);
        }
        return;
    }
    match kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            if ingest_debounce.should_skip(path) {
                println!(
                    "[athena] Ignoring duplicate watcher event for {:?}",
                    path
                );
                return;
            }
            ingest_debounce.begin(path.clone());
            let _debounce_guard = IngestDebounceGuard {
                state: ingest_debounce,
                path: path.clone(),
            };
            try_process_file(db, path, app_data_dir, app, None);
        }
        EventKind::Remove(_) => {
            let path_str = path.to_string_lossy();
            let Ok(guard) = db.lock() else {
                return;
            };
            let db_ref = &*guard;
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
            {
                let _ = db_ref.delete_by_path_prefix(&path_str);
            } else {
                let _ = db_ref.delete_by_path(&path_str);
                let _ = db_ref.delete_embeddings_for_file(&path_str);
            }
        }
        _ => {}
    }
}

pub fn run_watcher_loop(
    db: Arc<Mutex<Database>>,
    cmd_rx: crossbeam_channel::Receiver<WatchCommand>,
    app_data_dir: PathBuf,
    app: tauri::AppHandle,
) {
    std::thread::spawn(move || {
        let mut pending_folders: Option<Vec<String>> = None;
        let mut current_folders: Vec<String> = Vec::new();
        let mut _watcher_guard: Option<RecommendedWatcher> = None;
        let mut event_rx: Option<mpsc::Receiver<Result<notify::Event, notify::Error>>> = None;
        let debounce = Duration::from_millis(DEBOUNCE_MS);
        let mut pending_events: HashMap<PathBuf, EventKind> = HashMap::new();
        let mut ingest_debounce = IngestDebounceState::default();

        loop {
            if let Some(folders) = pending_folders.take() {
                _watcher_guard = None;
                event_rx = None;
                let (tx, rx) = mpsc::channel();
                let config = Config::default().with_poll_interval(Duration::from_secs(2));
                if let Ok(mut watcher) = RecommendedWatcher::new(
                    move |res: Result<notify::Event, notify::Error>| {
                        let _ = tx.send(res);
                    },
                    config,
                ) {
                    let mut any_ok = false;
                    for folder in &folders {
                        let folder_path = PathBuf::from(folder);
                        if folder_path.is_dir() {
                            initial_scan(&db, folder, &app_data_dir, &app);
                            if watcher
                                .watch(&folder_path, RecursiveMode::Recursive)
                                .is_ok()
                            {
                                any_ok = true;
                            }
                        }
                    }
                    if any_ok {
                        _watcher_guard = Some(watcher);
                        event_rx = Some(rx);
                        current_folders = folders;
                    }
                }
                continue;
            }

            if current_folders.is_empty() {
                match cmd_rx.recv() {
                    Ok(WatchCommand::SetFolders(folders)) => {
                        pending_folders = Some(folders);
                    }
                    Err(_) => break,
                }
                continue;
            }

            let rx = match event_rx.as_ref() {
                Some(r) => r,
                None => {
                    current_folders.clear();
                    continue;
                }
            };

            if let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    WatchCommand::SetFolders(folders) => {
                        let _ = flush_pending_events_map(
                            &mut pending_events,
                            &db,
                            &app_data_dir,
                            &app,
                            &mut ingest_debounce,
                        );
                        pending_folders = Some(folders);
                        _watcher_guard = None;
                        event_rx = None;
                        current_folders.clear();
                    }
                }
                continue;
            }

            match rx.recv_timeout(debounce) {
                Ok(Ok(ev)) => {
                    for path in ev.paths {
                        pending_events.insert(path, ev.kind);
                    }
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _ = flush_pending_events_map(
                        &mut pending_events,
                        &db,
                        &app_data_dir,
                        &app,
                        &mut ingest_debounce,
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = flush_pending_events_map(
                        &mut pending_events,
                        &db,
                        &app_data_dir,
                        &app,
                        &mut ingest_debounce,
                    );
                    event_rx = None;
                    current_folders.clear();
                }
            }
        }
    });
}
