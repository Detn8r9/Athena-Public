use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

use crate::db::Database;

const TEXT_EXTENSIONS: &[&str] = &[
    "txt",
    "md",
    "html",
    "htm",
    "rst",
    "log",
    "csv",
    "json",
    "xml",
    "yaml",
    "yml",
    "tex",
    "org",
    "ini",
    "cfg",
    "conf",
    "toml",
    "env",
    "properties",
    "asc",
    "text",
    "adoc",
    "asciidoc",
    "creole",
    "wiki",
    "me",
    "nfo",
    "readme",
];
const PDF_EXTENSIONS: &[&str] = &["pdf"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "tif"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "avi", "mkv", "m4v"];
const MAP_EXTENSIONS: &[&str] = &["geotiff", "tif", "tiff"];
const ZIM_EXT: &str = "zim";
const PAGES_EXT: &str = "pages";
const ODT_EXT: &str = "odt";
const DOC_EXT: &str = "doc";
const DOCX_EXT: &str = "docx";
const EPUB_EXT: &str = "epub";
const PPTX_EXT: &str = "pptx";
const XLSX_EXT: &str = "xlsx";
const RTF_EXT: &str = "rtf";
const NUMBERS_EXT: &str = "numbers";
const KEY_EXT: &str = "key";

const ZIM_ARTICLE_LIMIT: usize = 150_000;
const ZIM_ARTICLE_TEXT_MAX: usize = 60_000;
/// Below this length we always try OCR as well (Pages → PDF often has a little metadata text but body is bitmap).
const PDF_EMBEDDED_TRY_OCR_BELOW_CHARS: usize = 600;
/// Force OCR message and aggressive fallback when embedded text is this short or empty.
const PDF_FORCE_OCR_BELOW_CHARS: usize = 100;

/// Hard cap on stored indexed text per file (UTF-8 bytes). Truncated with a log line when exceeded.
pub const MAX_INDEXED_TEXT_BYTES: usize = 1_000_000;

/// Guardrail: skip embedded PDF parsing when the file is extremely large (OCR may still run depending on heuristics).
const PDF_MAX_PARSE_BYTES: u64 = 250 * 1024 * 1024;

/// Guardrail: don't keep unbounded page splits; cap stored page text blocks.
const PDF_MAX_PAGES_STORED: usize = 600;

fn truncate_utf8_to_max_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Merge embedded PDF text with OCR when both exist (Pages and some exports hide body text in images).

fn display_title_from_filename(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or(name)
        .to_string()
}

fn decode_pdf_title_bytes(data: &[u8]) -> Option<String> {
    if data.len() >= 2 && data[0] == 0xFE && data[1] == 0xFF {
        let u16s: Vec<u16> = data[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16(&u16s).ok();
    }
    std::str::from_utf8(data)
        .ok()
        .map(|x| x.to_string())
        .filter(|x| !x.trim().is_empty())
}

fn pdf_document_title(path: &Path) -> Option<String> {
    use lopdf::Object;
    let doc = lopdf::Document::load(path).ok()?;
    let info_ref = doc.trailer.get(b"Info").ok()?;
    let Object::Reference(rid) = info_ref else {
        return None;
    };
    let obj = doc.get_object(*rid).ok()?;
    let Object::Dictionary(dict) = obj else {
        return None;
    };
    let title_obj = dict.get(b"Title").ok()?;
    let Object::String(data, _) = title_obj else {
        return None;
    };
    decode_pdf_title_bytes(&data)
}

#[allow(dead_code)]
fn combine_pdf_embedded_and_ocr(
    embedded: Option<String>,
    ocr: Option<String>,
) -> Option<String> {
    match (&embedded, &ocr) {
        (Some(e), Some(o)) => {
            if o.contains(e.as_str()) && o.len() >= e.len() {
                Some(o.clone())
            } else if e.contains(o.as_str()) && e.len() >= o.len() {
                Some(e.clone())
            } else {
                Some(format!("{}\n\n{}", e, o))
            }
        }
        (Some(e), None) => Some(e.clone()),
        (None, Some(o)) => Some(o.clone()),
        (None, None) => None,
    }
}

/// Applies [`MAX_INDEXED_TEXT_BYTES`] after extraction so chunking never sees pathological strings.
pub(crate) fn limit_text_for_index(path: &Path, text: String) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if t.len() <= MAX_INDEXED_TEXT_BYTES {
        return Some(t.to_string());
    }
    eprintln!(
        "[indexer] Truncating indexed text to {} bytes for {} (original {} bytes)",
        MAX_INDEXED_TEXT_BYTES,
        path.display(),
        t.len()
    );
    Some(truncate_utf8_to_max_bytes(t, MAX_INDEXED_TEXT_BYTES))
}

/// macOS `textutil -convert txt -stdout` for RTF, Word, ODT, etc. Non-macOS returns `None`.
fn extract_text_via_textutil(path: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        match Command::new("textutil")
            .arg("-convert")
            .arg("txt")
            .arg("-stdout")
            .arg(path)
            .output()
        {
            Ok(output) => {
                if !output.status.success() {
                    eprintln!(
                        "[athena] textutil failed for {}: status {:?}, stderr: {}",
                        path.display(),
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                    return None;
                }
                let text = String::from_utf8_lossy(&output.stdout);
                let t = text.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            Err(e) => {
                eprintln!(
                    "[athena] textutil spawn error for {}: {}",
                    path.display(),
                    e
                );
                None
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Pdf,
    Image,
    Video,
    Map,
    Zim,
    Pages,
    Odt,
    Docx,
    Doc,
    Epub,
    Pptx,
    Xlsx,
    Rtf,
    Numbers,
    Key,
    /// Unknown extension: try UTF-8 plain text in `index_file`.
    Utf8Probe,
}

/// Plain text from `.doc` / `.docx` for RAG ingest (blocking-safe).
pub fn extract_word_like_text_for_rag(path: &Path) -> Result<String, String> {
    match file_kind_from_path(path) {
        FileKind::Docx => extract_text_from_docx(path)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| format!("No text extracted from DOCX: {}", path.display())),
        FileKind::Doc => extract_text_via_textutil(path)
            .or_else(|| extract_text_from_docx(path))
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| format!("No text extracted from DOC: {}", path.display())),
        _ => Err(format!(
            "Not a Word document path (expected .doc or .docx): {}",
            path.display()
        )),
    }
}

pub fn file_kind_from_path(path: &Path) -> FileKind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if PDF_EXTENSIONS.contains(&ext.as_str()) {
        return FileKind::Pdf;
    }
    if ext.as_str() == ZIM_EXT {
        return FileKind::Zim;
    }
    if ext.as_str() == PAGES_EXT {
        return FileKind::Pages;
    }
    if ext.as_str() == ODT_EXT {
        return FileKind::Odt;
    }
    if ext.as_str() == DOCX_EXT {
        return FileKind::Docx;
    }
    if ext.as_str() == DOC_EXT {
        return FileKind::Doc;
    }
    if ext.as_str() == EPUB_EXT {
        return FileKind::Epub;
    }
    if ext.as_str() == PPTX_EXT {
        return FileKind::Pptx;
    }
    if ext.as_str() == XLSX_EXT {
        return FileKind::Xlsx;
    }
    if ext.as_str() == RTF_EXT {
        return FileKind::Rtf;
    }
    if ext.as_str() == NUMBERS_EXT {
        return FileKind::Numbers;
    }
    if ext.as_str() == KEY_EXT {
        return FileKind::Key;
    }
    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return FileKind::Text;
    }
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        return FileKind::Video;
    }
    if MAP_EXTENSIONS.contains(&ext.as_str()) {
        return FileKind::Map;
    }
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return FileKind::Image;
    }
    FileKind::Utf8Probe
}

pub fn is_supported_path(path: &Path) -> bool {
    file_kind_from_path(path) != FileKind::Zim
}

/// Returns `Some(display_label)` when the file is already indexed with matching size and mtime.
/// Tries the portable `relative_path` key first, then the absolute `path` column (legacy / key mismatch).
///
/// `logical_path_for_portable`: when the indexed file was copied into AthenaData (e.g. Unsorted)
/// but the portable DB key should still reflect the original watch-folder path, pass that path here.
pub fn skip_if_up_to_date(
    path: &Path,
    db: &Database,
    ingest_context: Option<(&str, &Path)>,
    logical_path_for_portable: Option<&Path>,
) -> Option<String> {
    let fs_meta = std::fs::metadata(path).ok()?;
    let size = fs_meta.len() as i64;
    let modified_at = fs_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    fn matches_stored(
        size: i64,
        modified_at: Option<i64>,
        db_size: i64,
        db_mtime: Option<i64>,
    ) -> bool {
        db_size == size && db_mtime == modified_at
    }

    let portable_src = logical_path_for_portable.unwrap_or(path);
    if let Some((stored_root, resolved_root)) = ingest_context {
        if let Some((display_rel, db_key)) =
            portable_index_key(stored_root, resolved_root, portable_src)
        {
            if let Ok(Some((db_size, db_mtime))) = db.get_file_metadata(&db_key) {
                if matches_stored(size, modified_at, db_size, db_mtime) {
                    println!(
                        "[athena] Skipping {} (already up to date)",
                        display_rel
                    );
                    return Some(display_rel);
                }
            }
        }
    }

    let mut path_candidates: Vec<String> = Vec::new();
    let plain = path.to_string_lossy().to_string();
    path_candidates.push(plain.clone());
    if let Ok(canon) = path.canonicalize() {
        let s = canon.to_string_lossy().to_string();
        if s != plain {
            path_candidates.push(s);
        }
    }
    for cand in &path_candidates {
        if let Ok(Some((db_size, db_mtime))) = db.get_file_metadata_by_path(cand) {
            if matches_stored(size, modified_at, db_size, db_mtime) {
                let label = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(cand.as_str());
                println!("[athena] Skipping {} (already up to date)", label);
                return Some(plain.clone());
            }
        }
    }
    None
}

pub fn fill_relative_path_key(
    data: &mut IndexedFileData,
    ingest_context: Option<(&str, &Path)>,
    path: &Path,
) {
    if let Some((stored_root, resolved_root)) = ingest_context {
        if let Some((_, db_key)) = portable_index_key(stored_root, resolved_root, path) {
            data.relative_path = Some(db_key);
        }
    }
}

/// Full ingestion for one file (after [`skip_if_up_to_date`] returned `None`).
pub fn index_file(path: &Path) -> Option<IndexedFileData> {
    if !path.is_file() {
        eprintln!(
            "[athena] ingestion error: path is not a file: {}",
            path.display()
        );
        return None;
    }
    let kind = file_kind_from_path(path);
    if kind == FileKind::Zim {
        eprintln!(
            "[athena] ingestion skip: ZIM (handled separately): {}",
            path.display()
        );
        return None;
    }
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();
    let path_str = path.to_string_lossy().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "[athena] ingestion error: could not read metadata for {}: {e}",
                path.display()
            );
            return None;
        }
    };
    let size = metadata.len() as i64;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let (text_content, metadata_json, display_title, page_count, index_pages) = match kind {
        FileKind::Text => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let text = if ext == "html" || ext == "htm" {
                std::fs::read_to_string(path)
                    .ok()
                    .map(|raw| strip_html(&raw))
            } else {
                std::fs::read_to_string(path).ok()
            };
            let content = text.map(|s| s.to_string());
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Pdf => {
            let display_title = pdf_document_title(path).unwrap_or_else(|| display_title_from_filename(&name));
            let mut index_pages: Option<Vec<(u32, String)>> = None;

            // PDF parsing is CPU-bound and blocking. This function is called from non-async contexts,
            // so avoid `tauri::async_runtime::block_on` (which can panic if invoked on a Tokio worker).
            if (metadata.len() as u64) <= PDF_MAX_PARSE_BYTES {
                match pdf_extract::extract_text_by_pages(path) {
                        Ok(pages) if !pages.is_empty() => {
                            let mut mapped: Vec<(u32, String)> = pages
                                .into_iter()
                                .enumerate()
                                .map(|(i, s)| ((i + 1) as u32, s))
                                .collect();
                            if mapped.len() > PDF_MAX_PAGES_STORED {
                                mapped.truncate(PDF_MAX_PAGES_STORED);
                            }
                            if mapped.iter().all(|(_, t)| t.trim().is_empty()) {
                                None
                            } else {
                                Some(mapped)
                            }
                        }
                        Ok(_) => None,
                        Err(e) => {
                            eprintln!("[athena] PDF Extraction Error: {:?}", e);
                            None
                        }
                }
                .map(|v| index_pages = Some(v));
            } else {
                eprintln!(
                    "[athena] PDF parse skipped due to size guardrail ({} bytes): {}",
                    metadata.len(),
                    path.display()
                );
            }

            if index_pages.is_none() && (metadata.len() as u64) <= PDF_MAX_PARSE_BYTES {
                let flat = match pdf_extract::extract_text(path) {
                        Ok(t) => {
                            let t = t.trim().to_string();
                            if t.is_empty() { None } else { Some(t) }
                        }
                        Err(e) => {
                            eprintln!("[athena] PDF Extraction Error: {:?}", e);
                            None
                        }
                    };
                if let Some(t) = flat {
                    index_pages = Some(vec![(1u32, t)]);
                }
            }
            let mut embedded_flat = index_pages
                .as_ref()
                .map(|p| p.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n\n"))
                .unwrap_or_default();
            let embedded_trim_len = embedded_flat.trim().len();
            if embedded_trim_len < PDF_FORCE_OCR_BELOW_CHARS {
                println!(
                    "[athena] PDF text layer is empty or scrambled. Forcing OCR fallback..."
                );
            }
            let try_ocr = embedded_trim_len < PDF_EMBEDDED_TRY_OCR_BELOW_CHARS;
            let ocr = if try_ocr {
                crate::ocr::extract_text_from_scanned_pdf(path)
            } else {
                None
            };
            let ocr = ocr.map(|s: String| s.trim().to_string()).filter(|s: &String| !s.is_empty());
            if let Some(ref o) = ocr {
                match &mut index_pages {
                    Some(pv) if !pv.is_empty() => {
                        if pv[0].1.trim().is_empty() {
                            pv[0].1.clone_from(o);
                        } else {
                            pv[0].1.push_str("\n\n");
                            pv[0].1.push_str(o);
                        }
                    }
                    _ => {
                        index_pages = Some(vec![(1u32, o.clone())]);
                    }
                }
            }
            embedded_flat = index_pages
                .as_ref()
                .map(|p| p.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n\n"))
                .unwrap_or_default();
            println!(
                "[athena] PDF Parser extracted {} characters",
                embedded_flat.len()
            );
            let page_count = index_pages.as_ref().map(|p| p.len() as i32);
            let content = {
                let t = embedded_flat.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(embedded_flat)
                }
            };
            (content, None, display_title, page_count, index_pages)
        }
        FileKind::Image => {
            let meta = image_metadata(path);
            let text_content = crate::ocr::extract_text_from_image(path);
            (
                text_content,
                meta,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Map => {
            let meta = image_metadata(path);
            (None, meta, display_title_from_filename(&name), None, None)
        }
        FileKind::Video => (
            None,
            Some("{}".to_string()),
            display_title_from_filename(&name),
            None,
            None,
        ),
        FileKind::Pages => {
            let content = extract_text_from_pages(path);
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Odt => {
            let content = extract_text_via_textutil(path).or_else(|| extract_text_from_odt(path));
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Docx => {
            let content = extract_text_via_textutil(path).or_else(|| extract_text_from_docx(path));
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Doc => {
            #[cfg(target_os = "macos")]
            let content = extract_text_via_textutil(path);
            #[cfg(not(target_os = "macos"))]
            let content = {
                eprintln!(
                    "[athena] ingestion: .doc requires macOS textutil; skipping {}",
                    path.display()
                );
                None
            };
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Epub => {
            let content = extract_text_from_epub(path);
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Pptx => {
            let content = extract_text_from_pptx(path);
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Xlsx => {
            let content = extract_text_from_xlsx(path);
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Rtf => {
            let content = extract_text_via_textutil(path).or_else(|| extract_text_from_rtf(path));
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Numbers | FileKind::Key => {
            let content = extract_text_from_apple_archive(path);
            (
                content,
                None,
                display_title_from_filename(&name),
                None,
                None,
            )
        }
        FileKind::Utf8Probe => match std::fs::read_to_string(path) {
            Ok(s) => (
                Some(s),
                None,
                display_title_from_filename(&name),
                None,
                None,
            ),
            Err(e) => {
                eprintln!(
                    "[athena] Unsupported file type (not valid UTF-8 text): {} ({})",
                    path.display(),
                    e
                );
                return None;
            }
        },
        FileKind::Zim => (None, None, display_title_from_filename(&name), None, None),
    };

    let full_text_raw = text_content.clone();
    let text_content = text_content.and_then(|t| limit_text_for_index(path, t));
    let index_pages = match (&full_text_raw, &text_content) {
        (Some(o), Some(t)) if o.len() == t.len() => index_pages,
        _ => None,
    };


    let file_type = match kind {
        FileKind::Text
        | FileKind::Pdf
        | FileKind::Pages
        | FileKind::Odt
        | FileKind::Docx
        | FileKind::Doc
        | FileKind::Epub
        | FileKind::Pptx
        | FileKind::Xlsx
        | FileKind::Rtf
        | FileKind::Numbers
        | FileKind::Key
        | FileKind::Utf8Probe => "text",
        FileKind::Image => "image",
        FileKind::Video => "video",
        FileKind::Map => "map",
        FileKind::Zim => {
            eprintln!(
                "[athena] ingestion error: unexpected ZIM kind for {}",
                path.display()
            );
            return None;
        }
    };

    println!(
        "[athena] Finished extracting indexable content for: {}",
        path.display()
    );
    println!("[athena] Checkpoint I1: packing IndexedFileData for return");
    Some(IndexedFileData {
        path: path_str,
        file_type: file_type.to_string(),
        name,
        size,
        modified_at,
        text_content,
        metadata_json,
        display_title,
        page_count,
        index_pages,
        relative_path: None,
    })
}

fn extract_text_from_pages(path: &Path) -> Option<String> {
    use zip::ZipArchive;

    if path.is_dir() {
        let preview = path.join("QuickLook").join("Preview.pdf");
        if preview.is_file() {
            if let Ok(t) = pdf_extract::extract_text(&preview) {
                let s = t.trim();
                if s.len() >= 8 {
                    return Some(s.to_string());
                }
            }
        }
        for name in ["Index", "index.xml"] {
            let p = path.join(name);
            if p.is_file() {
                if let Ok(content) = std::fs::read(&p) {
                    if let Some(t) = parse_pages_content(&content) {
                        return Some(t);
                    }
                }
            }
        }
        let mut iwa_blob: Vec<u8> = Vec::new();
        let index_dir = path.join("Index");
        if index_dir.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&index_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iwa")) {
                        if let Ok(buf) = std::fs::read(&p) {
                            if let Some(d) = unpack_iwa_snappy(&buf) {
                                if iwa_blob.len() + d.len() < 2_000_000 {
                                    iwa_blob.extend_from_slice(&d);
                                }
                            }
                        }
                    }
                }
            }
        }
        if !iwa_blob.is_empty() {
            if let Some(t) = harvest_likely_text_from_iwork_blob(&iwa_blob) {
                if t.len() >= 8 {
                    return Some(t);
                }
            }
        }
        return None;
    }

    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut collected: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok()?;
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        if entry.read_to_end(&mut buf).is_err() {
            continue;
        }
        if buf.len() > 8_000_000 {
            continue;
        }
        collected.push((name, buf));
    }

    for (name, buf) in &collected {
        let nl = name.to_lowercase();
        if nl.ends_with("/preview.pdf") || nl == "preview.pdf" {
            if let Ok(mut tmp) = tempfile::NamedTempFile::new() {
                if tmp.write_all(buf).is_ok() && tmp.flush().is_ok() {
                    if let Ok(t) = pdf_extract::extract_text(tmp.path()) {
                        let s = t.trim();
                        if s.len() >= 8 {
                            return Some(s.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut best: Option<Vec<u8>> = None;
    for (name, buf) in &collected {
        let name_lower = name.to_lowercase();
        if name == "Index"
            || name_lower == "index.xml"
            || name_lower.ends_with("document.xml")
            || name_lower.contains("index/document")
        {
            if best.as_ref().map_or(0, |b| b.len()) < buf.len() {
                best = Some(buf.clone());
            }
        }
    }
    if let Some(content) = best {
        if let Some(t) = parse_pages_content(&content) {
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }

    let mut xml_parts: Vec<String> = Vec::new();
    for (name, buf) in &collected {
        if name.to_lowercase().ends_with(".xml") {
            if let Some(t) = parse_pages_content(buf) {
                let t = t.trim();
                if t.len() > 3 {
                    xml_parts.push(t.to_string());
                }
            }
        }
    }
    if !xml_parts.is_empty() {
        return Some(xml_parts.join("\n\n"));
    }

    let mut iwa_blob: Vec<u8> = Vec::new();
    for (name, buf) in &collected {
        if name.to_lowercase().ends_with(".iwa") {
            if let Some(d) = unpack_iwa_snappy(buf) {
                if iwa_blob.len() + d.len() < 2_000_000 {
                    iwa_blob.extend_from_slice(&d);
                }
            }
        }
    }
    if !iwa_blob.is_empty() {
        if let Some(t) = harvest_likely_text_from_iwork_blob(&iwa_blob) {
            if t.len() >= 8 {
                return Some(t);
            }
        }
    }

    None
}

/// iWork `.iwa` uses Snappy-compressed chunks with a 4-byte header (type + 24-bit LE length).
fn unpack_iwa_snappy(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset + 4 <= data.len() {
        let chunk_type = data[offset];
        let chunk_len = ((data[offset + 1] as u32)
            | ((data[offset + 2] as u32) << 8)
            | ((data[offset + 3] as u32) << 16)) as usize;
        offset += 4;
        if chunk_len == 0 || offset + chunk_len > data.len() {
            break;
        }
        let chunk = &data[offset..offset + chunk_len];
        offset += chunk_len;
        if chunk_type == 0 {
            let mut dec = snap::raw::Decoder::new();
            let mut dst = Vec::new();
            if dec.decompress(chunk, &mut dst).is_ok() && !dst.is_empty() {
                out.extend_from_slice(&dst);
            }
        }
    }
    if !out.is_empty() {
        return Some(out);
    }
    let mut dec = snap::raw::Decoder::new();
    let mut dst = Vec::new();
    if dec.decompress(data, &mut dst).is_ok() && !dst.is_empty() {
        return Some(dst);
    }
    None
}

/// Best-effort UTF-8 harvest from decompressed protobuf-heavy iWork payloads (no full schema).
fn harvest_likely_text_from_iwork_blob(data: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(data);
    let mut seen = std::collections::HashSet::<String>::new();
    let mut parts: Vec<String> = Vec::new();
    for part in s.split(|c: char| c.is_control() || c == '\0') {
        let p = part.trim();
        if p.len() < 4 || p.len() > 400 {
            continue;
        }
        let letters = p.chars().filter(|c| c.is_alphabetic()).count();
        if letters < 3 {
            continue;
        }
        let ratio = letters as f32 / p.chars().count().max(1) as f32;
        if ratio < 0.35 {
            continue;
        }
        if !p.contains(' ') && p.len() > 28 && p.chars().filter(|c| c.is_uppercase()).count() > 4 {
            continue;
        }
        if seen.insert(p.to_string()) {
            parts.push(p.to_string());
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(
        parts
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn parse_pages_content(content: &[u8]) -> Option<String> {
    if content.starts_with(b"<?xml") || content.starts_with(b"<") {
        if let Ok(s) = String::from_utf8(content.to_vec()) {
            let text = strip_html(&s);
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    if content.starts_with(b"bplist") {
        if let Ok(value) = plist::from_bytes::<plist::Value>(content) {
            let strings = collect_plist_strings(&value);
            let joined: String = strings.join(" ").trim().to_string();
            if joined.len() > 20 {
                return Some(joined);
            }
        }
    }
    None
}

fn read_zip_entry(path: &Path, entry_name: &str) -> Option<Vec<u8>> {
    use zip::ZipArchive;
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn extract_text_from_odt(path: &Path) -> Option<String> {
    let content = read_zip_entry(path, "content.xml")?;
    let s = String::from_utf8_lossy(&content);
    let text = strip_html(&s);
    if text.trim().is_empty() {
        return None;
    }
    Some(text)
}

fn extract_text_from_docx(path: &Path) -> Option<String> {
    let content = read_zip_entry(path, "word/document.xml")?;
    let s = String::from_utf8_lossy(&content);
    let text = strip_html(&s);
    if text.trim().is_empty() {
        return None;
    }
    Some(text)
}

fn extract_text_from_pptx(path: &Path) -> Option<String> {
    use zip::ZipArchive;
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).ok()?;
        let name = entry.name().to_string();
        if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            names.push(name);
        }
    }
    names.sort();
    let mut out = String::new();
    for name in names {
        let file = std::fs::File::open(path).ok()?;
        let mut archive2 = ZipArchive::new(file).ok()?;
        let mut buf = Vec::new();
        if let Ok(mut entry) = archive2.by_name(&name) {
            let _ = entry.read_to_end(&mut buf);
        }
        if !buf.is_empty() {
            out.push_str(&strip_html(&String::from_utf8_lossy(&buf)));
            out.push(' ');
        }
    }
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

fn extract_text_from_xlsx(path: &Path) -> Option<String> {
    let content = read_zip_entry(path, "xl/sharedStrings.xml")?;
    let s = String::from_utf8_lossy(&content);
    let text = strip_html(&s);
    if text.trim().is_empty() {
        return None;
    }
    Some(text)
}

fn extract_text_from_epub(path: &Path) -> Option<String> {
    use zip::ZipArchive;
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut texts = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok()?;
        let name = entry.name().to_string();
        if name.ends_with(".xhtml") || name.ends_with(".html") {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() && buf.len() < 5_000_000 {
                texts.push(strip_html(&String::from_utf8_lossy(&buf)));
            }
        }
    }
    let out: String = texts.join(" ").trim().to_string();
    if out.is_empty() {
        return None;
    }
    Some(out)
}

fn extract_text_from_rtf(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut out = String::new();
    let mut i = 0;
    let mut skip_group = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            if bytes[i].is_ascii_alphabetic() {
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                continue;
            }
            if bytes[i] == b'\'' && i + 2 <= bytes.len() {
                i += 3;
                continue;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'{' {
            skip_group += 1;
            i += 1;
            continue;
        }
        if bytes[i] == b'}' {
            if skip_group > 0 {
                skip_group -= 1;
            }
            i += 1;
            continue;
        }
        if skip_group == 0 && (bytes[i].is_ascii_graphic() || bytes[i] == b' ' || bytes[i] == b'\n')
        {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    let trimmed = out
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    if trimmed.len() < 10 {
        return None;
    }
    Some(trimmed)
}

fn extract_text_from_apple_archive(path: &Path) -> Option<String> {
    if path.is_dir() {
        for name in ["Index", "index.xml"] {
            let p = path.join(name);
            if p.is_file() {
                if let Ok(content) = std::fs::read(&p) {
                    if let Some(t) = parse_pages_content(&content) {
                        return Some(t);
                    }
                }
            }
        }
        return None;
    }
    use zip::ZipArchive;
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    for name in ["Index", "index.xml", "Data/document.xml"] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() {
                if let Some(t) = parse_pages_content(&buf) {
                    return Some(t);
                }
            }
        }
    }
    None
}

fn collect_plist_strings(v: &plist::Value) -> Vec<String> {
    use plist::Value;
    let mut out = Vec::new();
    match v {
        Value::String(s) => {
            if s.len() > 10 {
                out.push(s.clone());
            }
        }
        Value::Array(arr) => {
            for item in arr {
                out.extend(collect_plist_strings(item));
            }
        }
        Value::Dictionary(dict) => {
            for (_, val) in dict {
                out.extend(collect_plist_strings(val));
            }
        }
        _ => {}
    }
    out
}

fn image_metadata(path: &Path) -> Option<String> {
    let dims = image::image_dimensions(path).ok()?;
    let obj = serde_json::json!({
        "width": dims.0,
        "height": dims.1
    });
    Some(obj.to_string())
}

pub struct IndexedFileData {
    pub path: String,
    pub file_type: String,
    pub name: String,
    pub size: i64,
    pub modified_at: Option<i64>,
    pub text_content: Option<String>,
    pub metadata_json: Option<String>,
    pub display_title: String,
    pub page_count: Option<i32>,
    /// Per-page text for PDFs (1-based page numbers). Drives page-aware chunking.
    pub index_pages: Option<Vec<(u32, String)>>,
    /// Stable portable key: `stored_watch_root|relative/path` (see `portable_index_key`).
    pub relative_path: Option<String>,
}

/// Outcome of an index attempt (skip vs new content vs failure).
/// Used by the ingest pipeline; kept for a stable, named contract with the assistant tooling.
#[allow(dead_code)]
pub enum InferenceResult {
    Skipped { relative_path: String },
    Indexed(IndexedFileData),
    Failed,
}

/// Normalizes `stored_watch_root` from config/DB and the file-relative segment using forward slashes.
pub fn portable_index_key(
    stored_watch_root: &str,
    resolved_library_root: &Path,
    file_path: &Path,
) -> Option<(String, String)> {
    let root_key = stored_watch_root.trim().replace('\\', "/");
    let root = resolved_library_root.canonicalize().ok()?;
    let file = file_path.canonicalize().ok()?;
    let rel = file.strip_prefix(&root).ok()?;
    let rel_norm = rel.to_string_lossy().replace('\\', "/");
    let db_key = format!("{}|{}", root_key, rel_norm);
    Some((rel_norm, db_key))
}

fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'&' {
            let rest = std::str::from_utf8(&bytes[i..]).unwrap_or("");
            if rest.starts_with("&amp;") {
                out.push('&');
                i += 5;
                continue;
            }
            if rest.starts_with("&lt;") {
                out.push('<');
                i += 4;
                continue;
            }
            if rest.starts_with("&gt;") {
                out.push('>');
                i += 4;
                continue;
            }
            if rest.starts_with("&quot;") {
                out.push('"');
                i += 6;
                continue;
            }
            if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
                out.push('\'');
                i += if rest.starts_with("&apos;") { 6 } else { 5 };
                continue;
            }
        }
        if bytes[i].is_ascii_alphanumeric()
            || bytes[i] == b' '
            || bytes[i] == b'\n'
            || bytes[i] == b'\t'
        {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Indexes a ZIM file and reports progress as `(processed, total_limit)` where `total_limit` is the
/// configured upper bound (`ZIM_ARTICLE_LIMIT`). This keeps UI progress deterministic even though
/// we don't know the true article count upfront.
pub fn index_zim_file_with_progress<F>(path: &std::path::Path, mut on_progress: F) -> Vec<IndexedFileData>
where
    F: FnMut(usize, usize),
{
    println!("[athena] Starting ZIM ingestion for file: {}", path.display());
    if !path.is_file() {
        eprintln!(
            "[athena] ZIM ingestion error: not a file: {}",
            path.display()
        );
        return Vec::new();
    }
    let path_str = path.to_string_lossy().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "[athena] ZIM ingestion error: metadata for {}: {e}",
                path.display()
            );
            return Vec::new();
        }
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let zim = match zim::Zim::new(path) {
        Ok(z) => z,
        Err(e) => {
            eprintln!(
                "[athena] ZIM ingestion error: could not open {}: {e}",
                path.display()
            );
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    let mut kept: usize = 0;
    let mut seen: usize = 0;
    let total_limit = ZIM_ARTICLE_LIMIT;
    for (idx, entry) in zim.iterate_by_urls().enumerate() {
        if idx >= ZIM_ARTICLE_LIMIT {
            break;
        }
        seen = idx + 1;
        if seen == 1 || (seen % 4000 == 0) {
            on_progress(seen, total_limit);
        }
        let is_text = match &entry.mime_type {
            zim::MimeType::Type(s) => s == "text/html" || s == "text/plain",
            _ => false,
        };
        if !is_text {
            continue;
        }
        let (cluster_idx, blob_idx) = match &entry.target {
            Some(zim::Target::Cluster(c, b)) => (*c, *b),
            _ => continue,
        };
        let cluster = match zim.get_cluster(cluster_idx) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let blob = match cluster.get_blob(blob_idx) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let raw = String::from_utf8_lossy(blob.as_ref());
        let text = match &entry.mime_type {
            zim::MimeType::Type(s) if s.contains("html") => strip_html(&raw),
            _ => raw.trim().to_string(),
        };
        if text.len() < 20 {
            continue;
        }
        let text_content = if text.len() > ZIM_ARTICLE_TEXT_MAX {
            format!("{}...", &text[..ZIM_ARTICLE_TEXT_MAX])
        } else {
            text
        };
        let article_path = format!("{}::{}", path_str, entry.url);
        let name = if entry.title.is_empty() {
            entry.url.clone()
        } else {
            entry.title.clone()
        };
        let display_title = if entry.title.is_empty() {
            display_title_from_filename(&entry.url)
        } else {
            entry.title.clone()
        };
        kept += 1;
        out.push(IndexedFileData {
            path: article_path,
            file_type: "zim".to_string(),
            name,
            size: 0,
            modified_at,
            text_content: Some(text_content),
            metadata_json: None,
            display_title,
            page_count: None,
            index_pages: None,
            relative_path: None,
        });
        if kept == 1 || (kept % 2000 == 0) {
            on_progress(seen, total_limit);
        }
    }
    on_progress(seen, total_limit);
    println!(
        "[athena] ZIM ingestion finished: {} article(s) from {}",
        out.len(),
        path.display()
    );
    out
}

/// Indexes a ZIM file: one DB row per text article (HTML/plain). Caller must delete_by_path_prefix(zim_path) before upserting.
pub fn index_zim_file(path: &std::path::Path) -> Vec<IndexedFileData> {
    index_zim_file_with_progress(path, |_processed, _total| {})
}

#[cfg(test)]
mod tests {
    use super::{limit_text_for_index, MAX_INDEXED_TEXT_BYTES};
    use std::path::Path;

    /// Phase 3: `limit_text_for_index` caps extracted text at exactly [`MAX_INDEXED_TEXT_BYTES`] UTF-8 bytes.
    #[test]
    fn extracted_text_truncates_to_one_million_bytes() {
        let path = Path::new("mock_extract.txt");
        let two_million_chars = "x".repeat(2_000_000);
        assert_eq!(two_million_chars.len(), 2_000_000);

        let limited = limit_text_for_index(path, two_million_chars).expect("non-empty after cap");
        assert_eq!(limited.len(), MAX_INDEXED_TEXT_BYTES);
        assert_eq!(MAX_INDEXED_TEXT_BYTES, 1_000_000);
    }
}
