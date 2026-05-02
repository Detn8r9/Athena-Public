//! Route blockages: propose (rank 2+), approve/reject (rank 4+).

use chrono::Utc;
use tauri::State;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::auth::{authenticated_user, require_rank2_or_higher, require_rank4_or_higher, AuthSession};
use crate::db::RouteBlockageRow;
use crate::lan_sync;
use crate::AppState;

#[tauri::command]
pub fn list_route_blockages(
    include_rejected: bool,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<RouteBlockageRow>, String> {
    let _u = authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.list_route_blockages(include_rejected)
}

#[tauri::command]
pub fn propose_route_blockage(
    geometry_json: String,
    reason: String,
    severity: String,
    modes_affected: String,
    expires_at: Option<String>,
    notes: String,
    app: State<'_, AppState>,
    app_handle: AppHandle,
    session: State<'_, AuthSession>,
) -> Result<RouteBlockageRow, String> {
    let u = require_rank2_or_higher(&session)?;
    let g = geometry_json.trim();
    if g.is_empty() {
        return Err("geometry_json is required.".to_string());
    }
    serde_json::from_str::<serde_json::Value>(g)
        .map_err(|e| format!("geometry_json must be valid JSON: {e}"))?;
    let sev = match severity.trim().to_lowercase().as_str() {
        "impassable" | "slow" => severity.trim().to_lowercase(),
        _ => {
            return Err("severity must be 'impassable' or 'slow'.".to_string());
        }
    };
    let modes = if modes_affected.trim().is_empty() {
        "walk,drive,bike,offroad".to_string()
    } else {
        modes_affected.trim().to_string()
    };
    let now = Utc::now().to_rfc3339();
    let row = RouteBlockageRow {
        id: Uuid::new_v4().to_string(),
        geometry_json: g.to_string(),
        reason: reason.trim().to_string(),
        severity: sev,
        modes_affected: modes,
        starts_at: now.clone(),
        expires_at: expires_at.filter(|s| !s.trim().is_empty()),
        status: "pending".to_string(),
        proposed_by_personnel_id: u.id,
        approved_by_personnel_id: None,
        approved_at: None,
        notes: notes.trim().to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.insert_route_blockage(&row)?;
        if let Ok(n) = db.command_notification_insert(
            "map",
            "med",
            "Route blockage proposed (pending review)",
            Some(&format!("{} — {}", row.reason, row.id)),
            Some(&row.id),
        ) {
            let _ = app_handle.emit("athena:notification", &n);
        }
    }
    if let Ok(v) = serde_json::to_value(&row) {
        lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_ROUTE_BLOCKAGES, v);
    }
    Ok(row)
}

#[tauri::command]
pub fn review_route_blockage(
    id: String,
    approved: bool,
    review_notes: Option<String>,
    app: State<'_, AppState>,
    app_handle: AppHandle,
    session: State<'_, AuthSession>,
) -> Result<RouteBlockageRow, String> {
    let reviewer = require_rank4_or_higher(&session)?;
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("id is required.".to_string());
    }
    let mut row = {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.get_route_blockage(&id)?
    }
    .ok_or_else(|| "Blockage not found.".to_string())?;
    if row.status != "pending" {
        return Err("Only pending blockages can be reviewed.".to_string());
    }
    let now = Utc::now().to_rfc3339();
    if approved {
        row.status = "active".to_string();
        row.approved_by_personnel_id = Some(reviewer.id.clone());
        row.approved_at = Some(now.clone());
    } else {
        row.status = "rejected".to_string();
        row.approved_by_personnel_id = None;
        row.approved_at = None;
    }
    row.updated_at = now;
    if let Some(n) = review_notes {
        if !n.trim().is_empty() {
            let extra = n.trim();
            if row.notes.is_empty() {
                row.notes = extra.to_string();
            } else {
                row.notes = format!("{}\n[Review]: {}", row.notes, extra);
            }
        }
    }
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        db.update_route_blockage(&row)?;
        let status_label = if approved { "approved" } else { "rejected" };
        if let Ok(n) = db.command_notification_insert(
            "map",
            if approved { "low" } else { "med" },
            "Route blockage reviewed",
            Some(&format!("{status_label}: {} — {}", row.reason, row.id)),
            Some(&row.id),
        ) {
            let _ = app_handle.emit("athena:notification", &n);
        }
    }
    if let Ok(v) = serde_json::to_value(&row) {
        lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_ROUTE_BLOCKAGES, v);
    }
    Ok(row)
}
