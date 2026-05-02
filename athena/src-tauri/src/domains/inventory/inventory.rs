use chrono::Utc;
use tauri::State;
use tauri::{AppHandle, Emitter};

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

/// `key` is directorate key (or legacy `division` label).
fn require_inventory_write_key(
    actor: &crate::auth::AuthenticatedPersonnel,
    key: &str,
) -> Result<(), String> {
    let k = key.trim();
    if k.is_empty() {
        return Err("Inventory scope is empty.".to_string());
    }
    match actor.rank_level {
        0 | 1 | 2 => Err("Insufficient rank for inventory writes.".to_string()),
        3 => {
            if let Some(ref d) = actor.directorate {
                let t = d.trim();
                if !t.is_empty() && t.eq_ignore_ascii_case(k) {
                    return Ok(());
                }
            }
            if actor.division.trim().eq_ignore_ascii_case(k) {
                Ok(())
            } else {
                Err("You may only modify inventory for your own directorate.".to_string())
            }
        }
        _ => Ok(()),
    }
}

fn require_inventory_item(
    actor: &crate::auth::AuthenticatedPersonnel,
    row: &InventoryItemRow,
) -> Result<(), String> {
    let k = row
        .directorate
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(row.division.as_str());
    require_inventory_write_key(actor, k)
}

pub(crate) fn normalize_inventory_category(raw: &str) -> String {
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
    require_inventory_write_key(&actor, div)?;
    let cat = normalize_inventory_category(&category);
    let id = uuid::Uuid::new_v4().to_string();
    let ts = Utc::now().timestamp_millis();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.inventory_insert(&id, div, item, quantity, &cat, Some(div))?;
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
    app_handle: AppHandle,
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
    require_inventory_item(&actor, &row)?;
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
    if new_quantity < old_qty {
        let body = format!(
            "{} — {} (qty {} → {})",
            row.item_name, actor.name, old_qty, new_quantity
        );
        if let Ok(n) = db.command_notification_insert(
            "inventory",
            "med",
            "Inventory withdrawal / reduction",
            Some(&body),
            Some(iid),
        ) {
            let _ = app_handle.emit("athena:notification", &n);
        }
    }
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
    require_inventory_item(&actor, &row)?;
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

#[tauri::command]
pub fn apply_inventory_proposal(
    proposal_id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let actor = authenticated_user(&session)?;
    if actor.rank_level < 3 {
        return Err("Insufficient rank to apply inventory proposals.".to_string());
    }
    let pid = proposal_id.trim();
    if pid.is_empty() {
        return Err("proposalId is required.".to_string());
    }

    let ts = Utc::now().timestamp_millis();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let status = db.inventory_proposal_get_status(pid)?;
    if status.as_deref() != Some("pending") {
        return Err("Proposal is not pending.".to_string());
    }
    let lines = db.inventory_proposal_list_items(pid)?;
    if lines.is_empty() {
        db.inventory_proposal_set_status(pid, "rejected", None, None)?;
        return Err("Proposal contains no lines.".to_string());
    }

    for line in lines.iter() {
        let div = if line.division.trim().is_empty() {
            actor.division.trim().to_string()
        } else {
            line.division.trim().to_string()
        };
        if div.is_empty() {
            continue;
        }
        require_inventory_write_key(&actor, &div)?;

        let item_name = line.item_name.trim();
        if item_name.is_empty() {
            continue;
        }
        let q = line.quantity;
        let cat = normalize_inventory_category(&line.category);
        let action = line.action.trim().to_ascii_lowercase();

        if action == "add_item" {
            if q < 0 {
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            db.inventory_insert(&id, &div, item_name, q, &cat, Some(&div))?;
            db.inventory_log_insert(
                &uuid::Uuid::new_v4().to_string(),
                &id,
                &actor.id,
                "ai_add_item",
                None,
                Some(q),
                ts,
            )?;
            continue;
        }

        if action == "delta_quantity" {
            if let Some(existing) = db.inventory_find_by_directorate_item(&div, item_name)? {
                if q == 0 {
                    continue;
                }
                let new_qty = (existing.quantity + q).max(0);
                let iid = existing.id.clone();
                let old_qty = db
                    .inventory_set_quantity(&iid, new_qty)?
                    .ok_or_else(|| "Inventory item not found.".to_string())?;
                if cat != existing.category {
                    let _ = db.inventory_set_category(&iid, &cat);
                }
                db.inventory_log_insert(
                    &uuid::Uuid::new_v4().to_string(),
                    &iid,
                    &actor.id,
                    "ai_delta",
                    Some(old_qty),
                    Some(new_qty),
                    ts,
                )?;
            } else if q > 0 {
                let id = uuid::Uuid::new_v4().to_string();
                db.inventory_insert(&id, &div, item_name, q, &cat, Some(&div))?;
                db.inventory_log_insert(
                    &uuid::Uuid::new_v4().to_string(),
                    &id,
                    &actor.id,
                    "ai_add_item",
                    None,
                    Some(q),
                    ts,
                )?;
            }
            continue;
        }

        // set_quantity: absolute count (legacy / explicit)
        if q < 0 {
            continue;
        }
        if let Some(existing) = db.inventory_find_by_directorate_item(&div, item_name)? {
            let iid = existing.id.clone();
            let old_qty = db
                .inventory_set_quantity(&iid, q)?
                .ok_or_else(|| "Inventory item not found.".to_string())?;
            if cat != existing.category {
                let _ = db.inventory_set_category(&iid, &cat);
            }
            db.inventory_log_insert(
                &uuid::Uuid::new_v4().to_string(),
                &iid,
                &actor.id,
                "ai_set_quantity",
                Some(old_qty),
                Some(q),
                ts,
            )?;
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            db.inventory_insert(&id, &div, item_name, q, &cat, Some(&div))?;
            db.inventory_log_insert(
                &uuid::Uuid::new_v4().to_string(),
                &id,
                &actor.id,
                "ai_add_item",
                None,
                Some(q),
                ts,
            )?;
        }
    }

    db.inventory_proposal_set_status(pid, "applied", Some(actor.id.as_str()), Some(ts))?;
    Ok(())
}

#[tauri::command]
pub fn update_inventory_directorate(
    item_id: String,
    new_directorate: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<InventoryItemRow, String> {
    let actor = authenticated_user(&session)?;
    if actor.rank_level < 4 {
        return Err("Insufficient rank to re-bucket inventory directorate.".to_string());
    }
    let iid = item_id.trim();
    if iid.is_empty() {
        return Err("item_id is required.".to_string());
    }
    let d = new_directorate.trim();
    if d.is_empty() {
        return Err("directorate is required.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let row = db
        .inventory_get(iid)?
        .ok_or_else(|| "Inventory item not found.".to_string())?;
    require_inventory_item(&actor, &row)?;
    db.inventory_set_directorate(iid, d)?;
    db.inventory_get(iid)?
        .ok_or_else(|| "Inventory item not found.".to_string())
}

#[tauri::command]
pub fn rollback_inventory_change(
    log_id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<InventoryItemRow, String> {
    let actor = authenticated_user(&session)?;
    if actor.rank_level < 4 {
        return Err("Insufficient rank to rollback inventory changes.".to_string());
    }
    let lid = log_id.trim();
    if lid.is_empty() {
        return Err("logId is required.".to_string());
    }

    let ts = Utc::now().timestamp_millis();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let log = db
        .inventory_log_get(lid)?
        .ok_or_else(|| "Log entry not found.".to_string())?;
    let target = log
        .old_qty
        .ok_or_else(|| "Log entry has no previous quantity to rollback to.".to_string())?;

    let item = db
        .inventory_get(&log.item_id)?
        .ok_or_else(|| "Inventory item not found.".to_string())?;
    require_inventory_item(&actor, &item)?;

    let current = db
        .inventory_set_quantity(&item.id, target)?
        .ok_or_else(|| "Inventory item not found.".to_string())?;
    db.inventory_log_insert(
        &uuid::Uuid::new_v4().to_string(),
        &item.id,
        &actor.id,
        "rollback",
        Some(current),
        Some(target),
        ts,
    )?;
    db.inventory_get(&item.id)?
        .ok_or_else(|| "Inventory item not found.".to_string())
}
