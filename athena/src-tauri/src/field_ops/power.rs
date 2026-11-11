//! Power budget (S5): does the solar and battery setup keep up with what is plugged in?
//!
//! All figures are energy per day in watt-hours. The model is deliberately simple: average
//! generation from panel size and sun hours, average use from each load's watts and hours.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const SYSTEM_KIND: &str = "power_system";
pub const LOAD_KIND: &str = "power_load";
const SYSTEM_ID: &str = "power_system";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerSystem {
    /// Rated battery capacity in watt-hours (amp-hours × volts).
    pub battery_wh: f64,
    /// Share of the battery that can be used without damaging it, 0 to 1. Lead-acid is usually
    /// run to about half; lithium iron phosphate to 80% or more.
    pub usable_fraction: f64,
    /// Current state of charge, 0 to 100.
    pub charge_percent: f64,
    /// Total rated panel power in watts.
    pub panel_watts: f64,
    /// Hours a day the panels produce the equivalent of full rated output.
    pub sun_hours: f64,
    /// Share of panel output that reaches the loads after charge controller, wiring and
    /// inverter losses, 0 to 1.
    pub efficiency: f64,
}

impl Default for PowerSystem {
    fn default() -> Self {
        Self {
            battery_wh: 0.0,
            usable_fraction: 0.5,
            charge_percent: 100.0,
            panel_watts: 0.0,
            sun_hours: 4.0,
            efficiency: 0.75,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerLoad {
    pub name: String,
    pub watts: f64,
    pub hours_per_day: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadShare {
    pub name: String,
    pub wh_per_day: f64,
    /// Share of total daily use, 0 to 100.
    pub percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerBudget {
    pub generation_wh_per_day: f64,
    pub use_wh_per_day: f64,
    /// Generation minus use. Negative means the battery is being drained.
    pub net_wh_per_day: f64,
    /// Energy that can be drawn from the battery right now.
    pub stored_usable_wh: f64,
    /// Days the battery alone carries the loads with no sun at all.
    pub days_without_sun: Option<f64>,
    /// Days until the battery is empty at normal sun. `None` when generation covers use.
    pub days_until_empty: Option<f64>,
    pub sustainable: bool,
    /// Daily use that would have to be cut (or generated) to break even. 0 when sustainable.
    pub shortfall_wh_per_day: f64,
    /// Heaviest loads first.
    pub loads: Vec<LoadShare>,
}

fn clean(x: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        x
    } else {
        0.0
    }
}

pub fn build_power_budget(system: &PowerSystem, loads: &[PowerLoad]) -> PowerBudget {
    let generation = clean(system.panel_watts) * clean(system.sun_hours).min(24.0) * clean(system.efficiency).min(1.0);
    let mut shares: Vec<LoadShare> = loads
        .iter()
        .map(|l| LoadShare {
            name: l.name.clone(),
            wh_per_day: clean(l.watts) * clean(l.hours_per_day).min(24.0),
            percent: 0.0,
        })
        .collect();
    let use_total: f64 = shares.iter().map(|s| s.wh_per_day).sum();
    for s in &mut shares {
        s.percent = if use_total > 0.0 { s.wh_per_day / use_total * 100.0 } else { 0.0 };
    }
    shares.sort_by(|a, b| {
        b.wh_per_day
            .partial_cmp(&a.wh_per_day)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.name.cmp(&b.name))
    });

    let stored = clean(system.battery_wh)
        * clean(system.usable_fraction).min(1.0)
        * (clean(system.charge_percent).min(100.0) / 100.0);
    let net = generation - use_total;
    let sustainable = net >= 0.0;
    PowerBudget {
        generation_wh_per_day: generation,
        use_wh_per_day: use_total,
        net_wh_per_day: net,
        stored_usable_wh: stored,
        days_without_sun: (use_total > 0.0).then(|| stored / use_total),
        days_until_empty: (!sustainable).then(|| stored / -net),
        sustainable,
        shortfall_wh_per_day: if sustainable { 0.0 } else { -net },
        loads: shares,
    }
}

/// Watch officer rule: the battery is being drained and will be empty within a week.
pub fn findings(budget: &PowerBudget) -> Vec<Finding> {
    let Some(days) = budget.days_until_empty else {
        return Vec::new();
    };
    if budget.use_wh_per_day <= 0.0 || days >= 7.0 {
        return Vec::new();
    }
    let heaviest = budget
        .loads
        .first()
        .map(|l| format!(" The heaviest load is {} ({:.0}%).", l.name, l.percent))
        .unwrap_or_default();
    vec![Finding {
        id: "power.deficit".to_string(),
        severity: if days < 2.0 { Severity::Critical } else { Severity::Warning },
        area: "station",
        title: if days < 1.0 {
            "The battery will be empty in under a day".to_string()
        } else {
            format!("The battery will be empty in about {:.0} day{}", days.floor(), if days.floor() == 1.0 { "" } else { "s" })
        },
        detail: format!(
            "Using {:.0} Wh a day against {:.0} Wh generated. Cut {:.0} Wh a day to break even.{heaviest}",
            budget.use_wh_per_day, budget.generation_wh_per_day, budget.shortfall_wh_per_day
        ),
        go_to: Some("field_ops"),
    }]
}

pub fn load_system(db: &crate::db::Database) -> Option<PowerSystem> {
    store::get::<PowerSystem>(db, SYSTEM_KIND, SYSTEM_ID).ok().flatten().map(|r| r.data)
}

/// Budget from stored data. `None` until a power system has been entered.
pub fn budget_from_db(db: &crate::db::Database) -> Option<PowerBudget> {
    let system = load_system(db)?;
    let loads: Vec<PowerLoad> = store::list::<PowerLoad>(db, LOAD_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect();
    Some(build_power_budget(&system, &loads))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerOverview {
    pub system: PowerSystem,
    pub configured: bool,
    pub loads: Vec<Record<PowerLoad>>,
    pub budget: PowerBudget,
}

#[tauri::command]
pub fn get_power_overview(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<PowerOverview, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let stored = load_system(&db);
    let system = stored.clone().unwrap_or_default();
    let loads = store::list::<PowerLoad>(&db, LOAD_KIND)?;
    let plain: Vec<PowerLoad> = loads.iter().map(|r| r.data.clone()).collect();
    Ok(PowerOverview {
        budget: build_power_budget(&system, &plain),
        configured: stored.is_some(),
        system,
        loads,
    })
}

#[tauri::command]
pub fn save_power_system(
    system: PowerSystem,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    super::require_member(&session)?;
    let in_range = |v: f64, max: f64| v.is_finite() && (0.0..=max).contains(&v);
    if !in_range(system.battery_wh, 10_000_000.0) || !in_range(system.panel_watts, 1_000_000.0) {
        return Err("Battery or panel size is out of range.".to_string());
    }
    if !in_range(system.usable_fraction, 1.0) || !in_range(system.efficiency, 1.0) {
        return Err("Usable share and efficiency must be between 0 and 1.".to_string());
    }
    if !in_range(system.charge_percent, 100.0) {
        return Err("Charge must be between 0 and 100 percent.".to_string());
    }
    if !in_range(system.sun_hours, 24.0) {
        return Err("Sun hours must be between 0 and 24.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, SYSTEM_KIND, Some(SYSTEM_ID), &system).map(|_| ())
}

#[tauri::command]
pub fn save_power_load(
    id: Option<String>,
    mut load: PowerLoad,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<PowerLoad>, String> {
    super::require_member(&session)?;
    load.name = load.name.trim().to_string();
    if load.name.is_empty() {
        return Err("Name the device.".to_string());
    }
    if !load.watts.is_finite() || load.watts <= 0.0 || load.watts > 1_000_000.0 {
        return Err("Watts must be more than zero.".to_string());
    }
    if !load.hours_per_day.is_finite() || load.hours_per_day <= 0.0 || load.hours_per_day > 24.0 {
        return Err("Hours a day must be between 0 and 24.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, LOAD_KIND, id.as_deref(), &load)
}

#[tauri::command]
pub fn delete_power_load(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, LOAD_KIND, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system() -> PowerSystem {
        PowerSystem {
            battery_wh: 2400.0, // 200 Ah at 12 V
            usable_fraction: 0.5,
            charge_percent: 100.0,
            panel_watts: 400.0,
            sun_hours: 4.0,
            efficiency: 0.75,
        }
    }

    fn load(name: &str, watts: f64, hours: f64) -> PowerLoad {
        PowerLoad { name: name.to_string(), watts, hours_per_day: hours }
    }

    #[test]
    fn generation_use_and_net_are_daily_watt_hours() {
        // 400 W × 4 h × 0.75 = 1200 Wh; laptop 60 W × 10 h + radio 10 W × 24 h = 840 Wh.
        let b = build_power_budget(&system(), &[load("Laptop", 60.0, 10.0), load("Radio", 10.0, 24.0)]);
        assert!((b.generation_wh_per_day - 1200.0).abs() < 1e-9);
        assert!((b.use_wh_per_day - 840.0).abs() < 1e-9);
        assert!((b.net_wh_per_day - 360.0).abs() < 1e-9);
        assert!(b.sustainable);
        assert_eq!(b.days_until_empty, None);
        assert_eq!(b.shortfall_wh_per_day, 0.0);
    }

    #[test]
    fn stored_energy_respects_usable_share_and_charge() {
        let mut s = system();
        s.charge_percent = 50.0;
        let b = build_power_budget(&s, &[load("Radio", 10.0, 24.0)]);
        // 2400 × 0.5 usable × 0.5 charged = 600 Wh; 240 Wh a day = 2.5 days with no sun.
        assert!((b.stored_usable_wh - 600.0).abs() < 1e-9);
        assert!((b.days_without_sun.unwrap() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn a_deficit_drains_the_battery_and_names_the_shortfall() {
        // Freezer 100 W all day = 2400 Wh against 1200 Wh generated.
        let b = build_power_budget(&system(), &[load("Freezer", 100.0, 24.0), load("Laptop", 60.0, 10.0)]);
        assert!(!b.sustainable);
        assert!((b.shortfall_wh_per_day - 1800.0).abs() < 1e-9);
        // 1200 Wh usable / 1800 Wh daily deficit.
        assert!((b.days_until_empty.unwrap() - 1200.0 / 1800.0).abs() < 1e-9);
        assert_eq!(b.loads[0].name, "Freezer");
        assert!((b.loads[0].percent - 80.0).abs() < 1e-9);
    }

    #[test]
    fn no_loads_and_bad_numbers_do_not_divide_by_zero() {
        let b = build_power_budget(&system(), &[]);
        assert_eq!(b.use_wh_per_day, 0.0);
        assert_eq!(b.days_without_sun, None);
        assert!(b.sustainable);
        let b = build_power_budget(
            &PowerSystem { panel_watts: f64::NAN, sun_hours: -3.0, ..system() },
            &[load("Bad", f64::INFINITY, 5.0), load("Lamp", 5.0, 40.0)],
        );
        assert_eq!(b.generation_wh_per_day, 0.0);
        assert!((b.use_wh_per_day - 120.0).abs() < 1e-9, "hours are capped at 24 and non-finite watts ignored");
    }

    #[test]
    fn watch_finding_only_when_the_battery_empties_within_a_week() {
        let healthy = build_power_budget(&system(), &[load("Laptop", 60.0, 10.0)]);
        assert!(findings(&healthy).is_empty());

        let draining = build_power_budget(&system(), &[load("Freezer", 100.0, 24.0)]);
        let f = findings(&draining);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[0].title, "The battery will be empty in about 1 day");
        assert!(f[0].detail.starts_with("Using 2400 Wh a day against 1200 Wh generated. Cut 1200 Wh a day to break even."));
        assert!(f[0].detail.ends_with("The heaviest load is Freezer (100%)."));

        // A slow drain that lasts longer than a week is not raised.
        let slow = build_power_budget(&system(), &[load("Lights", 54.0, 24.0)]);
        assert!(!slow.sustainable);
        assert!(slow.days_until_empty.unwrap() > 7.0);
        assert!(findings(&slow).is_empty());
    }
}
