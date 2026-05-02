//! Tactical territory polygons (area control) for map + Valhalla avoidance.

use chrono::Utc;
use tauri::State;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::auth::{authenticated_user, require_clearance4_or_higher, AuthSession};
use crate::db::TerritoryRow;
use crate::AppState;

const TERRITORIES_UPDATED: &str = "athena:territories-updated";

#[tauri::command]
pub fn get_territories(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<TerritoryRow>, String> {
    let _u = authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.list_territories()
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertTerritoryArgs {
    pub id: Option<String>,
    pub name: String,
    pub status: String,
    pub color: String,
    pub geometry_json: String,
}

#[tauri::command]
pub fn upsert_territory(
    args: UpsertTerritoryArgs,
    app: State<'_, AppState>,
    app_handle: AppHandle,
    session: State<'_, AuthSession>,
) -> Result<TerritoryRow, String> {
    let u = require_clearance4_or_higher(&session)?;
    let name = args.name.trim().to_string();
    if name.is_empty() {
        return Err("Name is required.".to_string());
    }
    let st = match args.status.trim().to_lowercase().as_str() {
        "friendly" | "enemy" | "disputed" | "unexplored" => args.status.trim().to_lowercase(),
        _ => {
            return Err("status must be friendly, enemy, disputed, or unexplored.".to_string());
        }
    };
    let color = if args.color.trim().is_empty() {
        return Err("color is required.".to_string());
    } else {
        args.color.trim().to_string()
    };
    let g = args.geometry_json.trim();
    if g.is_empty() {
        return Err("geometry_json is required.".to_string());
    }
    let now = Utc::now().to_rfc3339();

    let db = app.db.lock().map_err(|e| e.to_string())?;
    if let Some(ref id) = args.id {
        let id = id.trim();
        if id.is_empty() {
            return Err("id is empty.".to_string());
        }
        let existing = db
            .get_territory(id)?
            .ok_or_else(|| "Territory not found.".to_string())?;
        let row = TerritoryRow {
            id: existing.id,
            name,
            status: st,
            color,
            geometry_json: g.to_string(),
            created_at: existing.created_at,
            updated_at: now,
            created_by_personnel_id: existing.created_by_personnel_id,
            deleted_at: None,
        };
        db.update_territory_row(&row)?;
        let _ = app_handle.emit(TERRITORIES_UPDATED, ());
        return Ok(row);
    }

    let row = TerritoryRow {
        id: Uuid::new_v4().to_string(),
        name,
        status: st,
        color,
        geometry_json: g.to_string(),
        created_at: now.clone(),
        updated_at: now,
        created_by_personnel_id: u.id,
        deleted_at: None,
    };
    db.insert_territory(&row)?;
    let _ = app_handle.emit(TERRITORIES_UPDATED, ());
    Ok(row)
}

#[tauri::command]
pub fn delete_territory(
    id: String,
    app: State<'_, AppState>,
    app_handle: AppHandle,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let _u = require_clearance4_or_higher(&session)?;
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("id is required.".to_string());
    }
    let now = Utc::now().to_rfc3339();
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.soft_delete_territory(&id, &now)?;
    }
    let _ = app_handle.emit(TERRITORIES_UPDATED, ());
    Ok(())
}
