//! Supply runway: how many days each stocked item will last at the rate it is actually being used.
//!
//! The estimate needs no setup. It reads the inventory change log, measures how fast each item has
//! been going down, and projects when it reaches zero. Restocks and corrections that raise a
//! quantity are ignored; only decreases count as use.
//!
//! This module is pure (no database, no clock) so the arithmetic can be tested exactly.

use serde::Serialize;

use crate::auth::AuthSession;
use crate::AppState;
use tauri::State;

const SECONDS_PER_DAY: f64 = 86_400.0;
/// How far back use is measured.
pub const DEFAULT_WINDOW_DAYS: i64 = 30;
/// A burn rate from fewer observations than this is a guess, not a measurement.
const MIN_USE_EVENTS: usize = 2;
/// One observed draw-down is enough once it is at least this old.
const MIN_SPAN_DAYS_SINGLE_EVENT: f64 = 7.0;
/// Below this many days an item needs action now.
const CRITICAL_DAYS: f64 = 7.0;
/// Below this many days an item needs a plan.
const LOW_DAYS: f64 = 30.0;

#[derive(Debug, Clone)]
pub struct StockItem {
    pub id: String,
    pub name: String,
    pub category: String,
    pub division: String,
    pub quantity: i64,
}

#[derive(Debug, Clone)]
pub struct StockChange {
    pub item_id: String,
    pub old_qty: Option<i64>,
    pub new_qty: Option<i64>,
    /// Unix seconds.
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunwayStatus {
    /// Nothing left.
    Out,
    /// Under a week at the current rate.
    Critical,
    /// Under a month at the current rate.
    Low,
    Ok,
    /// Not enough usage history to estimate.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRunway {
    pub id: String,
    pub name: String,
    pub category: String,
    pub division: String,
    pub quantity: i64,
    /// Units used per day over the measurement window, after any what-if scaling.
    pub daily_use: Option<f64>,
    pub days_left: Option<f64>,
    /// Unix seconds when the item is projected to reach zero.
    pub runs_out_at: Option<i64>,
    pub status: RunwayStatus,
    /// Number of draw-downs the estimate is based on.
    pub use_events: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunwayReport {
    /// Soonest to run out first; items with no estimate last.
    pub items: Vec<ItemRunway>,
    pub window_days: i64,
    /// People the estimate is scaled for.
    pub headcount: i64,
    /// People the usage history reflects.
    pub baseline_headcount: i64,
    pub out_count: usize,
    pub critical_count: usize,
    pub low_count: usize,
    /// Items with stock but too little history to estimate.
    pub unknown_count: usize,
}

/// Units used per day for one item, or `None` when history is too thin to trust.
fn observed_daily_use(
    item_id: &str,
    changes: &[StockChange],
    now: i64,
    window_days: i64,
) -> (Option<f64>, usize) {
    let window_start = now - window_days * SECONDS_PER_DAY as i64;
    let mut used: i64 = 0;
    let mut events = 0usize;
    let mut earliest = now;
    for c in changes {
        if c.item_id != item_id || c.timestamp < window_start || c.timestamp > now {
            continue;
        }
        let (Some(old), Some(new)) = (c.old_qty, c.new_qty) else {
            continue;
        };
        if new < old {
            used += old - new;
            events += 1;
            earliest = earliest.min(c.timestamp);
        }
    }
    if events == 0 || used <= 0 {
        return (None, events);
    }
    let span_days = ((now - earliest) as f64 / SECONDS_PER_DAY).max(1.0);
    if events < MIN_USE_EVENTS && span_days < MIN_SPAN_DAYS_SINGLE_EVENT {
        return (None, events);
    }
    (Some(used as f64 / span_days), events)
}

fn status_for(quantity: i64, days_left: Option<f64>) -> RunwayStatus {
    if quantity <= 0 {
        return RunwayStatus::Out;
    }
    match days_left {
        None => RunwayStatus::Unknown,
        Some(d) if d < CRITICAL_DAYS => RunwayStatus::Critical,
        Some(d) if d < LOW_DAYS => RunwayStatus::Low,
        Some(_) => RunwayStatus::Ok,
    }
}

/// Build the runway report.
///
/// `baseline_headcount` is how many people the usage history reflects; `headcount` is how many
/// to plan for. Use scales linearly between them, which answers "what if four more arrive?".
pub fn build_runway_report(
    items: &[StockItem],
    changes: &[StockChange],
    now: i64,
    window_days: i64,
    baseline_headcount: i64,
    headcount: i64,
) -> RunwayReport {
    let window_days = window_days.clamp(1, 365);
    let baseline = baseline_headcount.max(1);
    let planned = headcount.max(1);
    let people_factor = planned as f64 / baseline as f64;

    let mut out: Vec<ItemRunway> = items
        .iter()
        .map(|item| {
            let (observed, use_events) = observed_daily_use(&item.id, changes, now, window_days);
            let daily_use = observed.map(|u| u * people_factor);
            let days_left = match daily_use {
                Some(u) if u > 0.0 && item.quantity > 0 => Some(item.quantity as f64 / u),
                Some(_) if item.quantity <= 0 => Some(0.0),
                _ => None,
            };
            let runs_out_at = days_left.map(|d| now + (d * SECONDS_PER_DAY) as i64);
            ItemRunway {
                id: item.id.clone(),
                name: item.name.clone(),
                category: item.category.clone(),
                division: item.division.clone(),
                quantity: item.quantity,
                daily_use,
                days_left,
                runs_out_at,
                status: status_for(item.quantity, days_left),
                use_events,
            }
        })
        .collect();

    out.sort_by(|a, b| {
        let rank = |r: &ItemRunway| match r.status {
            RunwayStatus::Out => 0,
            RunwayStatus::Critical | RunwayStatus::Low | RunwayStatus::Ok => 1,
            RunwayStatus::Unknown => 2,
        };
        rank(a)
            .cmp(&rank(b))
            .then(
                a.days_left
                    .unwrap_or(f64::MAX)
                    .partial_cmp(&b.days_left.unwrap_or(f64::MAX))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let count = |s: RunwayStatus| out.iter().filter(|i| i.status == s).count();
    RunwayReport {
        out_count: count(RunwayStatus::Out),
        critical_count: count(RunwayStatus::Critical),
        low_count: count(RunwayStatus::Low),
        unknown_count: count(RunwayStatus::Unknown),
        items: out,
        window_days,
        headcount: planned,
        baseline_headcount: baseline,
    }
}

/// The inventory log stores milliseconds; older rows may hold seconds. Normalize to seconds.
pub fn log_timestamp_to_seconds(raw: i64) -> i64 {
    // Anything past year 5138 in seconds is a millisecond value.
    if raw > 100_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

/// Days each supply will last at its observed rate of use.
///
/// `extra_people` scales use for a what-if ("four more arrive" = 4, "two leave" = -2).
#[tauri::command]
pub fn get_supply_runway(
    extra_people: Option<i64>,
    window_days: Option<i64>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<RunwayReport, String> {
    crate::inventory::require_inventory_list(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let items: Vec<StockItem> = db
        .inventory_list()?
        .into_iter()
        .map(|r| StockItem {
            id: r.id,
            name: r.item_name,
            category: r.category,
            division: r.division,
            quantity: r.quantity,
        })
        .collect();
    let changes: Vec<StockChange> = db
        .inventory_log_list()?
        .into_iter()
        .map(|l| StockChange {
            item_id: l.item_id,
            old_qty: l.old_qty,
            new_qty: l.new_qty,
            timestamp: log_timestamp_to_seconds(l.timestamp),
        })
        .collect();
    let baseline = db.personnel_count().unwrap_or(1).max(1);
    let planned = (baseline + extra_people.unwrap_or(0)).max(1);
    Ok(build_runway_report(
        &items,
        &changes,
        chrono::Utc::now().timestamp(),
        window_days.unwrap_or(DEFAULT_WINDOW_DAYS),
        baseline,
        planned,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn item(id: &str, qty: i64) -> StockItem {
        StockItem {
            id: id.to_string(),
            name: id.to_string(),
            category: "general".to_string(),
            division: "LOGISTICS".to_string(),
            quantity: qty,
        }
    }

    fn change(id: &str, old: i64, new: i64, days_ago: i64) -> StockChange {
        StockChange {
            item_id: id.to_string(),
            old_qty: Some(old),
            new_qty: Some(new),
            timestamp: NOW - days_ago * DAY,
        }
    }

    fn report(items: &[StockItem], changes: &[StockChange]) -> RunwayReport {
        build_runway_report(items, changes, NOW, 30, 4, 4)
    }

    #[test]
    fn projects_days_left_from_steady_use() {
        // 20 units used over 10 days = 2 a day; 30 left = 15 days.
        let changes = vec![change("rice", 50, 40, 10), change("rice", 40, 30, 5)];
        let r = report(&[item("rice", 30)], &changes);
        let rice = &r.items[0];
        assert_eq!(rice.use_events, 2);
        assert!((rice.daily_use.unwrap() - 2.0).abs() < 1e-9);
        assert!((rice.days_left.unwrap() - 15.0).abs() < 1e-9);
        assert_eq!(rice.runs_out_at, Some(NOW + 15 * DAY));
        assert_eq!(rice.status, RunwayStatus::Low);
    }

    #[test]
    fn restocks_do_not_count_as_use() {
        let changes = vec![
            change("fuel", 20, 10, 10),
            change("fuel", 10, 60, 6), // restock
            change("fuel", 60, 50, 2),
        ];
        let r = report(&[item("fuel", 50)], &changes);
        // 20 used over 10 days = 2 a day; 50 left = 25 days.
        assert!((r.items[0].days_left.unwrap() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn thin_history_is_unknown_not_a_guess() {
        // One draw-down yesterday says nothing about the rate.
        let r = report(&[item("bandages", 40)], &[change("bandages", 45, 40, 1)]);
        assert_eq!(r.items[0].status, RunwayStatus::Unknown);
        assert!(r.items[0].days_left.is_none());
        assert_eq!(r.unknown_count, 1);
    }

    #[test]
    fn one_old_draw_down_is_enough() {
        // 10 used 10 days ago and nothing since: 1 a day.
        let r = report(&[item("salt", 20)], &[change("salt", 30, 20, 10)]);
        assert!((r.items[0].days_left.unwrap() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn changes_outside_the_window_are_ignored() {
        let r = report(&[item("old", 10)], &[change("old", 100, 10, 90), change("old", 110, 100, 95)]);
        assert_eq!(r.items[0].status, RunwayStatus::Unknown);
    }

    #[test]
    fn empty_stock_is_out_and_sorted_first() {
        let changes = vec![
            change("water", 20, 10, 10),
            change("water", 10, 0, 5),
            change("rice", 50, 40, 10),
            change("rice", 40, 30, 5),
            change("matches", 100, 99, 10),
            change("matches", 99, 98, 5),
        ];
        let r = report(&[item("rice", 30), item("spare", 5), item("matches", 98), item("water", 0)], &changes);
        let order: Vec<&str> = r.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, vec!["water", "rice", "matches", "spare"]);
        assert_eq!(r.items[0].status, RunwayStatus::Out);
        assert_eq!(r.out_count, 1);
        assert_eq!(r.low_count, 1);
        assert_eq!(r.items[2].status, RunwayStatus::Ok);
        assert_eq!(r.items[3].status, RunwayStatus::Unknown);
    }

    #[test]
    fn more_people_shorten_the_runway_proportionally() {
        let changes = vec![change("rice", 50, 40, 10), change("rice", 40, 30, 5)];
        let base = build_runway_report(&[item("rice", 30)], &changes, NOW, 30, 4, 4);
        let more = build_runway_report(&[item("rice", 30)], &changes, NOW, 30, 4, 8);
        assert!((base.items[0].days_left.unwrap() - 15.0).abs() < 1e-9);
        assert!((more.items[0].days_left.unwrap() - 7.5).abs() < 1e-9);
        assert_eq!(more.headcount, 8);
        assert_eq!(more.baseline_headcount, 4);
    }

    #[test]
    fn log_timestamps_in_milliseconds_are_normalized() {
        assert_eq!(log_timestamp_to_seconds(1_800_000_000_123), 1_800_000_000);
        assert_eq!(log_timestamp_to_seconds(1_800_000_000), 1_800_000_000);
    }

    #[test]
    fn under_a_week_is_critical() {
        let changes = vec![change("insulin", 20, 15, 10), change("insulin", 15, 10, 5)];
        let r = report(&[item("insulin", 3)], &changes);
        assert_eq!(r.items[0].status, RunwayStatus::Critical);
        assert_eq!(r.critical_count, 1);
    }
}
