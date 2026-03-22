use std::io::Read;
use std::path::Path;

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "html", "htm", "rst", "log", "csv", "json", "xml", "yaml", "yml",
    "tex", "org", "ini", "cfg", "conf", "toml", "env", "properties", "asc", "text",
    "adoc", "asciidoc", "creole", "wiki", "me", "nfo", "readme",
];
const PDF_EXTENSIONS: &[&str] = &["pdf"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "tif"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "avi", "mkv", "m4v"];
const MAP_EXTENSIONS: &[&str] = &["geotiff", "tif", "tiff"];
const ZIM_EXT: &str = "zim";
const PAGES_EXT: &str = "pages";
const ODT_EXT: &str = "odt";
const DOCX_EXT: &str = "docx";
const EPUB_EXT: &str = "epub";
const PPTX_EXT: &str = "pptx";
const XLSX_EXT: &str = "xlsx";
const RTF_EXT: &str = "rtf";
const NUMBERS_EXT: &str = "numbers";
const KEY_EXT: &str = "key";

const ZIM_ARTICLE_LIMIT: usize = 150_000;
const ZIM_ARTICLE_TEXT_MAX: usize = 60_000;
const SCANNED_PDF_MIN_TEXT_LEN: usize = 80;

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
    Epub,
    Pptx,
    Xlsx,
    Rtf,
    Numbers,
    Key,
    Unsupported,
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
    FileKind::Unsupported
}

pub fn is_supported_path(path: &Path) -> bool {
    file_kind_from_path(path) != FileKind::Unsupported
}

pub fn index_file(path: &std::path::Path) -> Option<IndexedFileData> {
    if !path.is_file() {
        return None;
    }
    let kind = file_kind_from_path(path);
    if kind == FileKind::Unsupported || kind == FileKind::Zim {
        return None;
    }
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();
    let path_str = path.to_string_lossy().to_string();
    let metadata = std::fs::metadata(path).ok()?;
    let size = metadata.len() as i64;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let (text_content, metadata_json) = match kind {
        FileKind::Text => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let text = if ext == "html" || ext == "htm" {
                std::fs::read_to_string(path).ok().map(|raw| strip_html(&raw))
            } else {
                std::fs::read_to_string(path).ok()
            };
            let content = text.filter(|s| s.len() <= 2_000_000).map(|s| s.to_string());
            (content, None)
        }
        FileKind::Pdf => {
            let text = pdf_extract::extract_text(path).ok();
            let content = match &text {
                Some(t) if t.trim().len() >= SCANNED_PDF_MIN_TEXT_LEN => {
                    text.filter(|s| s.len() <= 2_000_000).map(|s| s.to_string())
                }
                _ => {
                    crate::ocr::extract_text_from_scanned_pdf(path)
                        .or(text.filter(|s| !s.trim().is_empty()).map(|s| s.to_string()))
                }
            };
            let content = content.filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Image => {
            let meta = image_metadata(path);
            let text_content = crate::ocr::extract_text_from_image(path);
            (text_content, meta)
        }
        FileKind::Map => {
            let meta = image_metadata(path);
            (None, meta)
        }
        FileKind::Video => (None, Some("{}".to_string())),
        FileKind::Pages => {
            let content = extract_text_from_pages(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Odt => {
            let content = extract_text_from_odt(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Docx => {
            let content = extract_text_from_docx(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Epub => {
            let content = extract_text_from_epub(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Pptx => {
            let content = extract_text_from_pptx(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Xlsx => {
            let content = extract_text_from_xlsx(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Rtf => {
            let content = extract_text_from_rtf(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Numbers | FileKind::Key => {
            let content = extract_text_from_apple_archive(path).filter(|s| s.len() <= 2_000_000);
            (content, None)
        }
        FileKind::Zim | FileKind::Unsupported => (None, None),
    };

    let file_type = match kind {
        FileKind::Text | FileKind::Pdf | FileKind::Pages | FileKind::Odt | FileKind::Docx
        | FileKind::Epub | FileKind::Pptx | FileKind::Xlsx | FileKind::Rtf | FileKind::Numbers | FileKind::Key => "text",
        FileKind::Image => "image",
        FileKind::Video => "video",
        FileKind::Map => "map",
        FileKind::Zim | FileKind::Unsupported => return None,
    };

    Some(IndexedFileData {
        path: path_str,
        file_type: file_type.to_string(),
        name,
        size,
        modified_at,
        text_content,
        metadata_json,
    })
}

fn extract_text_from_pages(path: &Path) -> Option<String> {
    use zip::ZipArchive;
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
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut best: Option<(String, Vec<u8>)> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok()?;
        let name = entry.name().to_string();
        let name_lower = name.to_lowercase();
        if name == "Index" || name_lower == "index.xml" || name_lower.ends_with("document.xml") || name_lower.contains("index/document") {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_err() {
                continue;
            }
            if buf.len() > 2_000_000 {
                continue;
            }
            if best.as_ref().map_or(0, |(_, b)| b.len()) < buf.len() {
                best = Some((name, buf));
            }
        }
    }
    let (_, content) = best?;
    parse_pages_content(&content)
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
            if entry.read_to_end(&mut buf).is_ok() && buf.len() < 2_000_000 {
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
        if skip_group == 0 && (bytes[i].is_ascii_graphic() || bytes[i] == b' ' || bytes[i] == b'\n') {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    let trimmed = out.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string();
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
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b' ' || bytes[i] == b'\n' || bytes[i] == b'\t' {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Indexes a ZIM file: one DB row per text article (HTML/plain). Caller must delete_by_path_prefix(zim_path) before upserting.
pub fn index_zim_file(path: &std::path::Path) -> Vec<IndexedFileData> {
    if !path.is_file() {
        return Vec::new();
    }
    let path_str = path.to_string_lossy().to_string();
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let _size = metadata.len() as i64;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let zim = match zim::Zim::new(path) {
        Ok(z) => z,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for (idx, entry) in zim.iterate_by_urls().enumerate() {
        if idx >= ZIM_ARTICLE_LIMIT {
            break;
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
        out.push(IndexedFileData {
            path: article_path,
            file_type: "zim".to_string(),
            name,
            size: 0,
            modified_at,
            text_content: Some(text_content),
            metadata_json: None,
        });
    }
    out
}
