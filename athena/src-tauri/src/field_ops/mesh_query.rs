//! Ask the base (S10): short commands a person can send from a bare radio to get facts back
//! from the station, with no AI model and no computer at their end.
//!
//! Commands start with `?`. Replies are written to fit in one or two LoRa messages.

use crate::db::Database;
use crate::runway::{ItemRunway, RunwayStatus};
use crate::watch_officer::{Finding, Severity};

use super::{checkins, needs, store};

/// Longest reply, in bytes. Two radio messages' worth.
pub const MAX_REPLY_BYTES: usize = 380;

#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    Help,
    /// `?inv rice`
    Inventory(String),
    /// `?runway`
    Runway,
    /// `?checkin [note]`
    Checkin(Option<String>),
    /// `?overdue`
    Overdue,
    /// `?brief`
    Brief,
    /// `?find how long to boil water`
    Find(String),
}

pub const HELP_TEXT: &str = "ATHENA: ?inv <item> | ?runway | ?checkin [note] | ?overdue | ?brief | ?find <question> | /ai <question> | SOS";

/// Parse a radio message. `None` means it is not a station command.
pub fn parse(text: &str) -> Option<Query> {
    let t = text.trim();
    let rest = t.strip_prefix('?')?;
    let mut parts = rest.trim_start().splitn(2, char::is_whitespace);
    let word = parts.next().unwrap_or("").to_ascii_lowercase();
    let arg = parts.next().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    Some(match (word.as_str(), arg) {
        ("" | "help" | "h", _) => Query::Help,
        ("inv" | "inventory" | "stock", Some(a)) => Query::Inventory(a),
        ("inv" | "inventory" | "stock", None) => Query::Help,
        ("runway" | "supplies" | "run", _) => Query::Runway,
        ("checkin" | "check" | "ci" | "ok", note) => Query::Checkin(note),
        ("overdue" | "roll" | "who", _) => Query::Overdue,
        ("brief" | "status", _) => Query::Brief,
        ("find" | "lib" | "search", Some(a)) => Query::Find(a),
        ("find" | "lib" | "search", None) => Query::Help,
        // An unknown `?word` gets the help text rather than being passed to the AI by accident.
        _ => Query::Help,
    })
}

/// Cut to a byte limit on a character boundary, marking the cut.
pub fn fit(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.saturating_sub(3);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", text[..end].trim_end())
}

pub fn answer_inventory(items: &[(String, i64)], term: &str) -> String {
    let needle = term.to_lowercase();
    let mut hits: Vec<&(String, i64)> = items.iter().filter(|(name, _)| name.to_lowercase().contains(&needle)).collect();
    if hits.is_empty() {
        return format!("No inventory item matches \"{term}\".");
    }
    hits.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    let shown: Vec<String> = hits.iter().take(6).map(|(name, qty)| format!("{name}: {qty}")).collect();
    let more = hits.len().saturating_sub(6);
    format!("{}{}", shown.join("; "), if more > 0 { format!("; +{more} more") } else { String::new() })
}

pub fn answer_runway(items: &[ItemRunway], needs_report: &needs::NeedsReport) -> String {
    let mut parts: Vec<String> = Vec::new();
    for r in &needs_report.resources {
        match r.status {
            needs::NeedStatus::Untracked => {}
            needs::NeedStatus::Out => parts.push(format!("{} OUT", r.label)),
            _ => parts.push(format!("{} {}d", r.label, r.days_left.unwrap_or(0.0).floor())),
        }
    }
    let out: Vec<&str> = items.iter().filter(|i| i.status == RunwayStatus::Out).map(|i| i.name.as_str()).take(4).collect();
    if !out.is_empty() {
        parts.push(format!("Out: {}", out.join(", ")));
    }
    let soon: Vec<String> = items
        .iter()
        .filter(|i| matches!(i.status, RunwayStatus::Critical | RunwayStatus::Low))
        .take(4)
        .map(|i| format!("{} {}d", i.name, i.days_left.unwrap_or(0.0).floor()))
        .collect();
    if !soon.is_empty() {
        parts.push(format!("Low: {}", soon.join(", ")));
    }
    if parts.is_empty() {
        "Supplies: nothing running low on record.".to_string()
    } else {
        parts.join(". ")
    }
}

pub fn answer_overdue(plans: &[(checkins::CheckinPlan, Option<i64>)], now: i64) -> String {
    let mut late: Vec<(String, i64)> = plans
        .iter()
        .map(|(p, heard)| (p, checkins::checkin_status(p, *heard, now)))
        .filter(|(_, s)| s.state == checkins::CheckinState::Overdue)
        .map(|(p, s)| (p.name.clone(), s.late_by_secs))
        .collect();
    if late.is_empty() {
        return "All check-ins are up to date.".to_string();
    }
    late.sort_by(|a, b| b.1.cmp(&a.1));
    let list: Vec<String> = late
        .iter()
        .take(6)
        .map(|(name, secs)| {
            if *secs < 5400 {
                format!("{name} {}m", (secs / 60).max(1))
            } else {
                format!("{name} {}h", secs / 3600)
            }
        })
        .collect();
    format!("Overdue: {}", list.join(", "))
}

pub fn answer_brief(headline: &str, findings: &[Finding]) -> String {
    let urgent: Vec<&str> = findings
        .iter()
        .filter(|f| f.severity == Severity::Critical)
        .take(3)
        .map(|f| f.title.as_str())
        .collect();
    if urgent.is_empty() {
        headline.to_string()
    } else {
        format!("{headline} {}", urgent.join(". "))
    }
}

pub fn answer_find(title: Option<&str>, passage: Option<&str>) -> String {
    match (title, passage) {
        (Some(t), Some(p)) => format!("{t}: {}", p.split_whitespace().collect::<Vec<_>>().join(" ")),
        _ => "Nothing in the station library matches that.".to_string(),
    }
}

/// Record a check-in for the plan tied to this radio. Returns the reply text.
fn do_checkin(db: &Database, node_id: i64, note: Option<String>, now: i64) -> String {
    let plans = store::list::<checkins::CheckinPlan>(db, checkins::KIND).unwrap_or_default();
    let Some(rec) = plans.into_iter().find(|r| r.data.node_id == Some(node_id)) else {
        return "This radio has no check-in plan at the station. Your message was still logged.".to_string();
    };
    let mut data = rec.data;
    data.last_manual_at = Some(now);
    data.last_note = note;
    let interval_h = data.interval_hours;
    match store::put(db, checkins::KIND, Some(&rec.id), &data) {
        Ok(_) => format!("Check-in recorded for {}. Next due in {interval_h} h.", data.name),
        Err(e) => format!("Check-in could not be saved: {e}"),
    }
}

/// Answer a query from station data. Never calls the chat model.
pub fn answer(db: &Database, node_id: i64, query: &Query, now: i64) -> String {
    let text = match query {
        Query::Help => HELP_TEXT.to_string(),
        Query::Inventory(term) => {
            let items: Vec<(String, i64)> = db
                .inventory_list()
                .unwrap_or_default()
                .into_iter()
                .map(|i| (i.item_name, i.quantity))
                .collect();
            answer_inventory(&items, term)
        }
        Query::Runway => {
            let items: Vec<crate::runway::StockItem> = db
                .inventory_list()
                .unwrap_or_default()
                .into_iter()
                .map(|r| crate::runway::StockItem {
                    id: r.id,
                    name: r.item_name,
                    category: r.category,
                    division: r.division,
                    quantity: r.quantity,
                })
                .collect();
            let changes: Vec<crate::runway::StockChange> = db
                .inventory_log_list()
                .unwrap_or_default()
                .into_iter()
                .map(|l| crate::runway::StockChange {
                    item_id: l.item_id,
                    old_qty: l.old_qty,
                    new_qty: l.new_qty,
                    timestamp: crate::runway::log_timestamp_to_seconds(l.timestamp),
                })
                .collect();
            let headcount = db.personnel_count().unwrap_or(1).max(1);
            let report = crate::runway::build_runway_report(
                &items,
                &changes,
                now,
                crate::runway::DEFAULT_WINDOW_DAYS,
                headcount,
                headcount,
            );
            answer_runway(&report.items, &needs::report_from_db(db))
        }
        Query::Checkin(note) => do_checkin(db, node_id, note.clone(), now),
        Query::Overdue => {
            let plans: Vec<(checkins::CheckinPlan, Option<i64>)> = checkins::load_with_radio(db)
                .into_iter()
                .map(|(r, heard)| (r.data, heard))
                .collect();
            answer_overdue(&plans, now)
        }
        Query::Brief => {
            let findings = super::watch_findings(db, now);
            answer_brief(&crate::watch_officer::headline(&findings), &findings)
        }
        Query::Find(question) => match db.hybrid_search(question) {
            Ok(hits) => {
                let top = hits.first();
                answer_find(
                    top.map(|h| h.display_title.as_deref().unwrap_or("Library")),
                    top.map(|h| h.expanded_text.as_str()),
                )
            }
            Err(_) => "The station library search is not available right now.".to_string(),
        },
    };
    fit(&text, MAX_REPLY_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_case_insensitively_with_arguments() {
        assert_eq!(parse("?inv rice"), Some(Query::Inventory("rice".into())));
        assert_eq!(parse("  ?INV   Water Jugs "), Some(Query::Inventory("Water Jugs".into())));
        assert_eq!(parse("?runway"), Some(Query::Runway));
        assert_eq!(parse("?checkin all quiet at the ridge"), Some(Query::Checkin(Some("all quiet at the ridge".into()))));
        assert_eq!(parse("?ok"), Some(Query::Checkin(None)));
        assert_eq!(parse("?overdue"), Some(Query::Overdue));
        assert_eq!(parse("?brief"), Some(Query::Brief));
        assert_eq!(parse("?find how long to boil water"), Some(Query::Find("how long to boil water".into())));
    }

    #[test]
    fn ordinary_messages_are_not_commands() {
        assert_eq!(parse("how are you?"), None);
        assert_eq!(parse("/ai what is the plan"), None);
        assert_eq!(parse("SOS at the bridge"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn bare_or_unknown_or_incomplete_commands_get_help() {
        assert_eq!(parse("?"), Some(Query::Help));
        assert_eq!(parse("?help"), Some(Query::Help));
        assert_eq!(parse("?inv"), Some(Query::Help));
        assert_eq!(parse("?find"), Some(Query::Help));
        assert_eq!(parse("?launch missiles"), Some(Query::Help));
        assert!(HELP_TEXT.len() <= 200, "help must fit one radio message");
    }

    #[test]
    fn inventory_answer_lists_matches_or_says_none() {
        let items = vec![("Rice 5kg".to_string(), 12), ("Brown rice".to_string(), 3), ("Rope".to_string(), 4)];
        assert_eq!(answer_inventory(&items, "RICE"), "Brown rice: 3; Rice 5kg: 12");
        assert_eq!(answer_inventory(&items, "fuel"), "No inventory item matches \"fuel\".");
    }

    #[test]
    fn long_inventory_answers_are_capped() {
        let items: Vec<(String, i64)> = (0..9).map(|i| (format!("Can {i}"), i)).collect();
        let a = answer_inventory(&items, "can");
        assert!(a.ends_with("; +3 more"), "{a}");
    }

    #[test]
    fn fit_cuts_on_a_character_boundary() {
        assert_eq!(fit("short", 10), "short");
        let cut = fit("ééééééééééé", 10);
        assert!(cut.len() <= 10 && cut.ends_with("..."), "{cut}");
        assert!(fit(&"x".repeat(500), MAX_REPLY_BYTES).len() <= MAX_REPLY_BYTES);
    }

    #[test]
    fn overdue_answer_orders_by_lateness() {
        let now = 1_800_000_000;
        let plan = |name: &str, interval: f64, hours_ago: f64| {
            (
                checkins::CheckinPlan {
                    name: name.to_string(),
                    node_id: None,
                    interval_hours: interval,
                    grace_minutes: 0,
                    active: true,
                    last_manual_at: Some(now - (hours_ago * 3600.0) as i64),
                    last_note: None,
                    started_at: None,
                },
                None,
            )
        };
        let plans = vec![plan("Patrol", 12.0, 13.0), plan("Farm", 12.0, 1.0), plan("Ridge", 6.0, 9.0)];
        assert_eq!(answer_overdue(&plans, now), "Overdue: Ridge 3h, Patrol 60m");
        assert_eq!(answer_overdue(&plans[1..2], now), "All check-ins are up to date.");
    }

    #[test]
    fn brief_answer_adds_urgent_titles_only() {
        let f = |sev, title: &str| Finding {
            id: title.to_string(),
            severity: sev,
            area: "supplies",
            title: title.to_string(),
            detail: String::new(),
            go_to: None,
        };
        let findings = vec![f(Severity::Critical, "Water has run out"), f(Severity::Warning, "Storage is nearly full")];
        assert_eq!(
            answer_brief("1 thing needs action now, and 1 to watch.", &findings),
            "1 thing needs action now, and 1 to watch. Water has run out"
        );
        assert_eq!(answer_brief("All clear.", &[]), "All clear.");
    }

    #[test]
    fn find_answer_gives_title_and_flattened_passage() {
        assert_eq!(
            answer_find(Some("Water Purification"), Some("Boil for\n one  minute.")),
            "Water Purification: Boil for one minute."
        );
        assert_eq!(answer_find(None, None), "Nothing in the station library matches that.");
    }

    #[test]
    fn checkin_over_radio_updates_the_plan_for_that_radio() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = Database::new(&dir.path().join("t.db")).expect("db");
        let now = 1_800_000_000;
        let plan = checkins::CheckinPlan {
            name: "Ridge".into(),
            node_id: Some(4242),
            interval_hours: 6.0,
            grace_minutes: 0,
            active: true,
            last_manual_at: None,
            last_note: None,
            started_at: Some(now - 100_000),
        };
        let rec = store::put(&db, checkins::KIND, None, &plan).unwrap();

        let reply = answer(&db, 4242, &Query::Checkin(Some("all quiet".into())), now);
        assert_eq!(reply, "Check-in recorded for Ridge. Next due in 6 h.");
        let saved = store::get::<checkins::CheckinPlan>(&db, checkins::KIND, &rec.id).unwrap().unwrap();
        assert_eq!(saved.data.last_manual_at, Some(now));
        assert_eq!(saved.data.last_note.as_deref(), Some("all quiet"));

        let stranger = answer(&db, 999, &Query::Checkin(None), now);
        assert!(stranger.starts_with("This radio has no check-in plan"), "{stranger}");
    }
}
