//! Host watch-folder → managed staging mirror copy and zip/tar extraction under `library/<category>/<bundle>/`.
//! All extracted paths are validated to stay under the destination root (zip-slip safe).

use std::fs::{self, File};
use std::io::{copy, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use flate2::read::GzDecoder;
use tauri::Emitter;
use tar::Archive;
use tokio::sync::Semaphore;
use zip::ZipArchive;

/// Progress for long-running archive unpack (UI listens on this channel).
pub const LIBRARY_ARCHIVE_EXTRACTION_PROGRESS_EVENT: &str = "athena:library-archive-extraction-progress";

// Unified managed downloads layout under AthenaData/user/.
const UNSORTED_REL: &str = "user/downloads/documents/staging";
const ARCHIVE_REL: &str = "user/downloads/documents/archive";
const EMIT_ENTRY_INTERVAL: usize = 32;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveExtractionProgressPayload {
    pub archive_path: String,
    pub archive_label: String,
    pub category: String,
    pub phase: String,
    pub entries_processed: u64,
    pub entries_total: Option<u64>,
    pub bytes_written: u64,
    pub message: Option<String>,
}

#[inline]
pub fn unsorted_dir(athena_root: &Path) -> PathBuf {
    athena_root.join(UNSORTED_REL)
}

#[inline]
pub fn archive_dir(athena_root: &Path) -> PathBuf {
    athena_root.join(ARCHIVE_REL)
}

/// If `path` is inside `Unsorted/`, move it into sibling `archive/` on success/up-to-date.
/// Returns the new archived path when moved.
pub fn move_unsorted_to_archive(path: &Path, athena_root: &Path) -> Result<Option<PathBuf>, String> {
    let unsorted = unsorted_dir(athena_root);
    let p = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let unsorted_can = unsorted
        .canonicalize()
        .unwrap_or_else(|_| unsorted.clone());
    if !p.starts_with(&unsorted_can) {
        return Ok(None);
    }

    let archive = archive_dir(athena_root);
    fs::create_dir_all(&archive).map_err(|e| e.to_string())?;
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "source has no file name".to_string())?;
    let dest = dedupe_destination_path(&archive, fname);
    fs::rename(path, &dest).map_err(|e| format!("archive move {} -> {}: {e}", path.display(), dest.display()))?;
    Ok(Some(dest))
}

/// True for `.zip`, `.tar`, `.tgz`, or `.tar.gz` (case-insensitive).
pub fn is_library_archive(path: &Path) -> bool {
    let lossy = path.to_string_lossy().to_ascii_lowercase();
    if lossy.ends_with(".tar.gz") || lossy.ends_with(".tgz") {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            let e = e.to_ascii_lowercase();
            e == "zip" || e == "tar"
        })
}

fn sanitize_bundle_dir_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '<' | '>' | '|' | '?' | '*' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        "archive".into()
    } else {
        s
    }
}

fn archive_bundle_stem(path: &Path) -> String {
    let lossy = path.to_string_lossy();
    let lower = lossy.to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archive");
    if lower.ends_with(".tar.gz") {
        return sanitize_bundle_dir_name(&name[..name.len().saturating_sub(7)]);
    }
    if lower.ends_with(".tgz") {
        return sanitize_bundle_dir_name(
            path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("archive"),
        );
    }
    sanitize_bundle_dir_name(
        path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("archive"),
    )
}

/// `name (1).ext`, `name (2).ext`, … if `name.ext` already exists.
pub fn dedupe_destination_path(dest_dir: &Path, original_name: &str) -> PathBuf {
    let primary = dest_dir.join(original_name);
    if !primary.exists() {
        return primary;
    }
    let p = Path::new(original_name);
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(original_name);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();
    for n in 1u32..10_000 {
        let cand = dest_dir.join(format!("{} ({}){}", stem, n, ext));
        if !cand.exists() {
            return cand;
        }
    }
    dest_dir.join(format!(
        "{}_{}",
        original_name,
        uuid::Uuid::new_v4().simple()
    ))
}

static LIBRARY_ARCHIVE_EXTRACT_SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn library_archive_extract_sem() -> Arc<Semaphore> {
    LIBRARY_ARCHIVE_EXTRACT_SEM
        .get_or_init(|| Arc::new(Semaphore::new(1)))
        .clone()
}

/// Blocking disk copy (canonicalize, dedupe, `fs::copy`). Run only from `spawn_blocking` or a dedicated OS thread.
fn copy_watch_file_to_unsorted_if_external_blocking(
    src: &Path,
    athena_root: &Path,
) -> Result<Option<PathBuf>, String> {
    if src
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zim"))
    {
        return Ok(None);
    }
    let src_can = src.canonicalize().map_err(|e| format!("{}: {e}", src.display()))?;
    let ath_can = athena_root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", athena_root.display()))?;
    if src_can.starts_with(&ath_can) {
        return Ok(None);
    }
    let unsorted = unsorted_dir(athena_root);
    fs::create_dir_all(&unsorted).map_err(|e| e.to_string())?;
    let archive = archive_dir(athena_root);
    let _ = fs::create_dir_all(&archive);
    let fname = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "source has no file name".to_string())?;
    let dest = dedupe_destination_path(&unsorted, fname);
    if dest.exists() {
        let sm = fs::metadata(&src_can).map_err(|e| e.to_string())?;
        let dm = fs::metadata(&dest).map_err(|e| e.to_string())?;
        if sm.len() == dm.len() {
            if let (Ok(sm_m), Ok(dm_m)) = (sm.modified(), dm.modified()) {
                if sm_m == dm_m {
                    return Ok(Some(dest));
                }
            }
        }
    }
    fs::copy(&src_can, &dest).map_err(|e| {
        format!(
            "copy {} -> {}: {e}",
            src_can.display(),
            dest.display()
        )
    })?;
    Ok(Some(dest))
}

/// Copy host watch files into `Unsorted` using the blocking pool so Tokio worker threads are not stalled on disk I/O.
pub fn copy_watch_file_to_unsorted_if_external(
    src: &Path,
    athena_root: &Path,
) -> Result<Option<PathBuf>, String> {
    let src = src.to_path_buf();
    let root = athena_root.to_path_buf();
    tauri::async_runtime::block_on(async {
        tokio::task::spawn_blocking(move || copy_watch_file_to_unsorted_if_external_blocking(&src, &root)).await
    })
    .map_err(|e| format!("watch-folder copy task: {e}"))?
}

fn emit_progress(
    app: &tauri::AppHandle,
    payload: ArchiveExtractionProgressPayload,
) {
    let _ = app.emit(LIBRARY_ARCHIVE_EXTRACTION_PROGRESS_EVENT, payload);
}

/// Join `relative` (archive-internal path) under `root`; rejects `..`, absolute segments, and escapes.
fn safe_path_under_root(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = relative.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') {
        return Err(format!("unsafe archive path: {relative:?}"));
    }
    let mut out = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(format!("path traversal in archive: {relative:?}"));
        }
        if part.contains(':') {
            return Err(format!("unsafe path segment: {part:?}"));
        }
        out.push(part);
    }
    let root_can = root.canonicalize().map_err(|e| e.to_string())?;
    let mut probe = out.clone();
    loop {
        match probe.canonicalize() {
            Ok(c) => {
                if !c.starts_with(&root_can) {
                    return Err(format!(
                        "resolved path escapes extraction root: {}",
                        c.display()
                    ));
                }
                break;
            }
            Err(_) => {
                if !probe.pop() {
                    return Err("path escapes extraction root".into());
                }
            }
        }
    }
    Ok(out)
}

fn extraction_dest_root(athena_root: &Path, category: &str, archive_path: &Path) -> Result<PathBuf, String> {
    let bundle = archive_bundle_stem(archive_path);
    let lib = crate::athena_paths::AthenaPaths::new(athena_root)
        .user_library_dir()
        .join(category)
        .join(&bundle);
    fs::create_dir_all(&lib).map_err(|e| e.to_string())?;
    Ok(lib)
}

fn open_zip_seekable(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|e| format!("open zip {}: {e}", path.display()))
}

/// Full archive unpack: acquire async semaphore, run heavy work on `spawn_blocking`.
pub async fn extract_archive_with_progress_async(
    app: tauri::AppHandle,
    archive_path: PathBuf,
    athena_root: PathBuf,
    category: String,
) -> Result<PathBuf, String> {
    let sem = library_archive_extract_sem();
    let _permit = sem
        .acquire_owned()
        .await
        .map_err(|_| "library archive extract semaphore closed".to_string())?;
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        extract_archive_blocking_body(&app2, &archive_path, &athena_root, &category)
    })
    .await
    .map_err(|e| format!("library archive extract join: {e}"))?
}

/// Extract an archive into a sibling folder next to the archive file:
/// `<archive_dir>/<bundle-stem>/...`
///
/// This is used for downloaded package archives so we can delete the original `.zip/.tar*`
/// immediately after a successful extraction to save space.
pub async fn extract_archive_in_place_with_progress_async(
    app: tauri::AppHandle,
    archive_path: PathBuf,
) -> Result<PathBuf, String> {
    let sem = library_archive_extract_sem();
    let _permit = sem
        .acquire_owned()
        .await
        .map_err(|_| "library archive extract semaphore closed".to_string())?;
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        let lossy = archive_path.to_string_lossy().to_ascii_lowercase();
        let label = archive_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archive")
            .to_string();
        let ap = archive_path.to_string_lossy().to_string();
        let parent = archive_path
            .parent()
            .ok_or_else(|| "Archive has no parent directory".to_string())?;
        let bundle = archive_bundle_stem(&archive_path);
        let dest_root = parent.join(bundle);
        fs::create_dir_all(&dest_root).map_err(|e| e.to_string())?;

        let inner = || -> Result<PathBuf, String> {
            if lossy.ends_with(".zip") {
                extract_zip(&app2, &archive_path, &dest_root, &label, "in_place")
            } else if lossy.ends_with(".tar.gz") || lossy.ends_with(".tgz") {
                extract_tar_gz(&app2, &archive_path, &dest_root, &label, "in_place")
            } else if lossy.ends_with(".tar") {
                extract_tar_plain(&app2, &archive_path, &dest_root, &label, "in_place")
            } else {
                Err(format!("unsupported archive type: {}", archive_path.display()))
            }
        };

        match inner() {
            Ok(p) => Ok(p),
            Err(e) => {
                emit_progress(
                    &app2,
                    ArchiveExtractionProgressPayload {
                        archive_path: ap,
                        archive_label: label,
                        category: "in_place".into(),
                        phase: "error".into(),
                        entries_processed: 0,
                        entries_total: None,
                        bytes_written: 0,
                        message: Some(e.clone()),
                    },
                );
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("library archive extract join: {e}"))?
}

/// Synchronous entry for dedicated watcher threads: schedules async extraction on the Tauri runtime.
pub fn extract_archive_with_progress(
    app: &tauri::AppHandle,
    archive_path: &Path,
    athena_root: &Path,
    category: &str,
) -> Result<PathBuf, String> {
    let app = app.clone();
    let archive_path = archive_path.to_path_buf();
    let athena_root = athena_root.to_path_buf();
    let category = category.to_string();
    tauri::async_runtime::block_on(extract_archive_with_progress_async(
        app,
        archive_path,
        athena_root,
        category,
    ))
}

fn extract_archive_blocking_body(
    app: &tauri::AppHandle,
    archive_path: &Path,
    athena_root: &Path,
    category: &str,
) -> Result<PathBuf, String> {
    let lossy = archive_path.to_string_lossy().to_ascii_lowercase();
    let label = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archive")
        .to_string();
    let ap = archive_path.to_string_lossy().to_string();

    let inner = || -> Result<PathBuf, String> {
        let dest_root = extraction_dest_root(athena_root, category, archive_path)?;
        if lossy.ends_with(".zip") {
            extract_zip(app, archive_path, &dest_root, &label, category)
        } else if lossy.ends_with(".tar.gz") || lossy.ends_with(".tgz") {
            extract_tar_gz(app, archive_path, &dest_root, &label, category)
        } else if lossy.ends_with(".tar") {
            extract_tar_plain(app, archive_path, &dest_root, &label, category)
        } else {
            Err(format!(
                "unsupported archive type: {}",
                archive_path.display()
            ))
        }
    };

    match inner() {
        Ok(p) => Ok(p),
        Err(e) => {
            emit_progress(
                app,
                ArchiveExtractionProgressPayload {
                    archive_path: ap,
                    archive_label: label,
                    category: category.to_string(),
                    phase: "error".into(),
                    entries_processed: 0,
                    entries_total: None,
                    bytes_written: 0,
                    message: Some(e.clone()),
                },
            );
            Err(e)
        }
    }
}

fn extract_zip(
    app: &tauri::AppHandle,
    archive_path: &Path,
    dest_root: &Path,
    label: &str,
    category: &str,
) -> Result<PathBuf, String> {
    let f = open_zip_seekable(archive_path)?;
    let mut za = ZipArchive::new(f).map_err(|e| format!("zip: {e}"))?;
    let zip_len = za.len();
    let total = zip_len as u64;
    let mut written: u64 = 0;
    let ap = archive_path.to_string_lossy().to_string();
    for i in 0..zip_len {
        let mut file = za.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let name = file.name().to_string();
        if file.is_dir() || name.ends_with('/') {
            let dir_path = safe_path_under_root(dest_root, name.trim_end_matches('/'))?;
            fs::create_dir_all(&dir_path).map_err(|e| e.to_string())?;
        } else {
            let out_path = safe_path_under_root(dest_root, &name)?;
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = File::create(&out_path).map_err(|e| e.to_string())?;
            copy(&mut file, &mut out).map_err(|e| e.to_string())?;
            written = written.saturating_add(out.metadata().map(|m| m.len()).unwrap_or(0));
        }
        if i == 0 || (i + 1) % EMIT_ENTRY_INTERVAL == 0 || i + 1 == zip_len {
            emit_progress(
                app,
                ArchiveExtractionProgressPayload {
                    archive_path: ap.clone(),
                    archive_label: label.to_string(),
                    category: category.to_string(),
                    phase: "zip".into(),
                    entries_processed: (i + 1) as u64,
                    entries_total: Some(total),
                    bytes_written: written,
                    message: None,
                },
            );
        }
    }
    emit_progress(
        app,
        ArchiveExtractionProgressPayload {
            archive_path: ap,
            archive_label: label.to_string(),
            category: category.to_string(),
            phase: "done".into(),
            entries_processed: total,
            entries_total: Some(total),
            bytes_written: written,
            message: None,
        },
    );
    Ok(dest_root.to_path_buf())
}

fn extract_tar_inner<R: Read>(
    app: &tauri::AppHandle,
    mut archive: Archive<R>,
    dest_root: &Path,
    label: &str,
    category: &str,
    archive_path_str: &str,
    phase: &str,
) -> Result<PathBuf, String> {
    let mut entries = archive.entries().map_err(|e| format!("tar entries: {e}"))?;
    let mut i: u64 = 0;
    let mut written: u64 = 0;
    while let Some(mut entry) = entries.next().transpose().map_err(|e| e.to_string())? {
        let path = entry.path().map_err(|e| e.to_string())?;
        let rel = path.to_string_lossy().replace('\\', "/");
        let out_path = safe_path_under_root(dest_root, rel.trim_start_matches('/'))?;
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            entry.unpack(&out_path).map_err(|e| {
                format!(
                    "unpack {} -> {}: {e}",
                    archive_path_str,
                    out_path.display()
                )
            })?;
            written = written.saturating_add(
                fs::metadata(&out_path)
                    .map(|m| m.len())
                    .unwrap_or(0),
            );
        }
        i += 1;
        if i == 1 || i % EMIT_ENTRY_INTERVAL as u64 == 0 {
            emit_progress(
                app,
                ArchiveExtractionProgressPayload {
                    archive_path: archive_path_str.to_string(),
                    archive_label: label.to_string(),
                    category: category.to_string(),
                    phase: phase.into(),
                    entries_processed: i,
                    entries_total: None,
                    bytes_written: written,
                    message: None,
                },
            );
        }
    }
    emit_progress(
        app,
        ArchiveExtractionProgressPayload {
            archive_path: archive_path_str.to_string(),
            archive_label: label.to_string(),
            category: category.to_string(),
            phase: "done".into(),
            entries_processed: i,
            entries_total: Some(i),
            bytes_written: written,
            message: None,
        },
    );
    Ok(dest_root.to_path_buf())
}

fn extract_tar_plain(
    app: &tauri::AppHandle,
    archive_path: &Path,
    dest_root: &Path,
    label: &str,
    category: &str,
) -> Result<PathBuf, String> {
    let f = File::open(archive_path).map_err(|e| e.to_string())?;
    let ar = Archive::new(f);
    let ap = archive_path.to_string_lossy().to_string();
    extract_tar_inner(app, ar, dest_root, label, category, &ap, "tar")
}

fn extract_tar_gz(
    app: &tauri::AppHandle,
    archive_path: &Path,
    dest_root: &Path,
    label: &str,
    category: &str,
) -> Result<PathBuf, String> {
    let f = File::open(archive_path).map_err(|e| e.to_string())?;
    let dec = GzDecoder::new(f);
    let ar = Archive::new(dec);
    let ap = archive_path.to_string_lossy().to_string();
    extract_tar_inner(app, ar, dest_root, label, category, &ap, "tar_gz")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn dedupe_suffix_increments() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("a.txt"), b"x").unwrap();
        let p2 = dedupe_destination_path(d.path(), "a.txt");
        assert!(p2.ends_with("a (1).txt"));
    }

    #[test]
    fn safe_path_rejects_dotdot() {
        let root = tempdir().unwrap();
        let r = root.path().join("out");
        fs::create_dir_all(&r).unwrap();
        assert!(safe_path_under_root(&r, "foo/../bar").is_err());
    }

    #[test]
    fn safe_path_accepts_nested_file() {
        let root = tempdir().unwrap();
        let r = root.path().join("out");
        fs::create_dir_all(&r).unwrap();
        let p = safe_path_under_root(&r, "a/b/c.txt").unwrap();
        assert!(p.ends_with("c.txt"));
    }
}