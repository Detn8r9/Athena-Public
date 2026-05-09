use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use crate::db::{CommandNotificationRow, personnel_pin_hash, PersonnelPublic};
use crate::personnel_commander::{primary_role_label, RosterCommanderMaps};
use crate::AppState;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedPersonnel {
    pub id: String,
    pub name: String,
    pub division: String,
    pub rank_level: i32,
    #[serde(default)]
    pub clearance_level: i32,
    pub display_name: Option<String>,
    pub operational_status: String,
    pub directorate: Option<String>,
    pub tier: Option<String>,
    pub job_title: Option<String>,
    /// True only for the designated ATHENA creator account (not implied by rank 5).
    #[serde(default)]
    pub athena_architect: bool,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub role_l1: Option<String>,
    #[serde(default)]
    pub role_l2: Option<String>,
    #[serde(default)]
    pub role_l3: Option<String>,
    #[serde(default)]
    pub role_l4: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgChartNode {
    pub id: String,
    pub name: String,
    pub division: String,
    pub level: i32,
    pub role: String,
    pub commander_id: Option<String>,
    pub commander_name: Option<String>,
    pub children: Vec<OrgChartNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInventoryLogLine {
    #[serde(flatten)]
    pub log: crate::db::InventoryLogRow,
    pub item_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubordinateBrief {
    pub id: String,
    pub name: String,
    pub rank_level: i32,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonnelProfilePayload {
    pub person: PersonnelPublic,
    pub commander_id: Option<String>,
    pub commander_name: Option<String>,
    /// People whose resolved commander is this person.
    pub subordinates: Vec<SubordinateBrief>,
    pub recent_logbook_entries: Vec<crate::db::LogbookEntry>,
    pub recent_inventory_log: Vec<ProfileInventoryLogLine>,
    pub recent_inventory_intake_events: Vec<crate::db::InventoryIntakeEventRow>,
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

/// Rank 4+ command & control (map blockages review, AI staging inbox).
pub fn require_rank4_or_higher(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    require_personnel_admin(session)
}

/// Rank 5 (Caesar) only — auto-approve rules and other sovereign controls.
pub fn require_rank5(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(session)?;
    if u.rank_level < 5 {
        return Err("Insufficient rank: Level 5 (Caesar) required.".to_string());
    }
    Ok(u)
}

/// Field operators: rank 2+ (propose blockages, stage AI changes, etc.).
pub fn require_rank2_or_higher(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(session)?;
    if u.rank_level < 2 {
        return Err("Insufficient rank for this action.".to_string());
    }
    Ok(u)
}

fn require_clearance_level(session: &AuthSession, min: i32) -> Result<AuthenticatedPersonnel, String> {
    let u = authenticated_user(session)?;
    if u.clearance_level < min {
        return Err(format!("Requires clearance level {min}+"));
    }
    Ok(u)
}

/// Clearance 4+ — tactical data writes (territories, operational deletions, etc.).
pub fn require_clearance4_or_higher(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    require_clearance_level(session, 4)
}

/// Tauri event ID for in-app command notifications.
pub const NOTIFICATION_EVENT: &str = "athena:notification";

#[tauri::command]
pub fn list_command_notifications(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
    limit: Option<usize>,
) -> Result<Vec<CommandNotificationRow>, String> {
    require_clearance_level(&session, 4)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.list_command_notifications(limit.unwrap_or(100).min(500))
}

#[tauri::command]
pub fn count_unacked_command_notifications(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<i64, String> {
    require_clearance_level(&session, 4)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.count_unacked_command_notifications()
}

#[tauri::command]
pub fn ack_command_notification(
    notification_id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    let u = require_clearance_level(&session, 4)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.ack_command_notification(&notification_id, &u.id)
}

#[tauri::command]
pub fn ack_all_command_notifications(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<usize, String> {
    let u = require_clearance_level(&session, 4)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.ack_all_command_notifications(&u.id)
}

#[tauri::command]
pub fn list_role_catalog(state: State<'_, AppState>, session: State<'_, AuthSession>) -> Result<Vec<crate::db::RoleCatalogEntry>, String> {
    require_personnel_admin(&session)?;
    state
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .list_role_catalog()
}

#[tauri::command]
pub fn add_role_catalog_entry(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    session: State<'_, AuthSession>,
    domain: String,
    level: String,
    title: String,
) -> Result<(), String> {
    require_personnel_admin(&session)?;
    {
        let db = state
            .db
            .lock()
            .map_err(|e| e.to_string())?;
        db.add_role_catalog_entry(&domain, &level, &title)?;
    }
    let d = domain.trim();
    let l = level.trim();
    let t = title.trim();
    {
        let db = state
            .db
            .lock()
            .map_err(|e| e.to_string())?;
        if let Ok(n) = db.command_notification_insert(
            "personnel",
            "low",
            "New job title in catalog",
            Some(&format!("{d} / {l} / {t}")),
            None,
        ) {
            let _ = app_handle.emit(NOTIFICATION_EVENT, &n);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_personnel_profile(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
    personnel_id: String,
) -> Result<PersonnelProfilePayload, String> {
    require_personnel_admin(&session)?;
    let pid = personnel_id.trim();
    if pid.is_empty() {
        return Err("personnel_id is required.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let person = db
        .personnel_fetch_public_by_id(pid)?
        .ok_or_else(|| "Personnel not found.".to_string())?;

    let rows = db.personnel_list_public()?;
    let maps = RosterCommanderMaps::from_roster(&rows);
    let (commander_id, commander_name) = maps.resolve_commander(&person);

    let mut subordinates: Vec<SubordinateBrief> = Vec::new();
    for p in &rows {
        if p.id == person.id {
            continue;
        }
        let (cid, _) = maps.resolve_commander(p);
        if cid.as_deref() == Some(pid) {
            subordinates.push(SubordinateBrief {
                id: p.id.clone(),
                name: p.name.clone(),
                rank_level: p.rank_level,
                role: primary_role_label(p),
            });
        }
    }
    subordinates.sort_by(|a, b| a.name.cmp(&b.name));

    let logbook = db.list_logbook_entries_by_user(pid, 40).unwrap_or_default();
    let inv_log_raw = db.list_inventory_log_by_user(pid, 50).unwrap_or_default();
    let inv_log: Vec<ProfileInventoryLogLine> = inv_log_raw
        .into_iter()
        .map(|row| {
            let item_name = match db.inventory_get(&row.item_id) {
                Ok(Some(i)) => Some(i.item_name),
                _ => None,
            };
            ProfileInventoryLogLine { log: row, item_name }
        })
        .collect();
    let intake = db.list_inventory_intake_events_by_user(pid, 50).unwrap_or_default();

    Ok(PersonnelProfilePayload {
        person,
        commander_id,
        commander_name,
        subordinates,
        recent_logbook_entries: logbook,
        recent_inventory_log: inv_log,
        recent_inventory_intake_events: intake,
    })
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
        clearance_level: row.clearance_level,
        display_name: row.display_name.clone(),
        operational_status: row.operational_status.clone(),
        directorate: row.directorate.clone(),
        tier: row.tier.clone(),
        job_title: row.job_title.clone(),
        athena_architect: row.athena_architect != 0,
        domain: row.domain.clone(),
        role_l1: row.role_l1.clone(),
        role_l2: row.role_l2.clone(),
        role_l3: row.role_l3.clone(),
        role_l4: row.role_l4.clone(),
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
pub fn authenticate_guest(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<AuthenticatedPersonnel, String> {
    // Guest is a strict sandbox: clearance 0, no PIN, no personnel DB row.
    let out = AuthenticatedPersonnel {
        id: "guest".to_string(),
        name: "Guest".to_string(),
        division: "GUEST".to_string(),
        rank_level: 0,
        clearance_level: 0,
        display_name: None,
        operational_status: "active".to_string(),
        directorate: None,
        tier: None,
        job_title: None,
        athena_architect: false,
        domain: None,
        role_l1: None,
        role_l2: None,
        role_l3: None,
        role_l4: None,
    };
    *session.0.lock().map_err(|e| e.to_string())? = Some(out.clone());
    // Best-effort: emit nothing else; guest should not be enrolled in background tasks.
    let _ = app; // keep signature consistent with other auth commands
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
pub fn get_org_chart(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<OrgChartNode>, String> {
    // Rank 4+ only: org chart includes command structure.
    require_personnel_admin(&session)?;

    let db = app.db.lock().map_err(|e| e.to_string())?;
    let rows = db.personnel_list_public()?;
    let maps = RosterCommanderMaps::from_roster(&rows);

    #[derive(Clone)]
    struct Temp {
        id: String,
        name: String,
        division: String,
        level: i32,
        role: String,
        commander_id: Option<String>,
        commander_name: Option<String>,
    }

    let mut temps: Vec<Temp> = Vec::new();
    for p in rows {
        let role = primary_role_label(&p);
        let (commander_id, commander_name) = maps.resolve_commander(&p);
        temps.push(Temp {
            id: p.id,
            name: p.name,
            division: p.division,
            level: p.rank_level,
            role: role.trim().to_string(),
            commander_id,
            commander_name,
        });
    }

    // Build adjacency by commander_id.
    let mut by_id: HashMap<String, Temp> = HashMap::new();
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots: Vec<String> = Vec::new();
    for t in temps.into_iter() {
        let id = t.id.clone();
        if let Some(pid) = t.commander_id.clone() {
            children_map.entry(pid).or_default().push(id.clone());
        } else {
            roots.push(id.clone());
        }
        by_id.insert(id, t);
    }

    fn build(id: &str, by_id: &HashMap<String, Temp>, children_map: &HashMap<String, Vec<String>>) -> OrgChartNode {
        let Some(t) = by_id.get(id) else {
            return OrgChartNode {
                id: id.to_string(),
                name: "Unknown node".to_string(),
                division: String::new(),
                level: 0,
                role: "—".to_string(),
                commander_id: None,
                commander_name: None,
                children: vec![],
            };
        };
        let mut kids: Vec<OrgChartNode> = Vec::new();
        if let Some(child_ids) = children_map.get(id) {
            for cid in child_ids {
                kids.push(build(cid, by_id, children_map));
            }
            kids.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.name.cmp(&b.name)));
        }
        OrgChartNode {
            id: t.id.clone(),
            name: t.name.clone(),
            division: t.division.clone(),
            level: t.level,
            role: t.role.clone(),
            commander_id: t.commander_id.clone(),
            commander_name: t.commander_name.clone(),
            children: kids,
        }
    }

    let mut out: Vec<OrgChartNode> = roots.iter().map(|id| build(id, &by_id, &children_map)).collect();
    out.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[tauri::command]
pub fn add_personnel(
    name: String,
    division: String,
    rank_level: i32,
    pin: String,
    display_name: Option<String>,
    domain: Option<String>,
    role_l1: Option<String>,
    role_l2: Option<String>,
    role_l3: Option<String>,
    role_l4: Option<String>,
    app: State<'_, AppState>,
    app_handle: AppHandle,
    session: State<'_, AuthSession>,
) -> Result<PersonnelPublic, String> {
    require_personnel_admin(&session)?;
    let name = name.trim().to_string();
    let division = division.trim().to_string();
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
    let name_for_notify = name.clone();
    let disp = display_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let hash = personnel_pin_hash(&pin);
    let id = uuid::Uuid::new_v4().to_string();
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let dom = domain
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l1 = role_l1
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l2 = role_l2
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l3 = role_l3
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l4 = role_l4
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    db.personnel_insert(
        &id,
        &name,
        &division,
        rank_level,
        &hash,
        disp.as_deref(),
        None,
        None,
        None,
        0,
        dom.as_deref(),
        l1.as_deref(),
        l2.as_deref(),
        l3.as_deref(),
        l4.as_deref(),
    )?;
    let out = db
        .personnel_fetch_public_by_id(&id)?
        .ok_or_else(|| "Failed to load new personnel row.".to_string())?;
    if let Ok(n) = db.command_notification_insert(
        "personnel",
        "low",
        "New personnel enrolled",
        Some(&format!("{name_for_notify} (L{rank_level})")),
        Some(&id),
    ) {
        let _ = app_handle.emit(NOTIFICATION_EVENT, &n);
    }
    Ok(out)
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
    domain: Option<String>,
    role_l1: Option<String>,
    role_l2: Option<String>,
    role_l3: Option<String>,
    role_l4: Option<String>,
    app: State<'_, AppState>,
    app_handle: AppHandle,
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

    let dom = domain
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l1 = role_l1
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l2 = role_l2
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l3 = role_l3
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let l4 = role_l4
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    db.personnel_admin_update(
        target_id,
        &login_name,
        display_name.as_deref(),
        &division,
        rank_level,
        &operational_status,
        None,
        None,
        None,
        dom.as_deref(),
        l1.as_deref(),
        l2.as_deref(),
        l3.as_deref(),
        l4.as_deref(),
    )?;

    let out = db
        .personnel_fetch_public_by_id(target_id)?
        .ok_or_else(|| "Failed to load updated personnel record.".to_string())?;
    if let Ok(n) = db.command_notification_insert(
        "personnel",
        "low",
        "Personnel record updated",
        Some(&format!("{login_name} (L{rank_level}) — {operational_status}")),
        Some(target_id),
    ) {
        let _ = app_handle.emit(NOTIFICATION_EVENT, &n);
    }
    Ok(out)
}

/// Lowercase hex SHA-256 of UTF-8 `Project-Athena-Prime` (architect OOBE override passphrase).
const ARCHITECT_OVERRIDE_KEY_SHA256_HEX: &str =
    "e9f4c92458d7c9e77b5448812bef0a8cd28335c7105f80e018ee2bc576a83dbe";

fn digest_architect_override_key(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(raw.trim().as_bytes());
    format!("{:x}", h.finalize())
}

#[tauri::command]
pub fn oobe_first_account_pending(app: State<'_, AppState>) -> Result<bool, String> {
    let db = app.db.lock().map_err(|e| e.to_string())?;
    db.oobe_first_account_pending()
}

#[tauri::command]
pub async fn oobe_create_account(
    username: String,
    pin: String,
    architect_key: Option<String>,
    app: State<'_, AppState>,
) -> Result<(), String> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("Username is required.".to_string());
    }
    let pin = pin.trim().to_string();
    if pin.len() < 4 {
        return Err("PIN must be at least 4 characters.".to_string());
    }

    let want_architect = match architect_key.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => false,
        Some(k) => {
            let digest = digest_architect_override_key(k);
            if digest != ARCHITECT_OVERRIDE_KEY_SHA256_HEX {
                return Err("Invalid architect override key.".to_string());
            }
            true
        }
    };

    let db = app.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.oobe_remove_bootstrap_station_architect_if_present()?;
        if db.personnel_count()? != 0 {
            return Err("First account could not be created: roster is not empty.".to_string());
        }
        let rank = if want_architect { 5 } else { 4 };
        let arch_flag = if want_architect { 1 } else { 0 };
        let hash = personnel_pin_hash(&pin);
        let id = uuid::Uuid::new_v4().to_string();
        db.personnel_insert(
            &id,
            &username,
            "COMMAND",
            rank,
            &hash,
            None,
            None,
            None,
            None,
            arch_flag,
            None,
            None,
            None,
            None,
            None,
        )?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("First account task failed: {e}"))??;

    Ok(())
}
