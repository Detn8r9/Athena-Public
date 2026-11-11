//! Offline Academy: bundled curriculum JSON and ZIM article extraction for lessons.
//! Optional PDF units live under `payload/Academy/` (see `metadata.json` per unit).

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

use crate::compute_watcher_folders;
use crate::config::load_config;
use crate::kiwix::{scan_kiwix_zim_files_multi, DiscoveredZim};
use crate::AppState;

const CURRICULUM_JSON: &str = include_str!("../../../resources/academy_curriculum.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyLesson {
    pub id: String,
    pub title: String,
    pub zim_name_hint: String,
    pub article_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademySubject {
    pub id: String,
    pub label: String,
    pub lessons: Vec<AcademyLesson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyAgeGroup {
    pub id: String,
    pub label: String,
    pub subjects: Vec<AcademySubject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyCurriculumRoot {
    pub age_groups: Vec<AcademyAgeGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyLessonIdArg {
    pub lesson_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyLessonHtml {
    pub title: String,
    pub html: String,
}

/// One unit folder under `payload/Academy/.../UnitName/` with `metadata.json` and optional PDFs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademyPdfUnit {
    pub id: String,
    pub title: String,
    pub grade: u8,
    pub subject: String,
    pub tags: Vec<String>,
    pub reader_path: Option<String>,
    pub lecture_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AcademyPdfMetadata {
    title: String,
    grade: u8,
    subject: String,
    #[serde(default)]
    tags: Vec<String>,
}

pub fn get_curriculum() -> Result<AcademyCurriculumRoot, String> {
    serde_json::from_str(CURRICULUM_JSON).map_err(|e| format!("academy curriculum JSON: {e}"))
}

/// Discover bundled PDF curriculum units (`reader.pdf` / `lecture_guide.pdf`) from `payload/Academy`.
pub fn list_pdf_units(app: &AppHandle) -> Result<Vec<AcademyPdfUnit>, String> {
    let base = app
        .path()
        .resolve("payload/Academy", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Could not resolve payload/Academy: {e}"))?;
    if !base.is_dir() {
        return Ok(vec![]);
    }

    let mut out: Vec<AcademyPdfUnit> = Vec::new();
    for entry in WalkDir::new(&base).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.file_name() != Some(OsStr::new("metadata.json")) {
            continue;
        }
        let Some(unit_dir) = path.parent() else {
            continue;
        };
        let raw =
            std::fs::read_to_string(path).map_err(|e| format!("Read {}: {e}", path.display()))?;
        let meta: AcademyPdfMetadata =
            serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?;

        let rel = unit_dir
            .strip_prefix(&base)
            .map_err(|e| format!("Path strip: {e}"))?;
        let id = rel.to_string_lossy().replace('\\', "/");

        let reader = unit_dir.join("reader.pdf");
        let lecture = unit_dir.join("lecture_guide.pdf");
        out.push(AcademyPdfUnit {
            id,
            title: meta.title,
            grade: meta.grade,
            subject: meta.subject,
            tags: meta.tags,
            reader_path: reader.is_file().then(|| reader.to_string_lossy().into_owned()),
            lecture_path: lecture.is_file().then(|| lecture.to_string_lossy().into_owned()),
        });
    }

    out.sort_by(|a, b| {
        a.subject
            .cmp(&b.subject)
            .then_with(|| a.grade.cmp(&b.grade))
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(out)
}

fn find_lesson<'a>(root: &'a AcademyCurriculumRoot, lesson_id: &str) -> Option<&'a AcademyLesson> {
    let id = lesson_id.trim();
    for g in &root.age_groups {
        for s in &g.subjects {
            for l in &s.lessons {
                if l.id == id {
                    return Some(l);
                }
            }
        }
    }
    None
}

fn library_scan_roots(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config = load_config(&app_data_dir);
    let mut roots: Vec<PathBuf> = if let Some(state) = app.try_state::<AppState>() {
        let g = state.db.lock().map_err(|e| e.to_string())?;
        let folders = compute_watcher_folders(&*g, &app_data_dir, &config)?;
        folders.into_iter().map(PathBuf::from).collect()
    } else {
        vec![config
            .watch_folder
            .as_deref()
            .filter(|p| Path::new(p).is_dir())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                crate::athena_paths::AthenaPaths::new(&app_data_dir)
                    .user_downloads_dir()
                    .join("packages")
            })]
    };
    if let Ok(zdir) = crate::kiwix_serve::zim_archives_dir(app) {
        if !roots.iter().any(|r| r == &zdir) {
            roots.push(zdir);
        }
    }
    Ok(roots)
}

fn pick_zim(zims: &[DiscoveredZim], hint: &str) -> Option<PathBuf> {
    if zims.is_empty() {
        return None;
    }
    let h = hint.trim().to_lowercase();
    if h.is_empty() {
        return Some(PathBuf::from(zims[0].path.clone()));
    }
    zims.iter()
        .find(|z| z.name.to_lowercase().contains(&h) || z.path.to_lowercase().contains(&h))
        .map(|z| PathBuf::from(z.path.clone()))
        .or_else(|| Some(PathBuf::from(zims[0].path.clone())))
}

fn urls_match(a: &str, b: &str) -> bool {
    let a = a.trim().trim_start_matches('/').to_lowercase();
    let b = b.trim().trim_start_matches('/').to_lowercase();
    a == b
}

/// Load HTML for a curriculum lesson from a local ZIM (same discovery roots as Library Kiwix scan).
pub fn fetch_lesson_html(app: &AppHandle, lesson_id: &str) -> Result<AcademyLessonHtml, String> {
    let curriculum = get_curriculum()?;
    let lesson = find_lesson(&curriculum, lesson_id)
        .ok_or_else(|| format!("Unknown lesson id: {lesson_id}"))?;

    let roots = library_scan_roots(app)?;
    let zims = scan_kiwix_zim_files_multi(&roots).map_err(|e| format!("ZIM scan: {e}"))?;
    let zim_path = pick_zim(&zims, &lesson.zim_name_hint).ok_or_else(|| {
        "No .zim files found under the library / Kiwix folder. Add a ZIM to test lessons."
            .to_string()
    })?;

    let zim =
        zim::Zim::new(&zim_path).map_err(|e| format!("Open ZIM {}: {e}", zim_path.display()))?;

    let target_url = lesson.article_url.trim();
    for entry in zim.iterate_by_urls() {
        if !urls_match(&entry.url, target_url) {
            continue;
        }
        let is_html = matches!(&entry.mime_type, zim::MimeType::Type(s) if s.contains("html"));
        if !is_html {
            return Err(format!(
                "Article \"{}\" is not HTML in this ZIM (mime {:?}).",
                target_url, entry.mime_type
            ));
        }
        let (cluster_idx, blob_idx) = match &entry.target {
            Some(zim::Target::Cluster(c, b)) => (*c, *b),
            _ => continue,
        };
        let cluster = zim
            .get_cluster(cluster_idx)
            .map_err(|e| format!("ZIM cluster read: {e}"))?;
        let blob = cluster
            .get_blob(blob_idx)
            .map_err(|e| format!("ZIM blob read: {e}"))?;
        let html = String::from_utf8_lossy(blob.as_ref()).into_owned();
        let title = if entry.title.trim().is_empty() {
            lesson.title.clone()
        } else {
            entry.title.clone()
        };
        return Ok(AcademyLessonHtml { title, html });
    }

    Err(format!(
        "Article URL \"{}\" was not found in {}. Adjust academy_curriculum.json to match your ZIM.",
        target_url,
        zim_path.display()
    ))
}
