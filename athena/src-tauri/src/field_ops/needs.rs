//! Needs-based runway (S5): days of water, food and fuel for the number of people in the group.
//!
//! The observed-use runway (`crate::runway`) needs history. This one needs none: tell it what a
//! stocked item contains ("one jug = 20 litres of water") and it divides the total by what the
//! group needs each day.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const PROFILE_KIND: &str = "supply_profile";
pub const SETTINGS_KIND: &str = "needs_settings";
const SETTINGS_ID: &str = "needs_settings";

/// Planning figure for drinking and basic food preparation: about one US gallon per person per
/// day, the amount emergency-preparedness guidance commonly uses. Hot weather, illness and
/// hygiene need more; change it in settings.
pub const DEFAULT_WATER_L_PER_PERSON_DAY: f64 = 3.8;
/// General reference energy intake for an adult. Heavy labour and cold need more.
pub const DEFAULT_FOOD_KCAL_PER_PERSON_DAY: f64 = 2000.0;

const CRITICAL_DAYS: f64 = 7.0;
const LOW_DAYS: f64 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Water,
    Food,
    Fuel,
}

impl Resource {
    pub fn unit(self) -> &'static str {
        match self {
            Resource::Water => "litres",
            Resource::Food => "kcal",
            Resource::Fuel => "litres",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Resource::Water => "Water",
            Resource::Food => "Food",
            Resource::Fuel => "Fuel",
        }
    }
}

/// What one unit of an inventory item contains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyProfile {
    pub item_id: String,
    pub resource: Resource,
    /// Litres of water, kcal of food or litres of fuel in one counted unit of the item.
    pub amount_per_unit: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsSettings {
    pub water_l_per_person_day: f64,
    pub food_kcal_per_person_day: f64,
    /// Litres of fuel the group burns a day (generator, stove, vehicles). 0 = not tracked.
    pub fuel_l_per_day: f64,
}

impl Default for NeedsSettings {
    fn default() -> Self {
        Self {
            water_l_per_person_day: DEFAULT_WATER_L_PER_PERSON_DAY,
            food_kcal_per_person_day: DEFAULT_FOOD_KCAL_PER_PERSON_DAY,
            fuel_l_per_day: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedStatus {
    /// Nothing counted for this resource yet.
    Untracked,
    Out,
    Critical,
    Low,
    Ok,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRunway {
    pub resource: Resource,
    pub label: &'static str,
    pub unit: &'static str,
    /// Total on hand across every item counted towards this resource.
    pub total: f64,
    /// What the whole group needs per day.
    pub daily_need: f64,
    pub days_left: Option<f64>,
    pub status: NeedStatus,
    /// Inventory items counted towards this resource.
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsReport {
    pub headcount: i64,
    pub resources: Vec<ResourceRunway>,
}

/// `stock` is (item id, quantity on hand).
pub fn build_needs_report(
    stock: &[(String, i64)],
    profiles: &[SupplyProfile],
    settings: &NeedsSettings,
    headcount: i64,
) -> NeedsReport {
    let people = headcount.max(1) as f64;
    let resources = [Resource::Water, Resource::Food, Resource::Fuel]
        .into_iter()
        .map(|resource| {
            let mut total = 0.0;
            let mut item_count = 0usize;
            for p in profiles.iter().filter(|p| p.resource == resource) {
                if !p.amount_per_unit.is_finite() || p.amount_per_unit <= 0.0 {
                    continue;
                }
                if let Some((_, qty)) = stock.iter().find(|(id, _)| *id == p.item_id) {
                    total += (*qty).max(0) as f64 * p.amount_per_unit;
                    item_count += 1;
                }
            }
            let daily_need = match resource {
                Resource::Water => settings.water_l_per_person_day.max(0.0) * people,
                Resource::Food => settings.food_kcal_per_person_day.max(0.0) * people,
                Resource::Fuel => settings.fuel_l_per_day.max(0.0),
            };
            let days_left = (item_count > 0 && daily_need > 0.0).then(|| total / daily_need);
            let status = match (item_count, days_left) {
                (0, _) => NeedStatus::Untracked,
                (_, None) => NeedStatus::Untracked,
                (_, Some(_)) if total <= 0.0 => NeedStatus::Out,
                (_, Some(d)) if d < CRITICAL_DAYS => NeedStatus::Critical,
                (_, Some(d)) if d < LOW_DAYS => NeedStatus::Low,
                _ => NeedStatus::Ok,
            };
            ResourceRunway {
                resource,
                label: resource.label(),
                unit: resource.unit(),
                total,
                daily_need,
                days_left,
                status,
                item_count,
            }
        })
        .collect();
    NeedsReport { headcount: headcount.max(1), resources }
}

/// Watch officer rules: water, food or fuel that will not last.
pub fn findings(report: &NeedsReport) -> Vec<Finding> {
    report
        .resources
        .iter()
        .filter_map(|r| {
            let (severity, phrase) = match r.status {
                NeedStatus::Out => (Severity::Critical, "has run out".to_string()),
                NeedStatus::Critical => (
                    Severity::Critical,
                    format!("lasts {} more day{}", r.days_left.unwrap_or(0.0).floor(), if r.days_left.unwrap_or(0.0).floor() == 1.0 { "" } else { "s" }),
                ),
                NeedStatus::Low => (Severity::Warning, format!("lasts about {} more days", r.days_left.unwrap_or(0.0).round())),
                _ => return None,
            };
            Some(Finding {
                id: format!("needs.{}", r.label.to_lowercase()),
                severity,
                area: "supplies",
                title: format!("All {} together {phrase}", r.label.to_lowercase()),
                detail: format!(
                    "{:.0} {} on hand across {} counted item{}; {} people need {:.0} {} a day.",
                    r.total, r.unit, r.item_count, if r.item_count == 1 { "" } else { "s" }, report.headcount, r.daily_need, r.unit
                ),
                go_to: Some("field_ops"),
            })
        })
        .collect()
}

pub fn load_settings(db: &crate::db::Database) -> NeedsSettings {
    store::get::<NeedsSettings>(db, SETTINGS_KIND, SETTINGS_ID)
        .ok()
        .flatten()
        .map(|r| r.data)
        .unwrap_or_default()
}

pub fn report_from_db(db: &crate::db::Database) -> NeedsReport {
    let stock: Vec<(String, i64)> = db
        .inventory_list()
        .unwrap_or_default()
        .into_iter()
        .map(|i| (i.id, i.quantity))
        .collect();
    let profiles: Vec<SupplyProfile> = store::list::<SupplyProfile>(db, PROFILE_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect();
    build_needs_report(&stock, &profiles, &load_settings(db), db.personnel_count().unwrap_or(1))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryChoice {
    pub id: String,
    pub name: String,
    pub quantity: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsOverview {
    pub report: NeedsReport,
    pub settings: NeedsSettings,
    pub profiles: Vec<Record<SupplyProfile>>,
    pub inventory: Vec<InventoryChoice>,
}

#[tauri::command]
pub fn get_needs_overview(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<NeedsOverview, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut inventory: Vec<InventoryChoice> = db
        .inventory_list()?
        .into_iter()
        .map(|i| InventoryChoice { id: i.id, name: i.item_name, quantity: i.quantity })
        .collect();
    inventory.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(NeedsOverview {
        report: report_from_db(&db),
        settings: load_settings(&db),
        profiles: store::list(&db, PROFILE_KIND)?,
        inventory,
    })
}

#[tauri::command]
pub fn save_needs_settings(
    settings: NeedsSettings,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    super::require_member(&session)?;
    let ok = |v: f64, max: f64| v.is_finite() && (0.0..=max).contains(&v);
    if !ok(settings.water_l_per_person_day, 100.0)
        || !ok(settings.food_kcal_per_person_day, 10_000.0)
        || !ok(settings.fuel_l_per_day, 100_000.0)
    {
        return Err("One of those amounts is out of range.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, SETTINGS_KIND, Some(SETTINGS_ID), &settings).map(|_| ())
}

/// Say what one unit of an inventory item contains. One profile per item; saving replaces it.
#[tauri::command]
pub fn save_supply_profile(
    profile: SupplyProfile,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<SupplyProfile>, String> {
    super::require_member(&session)?;
    if profile.item_id.trim().is_empty() {
        return Err("Choose an inventory item.".to_string());
    }
    if !profile.amount_per_unit.is_finite() || profile.amount_per_unit <= 0.0 {
        return Err("Amount per unit must be more than zero.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    for old in store::list::<SupplyProfile>(&db, PROFILE_KIND)?.into_iter().filter(|r| r.data.item_id == profile.item_id) {
        store::remove(&db, PROFILE_KIND, &old.id)?;
    }
    store::put(&db, PROFILE_KIND, None, &profile)
}

#[tauri::command]
pub fn delete_supply_profile(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, PROFILE_KIND, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(item: &str, resource: Resource, amount: f64) -> SupplyProfile {
        SupplyProfile { item_id: item.to_string(), resource, amount_per_unit: amount }
    }

    fn stock(items: &[(&str, i64)]) -> Vec<(String, i64)> {
        items.iter().map(|(id, q)| (id.to_string(), *q)).collect()
    }

    fn get(report: &NeedsReport, r: Resource) -> &ResourceRunway {
        report.resources.iter().find(|x| x.resource == r).unwrap()
    }

    #[test]
    fn water_days_divide_total_litres_by_group_need() {
        // 10 jugs of 20 L = 200 L; 4 people at 3.8 L = 15.2 L a day; about 13.2 days.
        let r = build_needs_report(
            &stock(&[("jug", 10)]),
            &[profile("jug", Resource::Water, 20.0)],
            &NeedsSettings::default(),
            4,
        );
        let w = get(&r, Resource::Water);
        assert!((w.total - 200.0).abs() < 1e-9);
        assert!((w.daily_need - 15.2).abs() < 1e-9);
        assert!((w.days_left.unwrap() - 200.0 / 15.2).abs() < 1e-9);
        assert_eq!(w.status, NeedStatus::Low);
        assert_eq!(w.item_count, 1);
    }

    #[test]
    fn several_items_add_up_and_unprofiled_items_are_ignored() {
        let r = build_needs_report(
            &stock(&[("rice", 5), ("beans", 10), ("rope", 3)]),
            &[profile("rice", Resource::Food, 3600.0), profile("beans", Resource::Food, 1200.0)],
            &NeedsSettings::default(),
            2,
        );
        let f = get(&r, Resource::Food);
        assert!((f.total - 30_000.0).abs() < 1e-9);
        assert!((f.days_left.unwrap() - 7.5).abs() < 1e-9);
        assert_eq!(f.item_count, 2);
        assert_eq!(get(&r, Resource::Water).status, NeedStatus::Untracked);
    }

    #[test]
    fn fuel_is_untracked_until_a_daily_burn_is_set() {
        let profiles = [profile("can", Resource::Fuel, 20.0)];
        let s = stock(&[("can", 3)]);
        let r = build_needs_report(&s, &profiles, &NeedsSettings::default(), 4);
        assert_eq!(get(&r, Resource::Fuel).status, NeedStatus::Untracked);
        let settings = NeedsSettings { fuel_l_per_day: 4.0, ..Default::default() };
        let r = build_needs_report(&s, &profiles, &settings, 4);
        assert!((get(&r, Resource::Fuel).days_left.unwrap() - 15.0).abs() < 1e-9);
        // Fuel does not scale with headcount.
        let r8 = build_needs_report(&s, &profiles, &settings, 8);
        assert!((get(&r8, Resource::Fuel).days_left.unwrap() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn empty_stock_is_out_and_short_stock_is_critical() {
        let p = [profile("jug", Resource::Water, 20.0)];
        let r = build_needs_report(&stock(&[("jug", 0)]), &p, &NeedsSettings::default(), 4);
        assert_eq!(get(&r, Resource::Water).status, NeedStatus::Out);
        let r = build_needs_report(&stock(&[("jug", 2)]), &p, &NeedsSettings::default(), 4);
        assert_eq!(get(&r, Resource::Water).status, NeedStatus::Critical);
    }

    #[test]
    fn more_people_means_fewer_days() {
        let p = [profile("jug", Resource::Water, 20.0)];
        let s = stock(&[("jug", 10)]);
        let four = build_needs_report(&s, &p, &NeedsSettings::default(), 4);
        let eight = build_needs_report(&s, &p, &NeedsSettings::default(), 8);
        let ratio = get(&four, Resource::Water).days_left.unwrap() / get(&eight, Resource::Water).days_left.unwrap();
        assert!((ratio - 2.0).abs() < 1e-9);
    }

    #[test]
    fn watch_findings_cover_only_resources_that_will_not_last() {
        let r = build_needs_report(
            &stock(&[("jug", 2), ("rice", 100)]),
            &[profile("jug", Resource::Water, 20.0), profile("rice", Resource::Food, 3600.0)],
            &NeedsSettings::default(),
            4,
        );
        let f = findings(&r);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "needs.water");
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[0].title, "All water together lasts 2 more days");
        assert_eq!(f[0].detail, "40 litres on hand across 1 counted item; 4 people need 15 litres a day.");
    }
}
