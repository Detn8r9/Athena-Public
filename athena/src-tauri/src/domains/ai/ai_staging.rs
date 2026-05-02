//! AI / user staged changes with rank-4 review and rank-5 auto-approve rules.

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::auth::{
    authenticated_user, require_rank4_or_higher, require_rank5, AuthSession,
};
use crate::db::{
    AiAutoapproveRuleRow, AiStagedChangeRow, Database, MapMarker, RouteBlockageRow, SavedPlaceRow,
    TacticalPointOfInterest,
};
use crate::lan_sync;
use crate::AppState;

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiListStagedFilters {
    #[serde(default)]
    pub status: Option<String>,
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn now_ts() -> i64 {
    Utc::now().timestamp_millis()
}

/// Best-effort predicate: `{"field":"a.b","op":"<=","value":5}` on JSON payload.
fn eval_auto_predicate(pred: &serde_json::Value, payload: &serde_json::Value) -> bool {
    let field = pred.get("field").and_then(|v| v.as_str()).unwrap_or("");
    if field.is_empty() {
        return false;
    }
    let op = pred.get("op").and_then(|v| v.as_str()).unwrap_or("==");
    let mut cur = payload.clone();
    for part in field.split('.') {
        if part.is_empty() {
            continue;
        }
        cur = match cur {
            serde_json::Value::Object(m) => m
                .get(part)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Null,
        };
    }
    let rhs = pred.get("value").cloned().unwrap_or(serde_json::Value::Null);
    match op {
        "==" => cur == rhs,
        "!=" => cur != rhs,
        "<=" => {
            if let (Some(a), Some(b)) = (cur.as_f64(), rhs.as_f64()) {
                a <= b
            } else {
                false
            }
        }
        ">=" => {
            if let (Some(a), Some(b)) = (cur.as_f64(), rhs.as_f64()) {
                a >= b
            } else {
                false
            }
        }
        _ => false,
    }
}

fn try_auto_approve(
    db: &Database,
    row: &AiStagedChangeRow,
) -> Result<Option<String>, String> {
    let rules = db.list_ai_autoapprove_rules()?;
    let payload: serde_json::Value =
        serde_json::from_str(&row.payload_json).map_err(|e| e.to_string())?;
    for r in rules {
        if !r.enabled {
            continue;
        }
        if r.entity_type != row.entity_type || r.action != row.action {
            continue;
        }
        let pred: serde_json::Value =
            serde_json::from_str(&r.predicate_json).map_err(|e| e.to_string())?;
        if eval_auto_predicate(&pred, &payload) {
            return Ok(Some(r.id));
        }
    }
    Ok(None)
}

fn normalize_marker_category(raw: &str) -> String {
    let t = raw.trim().to_lowercase();
    if t.is_empty() {
        "hazard".to_string()
    } else {
        t
    }
}

/// Apply an approved payload to the database. `reviewer` is the approver.
pub fn execute_staged_change(
    db: &Database,
    change: &AiStagedChangeRow,
    reviewer: &crate::auth::AuthenticatedPersonnel,
) -> Result<(), String> {
    let payload: serde_json::Value =
        serde_json::from_str(&change.payload_json).map_err(|e| e.to_string())?;
    match (change.entity_type.as_str(), change.action.as_str()) {
        ("map_marker", "create") => {
            let lat = payload
                .get("lat")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "payload.lat required".to_string())?;
            let lng = payload
                .get("lng")
                .or_else(|| payload.get("lon"))
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "payload.lng required".to_string())?;
            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Marker")
                .to_string();
            let description = payload
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let category = normalize_marker_category(
                payload
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("hazard"),
            );
            let marker = MapMarker {
                id: Uuid::new_v4().to_string(),
                lat,
                lng,
                title: title.trim().to_string(),
                description,
                created_at: Utc::now().to_rfc3339(),
                category,
            };
            db.insert_map_marker(&marker)?;
            if let Ok(v) = serde_json::to_value(&marker) {
                lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_MAP_MARKERS, v);
            }
        }
        ("map_marker", "delete") => {
            let id = change
                .target_entity_id
                .as_deref()
                .or_else(|| payload.get("id").and_then(|v| v.as_str()))
                .ok_or_else(|| "target id required".to_string())?;
            db.delete_map_marker(id.trim())?;
        }
        ("tactical_poi", "create" | "update") => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "name required".to_string())?
                .to_string();
            let lat = payload
                .get("lat")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "lat required".to_string())?;
            let lon = payload
                .get("lon")
                .or_else(|| payload.get("lng"))
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "lon required".to_string())?;
            let notes = payload
                .get("notes")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let p = TacticalPointOfInterest {
                id,
                name,
                lat,
                lon,
                notes,
                updated_at: now_ts(),
            };
            db.upsert_tactical_point_of_interest(&p)?;
        }
        ("tactical_poi", "delete") => {
            let id = change
                .target_entity_id
                .as_deref()
                .or_else(|| payload.get("id").and_then(|v| v.as_str()))
                .ok_or_else(|| "id required".to_string())?;
            db.delete_tactical_point_of_interest(id)?;
        }
        ("blockage", "create" | "update") => {
            let row: RouteBlockageRow = serde_json::from_value(payload)
                .map_err(|e| format!("blockage payload: {e}"))?;
            if db.get_route_blockage(&row.id)?.is_some() {
                db.update_route_blockage(&row)?;
            } else {
                db.insert_route_blockage(&row)?;
            }
            if let Ok(v) = serde_json::to_value(&row) {
                lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_ROUTE_BLOCKAGES, v);
            }
        }
        ("saved_place", "create" | "update") => {
            let row: SavedPlaceRow = serde_json::from_value(payload)
                .map_err(|e| format!("saved_place payload: {e}"))?;
            if db.get_saved_place(&row.id)?.is_some() {
                db.update_saved_place(&row)?;
            } else {
                db.insert_saved_place(&row)?;
            }
            if row.scope == "shared" {
                if let Ok(v) = serde_json::to_value(&row) {
                    lan_sync::publish_local_sync_row(lan_sync::LAN_TBL_SAVED_PLACES, v);
                }
            }
        }
        ("saved_place", "delete") => {
            let id = change
                .target_entity_id
                .as_deref()
                .or_else(|| payload.get("id").and_then(|v| v.as_str()))
                .ok_or_else(|| "id required".to_string())?;
            db.delete_saved_place(id.trim())?;
        }
        ("logbook", "create") => {
            let body = payload
                .get("body")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "body required".to_string())?
                .to_string();
            if body.trim().is_empty() {
                return Err("body is empty".to_string());
            }
            let notebook = payload
                .get("notebook")
                .and_then(|v| v.as_str())
                .unwrap_or("general");
            let id = Uuid::new_v4().to_string();
            let name = reviewer
                .display_name
                .clone()
                .unwrap_or_else(|| reviewer.name.clone());
            let created_at = Utc::now().timestamp_millis();
            db.insert_logbook_local(
                &id,
                &body,
                created_at,
                &name,
                "",
                None,
                notebook,
            )?;
        }
        ("inventory", "create") => {
            let div = payload
                .get("division")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "division required".to_string())?;
            let item = payload
                .get("itemName")
                .or_else(|| payload.get("item_name"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "itemName required".to_string())?;
            let qty = payload
                .get("quantity")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let cat = payload
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("general");
            let id = Uuid::new_v4().to_string();
            let ts = Utc::now().timestamp_millis();
            db.inventory_insert(
                &id,
                div.trim(),
                item.trim(),
                qty,
                cat.trim().to_lowercase().as_str(),
                None,
            )?;
            db.inventory_log_insert(
                &Uuid::new_v4().to_string(),
                &id,
                &reviewer.id,
                "add_item",
                None,
                Some(qty),
                ts,
            )?;
        }
        ("inventory", "update") => {
            let item_id = payload
                .get("itemId")
                .or_else(|| payload.get("item_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "itemId required".to_string())?;
            let new_qty = payload
                .get("newQuantity")
                .or_else(|| payload.get("new_quantity"))
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "newQuantity required".to_string())?;
            let old = db
                .inventory_set_quantity(item_id.trim(), new_qty)?
                .ok_or_else(|| "Item not found.".to_string())?;
            let ts = Utc::now().timestamp_millis();
            db.inventory_log_insert(
                &Uuid::new_v4().to_string(),
                item_id.trim(),
                &reviewer.id,
                "update_quantity",
                Some(old),
                Some(new_qty),
                ts,
            )?;
        }
        _ => {
            return Err(format!(
                "Unsupported staged entity {}/{}",
                change.entity_type, change.action
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn ai_stage_change(
    entity_type: String,
    action: String,
    target_entity_id: Option<String>,
    payload_json: String,
    diff_summary: String,
    source: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AiStagedChangeRow, String> {
    let u = authenticated_user(&session)?;
    if payload_json.trim().is_empty() {
        return Err("payload_json is required.".to_string());
    }
    serde_json::from_str::<serde_json::Value>(&payload_json)
        .map_err(|e| format!("Invalid JSON: {e}"))?;
    let src = match source.to_lowercase().as_str() {
        "ai" | "user" => source.to_lowercase(),
        _ => return Err("source must be 'ai' or 'user'.".to_string()),
    };
    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let mut row = AiStagedChangeRow {
        id: id.clone(),
        entity_type: entity_type.trim().to_string(),
        action: action.trim().to_string(),
        target_entity_id,
        payload_json,
        diff_summary: diff_summary.trim().to_string(),
        source: src,
        proposed_by_personnel_id: Some(u.id.clone()),
        status: "pending".to_string(),
        reviewed_by_personnel_id: None,
        reviewed_at: None,
        review_notes: None,
        auto_approve_rule_id: None,
        created_at: now,
    };
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        if let Some(rule_id) = try_auto_approve(&db, &row)? {
            row.status = "auto_approved".to_string();
            row.reviewed_at = Some(Utc::now().to_rfc3339());
            row.auto_approve_rule_id = Some(rule_id);
            let reviewer = u.clone();
            db.insert_ai_staged_change(&row)?;
            // Execute under auto-approve (reviewer = proposer is wrong; use system: use proposer for inventory writes)
            execute_staged_change(&db, &row, &reviewer)?;
        } else {
            db.insert_ai_staged_change(&row)?;
        }
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.get_ai_staged_change(&id)?
        .ok_or_else(|| "Staged change missing after insert.".to_string())
}

#[tauri::command]
pub fn ai_list_staged(
    filters: AiListStagedFilters,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<AiStagedChangeRow>, String> {
    let u = authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    if u.rank_level >= 4 {
        if filters.status.as_deref() == Some("pending") || filters.status.is_none() {
            return db.list_ai_staged_pending();
        }
        // Full history not implemented: return pending only
        return db.list_ai_staged_pending();
    }
    db.list_ai_staged_for_user(&u.id)
}

#[tauri::command]
pub fn ai_count_pending(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<u32, String> {
    let u = authenticated_user(&session)?;
    if u.rank_level < 4 {
        return Ok(0);
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let n = db.list_ai_staged_pending()?.len() as u32;
    Ok(n)
}

#[tauri::command]
pub fn ai_review_change(
    id: String,
    approved: bool,
    review_notes: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AiStagedChangeRow, String> {
    let reviewer = require_rank4_or_higher(&session)?;
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("id is required.".to_string());
    }
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        let row = db
            .get_ai_staged_change(&id)?
            .ok_or_else(|| "Staged change not found.".to_string())?;
        if row.status != "pending" {
            return Err("Only pending changes can be reviewed.".to_string());
        }
        if approved {
            execute_staged_change(&db, &row, &reviewer)?;
            let now = Utc::now().to_rfc3339();
            db.update_ai_staged_status(
                &id,
                "approved",
                Some(&reviewer.id),
                Some(&now),
                review_notes.as_deref(),
                None,
            )?;
        } else {
            let now = Utc::now().to_rfc3339();
            db.update_ai_staged_status(
                &id,
                "rejected",
                Some(&reviewer.id),
                Some(&now),
                review_notes.as_deref(),
                None,
            )?;
        }
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.get_ai_staged_change(&id)?
        .ok_or_else(|| "Missing after review.".to_string())
}

#[tauri::command]
pub fn ai_get_autoapprove_rules(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<AiAutoapproveRuleRow>, String> {
    let _ = require_rank4_or_higher(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.list_ai_autoapprove_rules()
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAutoapproveRuleArgs {
    pub id: Option<String>,
    pub entity_type: String,
    pub action: String,
    pub predicate_json: String,
    pub enabled: bool,
}

#[tauri::command]
pub fn ai_set_autoapprove_rule(
    args: SetAutoapproveRuleArgs,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AiAutoapproveRuleRow, String> {
    let u = require_rank5(&session)?;
    let _p: serde_json::Value = serde_json::from_str(&args.predicate_json)
        .map_err(|e| format!("predicate_json invalid: {e}"))?;
    let id = args
        .id
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now().to_rfc3339();
    let row = AiAutoapproveRuleRow {
        id: id.clone(),
        entity_type: args.entity_type.trim().to_string(),
        action: args.action.trim().to_string(),
        predicate_json: args.predicate_json,
        enabled: args.enabled,
        created_by_personnel_id: u.id,
        created_at: now,
    };
    {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        if db
            .list_ai_autoapprove_rules()?
            .iter()
            .any(|r| r.id == id)
        {
            db.delete_ai_autoapprove_rule(&id)?;
        }
        db.insert_ai_autoapprove_rule(&row)?;
    }
    Ok(row)
}

#[tauri::command]
pub fn ai_delete_autoapprove_rule(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let _ = require_rank5(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.delete_ai_autoapprove_rule(&id)
}
