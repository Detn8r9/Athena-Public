use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::Emitter;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crate::db::Database;
use crate::indexer;
use crate::quarantine;
use crate::vector_index;

pub const FILE_QUARANTINED_EVENT: &str = "athena:file-quarantined";

pub enum WatchCommand {
    SetFolders(Vec<String>),
}

const DEBOUNCE_MS: u64 = 600;

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
fn try_process_file(
    db: &Database,
    path: &PathBuf,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
) {
    if !path.is_file() {
        return;
    }
    if !indexer::is_supported_path(path) {
        return;
    }
    let path_str = path.to_string_lossy();

    if path.extension().map_or(false, |e| e.eq_ignore_ascii_case("zim")) {
        let result = catch_unwind(AssertUnwindSafe(|| indexer::index_zim_file(path)));
        match result {
            Ok(vec) => {
                if vec.is_empty() {
                    if let Ok(qpath) = quarantine::move_to_quarantine(path, app_data_dir) {
                        emit_quarantine(
                            app,
                            &path_str,
                            "ZIM file could not be read or had no text entries.",
                            &qpath.to_string_lossy(),
                        );
                    }
                    return;
                }
                let _ = db.delete_by_path_prefix(&path_str);
                for data in vec {
                    let _ = db.upsert_file(
                        &data.path,
                        &data.file_type,
                        &data.name,
                        Some(data.size),
                        data.modified_at,
                        data.text_content.as_deref(),
                        data.metadata_json.as_deref(),
                    );
                }
            }
            Err(_) => {
                if let Ok(qpath) = quarantine::move_to_quarantine(path, app_data_dir) {
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

    let result = catch_unwind(AssertUnwindSafe(|| indexer::index_file(path)));
    match result {
        Ok(Some(data)) => {
            let _ = db.upsert_file(
                &data.path,
                &data.file_type,
                &data.name,
                Some(data.size),
                data.modified_at,
                data.text_content.as_deref(),
                data.metadata_json.as_deref(),
            );
            if let Some(ref text) = data.text_content {
                if !text.trim().is_empty() {
                    let _ = vector_index::index_document(db, &path_str, text);
                }
            }
        }
        Ok(None) => {
            if let Ok(qpath) = quarantine::move_to_quarantine(path, app_data_dir) {
                emit_quarantine(
                    app,
                    &path_str,
                    "File could not be read or format is corrupted.",
                    &qpath.to_string_lossy(),
                );
            }
        }
        Err(_) => {
            if let Ok(qpath) = quarantine::move_to_quarantine(path, app_data_dir) {
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
    db: &Database,
    root: &str,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
) {
    let root_path = PathBuf::from(root);
    if !root_path.is_dir() {
        return;
    }
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if path.is_file() {
            try_process_file(db, &path, app_data_dir, app);
        }
    }
}

fn process_event_path(
    db: &Database,
    path: &PathBuf,
    kind: &EventKind,
    app_data_dir: &std::path::Path,
    app: &tauri::AppHandle,
) {
    if path.is_dir() {
        if matches!(kind, EventKind::Remove(_)) {
            let path_str = path.to_string_lossy();
            let _ = db.delete_by_path(&path_str);
            let _ = db.delete_embeddings_by_path_prefix(&path_str);
        }
        return;
    }
    match kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            try_process_file(db, path, app_data_dir, app);
        }
        EventKind::Remove(_) => {
            let path_str = path.to_string_lossy();
            if path.extension().map_or(false, |e| e.eq_ignore_ascii_case("zim")) {
                let _ = db.delete_by_path_prefix(&path_str);
            } else {
                let _ = db.delete_by_path(&path_str);
                let _ = db.delete_embeddings_for_file(&path_str);
            }
        }
        _ => {}
    }
}

pub fn run_watcher_loop(
    db: Arc<Database>,
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

        loop {
            if let Some(folders) = pending_folders.take() {
                _watcher_guard = None;
                event_rx = None;
                let (tx, rx) = mpsc::channel();
                let config = Config::default().with_poll_interval(Duration::from_secs(1));
                match RecommendedWatcher::new(
                    move |res: Result<notify::Event, notify::Error>| {
                        let _ = tx.send(res);
                    },
                    config,
                ) {
                    Ok(mut watcher) => {
                        let mut any_ok = false;
                        for folder in &folders {
                            let folder_path = PathBuf::from(folder);
                            if folder_path.is_dir() {
                                initial_scan(db.as_ref(), folder, &app_data_dir, &app);
                                if watcher.watch(&folder_path, RecursiveMode::Recursive).is_ok() {
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
                    Err(_) => {}
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
                        for (path, kind) in pending_events.drain() {
                            process_event_path(db.as_ref(), &path, &kind, &app_data_dir, &app);
                        }
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
                        pending_events.insert(path, ev.kind.clone());
                    }
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    for (path, kind) in pending_events.drain() {
                        process_event_path(db.as_ref(), &path, &kind, &app_data_dir, &app);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    for (path, kind) in pending_events.drain() {
                        process_event_path(db.as_ref(), &path, &kind, &app_data_dir, &app);
                    }
                    event_rx = None;
                    current_folders.clear();
                }
            }
        }
    });
}
