use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::collections::{HashMap, HashSet};

use futures_util::StreamExt;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;
use crate::indexer::index_zim_file;

/// N.O.M.A.D defaults translated to character-based chunking:
/// - TARGET_TOKENS_PER_CHUNK = 1700
/// - CHAR_TO_TOKEN_RATIO = 3
/// - overlap tokens = 150
pub const DEFAULT_CHUNK_SIZE: usize = 5_100;
pub const DEFAULT_CHUNK_OVERLAP: usize = 450;

/// Chunker optimized for low-allocation splitting of large document text.
///
/// It prefers semantic boundaries in this order:
/// 1) paragraph break (`\n\n`)
/// 2) line break (`\n`)
/// 3) sentence-ish boundary (`. `)
/// 4) word boundary (` `)
///
/// Overlap is also aligned forward to one of these separators so the next chunk
/// starts at a natural boundary whenever possible.
#[derive(Debug, Clone, Copy)]
pub struct DocumentChunker {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

impl Default for DocumentChunker {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            chunk_overlap: DEFAULT_CHUNK_OVERLAP,
        }
    }
}

impl DocumentChunker {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        let size = chunk_size.max(1);
        let overlap = chunk_overlap.min(size.saturating_sub(1));
        Self {
            chunk_size: size,
            chunk_overlap: overlap,
        }
    }

    /// Split raw text into overlapping chunks with separator-aware boundaries.
    pub fn chunk_text(&self, raw_text: &str) -> Vec<String> {
        if raw_text.trim().is_empty() {
            return Vec::new();
        }

        // N.O.M.A.D is text-first and preserves source text; only line ending normalization
        // is applied to improve deterministic boundary checks.
        let normalized = raw_text.replace("\r\n", "\n");
        let text = normalized.as_str();
        let text_len = text.len();

        let mut chunks: Vec<String> = Vec::new();
        let mut start = 0usize;

        while start < text_len {
            while start < text_len && !text.is_char_boundary(start) {
                start += 1;
            }
            if start >= text_len {
                break;
            }

            let hard_end = char_boundary_at_or_before(text, start.saturating_add(self.chunk_size))
                .max(start + 1)
                .min(text_len);

            let mut end = if hard_end == text_len {
                text_len
            } else {
                find_best_split_end(text, start, hard_end).unwrap_or(hard_end)
            };

            if end <= start {
                end = hard_end;
            }
            if end <= start {
                break;
            }

            let piece = text[start..end].trim();
            if !piece.is_empty() {
                chunks.push(piece.to_string());
            }

            if end >= text_len {
                break;
            }

            let mut next_start = end.saturating_sub(self.chunk_overlap);
            next_start = align_overlap_start(text, next_start, end);

            if next_start <= start {
                next_start = end;
            }
            start = next_start;
        }

        chunks
    }
}

/// Convenience API requested for ingestion pipeline call sites.
pub fn chunk_text(raw_text: &str) -> Vec<String> {
    DocumentChunker::default().chunk_text(raw_text)
}

fn find_best_split_end(text: &str, start: usize, hard_end: usize) -> Option<usize> {
    if start >= hard_end || hard_end > text.len() {
        return None;
    }
    let window = &text[start..hard_end];
    let min_len = ((hard_end - start) / 4).max(64);

    // Prefer larger semantic boundaries first.
    for sep in ["\n\n", "\n", ". ", " "] {
        if let Some((idx, _)) = window.rmatch_indices(sep).next() {
            let candidate = start + idx + sep.len();
            if candidate > start && candidate - start >= min_len {
                return Some(candidate);
            }
        }
    }

    None
}

fn align_overlap_start(text: &str, nominal_start: usize, end: usize) -> usize {
    if nominal_start >= end || end > text.len() {
        return nominal_start.min(text.len());
    }

    let nominal = char_boundary_at_or_after(text, nominal_start).min(end);
    if nominal >= end {
        return nominal;
    }

    let window = &text[nominal..end];
    for sep in ["\n\n", "\n", ". ", " "] {
        if let Some((idx, _)) = window.match_indices(sep).next() {
            let candidate = nominal + idx + sep.len();
            if candidate < end {
                return candidate;
            }
        }
    }

    nominal
}

fn char_boundary_at_or_before(text: &str, mut idx: usize) -> usize {
    idx = idx.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn char_boundary_at_or_after(text: &str, mut idx: usize) -> usize {
    idx = idx.min(text.len());
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
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
        .ok_or_else(|| "Unsupported file extension (expected .txt, .md, .csv, .zim, or .pdf).".to_string())?;

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
        _ => Err("Unsupported file extension (expected .txt, .md, .csv, .zim, or .pdf).".to_string()),
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
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("col_{}", i + 1));
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

fn tokenize_terms(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| t.len() >= 3)
        .collect()
}

fn ensure_rag_metadata_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS rag_chunk_metadata (
            chunk_id TEXT PRIMARY KEY NOT NULL,
            source TEXT,
            content_type TEXT,
            tags_json TEXT,
            article_title TEXT,
            section_title TEXT,
            FOREIGN KEY(chunk_id) REFERENCES rag_chunks(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_rag_chunk_metadata_source ON rag_chunk_metadata(source);
        CREATE INDEX IF NOT EXISTS idx_rag_chunk_metadata_content_type ON rag_chunk_metadata(content_type);
        ",
    )
    .map_err(|e| e.to_string())
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

#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
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

#[derive(Debug, Clone, Deserialize)]
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

/// Generate one embedding vector from the local Ollama sidecar.
pub async fn generate_embedding(text: String) -> Result<Vec<f32>, String> {
    let prompt = text.trim().to_string();
    if prompt.is_empty() {
        return Err("Embedding prompt is empty.".to_string());
    }

    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:11434/api/embeddings")
        .json(&serde_json::json!({
            "model": "nomic-embed-text",
            "prompt": prompt,
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to call Ollama embeddings API: {e}"))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!(
            "Ollama embeddings request failed ({status}): {body}"
        ));
    }

    let parsed: OllamaEmbeddingResponse = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama embedding response: {e}"))?;
    if parsed.embedding.is_empty() {
        return Err("Ollama returned an empty embedding vector.".to_string());
    }
    Ok(parsed.embedding)
}

#[tauri::command]
pub async fn ingest_local_document(
    file_path: String,
    app_handle: tauri::AppHandle,
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

    let chunker = DocumentChunker::default();
    let chunks = chunker.chunk_text(&text);
    let total_chunks = chunks.len();
    let mut chunk_rows: Vec<(String, Vec<u8>, String)> = Vec::with_capacity(total_chunks);
    let document_tags: Vec<String> = filename
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| t.len() >= 3)
        .collect();
    let tags_json = serde_json::to_string(&document_tags).unwrap_or_else(|_| "[]".to_string());
    for chunk_text in chunks {
        let embedding = match generate_embedding(chunk_text.clone()).await {
            Ok(v) => v,
            Err(e) => {
                emit_payload(true, 0, None, Some(e.clone()));
                return Err(e);
            }
        };
        let embedding_blob = bytemuck::cast_slice::<f32, u8>(&embedding).to_vec();
        chunk_rows.push((chunk_text, embedding_blob, tags_json.clone()));
    }

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
    let db_path = app_data_dir.join("athena_index.db");
    let mut conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    ensure_rag_metadata_schema(&conn)?;

    let document_id = Uuid::new_v4().to_string();
    let file_type = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "txt".to_string());
    let ingested_at = chrono::Utc::now().to_rfc3339();

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO rag_documents (id, filename, file_type, total_chunks, ingested_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![document_id, filename, file_type, total_chunks as i64, ingested_at],
    )
    .map_err(|e| e.to_string())?;

    let mut stmt = tx
        .prepare(
            "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|e| e.to_string())?;
    let mut metadata_stmt = tx
        .prepare(
            "INSERT OR REPLACE INTO rag_chunk_metadata (chunk_id, source, content_type, tags_json, article_title, section_title)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL)",
        )
        .map_err(|e| e.to_string())?;
    for (idx, (content, embedding_blob, tags_json)) in chunk_rows.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();
        stmt.execute(params![
            chunk_id,
            document_id,
            idx as i64,
            content,
            embedding_blob
        ])
        .map_err(|e| e.to_string())?;
        metadata_stmt
            .execute(params![chunk_id, filename, file_type, tags_json])
            .map_err(|e| e.to_string())?;
    }
    drop(stmt);
    drop(metadata_stmt);
    tx.commit().map_err(|e| e.to_string())?;

    emit_payload(true, total_chunks, Some(document_id.clone()), None);
    Ok(document_id)
}

#[tauri::command]
pub async fn download_knowledge_package(
    package_id: String,
    url: String,
    app_handle: tauri::AppHandle,
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
    fs::create_dir_all(&packages_dir).map_err(|e| format!("Failed to create package directory: {e}"))?;

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
    let out_path: PathBuf = packages_dir.join(final_name);
    let mut out = tokio::fs::File::create(&out_path)
        .await
        .map_err(|e| format!("Failed to create destination file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_emitted_bucket: u8 = 0;
    let mut stream = response.bytes_stream();
    while let Some(next) = stream.next().await {
        let chunk = next.map_err(|e| format!("Download stream failed: {e}"))?;
        tokio::io::AsyncWriteExt::write_all(&mut out, &chunk)
            .await
            .map_err(|e| format!("Failed writing downloaded data: {e}"))?;
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
    tokio::io::AsyncWriteExt::flush(&mut out)
        .await
        .map_err(|e| format!("Failed to flush destination file: {e}"))?;

    let path_str = out_path
        .to_str()
        .ok_or_else(|| "Downloaded path is not valid UTF-8.".to_string())?
        .to_string();
    if let Err(e) = ingest_local_document(path_str, app_handle.clone()).await {
        emit_progress(100, true, Some(e.clone()));
        return Err(e);
    }

    emit_progress(100, true, None);
    Ok(true)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut a_norm_sq = 0.0f32;
    let mut b_norm_sq = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        a_norm_sq += x * x;
        b_norm_sq += y * y;
    }
    let denom = a_norm_sq.sqrt() * b_norm_sq.sqrt();
    if denom <= f32::EPSILON {
        return 0.0;
    }
    dot / denom
}

#[tauri::command]
pub async fn search_rag_chunks(
    query: String,
    limit: usize,
    filters: Option<RagSearchFilters>,
    app_handle: tauri::AppHandle,
) -> Result<Vec<RagSearchResult>, String> {
    let query_embedding = generate_embedding(query.clone()).await?;
    let query_terms = tokenize_terms(&query);
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_data_dir.join("athena_index.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    ensure_rag_metadata_schema(&conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT
                rc.id,
                rc.document_id,
                rc.chunk_index,
                rc.content,
                rc.embedding,
                rcm.source,
                rcm.content_type,
                rcm.tags_json
             FROM rag_chunks rc
             LEFT JOIN rag_chunk_metadata rcm ON rcm.chunk_id = rc.id
             WHERE rc.embedding IS NOT NULL AND length(rc.embedding) > 0",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut scored: Vec<RagSearchResult> = Vec::new();
    let mut source_count: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let (chunk_id, document_id, chunk_index, content, blob, source, content_type, tags_json) =
            row.map_err(|e| e.to_string())?;
        if let Some(ref f) = filters {
            if let Some(ref types) = f.file_types {
                let ct = content_type.clone().unwrap_or_default().to_ascii_lowercase();
                if !types.iter().any(|t| t.eq_ignore_ascii_case(&ct)) {
                    continue;
                }
            }
            if let Some(ref needle) = f.source_contains {
                let src = source.clone().unwrap_or_default().to_ascii_lowercase();
                if !src.contains(&needle.to_ascii_lowercase()) {
                    continue;
                }
            }
            if let Some(ref ct) = f.content_type {
                let ctv = content_type.clone().unwrap_or_default();
                if !ctv.eq_ignore_ascii_case(ct) {
                    continue;
                }
            }
            if let Some(ref wanted_tags) = f.tags {
                let tags: Vec<String> = tags_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .unwrap_or_default();
                let set: HashSet<String> = tags.into_iter().map(|t| t.to_ascii_lowercase()).collect();
                if !wanted_tags.iter().any(|t| set.contains(&t.to_ascii_lowercase())) {
                    continue;
                }
            }
        }
        if blob.len() % std::mem::size_of::<f32>() != 0 {
            continue;
        }
        let emb_slice = match bytemuck::try_cast_slice::<u8, f32>(&blob) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let semantic_score = cosine_similarity(&query_embedding, emb_slice);
        let chunk_terms = tokenize_terms(&content);
        let lexical_overlap = if query_terms.is_empty() {
            0.0
        } else {
            query_terms.intersection(&chunk_terms).count() as f32 / query_terms.len() as f32
        };
        let src_key = source.clone().unwrap_or_default();
        let seen = source_count.entry(src_key.clone()).or_insert(0);
        let diversity_penalty = (*seen as f32) * 0.05;
        *seen += 1;
        let score = (semantic_score * 0.75) + (lexical_overlap * 0.30) - diversity_penalty;
        scored.push(RagSearchResult {
            chunk_id,
            document_id,
            chunk_index,
            content,
            score,
            semantic_score,
            source,
            content_type,
        });
    }

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let cap = limit.max(1);
    if scored.len() > cap {
        scored.truncate(cap);
    }
    Ok(scored)
}

#[tauri::command]
pub async fn list_rag_documents(app_handle: tauri::AppHandle) -> Result<Vec<RagDocumentSummary>, String> {
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
    use super::{DocumentChunker, DEFAULT_CHUNK_OVERLAP, DEFAULT_CHUNK_SIZE};

    #[test]
    fn defaults_match_nomad_conversion() {
        let c = DocumentChunker::default();
        assert_eq!(c.chunk_size, DEFAULT_CHUNK_SIZE);
        assert_eq!(c.chunk_overlap, DEFAULT_CHUNK_OVERLAP);
        assert_eq!(c.chunk_size, 5_100);
        assert_eq!(c.chunk_overlap, 450);
    }

    #[test]
    fn chunking_creates_multiple_overlapping_chunks() {
        let text = "Para one sentence one. Para one sentence two.\n\nPara two sentence one. Para two sentence two.\n\nPara three sentence one. Para three sentence two.\n\nPara four sentence one. Para four sentence two.";
        let chunker = DocumentChunker::new(80, 20);
        let out = chunker.chunk_text(text);
        assert!(out.len() >= 2);
        assert!(out[0].contains("Para one"));
        assert!(out.last().expect("last").contains("Para four"));
    }

    #[test]
    fn handles_unicode_boundaries_without_panics() {
        let text = "Alpha 😄 bravo charlie.\n\nDelta echo foxtrot.\n\nGolf hotel india.";
        let chunker = DocumentChunker::new(24, 8);
        let out = chunker.chunk_text(text);
        assert!(!out.is_empty());
        assert!(out.iter().all(|s| !s.is_empty()));
    }
}
