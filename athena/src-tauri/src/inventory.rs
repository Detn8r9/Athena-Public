use chrono::Utc;
use tauri::State;

use crate::auth::{authenticated_user, AuthSession};
use crate::db::{InventoryItemRow, InventoryLogRow};
use crate::AppState;

fn require_inventory_list(session: &AuthSession) -> Result<crate::auth::AuthenticatedPersonnel, String> {
    let u = authenticated_user(session)?;
    if u.rank_level <= 1 {
        return Err("Insufficient rank to view inventory.".to_string());
    }
    Ok(u)
}

fn require_inventory_write(actor: &crate::auth::AuthenticatedPersonnel, item_division: &str) -> Result<(), String> {
    match actor.rank_level {
        0 | 1 => Err("Insufficient rank for inventory writes.".to_string()),
        2 => Err("Insufficient rank for inventory writes.".to_string()),
        3 => {
            if actor
                .division
                .trim()
                .eq_ignore_ascii_case(item_division.trim())
            {
                Ok(())
            } else {
                Err("You may only modify inventory for your own division.".to_string())
            }
        }
        _ => Ok(()),
    }
}

fn normalize_inventory_category(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "food" => "food".to_string(),
        "water" => "water".to_string(),
        "weapons" => "weapons".to_string(),
        "medical" => "medical".to_string(),
        "equipment" => "equipment".to_string(),
        "comms" => "comms".to_string(),
        "fuel" => "fuel".to_string(),
        "materials" => "materials".to_string(),
        _ => "general".to_string(),
    }
}

#[tauri::command]
pub fn list_inventory(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<InventoryItemRow>, String> {
    require_inventory_list(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.inventory_list()
}

#[tauri::command]
pub fn add_inventory_item(
    division: String,
    item_name: String,
    quantity: i64,
    category: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<InventoryItemRow, String> {
    let actor = authenticated_user(&session)?;
    let div = division.trim();
    let item = item_name.trim();
    if div.is_empty() || item.is_empty() {
        return Err("division and item_name are required.".to_string());
    }
    if quantity < 0 {
        return Err("quantity cannot be negative.".to_string());
    }
    require_inventory_write(&actor, div)?;
    let cat = normalize_inventory_category(&category);
    let id = uuid::Uuid::new_v4().to_string();
    let ts = Utc::now().timestamp_millis();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.inventory_insert(&id, div, item, quantity, &cat)?;
    db.inventory_log_insert(
        &uuid::Uuid::new_v4().to_string(),
        &id,
        &actor.id,
        "add_item",
        None,
        Some(quantity),
        ts,
    )?;
    db.inventory_get(&id)?
        .ok_or_else(|| "Failed to load new inventory row.".to_string())
}

#[tauri::command]
pub fn update_inventory_quantity(
    item_id: String,
    new_quantity: i64,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<InventoryItemRow, String> {
    let actor = authenticated_user(&session)?;
    if new_quantity < 0 {
        return Err("new_quantity cannot be negative.".to_string());
    }
    let iid = item_id.trim();
    if iid.is_empty() {
        return Err("item_id is required.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let row = db
        .inventory_get(iid)?
        .ok_or_else(|| "Inventory item not found.".to_string())?;
    require_inventory_write(&actor, &row.division)?;
    let old_qty = db
        .inventory_set_quantity(iid, new_quantity)?
        .ok_or_else(|| "Inventory item not found.".to_string())?;
    let ts = Utc::now().timestamp_millis();
    db.inventory_log_insert(
        &uuid::Uuid::new_v4().to_string(),
        iid,
        &actor.id,
        "update_quantity",
        Some(old_qty),
        Some(new_quantity),
        ts,
    )?;
    db.inventory_get(iid)?
        .ok_or_else(|| "Inventory item not found.".to_string())
}

#[tauri::command]
pub fn update_inventory_category(
    item_id: String,
    category: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<InventoryItemRow, String> {
    let actor = authenticated_user(&session)?;
    let iid = item_id.trim();
    if iid.is_empty() {
        return Err("item_id is required.".to_string());
    }
    let cat = normalize_inventory_category(&category);
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let row = db
        .inventory_get(iid)?
        .ok_or_else(|| "Inventory item not found.".to_string())?;
    require_inventory_write(&actor, &row.division)?;
    db.inventory_set_category(iid, &cat)?;
    db.inventory_get(iid)?
        .ok_or_else(|| "Inventory item not found.".to_string())
}

#[tauri::command]
pub fn list_inventory_log(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<InventoryLogRow>, String> {
    let u = authenticated_user(&session)?;
    if u.rank_level < 4 {
        return Err("Insufficient rank to view inventory audit log.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.inventory_log_list()
}

#[tauri::command]
pub fn edit_log_fingerprint(
    log_id: String,
    correction_note: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let u = authenticated_user(&session)?;
    if u.rank_level < 5 {
        return Err("Only Level 5 (Caesar) personnel may annotate inventory log fingerprints.".to_string());
    }
    let lid = log_id.trim();
    if lid.is_empty() {
        return Err("log_id is required.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.inventory_log_append_fingerprint(lid, &correction_note)?;
    Ok(())
}
