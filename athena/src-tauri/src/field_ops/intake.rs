//! Newcomer intake (S25): record who arrived, what they can do and need, and track quarantine.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const KIND: &str = "intake";
const DAY: i64 = 86_400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeRecord {
    pub name: String,
    /// Unix seconds.
    pub arrived_at: i64,
    #[serde(default)]
    pub party_size: Option<u32>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub medical_notes: Option<String>,
    #[serde(default)]
    pub allergies: Option<String>,
    #[serde(default)]
    pub came_from: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// 0 means no quarantine.
    pub quarantine_days: i64,
    /// `quarantine`, `cleared` or `departed`.
    pub status: String,
    #[serde(default)]
    pub cleared_at: Option<i64>,
    #[serde(default)]
    pub cleared_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineStatus {
    /// Unix seconds when quarantine ends.
    pub ends_at: i64,
    /// Seconds left; 0 once it has ended.
    pub remaining_secs: i64,
    pub ended: bool,
    /// Whole days left, rounded up, for display.
    pub days_left: i64,
}

pub fn quarantine_status(arrived_at: i64, quarantine_days: i64, now: i64) -> QuarantineStatus {
    let ends_at = arrived_at + quarantine_days.max(0) * DAY;
    let remaining_secs = (ends_at - now).max(0);
    QuarantineStatus {
        ends_at,
        remaining_secs,
        ended: remaining_secs == 0,
        days_left: (remaining_secs + DAY - 1) / DAY,
    }
}

/// People still marked as in quarantine whose quarantine period has run out.
pub fn findings(records: &[IntakeRecord], now: i64) -> Vec<Finding> {
    let ready: Vec<&IntakeRecord> = records
        .iter()
        .filter(|r| r.status == "quarantine")
        .filter(|r| quarantine_status(r.arrived_at, r.quarantine_days, now).ended)
        .collect();
    if ready.is_empty() {
        return Vec::new();
    }
    vec![Finding {
        id: "intake.quarantine_ended".to_string(),
        severity: Severity::Info,
        area: "people",
        title: format!(
            "Quarantine has ended for {}",
            if ready.len() == 1 { "1 newcomer".to_string() } else { format!("{} newcomers", ready.len()) }
        ),
        detail: format!(
            "{}. Check on them and mark them cleared.",
            ready.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ")
        ),
        go_to: Some("field_ops"),
    }]
}

pub fn load(db: &crate::db::Database) -> Vec<IntakeRecord> {
    store::list::<IntakeRecord>(db, KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeRow {
    #[serde(flatten)]
    pub record: Record<IntakeRecord>,
    pub quarantine: QuarantineStatus,
}

#[tauri::command]
pub fn list_intake_records(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<IntakeRow>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let now = store::now();
    let mut rows: Vec<IntakeRow> = store::list::<IntakeRecord>(&db, KIND)?
        .into_iter()
        .map(|record| IntakeRow {
            quarantine: quarantine_status(record.data.arrived_at, record.data.quarantine_days, now),
            record,
        })
        .collect();
    rows.sort_by(|a, b| b.record.data.arrived_at.cmp(&a.record.data.arrived_at));
    Ok(rows)
}

#[tauri::command]
pub fn save_intake_record(
    id: Option<String>,
    mut record: IntakeRecord,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<IntakeRecord>, String> {
    super::require_member(&session)?;
    record.name = record.name.trim().to_string();
    if record.name.is_empty() {
        return Err("Enter a name.".to_string());
    }
    if !(0..=60).contains(&record.quarantine_days) {
        return Err("Quarantine must be between 0 and 60 days.".to_string());
    }
    if !matches!(record.status.as_str(), "quarantine" | "cleared" | "departed") {
        record.status = if record.quarantine_days > 0 { "quarantine" } else { "cleared" }.to_string();
    }
    if record.arrived_at <= 0 {
        record.arrived_at = store::now();
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, KIND, id.as_deref(), &record)
}

#[tauri::command]
pub fn set_intake_status(
    id: String,
    status: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<IntakeRecord>, String> {
    let user = super::require_member(&session)?;
    if !matches!(status.as_str(), "quarantine" | "cleared" | "departed") {
        return Err("Unknown status.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let existing = store::get::<IntakeRecord>(&db, KIND, &id)?.ok_or("Record not found.")?;
    let mut data = existing.data;
    data.status = status;
    if data.status == "cleared" {
        data.cleared_at = Some(store::now());
        data.cleared_by = Some(user.display_name.clone().unwrap_or(user.name.clone()));
    }
    store::put(&db, KIND, Some(&id), &data)
}

#[tauri::command]
pub fn delete_intake_record(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, KIND, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn rec(name: &str, days_ago: i64, quarantine_days: i64, status: &str) -> IntakeRecord {
        IntakeRecord {
            name: name.to_string(),
            arrived_at: NOW - days_ago * DAY,
            party_size: None,
            skills: vec![],
            medical_notes: None,
            allergies: None,
            came_from: None,
            notes: None,
            quarantine_days,
            status: status.to_string(),
            cleared_at: None,
            cleared_by: None,
        }
    }

    #[test]
    fn quarantine_counts_down_and_rounds_days_up() {
        let s = quarantine_status(NOW - 2 * DAY - 3600, 5, NOW);
        assert!(!s.ended);
        assert_eq!(s.remaining_secs, 3 * DAY - 3600);
        assert_eq!(s.days_left, 3);
        assert_eq!(s.ends_at, NOW + 3 * DAY - 3600);
    }

    #[test]
    fn quarantine_ends_exactly_at_the_deadline() {
        assert!(quarantine_status(NOW - 5 * DAY, 5, NOW).ended);
        assert!(!quarantine_status(NOW - 5 * DAY + 1, 5, NOW).ended);
        let none = quarantine_status(NOW, 0, NOW);
        assert!(none.ended);
        assert_eq!(none.days_left, 0);
    }

    #[test]
    fn only_people_still_in_quarantine_past_their_date_are_flagged() {
        let records = vec![
            rec("Dana", 6, 5, "quarantine"),
            rec("Eli", 2, 5, "quarantine"),
            rec("Fay", 9, 5, "cleared"),
            rec("Gus", 9, 5, "departed"),
        ];
        let f = findings(&records, NOW);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].title, "Quarantine has ended for 1 newcomer");
        assert!(f[0].detail.starts_with("Dana."));
        assert!(findings(&records[1..], NOW).is_empty());
    }
}
