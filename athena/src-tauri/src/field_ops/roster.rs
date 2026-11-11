//! Duty roster (S24): share watch, water, cooking and other duties fairly, with rest between shifts.
//!
//! The generator is deterministic: the same people, duties and options always give the same
//! roster, so two stations that hold the same data agree on who is on watch.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::AppState;

pub const DUTY_KIND: &str = "roster_duty";
pub const EXCLUSION_KIND: &str = "roster_exclusion";

pub const DEFAULT_MIN_REST_HOURS: f64 = 8.0;
/// Nobody is rostered for longer than this without a break, even when short-handed.
pub const MAX_CONTINUOUS_HOURS: f64 = 16.0;
/// A gap shorter than this does not count as a break.
const BREAK_HOURS: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Duty {
    pub name: String,
    /// Shifts are spaced evenly through the day: 3 means one every 8 hours.
    pub shifts_per_day: u32,
    pub people_per_shift: u32,
    /// Length of each shift. Left empty, shifts run back to back and cover the whole day (a
    /// watch). Set it for jobs that take a fixed time, such as a 2-hour water run.
    #[serde(default)]
    pub shift_hours: Option<f64>,
    /// Hour of day the first shift starts (0 to 24). Defaults to midnight.
    #[serde(default)]
    pub start_hour: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exclusion {
    pub person_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RosterPerson {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignedPerson {
    pub id: String,
    pub name: String,
    /// True when this person had less than the minimum rest before the shift.
    pub short_rest: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShiftAssignment {
    /// 0 = first day of the roster.
    pub day: u32,
    pub duty: String,
    pub shift: u32,
    /// Hour of day the shift starts, 0 to 24.
    pub start_hour: f64,
    pub end_hour: f64,
    pub people: Vec<AssignedPerson>,
    /// Places nobody could fill.
    pub unfilled: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonLoad {
    pub id: String,
    pub name: String,
    pub hours: f64,
    pub shifts: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Roster {
    pub days: u32,
    pub assignments: Vec<ShiftAssignment>,
    /// Heaviest load first.
    pub loads: Vec<PersonLoad>,
    pub unfilled_places: u32,
    pub short_rest_count: u32,
    /// Hours between the most and least loaded person. Small means fair.
    pub spread_hours: f64,
}

struct Slot {
    day: u32,
    duty_index: usize,
    shift: u32,
    start: f64,
    end: f64,
}

struct Worker {
    hours: f64,
    shifts: u32,
    last_end: Option<f64>,
    streak_start: f64,
}

/// Build a roster for `days` days.
pub fn generate_roster(
    people: &[RosterPerson],
    duties: &[Duty],
    days: u32,
    min_rest_hours: f64,
) -> Roster {
    let days = days.clamp(1, 31);
    let min_rest = min_rest_hours.clamp(0.0, 24.0);

    let mut slots: Vec<Slot> = Vec::new();
    for day in 0..days {
        for (duty_index, duty) in duties.iter().enumerate() {
            let n = duty.shifts_per_day.clamp(1, 24);
            let spacing = 24.0 / n as f64;
            let len = duty
                .shift_hours
                .filter(|h| h.is_finite() && *h > 0.0)
                .map(|h| h.min(spacing))
                .unwrap_or(spacing);
            let first = duty.start_hour.filter(|h| h.is_finite()).unwrap_or(0.0).clamp(0.0, 24.0);
            for shift in 0..n {
                let start = day as f64 * 24.0 + first + shift as f64 * spacing;
                slots.push(Slot { day, duty_index, shift, start, end: start + len });
            }
        }
    }
    slots.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.duty_index.cmp(&b.duty_index))
    });

    let mut workers: Vec<Worker> = people
        .iter()
        .map(|_| Worker { hours: 0.0, shifts: 0, last_end: None, streak_start: 0.0 })
        .collect();
    let mut assignments = Vec::with_capacity(slots.len());
    let mut unfilled_places = 0u32;
    let mut short_rest_count = 0u32;

    for slot in &slots {
        let duty = &duties[slot.duty_index];
        let wanted = duty.people_per_shift.max(1) as usize;
        let mut chosen: Vec<(usize, bool)> = Vec::new();

        for _ in 0..wanted {
            // (index, rested) of the best available person.
            let mut best: Option<(usize, bool)> = None;
            for (i, w) in workers.iter().enumerate() {
                if chosen.iter().any(|(c, _)| *c == i) {
                    continue;
                }
                let (rested, continuous) = match w.last_end {
                    None => (true, false),
                    Some(end) => {
                        if end > slot.start + 1e-9 {
                            continue; // already on a shift that overlaps this one
                        }
                        let gap = slot.start - end;
                        (gap + 1e-9 >= min_rest, gap < BREAK_HOURS)
                    }
                };
                if continuous && slot.end - w.streak_start > MAX_CONTINUOUS_HOURS + 1e-9 {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some((b, b_rested)) => {
                        let bw = &workers[b];
                        // Rested first, then least hours, then longest since last shift.
                        (!rested, w.hours, w.last_end.unwrap_or(f64::MIN))
                            < (!b_rested, bw.hours, bw.last_end.unwrap_or(f64::MIN))
                    }
                };
                if better {
                    best = Some((i, rested));
                }
            }
            match best {
                Some(pick) => chosen.push(pick),
                None => break,
            }
        }

        for (i, rested) in &chosen {
            let w = &mut workers[*i];
            let continues = w.last_end.map(|e| slot.start - e < BREAK_HOURS).unwrap_or(false);
            if !continues {
                w.streak_start = slot.start;
            }
            w.hours += slot.end - slot.start;
            w.shifts += 1;
            w.last_end = Some(slot.end);
            if !rested {
                short_rest_count += 1;
            }
        }
        let unfilled = (wanted - chosen.len()) as u32;
        unfilled_places += unfilled;
        assignments.push(ShiftAssignment {
            day: slot.day,
            duty: duty.name.clone(),
            shift: slot.shift,
            start_hour: slot.start - slot.day as f64 * 24.0,
            end_hour: slot.end - slot.day as f64 * 24.0,
            people: chosen
                .iter()
                .map(|(i, rested)| AssignedPerson {
                    id: people[*i].id.clone(),
                    name: people[*i].name.clone(),
                    short_rest: !rested,
                })
                .collect(),
            unfilled,
        });
    }

    let mut loads: Vec<PersonLoad> = people
        .iter()
        .zip(workers.iter())
        .map(|(p, w)| PersonLoad { id: p.id.clone(), name: p.name.clone(), hours: w.hours, shifts: w.shifts })
        .collect();
    let max = loads.iter().map(|l| l.hours).fold(0.0, f64::max);
    let min = loads.iter().map(|l| l.hours).fold(f64::MAX, f64::min);
    loads.sort_by(|a, b| {
        b.hours
            .partial_cmp(&a.hours)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });
    Roster {
        days,
        assignments,
        spread_hours: if loads.is_empty() { 0.0 } else { max - min },
        loads,
        unfilled_places,
        short_rest_count,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterSetup {
    pub duties: Vec<Record<Duty>>,
    pub exclusions: Vec<Record<Exclusion>>,
}

#[tauri::command]
pub fn get_roster_setup(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<RosterSetup, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    Ok(RosterSetup {
        duties: store::list(&db, DUTY_KIND)?,
        exclusions: store::list(&db, EXCLUSION_KIND)?,
    })
}

#[tauri::command]
pub fn save_roster_duty(
    id: Option<String>,
    mut duty: Duty,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<Duty>, String> {
    super::require_member(&session)?;
    duty.name = duty.name.trim().to_string();
    if duty.name.is_empty() {
        return Err("Name the duty.".to_string());
    }
    if !(1..=24).contains(&duty.shifts_per_day) {
        return Err("Shifts per day must be between 1 and 24.".to_string());
    }
    if !(1..=50).contains(&duty.people_per_shift) {
        return Err("People per shift must be between 1 and 50.".to_string());
    }
    if let Some(h) = duty.shift_hours {
        if !h.is_finite() || h <= 0.0 || h > 24.0 {
            return Err("Shift length must be between a few minutes and 24 hours.".to_string());
        }
    }
    if let Some(h) = duty.start_hour {
        if !h.is_finite() || !(0.0..24.0).contains(&h) {
            return Err("Start hour must be between 0 and 23.".to_string());
        }
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, DUTY_KIND, id.as_deref(), &duty)
}

#[tauri::command]
pub fn delete_roster_duty(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, DUTY_KIND, &id)
}

/// Excuse a person from the roster (ill, injured, away), or put them back.
#[tauri::command]
pub fn set_roster_exclusion(
    person_id: String,
    excluded: bool,
    reason: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let existing: Vec<Record<Exclusion>> = store::list(&db, EXCLUSION_KIND)?;
    for rec in existing.iter().filter(|r| r.data.person_id == person_id) {
        store::remove(&db, EXCLUSION_KIND, &rec.id)?;
    }
    if excluded {
        store::put(&db, EXCLUSION_KIND, None, &Exclusion { person_id, reason })?;
    }
    Ok(())
}

#[tauri::command]
pub fn generate_duty_roster(
    days: Option<u32>,
    min_rest_hours: Option<f64>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Roster, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let excluded: std::collections::HashSet<String> = store::list::<Exclusion>(&db, EXCLUSION_KIND)?
        .into_iter()
        .map(|r| r.data.person_id)
        .collect();
    let mut people: Vec<RosterPerson> = db
        .personnel_list_public()?
        .into_iter()
        .filter(|p| p.clearance_level > 0 && !excluded.contains(&p.id))
        .map(|p| RosterPerson {
            name: p.display_name.clone().filter(|s| !s.trim().is_empty()).unwrap_or(p.name),
            id: p.id,
        })
        .collect();
    people.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    let duties: Vec<Duty> = store::list::<Duty>(&db, DUTY_KIND)?.into_iter().map(|r| r.data).collect();
    if duties.is_empty() {
        return Err("Add at least one duty first.".to_string());
    }
    if people.is_empty() {
        return Err("Nobody is available for the roster.".to_string());
    }
    Ok(generate_roster(
        &people,
        &duties,
        days.unwrap_or(7),
        min_rest_hours.unwrap_or(DEFAULT_MIN_REST_HOURS),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people(names: &[&str]) -> Vec<RosterPerson> {
        names.iter().map(|n| RosterPerson { id: n.to_lowercase(), name: n.to_string() }).collect()
    }

    fn duty(name: &str, shifts: u32, per_shift: u32) -> Duty {
        Duty {
            name: name.to_string(),
            shifts_per_day: shifts,
            people_per_shift: per_shift,
            shift_hours: None,
            start_hour: None,
        }
    }

    fn timed(name: &str, shifts: u32, per_shift: u32, hours: f64, start: f64) -> Duty {
        Duty { shift_hours: Some(hours), start_hour: Some(start), ..duty(name, shifts, per_shift) }
    }

    /// (person id, absolute start hour, absolute end hour) for every assignment.
    fn spans(r: &Roster) -> Vec<(String, f64, f64)> {
        r.assignments
            .iter()
            .flat_map(|a| {
                a.people.iter().map(move |p| {
                    (p.id.clone(), a.day as f64 * 24.0 + a.start_hour, a.day as f64 * 24.0 + a.end_hour)
                })
            })
            .collect()
    }

    #[test]
    fn shares_a_round_the_clock_watch_evenly() {
        // 4 people, three 8-hour watches a day, 4 days = 12 shifts = 3 each.
        let r = generate_roster(&people(&["Ana", "Ben", "Cy", "Di"]), &[duty("Watch", 3, 1)], 4, 8.0);
        assert_eq!(r.assignments.len(), 12);
        assert_eq!(r.unfilled_places, 0);
        assert_eq!(r.short_rest_count, 0);
        assert!(r.loads.iter().all(|l| l.shifts == 3 && (l.hours - 24.0).abs() < 1e-9));
        assert_eq!(r.spread_hours, 0.0);
    }

    #[test]
    fn nobody_is_double_booked_and_rest_is_respected() {
        let r = generate_roster(
            &people(&["Ana", "Ben", "Cy", "Di", "Eve", "Flo"]),
            &[duty("Watch", 3, 1), timed("Water", 1, 2, 2.0, 7.0), timed("Cooking", 2, 1, 2.0, 6.0)],
            7,
            8.0,
        );
        assert_eq!(r.unfilled_places, 0);
        let mut by_person: std::collections::HashMap<String, Vec<(f64, f64)>> = Default::default();
        for (id, s, e) in spans(&r) {
            by_person.entry(id).or_default().push((s, e));
        }
        for (id, mut list) in by_person {
            list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for pair in list.windows(2) {
                assert!(pair[1].0 >= pair[0].1 - 1e-9, "{id} is double-booked: {pair:?}");
            }
        }
    }

    #[test]
    fn fills_every_place_on_each_shift() {
        let r = generate_roster(&people(&["Ana", "Ben", "Cy", "Di"]), &[duty("Watch", 2, 2)], 1, 8.0);
        assert_eq!(r.assignments.len(), 2);
        assert!(r.assignments.iter().all(|a| a.people.len() == 2 && a.unfilled == 0));
        // 12-hour shifts with 4 people: the second shift is the other pair.
        let first: Vec<&str> = r.assignments[0].people.iter().map(|p| p.id.as_str()).collect();
        let second: Vec<&str> = r.assignments[1].people.iter().map(|p| p.id.as_str()).collect();
        assert!(first.iter().all(|p| !second.contains(p)));
    }

    #[test]
    fn short_handed_roster_reports_gaps_instead_of_overworking() {
        // One person cannot cover a 24-hour watch.
        let r = generate_roster(&people(&["Ana"]), &[duty("Watch", 3, 1)], 2, 8.0);
        assert!(r.unfilled_places > 0);
        let ana: Vec<(f64, f64)> = spans(&r).into_iter().map(|(_, s, e)| (s, e)).collect();
        // Never more than 16 hours without a break.
        let mut streak_start = ana[0].0;
        for pair in ana.windows(2) {
            if pair[1].0 - pair[0].1 >= 1.0 {
                streak_start = pair[1].0;
            }
            assert!(pair[1].1 - streak_start <= MAX_CONTINUOUS_HOURS + 1e-9);
        }
        assert!(r.short_rest_count > 0, "back-to-back shifts are flagged");
    }

    #[test]
    fn too_few_people_leaves_places_unfilled() {
        let r = generate_roster(&people(&["Ana", "Ben"]), &[duty("Patrol", 1, 3)], 1, 8.0);
        assert_eq!(r.assignments[0].people.len(), 2);
        assert_eq!(r.assignments[0].unfilled, 1);
        assert_eq!(r.unfilled_places, 1);
    }

    #[test]
    fn timed_duties_start_when_told_and_last_as_long_as_told() {
        let r = generate_roster(&people(&["Ana", "Ben", "Cy"]), &[timed("Cooking", 2, 1, 2.0, 6.0)], 1, 8.0);
        let hours: Vec<(f64, f64)> = r.assignments.iter().map(|a| (a.start_hour, a.end_hour)).collect();
        assert_eq!(hours, vec![(6.0, 8.0), (18.0, 20.0)]);
        assert!((r.loads.iter().map(|l| l.hours).sum::<f64>() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn same_inputs_give_the_same_roster() {
        let p = people(&["Ana", "Ben", "Cy"]);
        let d = [duty("Watch", 3, 1), duty("Cooking", 1, 1)];
        assert_eq!(generate_roster(&p, &d, 5, 8.0), generate_roster(&p, &d, 5, 8.0));
    }

    #[test]
    fn shift_hours_are_reported_within_the_day() {
        let r = generate_roster(&people(&["Ana", "Ben", "Cy", "Di"]), &[duty("Watch", 3, 1)], 2, 8.0);
        let day2: Vec<(f64, f64)> = r.assignments.iter().filter(|a| a.day == 1).map(|a| (a.start_hour, a.end_hour)).collect();
        assert_eq!(day2, vec![(0.0, 8.0), (8.0, 16.0), (16.0, 24.0)]);
    }
}
