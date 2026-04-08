use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

use crate::db::{personnel_pin_hash, PersonnelPublic};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AuthenticatedPersonnel {
    pub id: String,
    pub name: String,
    pub division: String,
    pub rank_level: i32,
    pub display_name: Option<String>,
    pub operational_status: String,
}

/// Server-side session for Roman-tier RBAC (PIN verified against SQLite `personnel`).
pub struct AuthSession(pub Mutex<Option<AuthenticatedPersonnel>>);

impl Default for AuthSession {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

pub fn authenticated_user(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    let guard = session.0.lock().map_err(|e| e.to_string())?;
    let u = guard
        .clone()
        .ok_or_else(|| "Not authenticated.".to_string())?;
    let st = u.operational_status.trim().to_ascii_lowercase();
    if st == "mia" || st == "kia" {
        drop(guard);
        if let Ok(mut g) = session.0.lock() {
            *g = None;
        }
        return Err("This account is locked (MIA/KIA). Contact Command.".to_string());
    }
    Ok(u)
}

fn require_personnel_admin(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(session)?;
    if u.rank_level < 4 {
        return Err("Insufficient rank for personnel administration.".to_string());
    }
    Ok(u)
}

fn is_locked_status(st: &str) -> bool {
    let t = st.trim().to_ascii_lowercase();
    t == "mia" || t == "kia"
}

fn enforce_rank4_cannot_touch_caesar(
    actor: &AuthenticatedPersonnel,
    target_rank_level: i32,
) -> Result<(), String> {
    if actor.rank_level == 4 && target_rank_level >= 5 {
        return Err("Rank 4 cannot modify a Caesar (Level 5).".to_string());
    }
    Ok(())
}

fn session_personnel_from_row(row: &crate::db::PersonnelRow) -> AuthenticatedPersonnel {
    AuthenticatedPersonnel {
        id: row.id.clone(),
        name: row.name.clone(),
        division: row.division.clone(),
        rank_level: row.rank_level,
        display_name: row.display_name.clone(),
        operational_status: row.operational_status.clone(),
    }
}

#[tauri::command]
pub fn authenticate_pin(
    name: String,
    pin: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AuthenticatedPersonnel, String> {
    let db_guard = app.db.lock().map_err(|e| e.to_string())?;
    let row = db_guard
        .personnel_fetch_by_name(&name)?
        .ok_or_else(|| "Unknown operator name.".to_string())?;
    let digest = personnel_pin_hash(&pin);
    if digest != row.pin_hash {
        return Err("Invalid PIN.".to_string());
    }
    if is_locked_status(&row.operational_status) {
        return Err("This account is locked (MIA/KIA). Contact Command.".to_string());
    }
    let out = session_personnel_from_row(&row);
    *session.0.lock().map_err(|e| e.to_string())? = Some(out.clone());
    Ok(out)
}

#[tauri::command]
pub fn logout(session: State<'_, AuthSession>) -> Result<(), String> {
    *session.0.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

#[tauri::command]
pub fn set_my_personnel_display_name(
    display_name: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(&session)?;
    let trimmed = display_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let as_ref = trimmed.as_deref();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.personnel_set_display_name(&u.id, as_ref)?;
    let row = db
        .personnel_fetch_by_id(&u.id)?
        .ok_or_else(|| "Personnel record missing.".to_string())?;
    let out = session_personnel_from_row(&row);
    *session.0.lock().map_err(|e| e.to_string())? = Some(out.clone());
    Ok(out)
}

#[tauri::command]
pub fn set_my_personnel_login_name(
    new_name: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.personnel_set_name(&u.id, &new_name)?;
    let row = db
        .personnel_fetch_by_id(&u.id)?
        .ok_or_else(|| "Personnel record missing.".to_string())?;
    let out = session_personnel_from_row(&row);
    *session.0.lock().map_err(|e| e.to_string())? = Some(out.clone());
    Ok(out)
}

#[tauri::command]
pub fn set_my_personnel_pin(
    current_pin: String,
    new_pin: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let u = authenticated_user(&session)?;
    let new_pin = new_pin.trim();
    if new_pin.is_empty() {
        return Err("New PIN cannot be empty.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let row = db
        .personnel_fetch_by_id(&u.id)?
        .ok_or_else(|| "Personnel record missing.".to_string())?;
    if personnel_pin_hash(&current_pin) != row.pin_hash {
        return Err("Current PIN is incorrect.".to_string());
    }
    let hash = personnel_pin_hash(new_pin);
    db.personnel_set_pin_hash(&u.id, &hash)?;
    Ok(())
}

#[tauri::command]
pub fn set_personnel_display_name(
    personnel_id: String,
    display_name: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let actor = require_personnel_admin(&session)?;
    let pid = personnel_id.trim();
    if pid.is_empty() {
        return Err("personnel_id is required.".to_string());
    }
    let trimmed = display_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let as_ref = trimmed.as_deref();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let target_row = db
        .personnel_fetch_by_id(pid)?
        .ok_or_else(|| "Personnel record not found.".to_string())?;
    enforce_rank4_cannot_touch_caesar(&actor, target_row.rank_level)?;
    db.personnel_set_display_name(pid, as_ref)?;
    if actor.id == pid {
        let row = db
            .personnel_fetch_by_id(pid)?
            .ok_or_else(|| "Personnel record missing.".to_string())?;
        if is_locked_status(&row.operational_status) {
            *session.0.lock().map_err(|e| e.to_string())? = None;
            return Err("This account is locked (MIA/KIA). Contact Command.".to_string());
        }
        let out = session_personnel_from_row(&row);
        *session.0.lock().map_err(|e| e.to_string())? = Some(out);
    }
    Ok(())
}

#[tauri::command]
pub fn refresh_auth_session(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let row = db
        .personnel_fetch_by_id(&u.id)?
        .ok_or_else(|| "Personnel record missing.".to_string())?;
    if is_locked_status(&row.operational_status) {
        *session.0.lock().map_err(|e| e.to_string())? = None;
        return Err("This account is locked (MIA/KIA). Contact Command.".to_string());
    }
    let out = session_personnel_from_row(&row);
    *session.0.lock().map_err(|e| e.to_string())? = Some(out.clone());
    Ok(out)
}

#[tauri::command]
pub fn list_personnel(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<PersonnelPublic>, String> {
    require_personnel_admin(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.personnel_list_public()
}

#[tauri::command]
pub fn add_personnel(
    name: String,
    division: String,
    rank_level: i32,
    pin: String,
    display_name: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<PersonnelPublic, String> {
    require_personnel_admin(&session)?;
    let name = name.trim();
    let division = division.trim();
    if name.is_empty() || division.is_empty() {
        return Err("Name and division are required.".to_string());
    }
    if !(1..=4).contains(&rank_level) {
        return Err(
            "New accounts may only be Level 1–4. Level 5 (Caesar) is exclusive and cannot be assigned here."
                .to_string(),
        );
    }
    if pin.trim().is_empty() {
        return Err("PIN is required.".to_string());
    }
    let disp = display_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let hash = personnel_pin_hash(&pin);
    let id = uuid::Uuid::new_v4().to_string();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.personnel_insert(
        &id,
        name,
        division,
        rank_level,
        &hash,
        disp.as_deref(),
    )?;
    db.personnel_fetch_public_by_id(&id)?
        .ok_or_else(|| "Failed to load new personnel row.".to_string())
}

#[tauri::command]
pub fn remove_personnel(
    personnel_id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    let actor = require_personnel_admin(&session)?;
    let pid = personnel_id.trim();
    if pid.is_empty() {
        return Err("personnel_id is required.".to_string());
    }
    if pid == actor.id {
        return Err("You cannot remove your own personnel record while logged in.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let target_row = db
        .personnel_fetch_by_id(pid)?
        .ok_or_else(|| "Personnel record not found.".to_string())?;
    enforce_rank4_cannot_touch_caesar(&actor, target_row.rank_level)?;
    if target_row.rank_level >= 5 {
        return Err("Caesar (Level 5) cannot be removed from the registry.".to_string());
    }
    let n = db.personnel_delete_by_id(pid)?;
    if n == 0 {
        return Err("Personnel record not found.".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn admin_update_personnel(
    personnel_id: String,
    login_name: String,
    display_name: Option<String>,
    division: String,
    rank_level: i32,
    operational_status: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<PersonnelPublic, String> {
    let actor = require_personnel_admin(&session)?;
    let target_id = personnel_id.trim();
    if target_id.is_empty() {
        return Err("personnel_id is required.".to_string());
    }

    let db = app.db.lock().map_err(|e| e.to_string())?;
    let target_row = db
        .personnel_fetch_by_id(target_id)?
        .ok_or_else(|| "Personnel record not found.".to_string())?;

    if actor.rank_level == 4 {
        if target_row.rank_level >= 5 {
            return Err("Rank 4 cannot edit a Caesar (Level 5).".to_string());
        }
        if rank_level >= 5 {
            return Err("Rank 4 cannot promote anyone to Caesar (Level 5).".to_string());
        }
    }

    if actor.rank_level >= 5 && rank_level >= 5 && target_id != actor.id {
        db.personnel_transfer_caesar(&actor.id, target_id, &login_name, display_name.as_deref(), &division, &operational_status)?;
    } else {
        db.personnel_admin_update(
            target_id,
            &login_name,
            display_name.as_deref(),
            &division,
            rank_level,
            &operational_status,
        )?;
    }

    db.personnel_fetch_public_by_id(target_id)?
        .ok_or_else(|| "Failed to load updated personnel record.".to_string())
}
