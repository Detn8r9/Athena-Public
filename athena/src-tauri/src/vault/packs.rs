//! Library packs: a folder of documents packed into one compressed ZIM archive.
//!
//! A pack stays readable in place (Academy and any Kiwix reader open it without unpacking). What
//! is staged is the chat index: a pack is browsable as soon as it exists, and chat search over it
//! is switched on later, when there is disk space for the index. ATHENA says how much that is.
//!
//! Inside a pack:
//! - `orig/<path>`: the original file, byte for byte, for formats that are already compressed
//!   (PDF, EPUB, Word, images). Nothing is lost.
//! - `text/<path>.html`: a text copy of that file with page markers. Text compresses well, and it
//!   is what chat reads; a citation still names the page of the original.
//! - `text/<path>` for plain-text sources: the file itself, compressed.
//! - `athena/manifest.json`: every document with its size, SHA-256 and topic.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::coverage;
use super::zimwrite::{PlannedEntry, ZimWriter, MAX_STORED_BYTES};
use crate::db::Database;
use crate::indexer;

/// File-name prefix that marks a ZIM as an ATHENA pack (the folder watcher serves these but does
/// not index them article by article: their index is staged).
pub const PACK_PREFIX: &str = "athena-pack_";
/// Index key prefix for documents that live inside a pack.
pub const INDEX_SCHEME: &str = "athena-pack://";
pub const MANIFEST_PATH: &str = "athena/manifest.json";
const NS: u8 = b'A';

/// Chat index bytes needed per byte of document text. Measured on the test library (12 Nov
/// 2026): 4.1 MB of book text produced 22.6 MB of chunk rows (parent and child text, retrieval
/// text, one 384-number vector per child), plus the stored text itself and a second copy of each
/// vector for approximate search: about 8 times the text. 9 leaves a margin.
const INDEX_BYTES_PER_TEXT_BYTE: u64 = 9;

pub fn is_pack_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(PACK_PREFIX) && n.to_ascii_lowercase().ends_with(".zim"))
}

pub fn estimate_index_bytes(text_bytes: u64) -> u64 {
    text_bytes.saturating_mul(INDEX_BYTES_PER_TEXT_BYTE)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackDoc {
    /// Path of the source file relative to the packed folder, with forward slashes.
    pub rel: String,
    pub title: String,
    pub mime: String,
    pub bytes: u64,
    pub sha256: String,
    /// Entry holding the untouched original, when it is stored separately from the text.
    #[serde(default)]
    pub orig: Option<String>,
    /// Entry holding the text chat reads.
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub pages: u32,
    #[serde(default)]
    pub text_bytes: u64,
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub version: u32,
    pub title: String,
    pub created_at: i64,
    pub original_bytes: u64,
    pub text_bytes: u64,
    pub docs: Vec<PackDoc>,
}

struct Source {
    path: PathBuf,
    rel: String,
    bytes: u64,
    /// Plain text is stored once, compressed. Everything else keeps its original plus a text copy.
    plain: bool,
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "html" | "htm" => "text/html",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "txt" | "text" | "vtt" | "srt" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "application/octet-stream",
    }
}

fn is_plain_text(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
        "txt" | "text" | "md" | "csv" | "vtt" | "srt"
    )
}

fn collect_sources(dir: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let hidden = path
            .strip_prefix(dir)
            .ok()
            .is_some_and(|rel| rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')));
        if hidden || !path.is_file() || !indexer::is_supported_path(path) {
            continue;
        }
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zim")) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(dir) else { continue };
        let Ok(meta) = path.metadata() else { continue };
        if meta.len() == 0 || meta.len() > MAX_STORED_BYTES {
            continue;
        }
        out.push(Source {
            path: path.to_path_buf(),
            rel: rel.to_string_lossy().replace('\\', "/"),
            bytes: meta.len(),
            plain: is_plain_text(path),
        });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

/// The text copy of a document: one `<section data-page="N">` per page, so the page numbers
/// survive packing and chat can cite them.
pub fn text_copy_html(title: &str, orig_rel: &str, pages: &[(u32, String)]) -> String {
    let depth = orig_rel.matches('/').count() + 1;
    let up = "../".repeat(depth);
    let mut out = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{t}</title></head><body><h1>{t}</h1>\
         <p><a href=\"{up}orig/{href}\">Open the original file</a></p>",
        t = escape_html(title),
        href = escape_html(orig_rel),
    );
    for (page, text) in pages {
        out.push_str(&format!(
            "<section data-page=\"{page}\"><h2>Page {page}</h2><pre>{}</pre></section>",
            escape_html(text)
        ));
    }
    out.push_str("</body></html>");
    out
}

/// Reverse of [`text_copy_html`].
pub fn parse_text_copy(html: &str) -> Vec<(u32, String)> {
    let mut pages = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<section data-page=\"") {
        rest = &rest[i + "<section data-page=\"".len()..];
        let Some(q) = rest.find('"') else { break };
        let page: u32 = rest[..q].parse().unwrap_or(0);
        let Some(start) = rest.find("<pre>") else { break };
        let body = &rest[start + 5..];
        let Some(end) = body.find("</pre></section>") else { break };
        pages.push((page, unescape_html(&body[..end])));
        rest = &body[end..];
    }
    pages
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn title_from_rel(rel: &str) -> String {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    stem.replace(['_', '-'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text of a non-plain document as numbered pages (a document without pages is one page).
fn extract_pages(path: &Path) -> (String, Vec<(u32, String)>) {
    let data = std::panic::catch_unwind(|| indexer::index_file(path)).ok().flatten();
    let Some(data) = data else {
        return (String::new(), Vec::new());
    };
    let pages = match data.index_pages {
        Some(p) if !p.is_empty() => p,
        _ => data.text_content.filter(|t| !t.trim().is_empty()).map(|t| vec![(1u32, t)]).unwrap_or_default(),
    };
    (data.display_title, pages)
}

/// Pack every supported document under `source_dir` into `out_path`. `on_progress(done, total,
/// name)` is called before each document. Returns the manifest written into the pack.
pub fn build_pack(
    source_dir: &Path,
    out_path: &Path,
    title: &str,
    level: i32,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> Result<PackManifest, String> {
    let sources = collect_sources(source_dir);
    if sources.is_empty() {
        return Err("That folder has no documents ATHENA can read.".to_string());
    }

    let mut planned = vec![
        PlannedEntry { namespace: NS, url: "index.html".into(), title: title.to_string(), mime: "text/html".into() },
        PlannedEntry { namespace: NS, url: MANIFEST_PATH.into(), title: "Pack manifest".into(), mime: "application/json".into() },
    ];
    for key in ["Title", "Description", "Language", "Creator", "Publisher", "Date", "Name"] {
        planned.push(PlannedEntry { namespace: b'M', url: key.into(), title: key.into(), mime: "text/plain".into() });
    }
    for s in &sources {
        let doc_title = title_from_rel(&s.rel);
        if s.plain {
            planned.push(PlannedEntry {
                namespace: NS,
                url: format!("text/{}", s.rel),
                title: doc_title,
                mime: "text/plain".into(),
            });
        } else {
            planned.push(PlannedEntry {
                namespace: NS,
                url: format!("orig/{}", s.rel),
                title: format!("{doc_title} (original)"),
                mime: mime_for(&s.path).into(),
            });
            planned.push(PlannedEntry {
                namespace: NS,
                url: format!("text/{}.html", s.rel),
                title: doc_title,
                mime: "text/html".into(),
            });
        }
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let part = out_path.with_extension("zim.part");
    let mut writer = ZimWriter::create(&part, planned, Some((NS, "index.html")), level)?;

    let mut docs = Vec::with_capacity(sources.len());
    let total = sources.len();
    for (i, s) in sources.iter().enumerate() {
        on_progress(i, total, &s.rel);
        let sha256 = sha256_file(&s.path)?;
        let mut doc = PackDoc {
            rel: s.rel.clone(),
            title: title_from_rel(&s.rel),
            mime: mime_for(&s.path).to_string(),
            bytes: s.bytes,
            sha256,
            orig: None,
            text: None,
            pages: 0,
            text_bytes: 0,
            topic: None,
        };
        let sample: String;
        if s.plain {
            let bytes = std::fs::read(&s.path).map_err(|e| format!("{}: {e}", s.rel))?;
            let url = format!("text/{}", s.rel);
            writer.add_compressed(NS, &url, &bytes)?;
            doc.text_bytes = bytes.len() as u64;
            doc.text = Some(url);
            sample = String::from_utf8_lossy(&bytes[..bytes.len().min(20_000)]).into_owned();
        } else {
            let url = format!("orig/{}", s.rel);
            let mut file = std::fs::File::open(&s.path).map_err(|e| format!("{}: {e}", s.rel))?;
            writer.add_stored(NS, &url, s.bytes, &mut file)?;
            doc.orig = Some(url);
            let (found_title, pages) = extract_pages(&s.path);
            if !found_title.trim().is_empty() {
                doc.title = found_title.trim().to_string();
            }
            sample = pages.iter().take(3).map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n").chars().take(20_000).collect();
            if !pages.is_empty() {
                let text_url = format!("text/{}.html", s.rel);
                let html = text_copy_html(&doc.title, &s.rel, &pages);
                writer.add_compressed(NS, &text_url, html.as_bytes())?;
                doc.pages = pages.len() as u32;
                doc.text_bytes = pages.iter().map(|(_, t)| t.len() as u64).sum();
                doc.text = Some(text_url);
            }
        }
        doc.topic = coverage::best_topic(&format!("{} {}", doc.title, doc.rel), &sample).map(|t| t.key().to_string());
        docs.push(doc);
    }

    let manifest = PackManifest {
        version: 1,
        title: title.to_string(),
        created_at: chrono::Utc::now().timestamp(),
        original_bytes: docs.iter().map(|d| d.bytes).sum(),
        text_bytes: docs.iter().map(|d| d.text_bytes).sum(),
        docs,
    };

    let mut index = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{t}</title></head><body><h1>{t}</h1><p>{n} documents packed by ATHENA.</p><ul>",
        t = escape_html(title),
        n = manifest.docs.len()
    );
    for d in &manifest.docs {
        let text = d.text.as_ref().map(|u| format!("<a href=\"{}\">{}</a>", escape_html(u), escape_html(&d.title)));
        let orig = d.orig.as_ref().map(|u| format!(" (<a href=\"{}\">original</a>)", escape_html(u)));
        index.push_str(&format!(
            "<li>{}{}</li>",
            text.unwrap_or_else(|| escape_html(&d.title)),
            orig.unwrap_or_default()
        ));
    }
    index.push_str("</ul></body></html>");
    writer.add_compressed(NS, "index.html", index.as_bytes())?;
    writer.add_compressed(NS, MANIFEST_PATH, &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let description = format!("{} documents packed by ATHENA", manifest.docs.len());
    let pack_name = out_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "athena-pack".to_string());
    for (key, value) in [
        ("Title", title),
        ("Description", description.as_str()),
        ("Language", "eng"),
        ("Creator", "ATHENA"),
        ("Publisher", "ATHENA"),
        ("Date", date.as_str()),
        ("Name", pack_name.as_str()),
    ] {
        writer.add_compressed(b'M', key, value.as_bytes())?;
    }
    writer.finish()?;

    // Only a pack that reads back and matches its checksum replaces any earlier file.
    let check = indexer::open_zim(&part).and_then(|z| z.verify_checksum().map_err(|e| e.to_string()));
    if let Err(e) = check {
        let _ = std::fs::remove_file(&part);
        return Err(format!("The pack did not verify after writing: {e}"));
    }
    std::fs::rename(&part, out_path).map_err(|e| e.to_string())?;
    Ok(manifest)
}

fn read_entry(zim: &zim::Zim, url: &str) -> Result<Vec<u8>, String> {
    let entry = zim
        .get_by_path(zim::Namespace::Articles, url)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{url} is not in the pack."))?;
    let content = zim.entry_content(&entry).map_err(|e| e.to_string())?.ok_or_else(|| format!("{url} has no content."))?;
    content.to_vec().map_err(|e| e.to_string())
}

pub fn read_manifest(pack: &Path) -> Result<PackManifest, String> {
    let zim = indexer::open_zim(pack)?;
    let bytes = read_entry(&zim, MANIFEST_PATH)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Pack manifest: {e}"))
}

/// Check that every original in the pack still matches the SHA-256 recorded when it was packed.
pub fn verify_pack(pack: &Path) -> Result<usize, String> {
    let zim = indexer::open_zim(pack)?;
    zim.verify_checksum().map_err(|e| e.to_string())?;
    let manifest: PackManifest = serde_json::from_slice(&read_entry(&zim, MANIFEST_PATH)?).map_err(|e| e.to_string())?;
    for d in &manifest.docs {
        let url = d.orig.as_ref().or(d.text.as_ref()).ok_or_else(|| format!("{} has no content entry.", d.rel))?;
        let bytes = read_entry(&zim, url)?;
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if got != d.sha256 {
            return Err(format!("{} does not match its recorded checksum.", d.rel));
        }
    }
    Ok(manifest.docs.len())
}

pub fn index_key(pack_file_name: &str, rel: &str) -> String {
    format!("{INDEX_SCHEME}{pack_file_name}/{rel}")
}

/// The pages of one packed document, as chat will read them.
fn doc_pages(zim: &zim::Zim, doc: &PackDoc) -> Result<Vec<(u32, String)>, String> {
    let Some(url) = doc.text.as_ref() else {
        return Ok(Vec::new());
    };
    let bytes = read_entry(zim, url)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(if doc.orig.is_some() { parse_text_copy(&text) } else { vec![(1, text)] })
}

/// Switch on chat search for a pack: read each document's text out of the archive and index it.
/// Returns how many documents were indexed. `should_stop` is checked between documents.
pub fn index_pack(
    db: &Mutex<Database>,
    pack: &Path,
    mut on_progress: impl FnMut(usize, usize, &str),
    should_stop: impl Fn() -> bool,
) -> Result<usize, String> {
    let file_name = pack.file_name().and_then(|n| n.to_str()).ok_or("Bad pack path.")?.to_string();
    let zim = indexer::open_zim(pack)?;
    let manifest: PackManifest = serde_json::from_slice(&read_entry(&zim, MANIFEST_PATH)?).map_err(|e| e.to_string())?;
    let total = manifest.docs.len();
    let mut done = 0;
    for (i, doc) in manifest.docs.iter().enumerate() {
        if should_stop() {
            break;
        }
        on_progress(i, total, &doc.title);
        let pages = doc_pages(&zim, doc)?;
        let text: String = pages.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n\n");
        if text.trim().is_empty() {
            continue;
        }
        let key = index_key(&file_name, &doc.rel);
        let file_type = doc.rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_else(|| "text".to_string());
        let name = doc.rel.rsplit('/').next().unwrap_or(&doc.rel);
        db.lock().map_err(|e| e.to_string())?.upsert_file(
            &key,
            &file_type,
            name,
            Some(doc.bytes as i64),
            Some(manifest.created_at),
            Some(&text),
            None,
            Some(&doc.title),
            Some(pages.len() as i64),
            None,
        )?;
        let page_refs = (pages.len() > 1).then_some(pages.as_slice());
        crate::vector_index::index_document_shared(db, &key, &doc.title, page_refs, &text)?;
        db.lock().map_err(|e| e.to_string())?.upsert_rag_document_for_file_path(&key, &file_type)?;
        done += 1;
    }
    Ok(done)
}

/// Switch chat search off for a pack and give its index space back. The pack itself is untouched.
pub fn unindex_pack(db: &Mutex<Database>, pack_file_name: &str) -> Result<usize, String> {
    let prefix = format!("{INDEX_SCHEME}{pack_file_name}/");
    let guard = db.lock().map_err(|e| e.to_string())?;
    let keys: Vec<String> = guard.with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT path FROM indexed_files WHERE substr(path, 1, ?1) = ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![prefix.len() as i64, prefix], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    })?;
    for key in &keys {
        guard.delete_chunk_embeddings_for_file(key)?;
        guard.delete_by_path(key)?;
        guard.with_conn(|conn| {
            conn.execute("DELETE FROM rag_documents WHERE id = ?1", rusqlite::params![key]).map_err(|e| e.to_string())?;
            Ok(())
        })?;
    }
    Ok(keys.len())
}

/// Documents of a pack that are in the chat index now.
pub fn indexed_doc_count(db: &Database, pack_file_name: &str) -> u32 {
    let prefix = format!("{INDEX_SCHEME}{pack_file_name}/");
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM indexed_files WHERE substr(path, 1, ?1) = ?2",
            rusqlite::params![prefix.len() as i64, prefix],
            |r| r.get::<_, u32>(0),
        )
        .map_err(|e| e.to_string())
    })
    .unwrap_or(0)
}

/// A file name for a new pack: `athena-pack_<slug>_<date>.zim`.
pub fn pack_file_name(title: &str, date: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "library".to_string() } else { slug.chars().take(48).collect() };
    format!("{PACK_PREFIX}{slug}_{date}.zim")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_copy_round_trips_pages_and_awkward_characters() {
        let pages = vec![(1, "Boil water <1 minute> & cool.".to_string()), (2, "\"Quoted\" text\nsecond line".to_string())];
        let html = text_copy_html("Water & you", "Medical/water guide.pdf", &pages);
        assert!(html.contains("href=\"../../orig/Medical/water guide.pdf\""));
        assert_eq!(parse_text_copy(&html), pages);
    }

    #[test]
    fn pack_names_are_safe_and_recognised() {
        let name = pack_file_name("Medical: books & guides!", "2026-11-12");
        assert_eq!(name, "athena-pack_medical-books-guides_2026-11-12.zim");
        assert!(is_pack_file(Path::new(&format!("/x/{name}"))));
        assert!(!is_pack_file(Path::new("/x/wikipedia_en_medicine_nopic_2026-10.zim")));
    }

    #[test]
    fn a_folder_packs_verifies_and_keeps_every_byte() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("Water")).unwrap();
        let guide = "Boil water for one minute to make it safe to drink.\n".repeat(400);
        std::fs::write(src.join("Water/boiling-water.txt"), &guide).unwrap();
        std::fs::write(src.join("notes.md"), "# Solar\nKeep the battery above half charge.\n").unwrap();
        std::fs::write(src.join(".hidden.txt"), "skip me").unwrap();

        let out = dir.path().join(pack_file_name("Test shelf", "2026-11-12"));
        let mut seen = Vec::new();
        let manifest = build_pack(&src, &out, "Test shelf", 19, |_, total, name| seen.push((total, name.to_string()))).unwrap();

        assert_eq!(manifest.docs.len(), 2);
        assert_eq!(seen.len(), 2);
        let water = manifest.docs.iter().find(|d| d.rel == "Water/boiling-water.txt").unwrap();
        assert_eq!(water.topic.as_deref(), Some("water"));
        assert_eq!(water.title, "boiling water");
        assert_eq!(water.text_bytes, guide.len() as u64);
        assert!(std::fs::metadata(&out).unwrap().len() < manifest.original_bytes, "repetitive text packs smaller");
        assert!(!out.with_extension("zim.part").exists());

        assert_eq!(read_manifest(&out).unwrap(), manifest);
        assert_eq!(verify_pack(&out).unwrap(), 2);
        let zim = indexer::open_zim(&out).unwrap();
        assert_eq!(read_entry(&zim, "text/Water/boiling-water.txt").unwrap(), guide.as_bytes());
        assert_eq!(doc_pages(&zim, water).unwrap(), vec![(1, guide)]);
    }

    /// Manual probe: `PACK_SRC=<folder> PACK_OUT=<file.zim> cargo test pack_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn pack_probe() {
        let (Ok(src), Ok(out)) = (std::env::var("PACK_SRC"), std::env::var("PACK_OUT")) else { return };
        let started = std::time::Instant::now();
        let m = build_pack(Path::new(&src), Path::new(&out), "Probe pack", 19, |i, n, name| eprintln!("{}/{n} {name}", i + 1)).unwrap();
        let packed = std::fs::metadata(&out).unwrap().len();
        eprintln!(
            "docs {} | original {} | packed {} ({:.1}%) | text {} | index estimate {} | {:?}",
            m.docs.len(),
            m.original_bytes,
            packed,
            100.0 * packed as f64 / m.original_bytes as f64,
            m.text_bytes,
            estimate_index_bytes(m.text_bytes),
            started.elapsed()
        );
        eprintln!("verified {} documents", verify_pack(Path::new(&out)).unwrap());
    }

    #[test]
    fn an_empty_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let err = build_pack(dir.path(), &dir.path().join("x.zim"), "Empty", 3, |_, _, _| {}).unwrap_err();
        assert!(err.contains("no documents"));
    }

    #[test]
    fn the_index_estimate_scales_with_the_text() {
        assert_eq!(estimate_index_bytes(0), 0);
        assert_eq!(estimate_index_bytes(1_000_000), 9_000_000);
    }
}
