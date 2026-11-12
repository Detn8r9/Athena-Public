//! Patient charts (S4): who was treated, what was done and when, and how they are doing.
//!
//! A chart is a timeline. Guided protocols write their transcript into it; vitals, medication
//! given and free notes are added by hand. Nothing here interprets the numbers: it records them
//! and rejects only values that cannot be real, which are almost always typing mistakes.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::AppState;

pub const PATIENT_KIND: &str = "patient";
pub const ENTRY_KIND: &str = "chart_entry";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patient {
    pub name: String,
    /// Personnel id when the patient is a member of the group.
    #[serde(default)]
    pub person_id: Option<String>,
    #[serde(default)]
    pub allergies: Option<String>,
    #[serde(default)]
    pub blood_type: Option<String>,
    #[serde(default)]
    pub conditions: Option<String>,
    #[serde(default)]
    pub regular_medication: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vitals {
    /// Beats per minute.
    #[serde(default)]
    pub pulse: Option<f64>,
    /// Breaths per minute.
    #[serde(default)]
    pub breathing: Option<f64>,
    #[serde(default)]
    pub temp_c: Option<f64>,
    #[serde(default)]
    pub systolic: Option<f64>,
    #[serde(default)]
    pub diastolic: Option<f64>,
    /// Oxygen saturation, percent.
    #[serde(default)]
    pub spo2: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartEntry {
    pub patient_id: String,
    /// Unix seconds the thing happened (not when it was typed in).
    pub at: i64,
    /// `protocol`, `vitals`, `medication` or `note`.
    pub entry_type: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub vitals: Option<Vitals>,
    #[serde(default)]
    pub recorded_by: Option<String>,
}

/// Reject values that cannot be real. Returns a message naming the first bad field.
pub fn validate_vitals(v: &Vitals) -> Result<(), String> {
    fn check(name: &str, value: Option<f64>, min: f64, max: f64) -> Result<(), String> {
        match value {
            Some(x) if !x.is_finite() || x < min || x > max => {
                Err(format!("{name} of {x} is not a possible reading. Check for a typing mistake."))
            }
            _ => Ok(()),
        }
    }
    check("Pulse", v.pulse, 0.0, 300.0)?;
    check("Breathing rate", v.breathing, 0.0, 100.0)?;
    if let Some(t) = v.temp_c {
        if (68.0..=113.0).contains(&t) {
            let c = (t - 32.0) * 5.0 / 9.0;
            return Err(format!(
                "Temperature of {t} looks like Fahrenheit. Enter it in °C: {t} °F is {c:.1} °C."
            ));
        }
    }
    check("Temperature", v.temp_c, 20.0, 45.0)?;
    check("Systolic pressure", v.systolic, 30.0, 300.0)?;
    check("Diastolic pressure", v.diastolic, 10.0, 200.0)?;
    check("Oxygen saturation", v.spo2, 30.0, 100.0)?;
    if let (Some(s), Some(d)) = (v.systolic, v.diastolic) {
        if d >= s {
            return Err("Diastolic pressure must be lower than systolic. Check the two numbers are not swapped.".to_string());
        }
    }
    if v == &Vitals::default() {
        return Err("Enter at least one reading.".to_string());
    }
    Ok(())
}

/// One-line rendering of a set of vitals for the timeline.
pub fn vitals_line(v: &Vitals) -> String {
    fn num(x: f64) -> String {
        if x.fract() == 0.0 {
            format!("{x:.0}")
        } else {
            format!("{x:.1}")
        }
    }
    let mut parts = Vec::new();
    if let Some(p) = v.pulse {
        parts.push(format!("pulse {}", num(p)));
    }
    if let Some(b) = v.breathing {
        parts.push(format!("breathing {}/min", num(b)));
    }
    if let Some(t) = v.temp_c {
        parts.push(format!("temp {} °C", num(t)));
    }
    match (v.systolic, v.diastolic) {
        (Some(s), Some(d)) => parts.push(format!("BP {}/{}", num(s), num(d))),
        (Some(s), None) => parts.push(format!("systolic {}", num(s))),
        _ => {}
    }
    if let Some(o) = v.spo2 {
        parts.push(format!("SpO2 {}%", num(o)));
    }
    parts.join(", ")
}

/// Newest first; entries at the same moment keep the order they were recorded in.
pub fn sort_timeline(entries: &mut [Record<ChartEntry>]) {
    entries.sort_by(|a, b| b.data.at.cmp(&a.data.at).then(b.created_at.cmp(&a.created_at)));
}

#[tauri::command]
pub fn list_patients(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Record<Patient>>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut patients = store::list::<Patient>(&db, PATIENT_KIND)?;
    patients.sort_by(|a, b| a.data.name.to_lowercase().cmp(&b.data.name.to_lowercase()));
    Ok(patients)
}

#[tauri::command]
pub fn save_patient(
    id: Option<String>,
    mut patient: Patient,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<Patient>, String> {
    super::require_member(&session)?;
    patient.name = patient.name.trim().to_string();
    if patient.name.is_empty() {
        return Err("Enter the patient's name.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, PATIENT_KIND, id.as_deref(), &patient)
}

/// Delete a patient and their whole chart.
#[tauri::command]
pub fn delete_patient(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    for entry in store::list::<ChartEntry>(&db, ENTRY_KIND)?.into_iter().filter(|e| e.data.patient_id == id) {
        store::remove(&db, ENTRY_KIND, &entry.id)?;
    }
    store::remove(&db, PATIENT_KIND, &id)
}

#[tauri::command]
pub fn list_chart_entries(
    patient_id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Record<ChartEntry>>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut entries: Vec<Record<ChartEntry>> = store::list::<ChartEntry>(&db, ENTRY_KIND)?
        .into_iter()
        .filter(|e| e.data.patient_id == patient_id)
        .collect();
    sort_timeline(&mut entries);
    Ok(entries)
}

#[tauri::command]
pub fn add_chart_entry(
    mut entry: ChartEntry,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<ChartEntry>, String> {
    let user = super::require_member(&session)?;
    if !matches!(entry.entry_type.as_str(), "protocol" | "vitals" | "medication" | "note") {
        return Err("Unknown chart entry type.".to_string());
    }
    entry.text = entry.text.trim().to_string();
    if entry.entry_type == "vitals" {
        let v = entry.vitals.as_ref().ok_or("Enter at least one reading.")?;
        validate_vitals(v)?;
        if entry.text.is_empty() {
            entry.text = vitals_line(v);
        }
    } else {
        entry.vitals = None;
        if entry.text.is_empty() {
            return Err("Write what happened.".to_string());
        }
    }
    if entry.at <= 0 {
        entry.at = store::now();
    }
    entry.recorded_by = Some(user.display_name.clone().unwrap_or(user.name.clone()));
    let db = app.db.lock().map_err(|e| e.to_string())?;
    if store::get::<Patient>(&db, PATIENT_KIND, &entry.patient_id)?.is_none() {
        return Err("That patient no longer exists.".to_string());
    }
    store::put(&db, ENTRY_KIND, None, &entry)
}

#[tauri::command]
pub fn delete_chart_entry(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, ENTRY_KIND, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plausible_vitals_pass() {
        let v = Vitals { pulse: Some(110.0), breathing: Some(22.0), temp_c: Some(38.4), systolic: Some(95.0), diastolic: Some(60.0), spo2: Some(94.0) };
        assert!(validate_vitals(&v).is_ok());
        assert_eq!(vitals_line(&v), "pulse 110, breathing 22/min, temp 38.4 °C, BP 95/60, SpO2 94%");
    }

    #[test]
    fn impossible_readings_are_rejected_with_the_field_named() {
        let err = validate_vitals(&Vitals { temp_c: Some(98.6), ..Default::default() }).unwrap_err();
        assert!(err.starts_with("Temperature of 98.6"), "{err}");
        assert!(err.contains("37.0 °C"), "{err}");
        let err = validate_vitals(&Vitals { pulse: Some(720.0), ..Default::default() }).unwrap_err();
        assert!(err.starts_with("Pulse of 720"), "{err}");
        assert!(validate_vitals(&Vitals { spo2: Some(104.0), ..Default::default() }).is_err());
    }

    #[test]
    fn swapped_blood_pressure_is_caught() {
        let err = validate_vitals(&Vitals { systolic: Some(70.0), diastolic: Some(120.0), ..Default::default() }).unwrap_err();
        assert!(err.contains("swapped"));
    }

    #[test]
    fn an_empty_set_of_vitals_is_rejected() {
        assert_eq!(validate_vitals(&Vitals::default()).unwrap_err(), "Enter at least one reading.");
    }

    #[test]
    fn partial_vitals_render_only_what_was_taken() {
        assert_eq!(vitals_line(&Vitals { pulse: Some(88.0), ..Default::default() }), "pulse 88");
        assert_eq!(vitals_line(&Vitals { systolic: Some(120.0), ..Default::default() }), "systolic 120");
    }

    #[test]
    fn timeline_is_newest_first() {
        let rec = |id: &str, at: i64, created: i64| Record {
            id: id.to_string(),
            created_at: created,
            updated_at: created,
            data: ChartEntry { patient_id: "p".into(), at, entry_type: "note".into(), text: id.into(), vitals: None, recorded_by: None },
        };
        let mut entries = vec![rec("old", 100, 1), rec("new", 300, 2), rec("same-a", 200, 3), rec("same-b", 200, 4)];
        sort_timeline(&mut entries);
        let order: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(order, vec!["new", "same-b", "same-a", "old"]);
    }
}
