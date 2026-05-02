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

/// Index a library document: delete prior rows, insert parent rows (zero vector, no vec index)
/// and child rows (embedding of `retrieval_text`, sqlite-vec row).
pub fn index_document(
    db: &Database,
    file_path: &str,
    document_title: &str,
    index_pages: Option<&[(u32, String)]>,
    full_text: &str,
) -> Result<(), String> {
    let words: Vec<WordPage> = if let Some(pages) = index_pages {
        if pages.is_empty() {
            words_from_plain_text(full_text)
        } else {
            words_from_page_map(pages)
        }
    } else {
        words_from_plain_text(full_text)
    };

    let plan = build_parent_child_plan(&words, document_title);
    if plan.is_empty() {
        return Ok(());
    }

    // Precompute embeddings outside the SQLite transaction.
    let zero = vec![0.0f32; embeddings::EMBEDDING_DIM];
    let mut chunk_index: i64 = 0;
    let mut rows: Vec<(
        i64,
        Option<&str>,
        Option<i64>,
        Option<i32>,
        Option<i32>,
        String,
        String,
        Vec<f32>,
        bool,
        bool,
    )> = Vec::new();

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
            ));
            chunk_index += 1;
        }
    }

    db.replace_file_chunk_embeddings_batched(file_path, rows)?;
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
        let db = db.lock().map_err(|e| e.to_string())?;
        index_document(
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
