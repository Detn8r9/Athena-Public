//! Watch officer: looks across the station and reports what needs attention before anyone asks.
//!
//! Design rule: **rules detect, the model explains, people decide.**
//!
//! * Detection is plain logic over station data (`evaluate`). It cannot hallucinate, costs no
//!   battery, and works with no model installed.
//! * The model is only asked to turn the detected facts into a short spoken-style brief. It is
//!   given the facts and told to add none.
//! * Nothing here acts. Every finding is advice with a suggested place to go; anything that
//!   transmits, deletes, rations or reassigns people stays a human decision.

use serde::Serialize;
use tauri::{Manager, State};

use crate::auth::AuthSession;
use crate::ollama::{ChatMessage, ModelOptions};
use crate::runway::{self, ItemRunway, RunwayReport, RunwayStatus};
use crate::AppState;

const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
/// A registered node silent for longer than this is flagged.
pub const SILENT_NODE_HOURS: i64 = 24;
/// Nodes silent longer than this are treated as gone rather than overdue.
const STALE_NODE_DAYS: i64 = 14;
/// Events starting within this window are listed in the brief.
const UPCOMING_EVENT_HOURS: i64 = 24;
const GIB: u64 = 1024 * 1024 * 1024;
/// Individual supply items named per severity before the rest are summarised as a count.
const MAX_SUPPLY_ITEMS_NAMED: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Act now.
    Critical,
    /// Needs a decision soon.
    Warning,
    /// Good to know.
    Info,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable key, so the UI can remember a dismissal.
    pub id: String,
    pub severity: Severity,
    /// `supplies`, `people`, `schedule` or `station`.
    pub area: &'static str,
    pub title: String,
    pub detail: String,
    /// App view where this can be dealt with.
    pub go_to: Option<&'static str>,
}

#[derive(Debug, Clone, Default)]
pub struct NodeContact {
    pub node_id: i64,
    pub name: String,
    pub registered: bool,
    pub sos_active: bool,
    /// Unix seconds of the last message heard from this node.
    pub last_heard: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpcomingEvent {
    pub title: String,
    /// Unix seconds.
    pub start: i64,
    pub location: Option<String>,
}

/// Everything the rules look at. Built by the command from the database and system state.
#[derive(Debug, Clone, Default)]
pub struct StationSnapshot {
    /// Unix seconds.
    pub now: i64,
    pub runway: Option<RunwayReport>,
    pub nodes: Vec<NodeContact>,
    pub events: Vec<UpcomingEvent>,
    pub disk_free_bytes: Option<u64>,
    pub disk_total_bytes: Option<u64>,
    /// `Some(reason)` when no chat model can run.
    pub ai_unavailable: Option<String>,
    /// False when library search is running on placeholder embeddings.
    pub search_models_ready: bool,
    /// Findings from feature modules that keep their own rules (check-ins, skills, intake).
    pub extra: Vec<Finding>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn days_phrase(days: f64) -> String {
    if days < 1.0 {
        "less than a day".to_string()
    } else {
        let d = days.round() as i64;
        format!("{d} day{}", if d == 1 { "" } else { "s" })
    }
}

fn hours_phrase(seconds: i64) -> String {
    let hours = seconds / HOUR;
    if hours < 48 {
        format!("{hours} hours")
    } else {
        format!("{} days", hours / 24)
    }
}

fn supply_findings(report: &RunwayReport, out: &mut Vec<Finding>) {
    let named = |items: &[&ItemRunway], with_days: bool| -> String {
        let mut parts: Vec<String> = items
            .iter()
            .take(MAX_SUPPLY_ITEMS_NAMED)
            .map(|i| match (with_days, i.days_left) {
                (true, Some(d)) => format!("{} ({})", i.name, days_phrase(d)),
                _ => i.name.clone(),
            })
            .collect();
        if items.len() > MAX_SUPPLY_ITEMS_NAMED {
            parts.push(format!("and {} more", items.len() - MAX_SUPPLY_ITEMS_NAMED));
        }
        parts.join(", ")
    };
    let of = |s: RunwayStatus| -> Vec<&ItemRunway> {
        report.items.iter().filter(|i| i.status == s).collect()
    };

    let gone = of(RunwayStatus::Out);
    if !gone.is_empty() {
        out.push(Finding {
            id: "supplies.out".to_string(),
            severity: Severity::Critical,
            area: "supplies",
            title: format!("{} out of stock", plural(gone.len(), "item is", "items are")),
            detail: named(&gone, false),
            go_to: Some("logbook"),
        });
    }
    let critical = of(RunwayStatus::Critical);
    if !critical.is_empty() {
        out.push(Finding {
            id: "supplies.critical".to_string(),
            severity: Severity::Critical,
            area: "supplies",
            title: format!(
                "{} out within a week",
                plural(critical.len(), "item runs", "items run")
            ),
            detail: named(&critical, true),
            go_to: Some("logbook"),
        });
    }
    let low = of(RunwayStatus::Low);
    if !low.is_empty() {
        out.push(Finding {
            id: "supplies.low".to_string(),
            severity: Severity::Warning,
            area: "supplies",
            title: format!("{} out within a month", plural(low.len(), "item runs", "items run")),
            detail: named(&low, true),
            go_to: Some("logbook"),
        });
    }
}

fn people_findings(snapshot: &StationSnapshot, out: &mut Vec<Finding>) {
    for n in snapshot.nodes.iter().filter(|n| n.sos_active) {
        let heard = n
            .last_heard
            .map(|t| format!("Last heard {} ago.", hours_phrase((snapshot.now - t).max(0))))
            .unwrap_or_else(|| "No message on record.".to_string());
        out.push(Finding {
            id: format!("people.sos.{}", n.node_id),
            severity: Severity::Critical,
            area: "people",
            title: format!("SOS active: {}", n.name),
            detail: heard,
            go_to: Some("comms_hub"),
        });
    }

    let mut silent: Vec<(&NodeContact, i64)> = snapshot
        .nodes
        .iter()
        .filter(|n| n.registered && !n.sos_active)
        .filter_map(|n| n.last_heard.map(|t| (n, snapshot.now - t)))
        .filter(|(_, age)| *age > SILENT_NODE_HOURS * HOUR && *age <= STALE_NODE_DAYS * DAY)
        .collect();
    silent.sort_by(|a, b| b.1.cmp(&a.1));
    if !silent.is_empty() {
        let detail = silent
            .iter()
            .take(5)
            .map(|(n, age)| format!("{} ({})", n.name, hours_phrase(*age)))
            .collect::<Vec<_>>()
            .join(", ");
        out.push(Finding {
            id: "people.silent".to_string(),
            severity: Severity::Warning,
            area: "people",
            title: format!(
                "No contact from {} in over {SILENT_NODE_HOURS} hours",
                plural(silent.len(), "radio", "radios")
            ),
            detail,
            go_to: Some("comms_hub"),
        });
    }
}

fn schedule_findings(snapshot: &StationSnapshot, out: &mut Vec<Finding>) {
    let horizon = snapshot.now + UPCOMING_EVENT_HOURS * HOUR;
    let mut soon: Vec<&UpcomingEvent> = snapshot
        .events
        .iter()
        .filter(|e| e.start >= snapshot.now && e.start <= horizon)
        .collect();
    soon.sort_by_key(|e| e.start);
    if soon.is_empty() {
        return;
    }
    let detail = soon
        .iter()
        .take(5)
        .map(|e| {
            let in_hours = ((e.start - snapshot.now) as f64 / HOUR as f64).round() as i64;
            let when = if in_hours <= 0 {
                "within the hour".to_string()
            } else {
                format!("in {in_hours} h")
            };
            match e.location.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(loc) => format!("{} ({when}, {loc})", e.title),
                None => format!("{} ({when})", e.title),
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    out.push(Finding {
        id: "schedule.upcoming".to_string(),
        severity: Severity::Info,
        area: "schedule",
        title: format!("{} in the next {UPCOMING_EVENT_HOURS} hours", plural(soon.len(), "event", "events")),
        detail,
        go_to: Some("calendar"),
    });
}

fn station_findings(snapshot: &StationSnapshot, out: &mut Vec<Finding>) {
    if let (Some(free), Some(total)) = (snapshot.disk_free_bytes, snapshot.disk_total_bytes) {
        if total > 0 {
            let pct = free as f64 / total as f64 * 100.0;
            let free_gb = free as f64 / GIB as f64;
            // Absolute thresholds: a percentage would cry wolf on large drives, where a few
            // percent is still tens of gigabytes.
            let severity = if free < GIB {
                Some(Severity::Critical)
            } else if free < 5 * GIB {
                Some(Severity::Warning)
            } else {
                None
            };
            if let Some(severity) = severity {
                out.push(Finding {
                    id: "station.disk".to_string(),
                    severity,
                    area: "station",
                    title: "Storage is nearly full".to_string(),
                    detail: format!(
                        "{free_gb:.1} GB free ({pct:.0}%). Indexing, map downloads and backups will start to fail."
                    ),
                    go_to: Some("data_acquisition"),
                });
            }
        }
    }
    if let Some(reason) = snapshot.ai_unavailable.as_deref() {
        out.push(Finding {
            id: "station.ai".to_string(),
            severity: Severity::Warning,
            area: "station",
            title: "The AI model is not running".to_string(),
            detail: format!(
                "Chat is answering with library passages only. {}",
                reason.trim()
            ),
            go_to: Some("marketplace"),
        });
    }
    if !snapshot.search_models_ready {
        out.push(Finding {
            id: "station.search".to_string(),
            severity: Severity::Warning,
            area: "station",
            title: "Library search is running in reduced mode".to_string(),
            detail: "The embedding or reranker model is not installed, so answers from your documents will be less accurate.".to_string(),
            go_to: Some("marketplace"),
        });
    }
}

/// Apply every rule to the snapshot. Most urgent first; stable order within a severity.
pub fn evaluate(snapshot: &StationSnapshot) -> Vec<Finding> {
    let mut out = Vec::new();
    people_findings(snapshot, &mut out);
    if let Some(report) = snapshot.runway.as_ref() {
        supply_findings(report, &mut out);
    }
    station_findings(snapshot, &mut out);
    schedule_findings(snapshot, &mut out);
    out.extend(snapshot.extra.iter().cloned());
    out.sort_by_key(|f| f.severity);
    out
}

/// One-line status that needs no model.
pub fn headline(findings: &[Finding]) -> String {
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let (critical, warning) = (count(Severity::Critical), count(Severity::Warning));
    match (critical, warning) {
        (0, 0) => "All clear. Nothing needs attention right now.".to_string(),
        (0, w) => format!("{} to watch.", plural(w, "thing", "things")),
        (c, 0) => format!("{} now.", plural(c, "thing needs action", "things need action")),
        (c, w) => format!(
            "{} now, and {} to watch.",
            plural(c, "thing needs action", "things need action"),
            w
        ),
    }
}

/// The facts handed to the model, one per line. Also what the model is checked against.
pub fn facts_for_model(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| {
            let level = match f.severity {
                Severity::Critical => "URGENT",
                Severity::Warning => "WATCH",
                Severity::Info => "INFO",
            };
            format!("- [{level}] {}: {}", f.title, f.detail)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchBrief {
    /// Unix seconds.
    pub generated_at: i64,
    pub headline: String,
    pub findings: Vec<Finding>,
    /// Model-written summary of `findings`, when a model was available and asked for.
    pub ai_summary: Option<String>,
}

fn build_snapshot(app: &tauri::AppHandle, state: &AppState) -> Result<StationSnapshot, String> {
    let now = chrono::Utc::now().timestamp();
    let app_data_dir = crate::portable::resolved_app_data_dir(app)?;
    let config = crate::load_config(&app_data_dir);

    let (runway_report, nodes, events, extra) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;

        let items: Vec<runway::StockItem> = db
            .inventory_list()
            .unwrap_or_default()
            .into_iter()
            .map(|r| runway::StockItem {
                id: r.id,
                name: r.item_name,
                category: r.category,
                division: r.division,
                quantity: r.quantity,
            })
            .collect();
        let changes: Vec<runway::StockChange> = db
            .inventory_log_list()
            .unwrap_or_default()
            .into_iter()
            .map(|l| runway::StockChange {
                item_id: l.item_id,
                old_qty: l.old_qty,
                new_qty: l.new_qty,
                timestamp: runway::log_timestamp_to_seconds(l.timestamp),
            })
            .collect();
        let headcount = db.personnel_count().unwrap_or(1).max(1);
        let runway_report = runway::build_runway_report(
            &items,
            &changes,
            now,
            runway::DEFAULT_WINDOW_DAYS,
            headcount,
            headcount,
        );

        let last_heard: std::collections::HashMap<i64, i64> = db
            .list_mesh_network_nodes()
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                (
                    i64::from(n.node_id),
                    runway::log_timestamp_to_seconds(n.last_message_at),
                )
            })
            .collect();
        let nodes: Vec<NodeContact> = db
            .mesh_profile_list()
            .unwrap_or_default()
            .into_iter()
            .map(|p| NodeContact {
                node_id: p.node_id,
                name: if p.username.trim().is_empty() {
                    format!("Node {}", p.node_id)
                } else {
                    p.username.clone()
                },
                registered: p.is_registered,
                sos_active: p.sos_state == crate::db::SOS_STATE_ACTIVE,
                last_heard: last_heard.get(&p.node_id).copied(),
            })
            .collect();

        let events: Vec<UpcomingEvent> = db
            .list_calendar_events(None, None, false)
            .unwrap_or_default()
            .into_iter()
            .map(|e| UpcomingEvent {
                title: e.title,
                start: runway::log_timestamp_to_seconds(e.start_ts),
                location: e.location_label,
            })
            .collect();

        let extra = crate::field_ops::watch_findings(&db, now);

        (runway_report, nodes, events, extra)
    };

    let storage = crate::storage::check_available_storage(app.clone(), None).ok();
    let models = crate::inference::load_models_for_inference(&config);
    let ai_unavailable = if config.ai_inference_sleep.unwrap_or(false) {
        None // asleep on purpose
    } else {
        crate::inference::ensure_inference_ready(&config, &models).err()
    };

    Ok(StationSnapshot {
        now,
        runway: Some(runway_report),
        nodes,
        events,
        disk_free_bytes: storage.as_ref().map(|s| s.available_bytes),
        disk_total_bytes: storage.as_ref().map(|s| s.total_bytes),
        ai_unavailable,
        search_models_ready: crate::neural_embed::global_is_onnx_loaded()
            && crate::rerank::global_is_onnx_loaded(),
        extra,
    })
}

fn write_ai_summary(app: &tauri::AppHandle, findings: &[Finding]) -> Option<String> {
    if findings.is_empty() {
        return None;
    }
    let app_data_dir = crate::portable::resolved_app_data_dir(app).ok()?;
    let config = crate::load_config(&app_data_dir);
    if config.ai_inference_sleep.unwrap_or(false) {
        return None;
    }
    let models = crate::inference::load_models_for_inference(&config);
    crate::inference::ensure_inference_ready(&config, &models).ok()?;
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You are the watch officer for a small off-grid group. You will be given a list of facts about the station. Write a brief of at most five short sentences for the person in charge: most urgent first, plain words, and one practical next step for each urgent item. Use only the facts given. Do not add numbers, names, causes or events that are not in the list. No headings, no bullet points, no greeting.".to_string(),
            images: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!("Station facts:\n{}", facts_for_model(findings)),
            images: None,
        },
    ];
    let options = ModelOptions {
        temperature: 0.2,
        num_ctx: 2048,
        num_predict: Some(260),
    };
    crate::inference::chat_unified(&config, &models, messages, Some(options))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The watch officer's current brief.
///
/// `with_ai` asks the chat model to write a short summary of the findings. The findings themselves
/// never depend on a model.
#[tauri::command]
pub async fn get_watch_brief(
    with_ai: Option<bool>,
    app: tauri::AppHandle,
    session: State<'_, AuthSession>,
) -> Result<WatchBrief, String> {
    let user = crate::auth::authenticated_user(&session)?;
    if user.clearance_level <= 0 {
        return Err("The watch brief is not available to guests.".to_string());
    }
    let with_ai = with_ai.unwrap_or(false);
    tokio::task::spawn_blocking(move || -> Result<WatchBrief, String> {
        let state = app.state::<AppState>();
        let snapshot = build_snapshot(&app, &state)?;
        let findings = evaluate(&snapshot);
        let ai_summary = if with_ai {
            write_ai_summary(&app, &findings)
        } else {
            None
        };
        Ok(WatchBrief {
            generated_at: snapshot.now,
            headline: headline(&findings),
            findings,
            ai_summary,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn base() -> StationSnapshot {
        StationSnapshot {
            now: NOW,
            search_models_ready: true,
            ..Default::default()
        }
    }

    fn runway_with(items: Vec<(&str, i64, Option<f64>, RunwayStatus)>) -> RunwayReport {
        RunwayReport {
            items: items
                .into_iter()
                .map(|(name, quantity, days_left, status)| ItemRunway {
                    id: name.to_string(),
                    name: name.to_string(),
                    category: "general".to_string(),
                    division: "LOGISTICS".to_string(),
                    quantity,
                    daily_use: None,
                    days_left,
                    runs_out_at: None,
                    status,
                    use_events: 2,
                })
                .collect(),
            window_days: 30,
            headcount: 4,
            baseline_headcount: 4,
            out_count: 0,
            critical_count: 0,
            low_count: 0,
            unknown_count: 0,
        }
    }

    fn node(name: &str, hours_ago: i64) -> NodeContact {
        NodeContact {
            node_id: name.len() as i64,
            name: name.to_string(),
            registered: true,
            sos_active: false,
            last_heard: Some(NOW - hours_ago * HOUR),
        }
    }

    #[test]
    fn quiet_station_is_all_clear() {
        let findings = evaluate(&base());
        assert!(findings.is_empty());
        assert_eq!(headline(&findings), "All clear. Nothing needs attention right now.");
    }

    #[test]
    fn supplies_are_grouped_by_urgency_with_days() {
        let mut s = base();
        s.runway = Some(runway_with(vec![
            ("Water", 0, Some(0.0), RunwayStatus::Out),
            ("Insulin", 3, Some(3.0), RunwayStatus::Critical),
            ("Rice", 30, Some(15.2), RunwayStatus::Low),
            ("Matches", 98, Some(400.0), RunwayStatus::Ok),
            ("Rope", 5, None, RunwayStatus::Unknown),
        ]));
        let f = evaluate(&s);
        let ids: Vec<&str> = f.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["supplies.out", "supplies.critical", "supplies.low"]);
        assert_eq!(f[0].title, "1 item is out of stock");
        assert_eq!(f[0].detail, "Water");
        assert_eq!(f[1].detail, "Insulin (3 days)");
        assert_eq!(f[2].detail, "Rice (15 days)");
        assert_eq!(f[2].severity, Severity::Warning);
    }

    #[test]
    fn long_supply_lists_are_summarised() {
        let mut s = base();
        s.runway = Some(runway_with(
            ["a", "b", "c", "d", "e", "f"]
                .iter()
                .map(|n| (*n, 0, Some(0.0), RunwayStatus::Out))
                .collect(),
        ));
        let f = evaluate(&s);
        assert_eq!(f[0].title, "6 items are out of stock");
        assert_eq!(f[0].detail, "a, b, c, d, and 2 more");
    }

    #[test]
    fn active_sos_comes_first() {
        let mut s = base();
        s.runway = Some(runway_with(vec![("Water", 0, Some(0.0), RunwayStatus::Out)]));
        let mut n = node("Ridge", 2);
        n.sos_active = true;
        s.nodes = vec![n];
        let f = evaluate(&s);
        assert_eq!(f[0].title, "SOS active: Ridge");
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[0].detail, "Last heard 2 hours ago.");
    }

    #[test]
    fn silent_registered_radios_are_flagged_but_stale_ones_are_not() {
        let mut s = base();
        let mut guest = node("Stranger", 40);
        guest.registered = false;
        s.nodes = vec![
            node("Outpost", 30),
            node("Farm", 3),
            node("Gone", 24 * 20),
            guest,
        ];
        let f = evaluate(&s);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "people.silent");
        assert_eq!(f[0].title, "No contact from 1 radio in over 24 hours");
        assert_eq!(f[0].detail, "Outpost (30 hours)");
    }

    #[test]
    fn upcoming_events_within_a_day_are_listed_soonest_first() {
        let mut s = base();
        s.events = vec![
            UpcomingEvent { title: "Supply run".into(), start: NOW + 5 * HOUR, location: Some("North gate".into()) },
            UpcomingEvent { title: "Net check-in".into(), start: NOW + 2 * HOUR, location: None },
            UpcomingEvent { title: "Next week".into(), start: NOW + 7 * DAY, location: None },
            UpcomingEvent { title: "Yesterday".into(), start: NOW - DAY, location: None },
        ];
        let f = evaluate(&s);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Info);
        assert_eq!(f[0].title, "2 events in the next 24 hours");
        assert_eq!(f[0].detail, "Net check-in (in 2 h); Supply run (in 5 h, North gate)");
    }

    #[test]
    fn low_disk_and_missing_models_are_station_warnings() {
        let mut s = base();
        s.disk_total_bytes = Some(500 * GIB);
        s.disk_free_bytes = Some(3 * GIB);
        s.ai_unavailable = Some("Ollama is not running.".into());
        s.search_models_ready = false;
        let f = evaluate(&s);
        let ids: Vec<&str> = f.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["station.disk", "station.ai", "station.search"]);
        assert!(f.iter().all(|x| x.severity == Severity::Warning));
        assert!(f[0].detail.starts_with("3.0 GB free (1%)"));

        s.disk_free_bytes = Some(GIB / 2);
        assert_eq!(evaluate(&s)[0].severity, Severity::Critical);
    }

    #[test]
    fn headline_counts_urgent_and_watch_items() {
        let mut s = base();
        s.runway = Some(runway_with(vec![
            ("Water", 0, Some(0.0), RunwayStatus::Out),
            ("Rice", 30, Some(15.0), RunwayStatus::Low),
        ]));
        s.ai_unavailable = Some("down".into());
        let f = evaluate(&s);
        assert_eq!(headline(&f), "1 thing needs action now, and 2 to watch.");
    }

    #[test]
    fn model_is_given_only_the_detected_facts() {
        let mut s = base();
        s.runway = Some(runway_with(vec![("Insulin", 3, Some(3.0), RunwayStatus::Critical)]));
        let facts = facts_for_model(&evaluate(&s));
        assert_eq!(facts, "- [URGENT] 1 item runs out within a week: Insulin (3 days)");
    }
}
