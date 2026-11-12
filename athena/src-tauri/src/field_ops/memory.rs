//! Group memory (S14): things the group has learned, written down once and answerable forever.
//!
//! A note is stored as a record and also written into the library as a small document, so the
//! same search and chat that read manuals also read "the north well is contaminated".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::AppState;

pub const KIND: &str = "memory_note";
const MAX_BODY_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNote {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// Title shown in sources and search results.
pub fn library_title(note: &MemoryNote) -> String {
    format!("Group memory: {}", note.title.trim())
}

/// The text that gets indexed. Who and when are part of the text so the assistant can say
/// "Ana recorded on 12 March that…".
pub fn note_document(note: &MemoryNote, recorded_at: i64) -> String {
    let date = chrono::DateTime::from_timestamp(recorded_at, 0)
        .map(|d| d.format("%-d %B %Y").to_string())
        .unwrap_or_else(|| "an unknown date".to_string());
    let mut out = format!("{}\n\n", note.title.trim());
    match note.author.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(author) => out.push_str(&format!("Recorded by {author} on {date}.\n")),
        None => out.push_str(&format!("Recorded on {date}.\n")),
    }
    let tags: Vec<&str> = note.tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
    if !tags.is_empty() {
        out.push_str(&format!("Topics: {}.\n", tags.join(", ")));
    }
    out.push('\n');
    out.push_str(note.body.trim());
    out.push('\n');
    out
}

/// Folder inside the user library that holds group memory notes.
pub const MEMORY_DIR_NAME: &str = "group-memory";

fn note_path(app_data_dir: &Path, id: &str) -> PathBuf {
    crate::athena_paths::AthenaPaths::new(app_data_dir)
        .user_library_dir()
        .join(MEMORY_DIR_NAME)
        .join(format!("{id}.md"))
}

/// Write the note into the library and index it for search.
fn publish(db: &crate::db::Database, app_data_dir: &Path, rec: &Record<MemoryNote>) -> Result<(), String> {
    let path = note_path(app_data_dir, &rec.id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = note_document(&rec.data, rec.created_at);
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().to_string();
    let title = library_title(&rec.data);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("note.md").to_string();
    db.upsert_file(
        &path_str,
        "text",
        &name,
        Some(text.len() as i64),
        Some(rec.updated_at),
        Some(&text),
        None,
        Some(&title),
        None,
        None,
    )?;
    crate::vector_index::index_document(db, &path_str, &title, None, &text)
}

/// Publish every note into the library. Used after notes arrive from another station.
pub fn republish_all(db: &crate::db::Database, app_data_dir: &Path) {
    for rec in store::list::<MemoryNote>(db, KIND).unwrap_or_default() {
        if let Err(e) = publish(db, app_data_dir, &rec) {
            eprintln!("[athena] could not publish group memory note {}: {e}", rec.id);
        }
    }
}

#[tauri::command]
pub fn list_memory_notes(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Record<MemoryNote>>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut notes = store::list::<MemoryNote>(&db, KIND)?;
    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(notes)
}

#[tauri::command]
pub async fn save_memory_note(
    id: Option<String>,
    mut note: MemoryNote,
    app: tauri::AppHandle,
    session: crate::auth::CallerSession,
) -> Result<Record<MemoryNote>, String> {
    let user = super::require_member(&session)?;
    note.title = note.title.trim().to_string();
    note.body = note.body.trim().to_string();
    note.tags = note.tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    if note.title.is_empty() || note.body.is_empty() {
        return Err("A note needs a title and something written in it.".to_string());
    }
    if note.body.chars().count() > MAX_BODY_CHARS {
        return Err("That note is too long. Split it into smaller notes, or add it to the library as a document.".to_string());
    }
    if note.author.as_deref().map(str::trim).unwrap_or("").is_empty() {
        note.author = Some(user.display_name.clone().unwrap_or(user.name.clone()));
    }
    tokio::task::spawn_blocking(move || -> Result<Record<MemoryNote>, String> {
        let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
        let state = app.state::<AppState>();
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let rec = store::put(&db, KIND, id.as_deref(), &note)?;
        publish(&db, &app_data_dir, &rec)?;
        Ok(rec)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn delete_memory_note(
    id: String,
    app_handle: tauri::AppHandle,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let app_data_dir = crate::portable::resolved_app_data_dir(&app_handle)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let removed = store::remove(&db, KIND, &id)?;
    let path = note_path(&app_data_dir, &id);
    let _ = db.remove_file_from_index(&path.to_string_lossy());
    let _ = std::fs::remove_file(&path);
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> MemoryNote {
        MemoryNote {
            title: "  North well is contaminated ".into(),
            body: "Three people sick after drinking from it. Boil or use the east spring.\n".into(),
            tags: vec!["water".into(), " ".into(), "hazard".into()],
            author: Some("Ana".into()),
        }
    }

    #[test]
    fn document_states_who_when_and_what() {
        // 2027-01-15 08:00:00 UTC
        let doc = note_document(&note(), 1_800_000_000);
        assert_eq!(
            doc,
            "North well is contaminated\n\nRecorded by Ana on 15 January 2027.\nTopics: water, hazard.\n\nThree people sick after drinking from it. Boil or use the east spring.\n"
        );
    }

    #[test]
    fn document_without_author_or_tags_stays_clean() {
        let mut n = note();
        n.author = None;
        n.tags.clear();
        let doc = note_document(&n, 1_800_000_000);
        assert!(doc.contains("Recorded on 15 January 2027.\n\nThree people"));
        assert!(!doc.contains("Topics"));
    }

    #[test]
    fn notes_are_searchable_once_published() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = crate::db::Database::new(&dir.path().join("t.db")).expect("db");
        let rec = store::put_at(&db, KIND, None, &note(), 1_800_000_000).unwrap();
        publish(&db, dir.path(), &rec).unwrap();

        let path = note_path(dir.path(), &rec.id);
        assert!(path.is_file());
        let found = db.search_text_content("contaminated well", 5).unwrap();
        assert!(
            found.iter().any(|f| f.path == path.to_string_lossy()),
            "note should be found by keyword search"
        );
        assert_eq!(library_title(&rec.data), "Group memory: North well is contaminated");
    }
}
