//! Check-ins (S7): who is expected to report in, how often, and who is overdue.
//!
//! A check-in counts from either source: someone marking it by hand at the station, or any
//! message heard from that person's radio. Whichever is more recent wins.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const KIND: &str = "checkin_plan";
const HOUR: i64 = 3_600;
const MINUTE: i64 = 60;
/// "Due soon" starts this long before the deadline.
const DUE_SOON_SECS: i64 = 30 * MINUTE;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinPlan {
    /// Person or team expected to report.
    pub name: String,
    /// Meshtastic node number of their radio, when they have one.
    #[serde(default)]
    pub node_id: Option<i64>,
    pub interval_hours: f64,
    /// Lateness allowed before the check-in counts as overdue.
    #[serde(default)]
    pub grace_minutes: i64,
    pub active: bool,
    /// Unix seconds of the last check-in marked by hand.
    #[serde(default)]
    pub last_manual_at: Option<i64>,
    #[serde(default)]
    pub last_note: Option<String>,
    /// Unix seconds the plan started, so a new plan is not instantly overdue.
    #[serde(default)]
    pub started_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckinState {
    /// Plan is switched off.
    Paused,
    Ok,
    /// Inside the last half hour before the deadline.
    DueSoon,
    /// Past the deadline but inside the grace period.
    Late,
    Overdue,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinStatus {
    pub state: CheckinState,
    /// Most recent contact from either source.
    pub last_contact_at: Option<i64>,
    /// When the next check-in is due.
    pub due_at: i64,
    /// Seconds past `due_at`; 0 when not yet due.
    pub late_by_secs: i64,
    /// True when the latest contact came from the radio rather than a manual mark.
    pub last_was_radio: bool,
}

pub fn checkin_status(plan: &CheckinPlan, radio_last_heard: Option<i64>, now: i64) -> CheckinStatus {
    let (last_contact_at, last_was_radio) = match (plan.last_manual_at, radio_last_heard) {
        (Some(m), Some(r)) if r > m => (Some(r), true),
        (Some(m), _) => (Some(m), false),
        (None, Some(r)) => (Some(r), true),
        (None, None) => (None, false),
    };
    let interval = (plan.interval_hours.max(0.05) * HOUR as f64) as i64;
    // Contact from before the plan started does not make it overdue on day one.
    let baseline = match (last_contact_at, plan.started_at) {
        (Some(c), Some(s)) => c.max(s),
        (Some(c), None) => c,
        (None, Some(s)) => s,
        (None, None) => now,
    };
    let due_at = baseline + interval;
    let late_by_secs = (now - due_at).max(0);
    let state = if !plan.active {
        CheckinState::Paused
    } else if late_by_secs > plan.grace_minutes.max(0) * MINUTE {
        CheckinState::Overdue
    } else if late_by_secs > 0 {
        CheckinState::Late
    } else if due_at - now <= DUE_SOON_SECS {
        CheckinState::DueSoon
    } else {
        CheckinState::Ok
    };
    CheckinStatus { state, last_contact_at, due_at, late_by_secs, last_was_radio }
}

fn duration_phrase(secs: i64) -> String {
    if secs < 90 * MINUTE {
        format!("{} min", (secs / MINUTE).max(1))
    } else if secs < 48 * HOUR {
        format!("{} h", secs / HOUR)
    } else {
        format!("{} days", secs / (24 * HOUR))
    }
}

/// Overdue check-ins. Someone a full interval past their deadline is treated as urgent.
pub fn findings(plans: &[(CheckinPlan, Option<i64>)], now: i64) -> Vec<Finding> {
    let mut overdue: Vec<(&CheckinPlan, CheckinStatus)> = plans
        .iter()
        .map(|(p, heard)| (p, checkin_status(p, *heard, now)))
        .filter(|(_, s)| s.state == CheckinState::Overdue)
        .collect();
    if overdue.is_empty() {
        return Vec::new();
    }
    overdue.sort_by(|a, b| b.1.late_by_secs.cmp(&a.1.late_by_secs));
    let urgent = overdue
        .iter()
        .any(|(p, s)| s.late_by_secs as f64 >= p.interval_hours.max(0.05) * HOUR as f64);
    vec![Finding {
        id: "checkins.overdue".to_string(),
        severity: if urgent { Severity::Critical } else { Severity::Warning },
        area: "people",
        title: format!(
            "{} overdue",
            if overdue.len() == 1 { "1 check-in is".to_string() } else { format!("{} check-ins are", overdue.len()) }
        ),
        detail: overdue
            .iter()
            .take(5)
            .map(|(p, s)| format!("{} ({} late)", p.name, duration_phrase(s.late_by_secs)))
            .collect::<Vec<_>>()
            .join(", "),
        go_to: Some("field_ops"),
    }]
}

/// Plans paired with the last time each plan's radio was heard.
pub fn load_with_radio(db: &crate::db::Database) -> Vec<(Record<CheckinPlan>, Option<i64>)> {
    let heard: std::collections::HashMap<i64, i64> = db
        .list_mesh_network_nodes()
        .unwrap_or_default()
        .into_iter()
        .map(|n| (i64::from(n.node_id), crate::runway::log_timestamp_to_seconds(n.last_message_at)))
        .collect();
    store::list::<CheckinPlan>(db, KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let h = r.data.node_id.and_then(|n| heard.get(&n).copied());
            (r, h)
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinRow {
    #[serde(flatten)]
    pub record: Record<CheckinPlan>,
    pub status: CheckinStatus,
}

#[tauri::command]
pub fn list_checkin_plans(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<CheckinRow>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let now = store::now();
    let mut rows: Vec<CheckinRow> = load_with_radio(&db)
        .into_iter()
        .map(|(record, heard)| CheckinRow { status: checkin_status(&record.data, heard, now), record })
        .collect();
    let rank = |s: CheckinState| match s {
        CheckinState::Overdue => 0,
        CheckinState::Late => 1,
        CheckinState::DueSoon => 2,
        CheckinState::Ok => 3,
        CheckinState::Paused => 4,
    };
    rows.sort_by(|a, b| {
        rank(a.status.state)
            .cmp(&rank(b.status.state))
            .then(a.status.due_at.cmp(&b.status.due_at))
    });
    Ok(rows)
}

#[tauri::command]
pub fn save_checkin_plan(
    id: Option<String>,
    mut plan: CheckinPlan,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<CheckinPlan>, String> {
    super::require_member(&session)?;
    plan.name = plan.name.trim().to_string();
    if plan.name.is_empty() {
        return Err("Say who is checking in.".to_string());
    }
    if !plan.interval_hours.is_finite() || !(0.25..=168.0).contains(&plan.interval_hours) {
        return Err("Interval must be between 15 minutes and 7 days.".to_string());
    }
    if !(0..=720).contains(&plan.grace_minutes) {
        return Err("Grace period must be between 0 and 720 minutes.".to_string());
    }
    if plan.started_at.is_none() {
        plan.started_at = Some(store::now());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, KIND, id.as_deref(), &plan)
}

/// Mark that someone has checked in (in person, by voice radio, or any way the app cannot hear).
#[tauri::command]
pub fn record_checkin(
    id: String,
    note: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<CheckinPlan>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let existing = store::get::<CheckinPlan>(&db, KIND, &id)?.ok_or("Check-in plan not found.")?;
    let mut data = existing.data;
    data.last_manual_at = Some(store::now());
    data.last_note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    store::put(&db, KIND, Some(&id), &data)
}

#[tauri::command]
pub fn delete_checkin_plan(
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

    fn plan(name: &str, interval_hours: f64, grace_minutes: i64, last_manual_hours_ago: Option<f64>) -> CheckinPlan {
        CheckinPlan {
            name: name.to_string(),
            node_id: None,
            interval_hours,
            grace_minutes,
            active: true,
            last_manual_at: last_manual_hours_ago.map(|h| NOW - (h * HOUR as f64) as i64),
            last_note: None,
            started_at: None,
        }
    }

    #[test]
    fn on_time_then_due_soon_then_late_then_overdue() {
        let p = |ago| plan("Patrol", 12.0, 30, Some(ago));
        assert_eq!(checkin_status(&p(2.0), None, NOW).state, CheckinState::Ok);
        assert_eq!(checkin_status(&p(11.75), None, NOW).state, CheckinState::DueSoon);
        let late = checkin_status(&p(12.25), None, NOW);
        assert_eq!(late.state, CheckinState::Late);
        assert_eq!(late.late_by_secs, 15 * MINUTE);
        assert_eq!(checkin_status(&p(13.0), None, NOW).state, CheckinState::Overdue);
    }

    #[test]
    fn a_radio_message_counts_as_a_check_in() {
        let p = plan("Outpost", 12.0, 0, Some(20.0));
        assert_eq!(checkin_status(&p, None, NOW).state, CheckinState::Overdue);
        let s = checkin_status(&p, Some(NOW - HOUR), NOW);
        assert_eq!(s.state, CheckinState::Ok);
        assert!(s.last_was_radio);
        assert_eq!(s.last_contact_at, Some(NOW - HOUR));
        // An older radio message does not override a newer manual mark.
        let s = checkin_status(&plan("Outpost", 12.0, 0, Some(1.0)), Some(NOW - 5 * HOUR), NOW);
        assert!(!s.last_was_radio);
    }

    #[test]
    fn a_new_plan_is_not_overdue_until_its_first_interval_passes() {
        let mut p = plan("New team", 6.0, 0, None);
        p.started_at = Some(NOW - 2 * HOUR);
        let s = checkin_status(&p, None, NOW);
        assert_eq!(s.state, CheckinState::Ok);
        assert_eq!(s.due_at, NOW + 4 * HOUR);
        // Old radio traffic from before the plan started does not count against it.
        let s = checkin_status(&p, Some(NOW - 50 * HOUR), NOW);
        assert_eq!(s.state, CheckinState::Ok);
        p.started_at = Some(NOW - 7 * HOUR);
        assert_eq!(checkin_status(&p, None, NOW).state, CheckinState::Overdue);
    }

    #[test]
    fn paused_plans_never_raise() {
        let mut p = plan("Away", 1.0, 0, Some(40.0));
        p.active = false;
        assert_eq!(checkin_status(&p, None, NOW).state, CheckinState::Paused);
        assert!(findings(&[(p, None)], NOW).is_empty());
    }

    #[test]
    fn overdue_findings_list_the_latest_first_and_escalate() {
        let plans = vec![
            (plan("Patrol", 12.0, 0, Some(13.0)), None),
            (plan("Farm", 12.0, 0, Some(2.0)), None),
            (plan("Ridge", 6.0, 0, Some(9.0)), None),
        ];
        let f = findings(&plans, NOW);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].title, "2 check-ins are overdue");
        assert_eq!(f[0].detail, "Ridge (3 h late), Patrol (60 min late)");
        assert_eq!(f[0].severity, Severity::Warning);

        // A full interval late is urgent.
        let f = findings(&[(plan("Ridge", 6.0, 0, Some(13.0)), None)], NOW);
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[0].title, "1 check-in is overdue");
    }
}
