use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::chunking;
use crate::embeddings;
use crate::indexer;
use crate::vector_index;
use crate::indexer::index_zim_file;
use crate::AppState;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::Manager;
use tauri::State;
use uuid::Uuid;

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    embeddings::cosine_similarity(a, b)
}

/// Read supported document formats into a single UTF-8 string.
///
/// Currently supported:
/// - `.txt`
/// - `.md`
/// - `.pdf`
pub fn read_document_to_text(file_path: &Path) -> Result<String, String> {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| {
            "Unsupported file extension (expected .txt, .md, .csv, .zim, or .pdf).".to_string()
        })?;

    match ext.as_str() {
        "txt" | "md" => fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e)),
        "csv" => read_csv_to_text(file_path),
        "pdf" => match pdf_extract::extract_text(file_path) {
            Ok(text) => Ok(text),
            Err(e) => {
                let lower = e.to_string().to_lowercase();
                if lower.contains("encrypt") || lower.contains("password") {
                    Err(format!(
                        "Failed to extract text from {}: PDF is encrypted/password-protected.",
                        file_path.display()
                    ))
                } else {
                    Err(format!(
                        "Failed to extract text from {}: PDF is unreadable or unsupported ({e}).",
                        file_path.display()
                    ))
                }
            }
        },
        "zim" => read_zim_to_text(file_path),
        _ => {
            Err("Unsupported file extension (expected .txt, .md, .csv, .zim, or .pdf).".to_string())
        }
    }
}

fn read_csv_to_text(file_path: &Path) -> Result<String, String> {
    let raw = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read CSV {}: {}", file_path.display(), e))?;
    let mut lines = raw.lines();
    let Some(header_line) = lines.next() else {
        return Ok(String::new());
    };
    let headers: Vec<String> = header_line
        .split(',')
        .map(|h| h.trim().trim_matches('"').to_string())
        .collect();
    let mut out = String::new();
    for (row_idx, line) in lines.enumerate() {
        let cols: Vec<String> = line
            .split(',')
            .map(|c| c.trim().trim_matches('"').to_string())
            .collect();
        let mut parts: Vec<String> = Vec::new();
        for (i, value) in cols.iter().enumerate() {
            let key = headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("col_{}", i + 1));
            if !value.is_empty() {
                parts.push(format!("{key}: {value}"));
            }
        }
        if !parts.is_empty() {
            out.push_str(&format!("Row {}: {}\n", row_idx + 1, parts.join("; ")));
        }
    }
    Ok(out)
}

fn read_zim_to_text(file_path: &Path) -> Result<String, String> {
    let rows = index_zim_file(file_path);
    if rows.is_empty() {
        return Err(format!(
            "Failed to extract text from {}: no readable ZIM articles found.",
            file_path.display()
        ));
    }
    let mut out = String::new();
    for row in rows.into_iter().take(300) {
        if let Some(text) = row.text_content {
            if !text.trim().is_empty() {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
    if out.trim().is_empty() {
        return Err(format!(
            "Failed to extract text from {}: ZIM contained no text payloads.",
            file_path.display()
        ));
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct RagIngestionProgressPayload {
    pub file_path: String,
    pub filename: String,
    pub completed: bool,
    pub chunks: usize,
    pub document_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagSearchResult {
    pub chunk_id: String,
    pub document_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub score: f32,
    pub semantic_score: f32,
    pub source: Option<String>,
    pub content_type: Option<String>,
}

/// Accepted from the frontend for future filter-aware search; not applied in v1 retrieval.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RagSearchFilters {
    pub file_types: Option<Vec<String>>,
    pub source_contains: Option<String>,
    pub content_type: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagDocumentSummary {
    pub id: String,
    pub filename: String,
    pub file_type: String,
    pub total_chunks: i64,
    pub ingested_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeDownloadProgressPayload {
    pub package_id: String,
    pub progress: u8,
    pub completed: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ingest_local_document(
    file_path: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let source_path = Path::new(&file_path);
    let filename = source_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file_path)
        .to_string();

    let emit_payload =
        |completed: bool, chunks: usize, document_id: Option<String>, error: Option<String>| {
            let payload = RagIngestionProgressPayload {
                file_path: file_path.clone(),
                filename: filename.clone(),
                completed,
                chunks,
                document_id,
                error,
            };
            let _ = app_handle.emit("rag-ingestion-progress", payload);
        };

    emit_payload(false, 0, None, None);

    let text = match read_document_to_text(source_path) {
        Ok(t) => t,
        Err(e) => {
            emit_payload(true, 0, None, Some(e.clone()));
            return Err(e);
        }
    };

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let path_for_db = file_path.clone();
    let doc_title = std::path::Path::new(&file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&filename)
        .to_string();
    let preview = indexer::limit_text_for_index(source_path, text.clone());
    let meta = source_path.metadata().ok();
    let size = meta.as_ref().map(|m| m.len() as i64);
    let modified_at = meta
        .and_then(|m| m.modified().ok())
        .and_then(|st| st.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    db.upsert_file(
        &path_for_db,
        "text",
        &filename,
        size,
        modified_at,
        preview.as_deref(),
        None,
        Some(doc_title.as_str()),
        None,
        None,
    )?;
    let words = chunking::words_from_plain_text(&text);
    let plan = chunking::build_parent_child_plan(&words, &doc_title);
    let total_chunks: usize = plan.iter().map(|p| 1 + p.children.len()).sum();
    if total_chunks == 0 {
        let err = "No indexable text.".to_string();
        emit_payload(true, 0, None, Some(err.clone()));
        return Err(err);
    }
    vector_index::index_document(&db, &path_for_db, &doc_title, None, &text)?;

    let document_id = Uuid::new_v4().to_string();
    emit_payload(true, total_chunks, Some(document_id.clone()), None);
    Ok(document_id)
}

#[tauri::command]
pub async fn download_knowledge_package(
    package_id: String,
    url: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let package_id = package_id.trim().to_string();
    let url = url.trim().to_string();
    if package_id.is_empty() {
        return Err("Package ID is required.".to_string());
    }
    if url.is_empty() {
        return Err("URL is required.".to_string());
    }

    let emit_progress = |progress: u8, completed: bool, error: Option<String>| {
        let payload = KnowledgeDownloadProgressPayload {
            package_id: package_id.clone(),
            progress,
            completed,
            error,
        };
        let _ = app_handle.emit("download-progress", payload);
    };

    emit_progress(0, false, None);

    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = format!("Download failed ({status}): {body}");
        emit_progress(0, true, Some(msg.clone()));
        return Err(msg);
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve app data directory: {e}"))?;
    let packages_dir = app_data_dir.join("knowledge_packages");
    fs::create_dir_all(&packages_dir)
        .map_err(|e| format!("Failed to create package directory: {e}"))?;

    let inferred_name = url
        .rsplit('/')
        .next()
        .and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| format!("{package_id}.txt"));
    let final_name = if Path::new(&inferred_name).extension().is_none() {
        format!("{inferred_name}.txt")
    } else {
        inferred_name
    };
    let out_path: PathBuf = packages_dir.join(&final_name);
    let _ = fs::remove_file(&out_path);
    let file_name = out_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Could not determine destination file name.".to_string())?;
    let part_path = out_path.with_file_name(format!("{file_name}.part"));
    let _ = fs::remove_file(&part_path);

    let mut out = match tokio::fs::File::create(&part_path).await {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("Failed to create temporary download file: {e}");
            emit_progress(0, true, Some(msg.clone()));
            return Err(msg);
        }
    };

    let mut downloaded: u64 = 0;
    let mut last_emitted_bucket: u8 = 0;
    let mut stream = response.bytes_stream();
    while let Some(next) = stream.next().await {
        let chunk = match next {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Download stream failed: {e}");
                let _ = fs::remove_file(&part_path);
                emit_progress(0, true, Some(msg.clone()));
                return Err(msg);
            }
        };
        if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut out, &chunk).await {
            let msg = format!("Failed writing downloaded data: {e}");
            let _ = fs::remove_file(&part_path);
            emit_progress(0, true, Some(msg.clone()));
            return Err(msg);
        }
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if total_bytes > 0 {
            let pct = ((downloaded.saturating_mul(100)) / total_bytes).min(100) as u8;
            let bucket = (pct / 10) * 10;
            if bucket >= last_emitted_bucket.saturating_add(10) {
                last_emitted_bucket = bucket;
                emit_progress(bucket, false, None);
            }
        }
    }
    if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut out).await {
        let msg = format!("Failed to flush temporary download file: {e}");
        let _ = fs::remove_file(&part_path);
        emit_progress(0, true, Some(msg.clone()));
        return Err(msg);
    }
    drop(out);

    if let Err(e) = tokio::fs::rename(&part_path, &out_path).await {
        let msg = format!("Failed to finalize download: {e}");
        let _ = fs::remove_file(&part_path);
        emit_progress(0, true, Some(msg.clone()));
        return Err(msg);
    }

    let path_str = out_path
        .to_str()
        .ok_or_else(|| "Downloaded path is not valid UTF-8.".to_string())?
        .to_string();
    if let Err(e) = ingest_local_document(path_str, app_handle.clone(), state).await {
        emit_progress(100, true, Some(e.clone()));
        return Err(e);
    }

    emit_progress(100, true, None);
    Ok(true)
}

#[tauri::command]
pub async fn search_rag_chunks(
    query: String,
    limit: usize,
    filters: Option<RagSearchFilters>,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<RagSearchResult>, String> {
    let query_embedding = embeddings::embed_text(&query);
    let cap = limit.clamp(1, 50);

    // Current v1: sqlite-vec semantic retrieval over `chunk_embeddings`.
    // Filters are not applied at this layer yet (no per-chunk metadata is stored in `chunk_embeddings`).
    let _ = (filters, app_handle);

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let hits = db.search_similar_chunks(&query, cap)?;
    let mut out: Vec<RagSearchResult> = Vec::new();
    for h in hits {
        let semantic_score = cosine_similarity(&query_embedding, &h.vector);
        out.push(RagSearchResult {
            chunk_id: format!("{}:{}", h.file_path, h.chunk_index),
            document_id: h.file_path.clone(),
            chunk_index: h.chunk_index,
            content: h.chunk_text.clone(),
            score: semantic_score,
            semantic_score,
            source: Some(h.file_path.clone()),
            content_type: None,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn list_rag_documents(
    app_handle: tauri::AppHandle,
) -> Result<Vec<RagDocumentSummary>, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_data_dir.join("athena_index.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, filename, file_type, total_chunks, ingested_at
             FROM rag_documents
             ORDER BY ingested_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RagDocumentSummary {
                id: row.get(0)?,
                filename: row.get(1)?,
                file_type: row.get(2)?,
                total_chunks: row.get(3)?,
                ingested_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    // Legacy chunker tests removed; chunking is now centralized in `crate::chunking`.

    #[test]
    fn placeholder() {
        assert!(true);
    }
}
