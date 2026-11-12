//! Plain record access for features whose logic lives in the UI (field tools).
//!
//! Only the kinds listed here can be read or written this way; everything else goes through its
//! own validated commands.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store;
use crate::auth::AuthSession;
use crate::AppState;

/// Kinds the UI may store directly, with the largest payload each may hold.
const ALLOWED: &[(&str, usize)] = &[
    ("pressure_reading", 400),
    ("time_check", 400),
    ("nav_route", 8_000),
    ("fed_post", 1_500),
    ("fed_person", 2_500),
];

fn limit_for(kind: &str) -> Result<usize, String> {
    ALLOWED
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, max)| *max)
        .ok_or_else(|| format!("Records of kind \"{kind}\" cannot be stored this way."))
}

/// Payload stored as-is. A wrapper keeps arbitrary JSON from being flattened into the record's
/// own fields (id, createdAt, updatedAt).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub value: serde_json::Value,
}

#[tauri::command]
pub fn list_field_records(
    kind: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<store::Record<Payload>>, String> {
    super::require_member(&session)?;
    limit_for(&kind)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::list::<Payload>(&db, &kind)
}

#[tauri::command]
pub fn save_field_record(
    kind: String,
    id: Option<String>,
    value: serde_json::Value,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<store::Record<Payload>, String> {
    super::require_member(&session)?;
    let max = limit_for(&kind)?;
    if !value.is_object() {
        return Err("A record must be an object.".to_string());
    }
    let size = serde_json::to_string(&value).map_err(|e| e.to_string())?.len();
    if size > max {
        return Err(format!("That record is too large ({size} bytes; the limit is {max})."));
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, &kind, id.as_deref(), &Payload { value })
}

#[tauri::command]
pub fn delete_field_record(
    kind: String,
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    limit_for(&kind)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, &kind, &id)
}

/// Largest binder file that will be written.
const MAX_BINDER_BYTES: usize = 20 * 1024 * 1024;

/// True when `path` is somewhere a binder may be written: an `.html` file, nothing else.
pub fn is_binder_path(path: &str) -> bool {
    let lower = path.trim().to_lowercase();
    (lower.ends_with(".html") || lower.ends_with(".htm")) && lower.len() > 5
}

/// Save the printable binder to a file the operator chose.
#[tauri::command]
pub fn save_binder_file(
    path: String,
    html: String,
    session: State<'_, AuthSession>,
) -> Result<usize, String> {
    super::require_member(&session)?;
    if !is_binder_path(&path) {
        return Err("Save the binder as an .html file.".to_string());
    }
    if html.len() > MAX_BINDER_BYTES {
        return Err("The binder is too large to save. Leave out a section and try again.".to_string());
    }
    std::fs::write(path.trim(), html.as_bytes()).map_err(|e| format!("Could not save the binder: {e}"))?;
    Ok(html.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_kinds_are_allowed() {
        assert!(limit_for("pressure_reading").is_ok());
        assert!(limit_for("time_check").is_ok());
        let err = limit_for("patient").unwrap_err();
        assert!(err.contains("patient"), "{err}");
        assert!(limit_for("").is_err());
    }

    #[test]
    fn binders_can_only_be_written_as_html() {
        assert!(is_binder_path("/Volumes/USB/binder.html"));
        assert!(is_binder_path("C:\\Users\\a\\Binder.HTM"));
        assert!(!is_binder_path("/Users/a/.zshrc"));
        assert!(!is_binder_path("/Users/a/binder.html.sh"));
        assert!(!is_binder_path(".html"));
    }
}
