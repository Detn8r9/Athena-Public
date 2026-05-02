//! CRUD for `saved_places` and LAN publish for shared scope.

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::auth::{authenticated_user, AuthSession};
use crate::db::SavedPlaceRow;
use crate::lan_sync;
use crate::AppState;

#[tauri::command]
pub fn list_saved_places(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<SavedPlaceRow>, String> {
    let u = authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.list_saved_places_for_user(&u.id)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSavedPlaceArgs {
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_color")]
    pub color: String,
    /// `"personal"` or `"shared"`.
    pub scope: String,
}

fn default_icon() -> String {
    "pin".to_string()
}

fn default_color() -> String {
    "#3388ff".to_string()
}

#[tauri::command]
pub fn add_saved_place(
    args: AddSavedPlaceArgs,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<SavedPlaceRow, String> {
    let u = authenticated_user(&session)?;
    if !(args.lat.is_finite() && args.lng.is_finite()) {
        return Err("Invalid coordinates.".to_string());
    }
    let name = args.name.trim().to_string();
    if name.is_empty() {
        return Err("Name is required.".to_string());
    }
    let scope = match args.scope.trim().to_lowercase().as_str() {
        "personal" => "personal",
        "shared" => "shared",
        _ => {
            return Err("scope must be 'personal' or 'shared'.".to_string());
        }
    }
    .to_string();
    let now = Utc::now().to_rfc3339();
    let row = SavedPlaceRow {
        id: Uuid::new_v4().to_string(),
        name,
        lat: args.lat,
        lng: args.lng,
        icon: if args.icon.trim().is_empty() {
            "pin".to_string()
        } else {
            args.icon.trim().to_string()
        },
        color: if args.color.trim().is_empty() {
            "#3388ff".to_string()
        } else {
            args.color.trim().to_string()
        },
        scope: scope.clone(),
        owner_personnel_id: u.id.clone(),
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    };
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.insert_saved_place(&row)?;
    }
    if scope == "shared" {
        if let Ok(v) = serde_json::to_value(&row) {
            lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_SAVED_PLACES, v);
        }
    }
    Ok(row)
}

#[tauri::command]
pub fn delete_saved_place(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let u = authenticated_user(&session)?;
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("id is required.".to_string());
    }
    let place = {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.get_saved_place(&id)?
    }
    .ok_or_else(|| "Saved place not found.".to_string())?;
    if place.owner_personnel_id != u.id {
        return Err("You can only delete your own saved places.".to_string());
    }
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.delete_saved_place(&id)?;
    }
    if place.scope == "shared" {
        if let Ok(v) = serde_json::to_value(&place) {
            // Peers may ignore deletes without tombstones; full sync re-pull handles consistency.
            lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_SAVED_PLACES, v);
        }
    }
    Ok(())
}
