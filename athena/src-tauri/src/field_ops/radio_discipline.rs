//! Emission discipline (S12): planned radio silence.
//!
//! Every transmission tells a listener that you exist and roughly where. Silence windows stop
//! ATHENA from transmitting at set times of day, and a manual switch silences the radio for a
//! set period. Receiving is never affected. Distress traffic (messages starting with `SOS`)
//! always goes out.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::AppState;

pub const WINDOW_KIND: &str = "silence_window";
const OVERRIDE_KIND: &str = "silence_override";
const OVERRIDE_ID: &str = "silence_override";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceWindow {
    pub name: String,
    /// Minutes after local midnight the window starts, 0 to 1439.
    pub start_minute: u32,
    /// Minutes after local midnight it ends. Smaller than `start_minute` means it runs past
    /// midnight. Equal to it means all day.
    pub end_minute: u32,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceOverride {
    /// Unix seconds until which the radio stays silent.
    pub until: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SilenceState {
    pub windows: Vec<SilenceWindow>,
    pub silent_until: Option<i64>,
}

fn hhmm(minute: u32) -> String {
    format!("{:02}:{:02}", (minute / 60) % 24, minute % 60)
}

pub fn window_covers(window: &SilenceWindow, minute_of_day: u32) -> bool {
    if !window.active {
        return false;
    }
    let (s, e, m) = (window.start_minute % 1440, window.end_minute % 1440, minute_of_day % 1440);
    if s == e {
        true
    } else if s < e {
        m >= s && m < e
    } else {
        m >= s || m < e
    }
}

/// Why the radio is silent right now, or `None` when transmitting is allowed.
pub fn silence_reason(state: &SilenceState, minute_of_day: u32, now: i64) -> Option<String> {
    if let Some(until) = state.silent_until {
        if until > now {
            let mins = ((until - now) + 59) / 60;
            return Some(format!("Radio silence is on for another {mins} minute{}.", if mins == 1 { "" } else { "s" }));
        }
    }
    state
        .windows
        .iter()
        .find(|w| window_covers(w, minute_of_day))
        .map(|w| format!("Radio silence \"{}\" is in effect until {}.", w.name, hhmm(w.end_minute)))
}

/// Distress traffic is never held.
pub fn is_emergency(text: &str) -> bool {
    text.trim_start().to_ascii_uppercase().starts_with("SOS")
}

static STATE: RwLock<Option<SilenceState>> = RwLock::new(None);

/// Load the silence plan from the database into memory, where the radio thread can read it
/// without touching the database.
pub fn refresh_from_db(db: &crate::db::Database) {
    let windows = store::list::<SilenceWindow>(db, WINDOW_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect();
    let silent_until = store::get::<SilenceOverride>(db, OVERRIDE_KIND, OVERRIDE_ID)
        .ok()
        .flatten()
        .and_then(|r| r.data.until);
    if let Ok(mut guard) = STATE.write() {
        *guard = Some(SilenceState { windows, silent_until });
    }
}

fn local_minute_of_day() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 60 + now.minute()
}

/// Called before every transmission. `Err` carries the reason to show the operator.
pub fn check_transmit(text: &str) -> Result<(), String> {
    if is_emergency(text) {
        return Ok(());
    }
    let guard = STATE.read().map_err(|e| e.to_string())?;
    let Some(state) = guard.as_ref() else {
        return Ok(());
    };
    match silence_reason(state, local_minute_of_day(), chrono::Utc::now().timestamp()) {
        Some(reason) => Err(format!("{reason} Nothing was transmitted. SOS messages are still sent.")),
        None => Ok(()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioDisciplineView {
    pub windows: Vec<Record<SilenceWindow>>,
    pub silent_until: Option<i64>,
    /// Reason the radio is silent right now, if it is.
    pub silent_now: Option<String>,
}

fn view(db: &crate::db::Database) -> Result<RadioDisciplineView, String> {
    refresh_from_db(db);
    let now = chrono::Utc::now().timestamp();
    let windows = store::list::<SilenceWindow>(db, WINDOW_KIND)?;
    let silent_until = store::get::<SilenceOverride>(db, OVERRIDE_KIND, OVERRIDE_ID)?
        .and_then(|r| r.data.until)
        .filter(|u| *u > now);
    let state = SilenceState { windows: windows.iter().map(|w| w.data.clone()).collect(), silent_until };
    Ok(RadioDisciplineView {
        silent_now: silence_reason(&state, local_minute_of_day(), now),
        windows,
        silent_until,
    })
}

#[tauri::command]
pub fn get_radio_discipline(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<RadioDisciplineView, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    view(&db)
}

#[tauri::command]
pub fn save_silence_window(
    id: Option<String>,
    mut window: SilenceWindow,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<RadioDisciplineView, String> {
    super::require_member(&session)?;
    window.name = window.name.trim().to_string();
    if window.name.is_empty() {
        return Err("Name the silence window.".to_string());
    }
    if window.start_minute >= 1440 || window.end_minute >= 1440 {
        return Err("Times must be within one day.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, WINDOW_KIND, id.as_deref(), &window)?;
    view(&db)
}

#[tauri::command]
pub fn delete_silence_window(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<RadioDisciplineView, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, WINDOW_KIND, &id)?;
    view(&db)
}

/// Silence the radio for `minutes` from now (0 lifts a manual silence). For other features that
/// need to quiet the station, such as hunker-down.
pub fn silence_for_minutes(db: &crate::db::Database, minutes: u32) -> Result<(), String> {
    let until = (minutes > 0).then(|| chrono::Utc::now().timestamp() + i64::from(minutes.min(7 * 24 * 60)) * 60);
    store::put(db, OVERRIDE_KIND, Some(OVERRIDE_ID), &SilenceOverride { until })?;
    refresh_from_db(db);
    Ok(())
}

/// Silence the radio for `minutes` from now, or lift a manual silence with `None` or 0.
#[tauri::command]
pub fn set_radio_silence(
    minutes: Option<u32>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<RadioDisciplineView, String> {
    super::require_member(&session)?;
    let until = match minutes {
        Some(m) if m > 0 => Some(chrono::Utc::now().timestamp() + i64::from(m.min(7 * 24 * 60)) * 60),
        _ => None,
    };
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, OVERRIDE_KIND, Some(OVERRIDE_ID), &SilenceOverride { until })?;
    view(&db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(name: &str, start: u32, end: u32) -> SilenceWindow {
        SilenceWindow { name: name.to_string(), start_minute: start, end_minute: end, active: true }
    }

    #[test]
    fn daytime_window_covers_only_its_hours() {
        let w = window("Midday", 12 * 60, 14 * 60);
        assert!(!window_covers(&w, 11 * 60 + 59));
        assert!(window_covers(&w, 12 * 60));
        assert!(window_covers(&w, 13 * 60 + 59));
        assert!(!window_covers(&w, 14 * 60), "the end minute is outside the window");
    }

    #[test]
    fn overnight_window_wraps_past_midnight() {
        let w = window("Night", 22 * 60, 5 * 60);
        assert!(window_covers(&w, 23 * 60));
        assert!(window_covers(&w, 0));
        assert!(window_covers(&w, 4 * 60 + 59));
        assert!(!window_covers(&w, 5 * 60));
        assert!(!window_covers(&w, 12 * 60));
    }

    #[test]
    fn equal_start_and_end_means_all_day_and_inactive_never_applies() {
        assert!(window_covers(&window("Always", 0, 0), 777));
        let mut off = window("Off", 0, 0);
        off.active = false;
        assert!(!window_covers(&off, 777));
    }

    #[test]
    fn reason_names_the_window_and_when_it_ends() {
        let state = SilenceState { windows: vec![window("Night", 22 * 60, 5 * 60 + 30)], silent_until: None };
        assert_eq!(
            silence_reason(&state, 23 * 60, 0).as_deref(),
            Some("Radio silence \"Night\" is in effect until 05:30.")
        );
        assert_eq!(silence_reason(&state, 12 * 60, 0), None);
    }

    #[test]
    fn manual_silence_counts_down_and_expires() {
        let state = SilenceState { windows: vec![], silent_until: Some(10_000) };
        assert_eq!(
            silence_reason(&state, 0, 10_000 - 90).as_deref(),
            Some("Radio silence is on for another 2 minutes.")
        );
        assert_eq!(
            silence_reason(&state, 0, 10_000 - 30).as_deref(),
            Some("Radio silence is on for another 1 minute.")
        );
        assert_eq!(silence_reason(&state, 0, 10_000), None);
    }

    #[test]
    fn distress_traffic_is_never_held() {
        assert!(is_emergency("SOS need help at the bridge"));
        assert!(is_emergency("  sos clear"));
        assert!(!is_emergency("All quiet, no SOS here"));
        assert!(!is_emergency("status ok"));
    }
}
