use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::chunking;
use crate::document_rag_read;
use crate::embeddings;
use crate::indexer;
use crate::vector_index;
use crate::indexer::index_zim_file;
use crate::AppState;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::State;
use uuid::Uuid;

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    embeddings::cosine_similarity(a, b)
}

fn parse_timestamp_to_seconds(ts: &str) -> Option<f64> {
    let head = ts.trim().split_whitespace().next().unwrap_or("").replace(',', ".");
    let parts: Vec<&str> = head.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().ok()?;
            let m: f64 = parts[1].parse().ok()?;
            let s: f64 = parts[2].parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        }
        2 => {
            let m: f64 = parts[0].parse().ok()?;
            let s: f64 = parts[1].parse().ok()?;
            Some(m * 60.0 + s)
        }
        _ => None,
    }
}

fn parse_webvtt_cues(content: &str) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.contains("-->") {
            let start_part = line.split("-->").next().unwrap_or("").trim();
            if let Some(t0) = parse_timestamp_to_seconds(start_part) {
                i += 1;
                let mut body: Vec<String> = Vec::new();
                while i < lines.len() {
                    let l = lines[i].trim();
                    if l.is_empty() {
                        break;
                    }
                    if l.contains("-->") {
                        break;
                    }
                    body.push(l.to_string());
                    i += 1;
                }
                let text = body.join("\n");
                if !text.trim().is_empty() {
                    out.push((t0, text));
                }
                continue;
            }
        }
        i += 1;
    }
    out
}

fn parse_srt_cues(content: &str) -> Result<Vec<(f64, String)>, String> {
    let mut out = Vec::new();
    let normalized = content.replace("\r\n", "\n");
    for block in normalized.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let mut ls = block.lines();
        let first = ls.next().unwrap_or("").trim();
        let timing_line = if first.chars().all(|c| c.is_ascii_digit()) {
            ls.next().unwrap_or("").trim()
        } else {
            first
        };
        if !timing_line.contains("-->") {
            continue;
        }
        let start_part = timing_line.split("-->").next().unwrap_or("").trim();
        let t0 = parse_timestamp_to_seconds(start_part).ok_or_else(|| {
            format!("Unrecognized SRT timing line: {timing_line}")
        })?;
        let text: String = ls
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if !text.is_empty() {
            out.push((t0, text));
        }
    }
    Ok(out)
}

fn parse_subtitle_cues_by_ext(ext: &str, content: &str) -> Result<Vec<(f64, String)>, String> {
    match ext {
        "vtt" => Ok(parse_webvtt_cues(content)),
        "srt" => parse_srt_cues(content),
        _ => Err("Expected .vtt or .srt subtitle file.".to_string()),
    }
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
            "Unsupported file extension (expected .txt, .md, .csv, .zim, .pdf, .doc, .docx, .vtt, or .srt)."
                .to_string()
        })?;

    match ext.as_str() {
        "txt" | "md" => document_rag_read::read_utf8_file_capped(file_path, document_rag_read::MAX_RAG_PLAIN_TEXT_BYTES),
        "csv" => read_csv_to_text_capped(file_path),
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
        "doc" | "docx" => crate::indexer::extract_word_like_text_for_rag(file_path),
        "vtt" | "srt" => {
            let raw = fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;
            let cues = parse_subtitle_cues_by_ext(&ext, &raw)?;
            if cues.is_empty() {
                return Err(format!("No subtitle cues in {}.", file_path.display()));
            }
            Ok(cues
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"))
        }
        _ => {
            Err(
                "Unsupported file extension (expected .txt, .md, .csv, .zim, .pdf, .doc, .docx, .vtt, or .srt)."
                    .to_string(),
            )
        }
    }
}

fn read_csv_to_text_capped(file_path: &Path) -> Result<String, String> {
    let raw = document_rag_read::read_utf8_file_capped(file_path, document_rag_read::MAX_RAG_PLAIN_TEXT_BYTES)?;
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
    pub source_package: String,
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

    let path_for_db = file_path.clone();
    let doc_title = std::path::Path::new(&file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&filename)
        .to_string();

    let ext_plain = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|x| x.to_ascii_lowercase());
    let uses_chunk_worker = matches!(ext_plain.as_deref(), Some("txt" | "md" | "csv"));
    let is_transcript = matches!(ext_plain.as_deref(), Some("vtt" | "srt"));

    if is_transcript {
        let ext_owned = ext_plain.clone().unwrap_or_default();
        let path_for_read = source_path.to_path_buf();
        let title = doc_title.clone();
        let (flat, cues, n) = match tokio::task::spawn_blocking(move || {
            let raw = fs::read_to_string(&path_for_read).map_err(|e| e.to_string())?;
            let cues = parse_subtitle_cues_by_ext(ext_owned.as_str(), &raw)?;
            if cues.is_empty() {
                return Err("No subtitle cues in file.".to_string());
            }
            let flat: String = cues
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let n = vector_index::count_transcript_index_rows(&title, &cues);
            Ok::<(String, Vec<(f64, String)>, usize), String>((flat, cues, n))
        })
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                emit_payload(true, 0, None, Some(e.clone()));
                return Err(e);
            }
            Err(e) => {
                let msg = format!("RAG transcript ingest task failed: {e}");
                emit_payload(true, 0, None, Some(msg.clone()));
                return Err(msg);
            }
        };
        if n == 0 {
            let err = "No indexable subtitle text.".to_string();
            emit_payload(true, 0, None, Some(err.clone()));
            return Err(err);
        }
        let preview = indexer::limit_text_for_index(source_path, flat.clone());
        let meta = source_path.metadata().ok();
        let size = meta.as_ref().map(|m| m.len() as i64);
        let modified_at = meta
            .and_then(|m| m.modified().ok())
            .and_then(|st| st.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        {
            let db = state.db.lock().map_err(|e| e.to_string())?;
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
        }
        vector_index::index_transcript_spawn_blocking(
            Arc::clone(&state.db),
            path_for_db.clone(),
            doc_title.clone(),
            cues,
        )
        .await?;
        let document_id = Uuid::new_v4().to_string();
        emit_payload(true, n, Some(document_id.clone()), None);
        return Ok(document_id);
    }

    // PDFs: keep the page each passage came from so citations can say "page 12".
    let pdf_pages: Option<Vec<(u32, String)>> = if ext_plain.as_deref() == Some("pdf") {
        let path_for_pages = source_path.to_path_buf();
        tokio::task::spawn_blocking(move || pdf_extract::extract_text_by_pages(&path_for_pages).ok())
            .await
            .ok()
            .flatten()
            .map(|pages| {
                pages
                    .into_iter()
                    .enumerate()
                    .map(|(i, text)| ((i + 1) as u32, text))
                    .collect::<Vec<_>>()
            })
            .filter(|pages| pages.len() > 1 && pages.iter().any(|(_, t)| !t.trim().is_empty()))
    } else {
        None
    };

    let (text_for_preview, total_chunks, skip_vector_index) = if uses_chunk_worker {
        let path_for_read = source_path.to_path_buf();
        let title = doc_title.clone();
        match tokio::task::spawn_blocking(move || {
            crate::rag_chunk_worker::chunk_plain_document_for_rag(&path_for_read, &title)
        })
        .await
        {
            Ok(Ok((text, plan))) => {
                let n: usize = plan.iter().map(|p| 1 + p.children.len()).sum();
                (text, n, false)
            }
            Ok(Err(e)) => {
                emit_payload(true, 0, None, Some(e.clone()));
                return Err(e);
            }
            Err(e) => {
                let msg = format!("RAG chunk worker task failed: {e}");
                emit_payload(true, 0, None, Some(msg.clone()));
                return Err(msg);
            }
        }
    } else if let Some(pages) = pdf_pages.as_ref() {
        let text = pages
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let words = chunking::words_from_page_map(pages);
        let plan = chunking::build_parent_child_plan(&words, &doc_title);
        let n: usize = plan.iter().map(|p| 1 + p.children.len()).sum();
        (text, n, false)
    } else {
        let path_for_read = source_path.to_path_buf();
        let text = match tokio::task::spawn_blocking(move || read_document_to_text(&path_for_read)).await {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                emit_payload(true, 0, None, Some(e.clone()));
                return Err(e);
            }
            Err(e) => {
                let msg = format!("RAG read task failed: {e}");
                emit_payload(true, 0, None, Some(msg.clone()));
                return Err(msg);
            }
        };
        let words = chunking::words_from_plain_text(&text);
        let plan = chunking::build_parent_child_plan(&words, &doc_title);
        let n: usize = plan.iter().map(|p| 1 + p.children.len()).sum();
        (text, n, false)
    };

    if total_chunks == 0 {
        let err = "No indexable text.".to_string();
        emit_payload(true, 0, None, Some(err.clone()));
        return Err(err);
    }

    let preview = indexer::limit_text_for_index(source_path, text_for_preview.clone());
    let meta = source_path.metadata().ok();
    let size = meta.as_ref().map(|m| m.len() as i64);
    let modified_at = meta
        .and_then(|m| m.modified().ok())
        .and_then(|st| st.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
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
    }
    if !skip_vector_index {
        vector_index::index_document_spawn_blocking(
            Arc::clone(&state.db),
            path_for_db.clone(),
            doc_title.clone(),
            pdf_pages,
            text_for_preview.clone(),
        )
        .await?;
    }

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
    let app_data_dir = crate::portable::resolved_app_data_dir(&app_handle)
        .map_err(|e| format!("Could not resolve Athena data directory: {e}"))?;
    let packages_dir = crate::athena_paths::AthenaPaths::new(&app_data_dir)
        .user_library_dir()
        .join("knowledge_packages");
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

/// Semantic RAG over `chunk_embeddings` (same scoring as the `search_rag_chunks` command).
/// Call from `spawn_blocking` or other blocking contexts only.
pub fn search_rag_chunks_blocking(
    db: &crate::Database,
    query: &str,
    limit: usize,
) -> Result<Vec<RagSearchResult>, String> {
    const MIN_SCORE: f32 = crate::Database::RAG_MIN_SIMILARITY_SCORE;
    let cap = limit.clamp(1, 50);
    let query_embedding = embeddings::embed_text(query);
    let hits = db.search_similar_chunks(query, cap)?;
    let mut out: Vec<RagSearchResult> = Vec::new();
    for h in hits {
        let semantic_score = cosine_similarity(&query_embedding, &h.vector);
        if semantic_score < MIN_SCORE {
            continue;
        }
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
pub async fn search_rag_chunks(
    query: String,
    limit: usize,
    filters: Option<RagSearchFilters>,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<RagSearchResult>, String> {
    // Current v1: sqlite-vec semantic retrieval over `chunk_embeddings`.
    // Filters are not applied at this layer yet (no per-chunk metadata is stored in `chunk_embeddings`).
    let _ = (filters, app_handle);

    let query_for_db = query.clone();
    let db_arc = std::sync::Arc::clone(&state.db);

    tokio::task::spawn_blocking(move || -> Result<Vec<RagSearchResult>, String> {
        let db = db_arc.lock().map_err(|e| e.to_string())?;
        search_rag_chunks_blocking(&*db, &query_for_db, limit)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One candidate from [`test_library_search`], with the retrieval pipeline's verdict.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySearchTestHit {
    pub title: String,
    pub file_path: String,
    pub page_number: Option<u32>,
    /// Cross-encoder relevance logit, or fused rank score when no reranker model is installed.
    pub score: f32,
    /// True when chat would be given this passage for the same question.
    pub relevant: bool,
    /// True when `score` came from the reranker model.
    pub reranked: bool,
    pub preview: String,
}

/// Run the exact retrieval chat uses and return what it found, so the operator can check that a
/// document is findable without spending a model turn on it.
#[tauri::command]
pub async fn test_library_search(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<LibrarySearchTestHit>, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let db_arc = std::sync::Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || -> Result<Vec<LibrarySearchTestHit>, String> {
        let db = db_arc.lock().map_err(|e| e.to_string())?;
        let scored = db.hybrid_search_scored(&q)?;
        Ok(scored
            .into_iter()
            .take(12)
            .map(|h| {
                let title = h
                    .hit
                    .display_title
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| {
                        Path::new(&h.hit.file_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Library document")
                            .to_string()
                    });
                let preview: String = h
                    .hit
                    .expanded_text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(280)
                    .collect();
                LibrarySearchTestHit {
                    title,
                    file_path: h.hit.file_path,
                    page_number: h.hit.page_number.and_then(|p| u32::try_from(p).ok()),
                    score: h.hit.score,
                    relevant: h.relevant,
                    reranked: h.reranked,
                    preview,
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_rag_documents(
    app_handle: tauri::AppHandle,
) -> Result<Vec<RagDocumentSummary>, String> {
    let app_data_dir = crate::portable::resolved_app_data_dir(&app_handle).map_err(|e| e.to_string())?;
    let db_path = crate::athena_paths::AthenaPaths::new(&app_data_dir).resolve_db_path();
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, filename, file_type, total_chunks, ingested_at, source_package
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
                source_package: row.get(5)?,
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
