use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::Semaphore;

use crate::db::Database;
use crate::deployment::athena_data_dir;
use crate::indexer;
use crate::system;
use crate::{library_ingest, watcher};

pub const INGESTION_QUEUE_EVENT: &str = "athena:ingestion-queue";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestionQueueItem {
    pub id: String,
    pub file_path: String,
    pub display_name: String,
    pub source_package: String,
    pub phase: String,
    pub pct: u8,
    pub queued_at_ms: i64,
}

#[derive(Default)]
struct Inner {
    queued: VecDeque<IngestionQueueItem>,
    in_progress: HashMap<String, IngestionQueueItem>,
    last_emit: Option<Instant>,
    worker_running: bool,
}

#[derive(Clone, Default)]
pub struct IngestionQueue {
    inner: Arc<Mutex<Inner>>,
}

fn now_ms() -> i64 {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0));
    ts.as_millis() as i64
}

fn normalize_source_package(raw: Option<&str>) -> String {
    let s = raw.unwrap_or("").trim();
    if s.is_empty() {
        "Downloads".to_string()
    } else {
        s.to_string()
    }
}

fn should_emit(inner: &mut Inner) -> bool {
    match inner.last_emit {
        None => true,
        Some(t) => t.elapsed() >= Duration::from_millis(200),
    }
}

fn emit_queue_snapshot(app: &tauri::AppHandle, inner: &mut Inner) {
    if !should_emit(inner) {
        return;
    }
    inner.last_emit = Some(Instant::now());
    let queued: Vec<IngestionQueueItem> = inner.queued.iter().cloned().collect();
    let mut in_progress: Vec<IngestionQueueItem> = inner.in_progress.values().cloned().collect();
    in_progress.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    let _ = app.emit(
        INGESTION_QUEUE_EVENT,
        serde_json::json!({
            "queued": queued,
            "inProgress": in_progress,
        }),
    );
}

fn is_on_battery() -> bool {
    let b = system::get_battery_state().unwrap_or(system::BatteryState {
        percentage: None,
        is_charging: None,
        time_to_empty_minutes: None,
    });
    // If we cannot determine, default to battery-friendly.
    let charging = b.is_charging.unwrap_or(false);
    !charging
}

fn concurrency_limit() -> usize {
    if is_on_battery() {
        1
    } else {
        2
    }
}

fn enqueue_item(
    inner: &mut Inner,
    file_path: &Path,
    source_package: &str,
    phase: &str,
) {
    let display_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let file_path = file_path.to_string_lossy().to_string();
    let id = format!("{}::{}", file_path, now_ms());
    inner.queued.push_back(IngestionQueueItem {
        id,
        file_path,
        display_name,
        source_package: source_package.to_string(),
        phase: phase.to_string(),
        pct: 0,
        queued_at_ms: now_ms(),
    });
}

fn walk_files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            out.extend(walk_files_under(&p));
        } else if p.is_file() {
            out.push(p);
        }
    }
    out
}

async fn process_one_path(
    app: &tauri::AppHandle,
    queue: &IngestionQueue,
    item_id: &str,
    db: &Arc<Mutex<Database>>,
    path: PathBuf,
    source_package: String,
) {
    let app_data_dir = match athena_data_dir(app) {
        Ok(p) => p,
        Err(_) => return,
    };

    // ZIM: keep indexing/embedding in smaller batches; use kiwix-serve for content availability.
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zim")) {
        // Serve it in Academy and chat search (no restart when kiwix-serve already has it).
        crate::kiwix_serve::serve_new_zim(app, &path);

        // Stream progress to the UI (header bar) while ZIM parsing runs.
        watcher::emit_ingestion_status(app, true, path.file_name().and_then(|n| n.to_str()));
        let db2 = Arc::clone(db);
        let app2 = app.clone();
        let queue2 = queue.clone();
        let item_id2 = item_id.to_string();
        let app_data_dir2 = app_data_dir.clone();
        let source_package2 = source_package.clone();
        let path2 = path.clone();

        let _ = tokio::task::spawn_blocking(move || {
            // One catalog row per archive; kiwix-serve searches the articles themselves.
            let rows: Vec<_> = indexer::zim_catalog_entry(&path2).into_iter().collect();

            if rows.is_empty() {
                watcher::emit_ingestion_status(&app2, false, None);
                return;
            }

            let Ok(guard) = db2.lock() else {
                watcher::emit_ingestion_status(&app2, false, None);
                return;
            };
            let db_ref = &*guard;
            let base_path = path2.to_string_lossy().to_string();
            let _ = db_ref.delete_by_path_prefix(&base_path);
            for (i, data) in rows.into_iter().enumerate() {
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
                if i == 0 || (i + 1) % 2000 == 0 {
                    if let Ok(mut g) = queue2.inner.lock() {
                        if let Some(it) = g.in_progress.get_mut(&item_id2) {
                            it.pct = 96;
                            it.phase = "writing".into();
                        }
                        emit_queue_snapshot(&app2, &mut g);
                    }
                }
            }

            // Source package tagging (library UI grouping) for the ZIM file path itself.
            let _ = db_ref.upsert_rag_document_source_package(&base_path, &source_package2);

            // Ensure watcher context (archive move, etc.) remains consistent for ZIM installs.
            let _ = crate::library_ingest::move_unsorted_to_archive(&path2, &app_data_dir2);
            watcher::emit_ingestion_status(&app2, false, None);
        })
        .await;

        return;
    }

    // Archive extraction for downloaded packages: extract in-place, then delete archive.
    if library_ingest::is_library_archive(&path) {
        watcher::emit_ingestion_status(app, true, path.file_name().and_then(|n| n.to_str()));
        if let Ok(extracted_root) = library_ingest::extract_archive_in_place_with_progress_async(
            app.clone(),
            path.clone(),
        )
        .await
        {
            let _ = tokio::fs::remove_file(&path).await;
            let files = walk_files_under(&extracted_root);
            for f in files {
                // Best-effort: index immediately rather than waiting for filesystem watcher debounce.
                watcher::try_process_file(db, &f, &app_data_dir, app, None);
            }
        }
        watcher::emit_ingestion_status(app, false, None);
        return;
    }

    // Default: index directly (includes OCR/PDF).
    // NOTE: Indexing can be CPU-heavy. Keep it off async executor threads.
    let db2 = Arc::clone(db);
    let app2 = app.clone();
    let app_data_dir2 = app_data_dir.clone();
    let path2 = path.clone();
    let _ = tokio::task::spawn_blocking(move || {
        watcher::try_process_file(&db2, &path2, &app_data_dir2, &app2, None);
    })
    .await;

    // Record source package for RAG document registration when embeddings exist.
    if let Ok(g) = db.lock() {
        let _ = g.upsert_rag_document_source_package(&path.to_string_lossy(), &source_package);
    }
}

impl IngestionQueue {
    pub fn enqueue_completed_download(
        &self,
        app: &tauri::AppHandle,
        db: &Arc<Mutex<Database>>,
        completed_file_path: PathBuf,
        source_package: Option<String>,
    ) {
        let source_package = normalize_source_package(source_package.as_deref());
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        enqueue_item(&mut guard, &completed_file_path, &source_package, "queued");
        emit_queue_snapshot(app, &mut guard);
        if guard.worker_running {
            return;
        }
        guard.worker_running = true;
        drop(guard);

        let queue = self.clone();
        let app = app.clone();
        let db = Arc::clone(db);
        tauri::async_runtime::spawn(async move {
            queue.run_worker_loop(app, db).await;
        });
    }

    async fn run_worker_loop(&self, app: tauri::AppHandle, db: Arc<Mutex<Database>>) {
        loop {
            let limit = concurrency_limit();
            let sem = Arc::new(Semaphore::new(limit));
            let mut handles = Vec::new();

            for _ in 0..limit {
                let next = {
                    let mut guard = match self.inner.lock() {
                        Ok(g) => g,
                        Err(_) => return,
                    };
                    let Some(mut item) = guard.queued.pop_front() else {
                        // No work
                        guard.worker_running = false;
                        emit_queue_snapshot(&app, &mut guard);
                        return;
                    };
                    item.phase = "indexing".into();
                    item.pct = 5;
                    let id = item.id.clone();
                    guard.in_progress.insert(id.clone(), item.clone());
                    emit_queue_snapshot(&app, &mut guard);
                    item
                };

                let permit = sem.clone().acquire_owned().await;
                if permit.is_err() {
                    return;
                }
                let _permit = permit.unwrap();
                let app2 = app.clone();
                let db2 = Arc::clone(&db);
                let queue2 = self.clone();
                handles.push(tauri::async_runtime::spawn(async move {
                    let path = PathBuf::from(next.file_path.clone());
                    process_one_path(&app2, &queue2, &next.id, &db2, path, next.source_package.clone()).await;
                    let mut guard = match queue2.inner.lock() {
                        Ok(g) => g,
                        Err(_) => return,
                    };
                    guard.in_progress.remove(&next.id);
                    emit_queue_snapshot(&app2, &mut guard);
                }));
            }

            for h in handles {
                let _ = h.await;
            }
        }
    }
}

