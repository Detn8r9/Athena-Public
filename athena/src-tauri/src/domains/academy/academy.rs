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
    let app_data_dir = crate::portable::resolved_app_data_dir(app)?;
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

/// Candidate ZIMs for a lesson, best first: name matches the hint, English before other languages
/// (an Adyghe Wikipedia was picked for an English lesson), then everything else as a fallback.
fn rank_zims(zims: &[DiscoveredZim], hint: &str) -> Vec<PathBuf> {
    let h = hint.trim().to_lowercase();
    let score = |z: &DiscoveredZim| {
        let name = z.name.to_lowercase();
        let hit = !h.is_empty() && (name.contains(&h) || z.path.to_lowercase().contains(&h));
        let english = name.contains("_en_");
        (!hit, !english)
    };
    let mut v: Vec<&DiscoveredZim> = zims.iter().collect();
    v.sort_by_key(|z| score(z));
    v.into_iter().map(|z| PathBuf::from(z.path.clone())).collect()
}

/// Split a curriculum URL such as `A/Water_purification` into its article path. Older ZIMs keep
/// articles in namespace `A`, current ones in `C`; the lookup tries both.
fn lesson_article_path(url: &str) -> &str {
    let u = url.trim().trim_start_matches('/');
    match u.split_once('/') {
        Some((ns, rest)) if ns.len() == 1 => rest,
        _ => u,
    }
}

fn read_lesson_from_zim(zim: &zim::Zim, path: &str) -> Result<Option<(String, String)>, String> {
    for ns in [zim::Namespace::UserContent, zim::Namespace::Articles] {
        let Some(entry) = zim.get_by_path(ns, path).map_err(|e| e.to_string())? else {
            continue;
        };
        let entry = zim.resolve(entry).map_err(|e| e.to_string())?;
        if !matches!(&entry.mime_type, zim::MimeType::Type(s) if s.contains("html")) {
            continue;
        }
        let Some(content) = zim.entry_content(&entry).map_err(|e| e.to_string())? else {
            continue;
        };
        let bytes = content.to_vec().map_err(|e| e.to_string())?;
        return Ok(Some((entry.title.clone(), String::from_utf8_lossy(&bytes).into_owned())));
    }
    Ok(None)
}

/// Load HTML for a curriculum lesson from a local ZIM (same discovery roots as Library Kiwix scan).
pub fn fetch_lesson_html(app: &AppHandle, lesson_id: &str) -> Result<AcademyLessonHtml, String> {
    let curriculum = get_curriculum()?;
    let lesson = find_lesson(&curriculum, lesson_id)
        .ok_or_else(|| format!("Unknown lesson id: {lesson_id}"))?;

    let roots = library_scan_roots(app)?;
    let zims = scan_kiwix_zim_files_multi(&roots).map_err(|e| format!("ZIM scan: {e}"))?;
    let candidates = rank_zims(&zims, &lesson.zim_name_hint);
    if candidates.is_empty() {
        return Err("No .zim files found under the library / Kiwix folder. Add a ZIM to use lessons.".to_string());
    }
    let path = lesson_article_path(&lesson.article_url);
    let mut last_err: Option<String> = None;
    for zim_path in &candidates {
        let zim = match crate::indexer::open_zim(zim_path) {
            Ok(z) => z,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        match read_lesson_from_zim(&zim, path) {
            Ok(Some((title, html))) => {
                let title = if title.trim().is_empty() { lesson.title.clone() } else { title };
                return Ok(AcademyLessonHtml { title, html });
            }
            Ok(None) => {}
            Err(e) => last_err = Some(e),
        }
    }
    Err(format!(
        "The article \"{}\" for this lesson is not in any installed ZIM ({} checked{}). Install an English Wikipedia ZIM from Library, Download Network.",
        path,
        candidates.len(),
        last_err.map(|e| format!("; last problem: {e}")).unwrap_or_default()
    ))
}
