//! Pipeline: full document text → parent/child chunks → local embeddings → vector DB.
//! Parents carry ~1000-word context; children (~250 words) are embedded for retrieval.
//!
//! ONNX embedding work (`embeddings::embed_text`) is CPU-heavy and must run only from
//! `tokio::task::spawn_blocking` (see [`index_document_spawn_blocking`]), never directly on
//! Tokio cooperative worker threads. SQLite writes use `Database::replace_file_chunk_embeddings_batched`,
//! which commits every `Database::CHUNK_EMBED_SQLITE_BATCH` rows.

use crate::chunking::{
    build_parent_child_plan, words_from_page_map, words_from_plain_text, WordPage,
};
use crate::db::Database;
use crate::embeddings;
use std::sync::{Arc, Mutex};

/// One row for `Database::replace_file_chunk_embeddings_batched`.
pub type ChunkRow = (
    i64,
    Option<&'static str>,
    Option<i64>,
    Option<i32>,
    Option<i32>,
    String,
    String,
    Vec<f32>,
    bool,
    bool,
    Option<f64>,
);

/// Index a library document: delete prior rows, insert parent rows (zero vector, no vec index)
/// and child rows (embedding of `retrieval_text`, sqlite-vec row).
pub fn index_document(
    db: &Database,
    file_path: &str,
    document_title: &str,
    index_pages: Option<&[(u32, String)]>,
    full_text: &str,
) -> Result<(), String> {
    let rows = prepare_document_rows(document_title, index_pages, full_text);
    if rows.is_empty() {
        return Ok(());
    }
    db.replace_file_chunk_embeddings_batched(file_path, rows)
}

/// Same as [`index_document`] for callers holding the shared database handle. Chunking and
/// embedding a book takes minutes on a slow machine; the lock is taken only to write the rows,
/// so the rest of the app (login, chat, every screen) keeps working while a library indexes.
pub fn index_document_shared(
    db: &Mutex<Database>,
    file_path: &str,
    document_title: &str,
    index_pages: Option<&[(u32, String)]>,
    full_text: &str,
) -> Result<(), String> {
    let rows = prepare_document_rows(document_title, index_pages, full_text);
    if rows.is_empty() {
        return Ok(());
    }
    write_rows_yielding(db, file_path, rows)
}

/// Rows written per lock hold. A large book has tens of thousands of chunks; writing them in one
/// hold made every other save in the app wait for the whole book.
const WRITE_BATCH_ROWS: usize = 200;

/// Write a file's chunks in small batches, releasing the shared lock between batches so the
/// user's own saves get a turn.
pub fn write_rows_yielding(db: &Mutex<Database>, file_path: &str, rows: Vec<ChunkRow>) -> Result<(), String> {
    db.lock().map_err(|e| e.to_string())?.delete_chunk_embeddings_for_file(file_path)?;
    let mut last_parent: Option<i64> = None;
    let mut iter = rows.into_iter().peekable();
    while iter.peek().is_some() {
        let batch: Vec<ChunkRow> = iter.by_ref().take(WRITE_BATCH_ROWS).collect();
        {
            let guard = db.lock().map_err(|e| e.to_string())?;
            last_parent = guard.insert_chunk_embedding_batch(file_path, batch, last_parent)?;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Ok(())
}

/// Chunk and embed a document without touching the database.
pub fn prepare_document_rows(
    document_title: &str,
    index_pages: Option<&[(u32, String)]>,
    full_text: &str,
) -> Vec<ChunkRow> {
    let words: Vec<WordPage> = match index_pages {
        // A single "page" means the extractor could not split the document (or it really is one
        // page). Either way a page number would be noise: every citation would read "page 1".
        Some([(_, only)]) => {
            if full_text.trim().is_empty() {
                words_from_plain_text(only)
            } else {
                words_from_plain_text(full_text)
            }
        }
        Some(pages) if !pages.is_empty() => words_from_page_map(pages),
        _ => words_from_plain_text(full_text),
    };

    let plan = build_parent_child_plan(&words, document_title);
    let zero = vec![0.0f32; embeddings::EMBEDDING_DIM];
    let mut chunk_index: i64 = 0;
    let mut rows: Vec<ChunkRow> = Vec::new();

    for parent in plan {
        rows.push((
            chunk_index,
            Some("parent"),
            None,
            parent.page_start,
            parent.page_end,
            parent.chunk_text,
            parent.retrieval_text,
            zero.clone(),
            false,
            false,
            None,
        ));
        chunk_index += 1;
        for child in parent.children {
            let v = embeddings::embed_text(&child.retrieval_text);
            rows.push((
                chunk_index,
                Some("child"),
                None, // resolved inside DB batched method (links to most recent parent)
                child.page_start,
                child.page_end,
                child.chunk_text,
                child.retrieval_text,
                v,
                false,
                true,
                None,
            ));
            chunk_index += 1;
        }
    }
    rows
}

/// Row count that [`index_transcript_document`] will insert (parents + children), for progress UI.
pub fn count_transcript_index_rows(document_title: &str, cues: &[(f64, String)]) -> usize {
    let mut n = 0usize;
    for (_, cue_text) in cues {
        let trimmed = cue_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let words = words_from_plain_text(trimmed);
        if words.is_empty() {
            continue;
        }
        let plan = build_parent_child_plan(&words, document_title);
        for parent in plan {
            n += 1 + parent.children.len();
        }
    }
    n
}

/// Index WebVTT / SRT cues: each cue supplies `timestamp_start` (seconds) on **child** rows for temporal RAG.
pub fn index_transcript_document(
    db: &Database,
    file_path: &str,
    document_title: &str,
    cues: &[(f64, String)],
) -> Result<usize, String> {
    let rows = prepare_transcript_rows(document_title, cues);
    let n = rows.len();
    if n == 0 {
        return Ok(0);
    }
    db.replace_file_chunk_embeddings_batched(file_path, rows)?;
    Ok(n)
}

fn prepare_transcript_rows(document_title: &str, cues: &[(f64, String)]) -> Vec<ChunkRow> {
    let zero = vec![0.0f32; embeddings::EMBEDDING_DIM];
    let mut rows: Vec<ChunkRow> = Vec::new();
    let mut chunk_index: i64 = 0;

    for (cue_start, cue_text) in cues {
        let trimmed = cue_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let words = words_from_plain_text(trimmed);
        if words.is_empty() {
            continue;
        }
        let plan = build_parent_child_plan(&words, document_title);
        for parent in plan {
            rows.push((
                chunk_index,
                Some("parent"),
                None,
                parent.page_start,
                parent.page_end,
                parent.chunk_text.clone(),
                parent.retrieval_text.clone(),
                zero.clone(),
                false,
                false,
                None,
            ));
            chunk_index += 1;
            for child in parent.children {
                let v = embeddings::embed_text(&child.retrieval_text);
                rows.push((
                    chunk_index,
                    Some("child"),
                    None,
                    child.page_start,
                    child.page_end,
                    child.chunk_text.clone(),
                    child.retrieval_text.clone(),
                    v,
                    false,
                    true,
                    Some(*cue_start),
                ));
                chunk_index += 1;
            }
        }
    }
    rows
}

pub async fn index_transcript_spawn_blocking(
    db: Arc<Mutex<Database>>,
    file_path: String,
    document_title: String,
    cues: Vec<(f64, String)>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let rows = prepare_transcript_rows(&document_title, &cues);
        if rows.is_empty() {
            return Ok::<(), String>(());
        }
        let db = db.lock().map_err(|e| e.to_string())?;
        db.replace_file_chunk_embeddings_batched(&file_path, rows)?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(())
}

/// Async-friendly wrapper: run chunking + embedding + batched SQLite writes on the blocking pool.
pub async fn index_document_spawn_blocking(
    db: Arc<Mutex<Database>>,
    file_path: String,
    document_title: String,
    index_pages: Option<Vec<(u32, String)>>,
    full_text: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        index_document_shared(
            &db,
            &file_path,
            &document_title,
            index_pages.as_deref(),
            &full_text,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
