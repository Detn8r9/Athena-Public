//! Decision log and ledger (S26): what the group decided and how, and who owes whom.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::AppState;

pub const DECISION_KIND: &str = "decision";
pub const LEDGER_KIND: &str = "ledger_entry";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vote {
    pub voter_id: String,
    pub voter_name: String,
    pub option: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
    pub options: Vec<String>,
    #[serde(default)]
    pub votes: Vec<Vote>,
    /// `open` or `closed`.
    pub status: String,
    /// What was decided, written when the decision is closed.
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub recorded_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    /// Votes per option, in option order.
    pub counts: Vec<usize>,
    pub voters: usize,
    /// Index of the option with the most votes; `None` on a tie or when nobody voted.
    pub leading: Option<usize>,
    pub tied: bool,
}

/// Count votes. Each person counts once: their latest vote replaces earlier ones. Votes for an
/// option that does not exist are ignored.
pub fn tally(decision: &Decision) -> Tally {
    let mut latest: Vec<(&str, usize)> = Vec::new();
    for v in &decision.votes {
        if v.option >= decision.options.len() {
            continue;
        }
        match latest.iter_mut().find(|(id, _)| *id == v.voter_id) {
            Some(slot) => slot.1 = v.option,
            None => latest.push((&v.voter_id, v.option)),
        }
    }
    let mut counts = vec![0usize; decision.options.len()];
    for (_, option) in &latest {
        counts[*option] += 1;
    }
    let top = counts.iter().copied().max().unwrap_or(0);
    let leaders: Vec<usize> = counts.iter().enumerate().filter(|(_, c)| **c == top && top > 0).map(|(i, _)| i).collect();
    Tally {
        voters: latest.len(),
        tied: leaders.len() > 1,
        leading: if leaders.len() == 1 { Some(leaders[0]) } else { None },
        counts,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    /// Who gave.
    pub from: String,
    /// Who received.
    pub to: String,
    pub amount: f64,
    /// What is being counted: "hours", "kg rice", "litres fuel".
    pub unit: String,
    #[serde(default)]
    pub note: Option<String>,
    /// True once the debt has been repaid or forgiven.
    #[serde(default)]
    pub settled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub person: String,
    pub unit: String,
    /// Positive: the group owes this person. Negative: this person owes the group.
    pub net: f64,
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Net position per person and unit across unsettled entries. Balanced positions are omitted.
pub fn balances(entries: &[LedgerEntry]) -> Vec<Balance> {
    use std::collections::BTreeMap;
    // (unit key, person key) -> (unit display, person display, net)
    let mut map: BTreeMap<(String, String), (String, String, f64)> = BTreeMap::new();
    for e in entries {
        if e.settled || !e.amount.is_finite() || e.amount <= 0.0 {
            continue;
        }
        let unit = norm(&e.unit);
        for (person, sign) in [(norm(&e.from), 1.0), (norm(&e.to), -1.0)] {
            if person.is_empty() {
                continue;
            }
            let slot = map
                .entry((unit.to_lowercase(), person.to_lowercase()))
                .or_insert_with(|| (unit.clone(), person.clone(), 0.0));
            slot.2 += sign * e.amount;
        }
    }
    let mut out: Vec<Balance> = map
        .into_values()
        .filter(|(_, _, net)| net.abs() > 1e-9)
        .map(|(unit, person, net)| Balance { person, unit, net })
        .collect();
    out.sort_by(|a, b| {
        a.unit
            .to_lowercase()
            .cmp(&b.unit.to_lowercase())
            .then(b.net.partial_cmp(&a.net).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.person.cmp(&b.person))
    });
    out
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRow {
    #[serde(flatten)]
    pub record: Record<Decision>,
    pub tally: Tally,
}

#[tauri::command]
pub fn list_decisions(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<DecisionRow>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut rows: Vec<DecisionRow> = store::list::<Decision>(&db, DECISION_KIND)?
        .into_iter()
        .map(|record| DecisionRow { tally: tally(&record.data), record })
        .collect();
    rows.sort_by(|a, b| b.record.created_at.cmp(&a.record.created_at));
    Ok(rows)
}

#[tauri::command]
pub fn save_decision(
    id: Option<String>,
    mut decision: Decision,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<Decision>, String> {
    let user = super::require_member(&session)?;
    decision.title = decision.title.trim().to_string();
    decision.options = decision.options.iter().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
    if decision.title.is_empty() {
        return Err("Give the decision a title.".to_string());
    }
    if decision.options.len() < 2 {
        return Err("A decision needs at least two options.".to_string());
    }
    if !matches!(decision.status.as_str(), "open" | "closed") {
        decision.status = "open".to_string();
    }
    if decision.recorded_by.is_none() {
        decision.recorded_by = Some(user.display_name.clone().unwrap_or(user.name.clone()));
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, DECISION_KIND, id.as_deref(), &decision)
}

/// Record the signed-in person's vote. Voting again changes their vote.
#[tauri::command]
pub fn cast_decision_vote(
    id: String,
    option: usize,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<DecisionRow, String> {
    let user = super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let existing = store::get::<Decision>(&db, DECISION_KIND, &id)?.ok_or("Decision not found.")?;
    let mut data = existing.data;
    if data.status != "open" {
        return Err("This decision is closed.".to_string());
    }
    if option >= data.options.len() {
        return Err("That option does not exist.".to_string());
    }
    data.votes.retain(|v| v.voter_id != user.id);
    data.votes.push(Vote {
        voter_id: user.id.clone(),
        voter_name: user.display_name.clone().unwrap_or(user.name.clone()),
        option,
    });
    let record = store::put(&db, DECISION_KIND, Some(&id), &data)?;
    Ok(DecisionRow { tally: tally(&record.data), record })
}

#[tauri::command]
pub fn close_decision(
    id: String,
    outcome: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<Decision>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let existing = store::get::<Decision>(&db, DECISION_KIND, &id)?.ok_or("Decision not found.")?;
    let mut data = existing.data;
    data.status = "closed".to_string();
    data.outcome = Some(outcome.trim().to_string()).filter(|s| !s.is_empty());
    store::put(&db, DECISION_KIND, Some(&id), &data)
}

#[tauri::command]
pub fn delete_decision(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, DECISION_KIND, &id)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerView {
    pub entries: Vec<Record<LedgerEntry>>,
    pub balances: Vec<Balance>,
}

#[tauri::command]
pub fn get_ledger(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<LedgerView, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut entries = store::list::<LedgerEntry>(&db, LEDGER_KIND)?;
    let plain: Vec<LedgerEntry> = entries.iter().map(|r| r.data.clone()).collect();
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(LedgerView { balances: balances(&plain), entries })
}

#[tauri::command]
pub fn save_ledger_entry(
    id: Option<String>,
    mut entry: LedgerEntry,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<LedgerEntry>, String> {
    super::require_member(&session)?;
    entry.from = norm(&entry.from);
    entry.to = norm(&entry.to);
    entry.unit = norm(&entry.unit);
    if entry.from.is_empty() || entry.to.is_empty() {
        return Err("Say who gave and who received.".to_string());
    }
    if entry.from.eq_ignore_ascii_case(&entry.to) {
        return Err("Giver and receiver must be different.".to_string());
    }
    if !entry.amount.is_finite() || entry.amount <= 0.0 {
        return Err("Amount must be more than zero.".to_string());
    }
    if entry.unit.is_empty() {
        return Err("Say what is being counted, for example hours or kg rice.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, LEDGER_KIND, id.as_deref(), &entry)
}

#[tauri::command]
pub fn delete_ledger_entry(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, LEDGER_KIND, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(options: &[&str], votes: &[(&str, usize)]) -> Decision {
        Decision {
            title: "Move camp?".into(),
            detail: None,
            options: options.iter().map(|s| s.to_string()).collect(),
            votes: votes
                .iter()
                .map(|(v, o)| Vote { voter_id: v.to_string(), voter_name: v.to_string(), option: *o })
                .collect(),
            status: "open".into(),
            outcome: None,
            recorded_by: None,
        }
    }

    #[test]
    fn counts_votes_and_names_the_leader() {
        let t = tally(&decision(&["Stay", "Move"], &[("a", 1), ("b", 1), ("c", 0)]));
        assert_eq!(t.counts, vec![1, 2]);
        assert_eq!(t.voters, 3);
        assert_eq!(t.leading, Some(1));
        assert!(!t.tied);
    }

    #[test]
    fn a_tie_has_no_leader() {
        let t = tally(&decision(&["Stay", "Move"], &[("a", 0), ("b", 1)]));
        assert!(t.tied);
        assert_eq!(t.leading, None);
    }

    #[test]
    fn each_person_counts_once_and_latest_vote_wins() {
        let t = tally(&decision(&["Stay", "Move"], &[("a", 0), ("a", 1), ("a", 1)]));
        assert_eq!(t.counts, vec![0, 1]);
        assert_eq!(t.voters, 1);
    }

    #[test]
    fn votes_for_missing_options_are_ignored_and_no_votes_is_not_a_tie() {
        let t = tally(&decision(&["Stay", "Move"], &[("a", 7)]));
        assert_eq!(t.voters, 0);
        assert_eq!(t.leading, None);
        assert!(!t.tied);
    }

    fn entry(from: &str, to: &str, amount: f64, unit: &str) -> LedgerEntry {
        LedgerEntry { from: from.into(), to: to.into(), amount, unit: unit.into(), note: None, settled: false }
    }

    #[test]
    fn balances_net_out_per_unit() {
        let b = balances(&[
            entry("Ana", "Ben", 5.0, "hours"),
            entry("Ben", "Ana", 2.0, "hours"),
            entry("Cy", "Ana", 3.0, "kg rice"),
        ]);
        assert_eq!(
            b,
            vec![
                Balance { person: "Ana".into(), unit: "hours".into(), net: 3.0 },
                Balance { person: "Ben".into(), unit: "hours".into(), net: -3.0 },
                Balance { person: "Cy".into(), unit: "kg rice".into(), net: 3.0 },
                Balance { person: "Ana".into(), unit: "kg rice".into(), net: -3.0 },
            ]
        );
    }

    #[test]
    fn settled_and_invalid_entries_do_not_count_and_even_positions_vanish() {
        let mut settled = entry("Ana", "Ben", 5.0, "hours");
        settled.settled = true;
        let b = balances(&[
            settled,
            entry("Ana", "Ben", -4.0, "hours"),
            entry("Ana", "Ben", 2.0, "Hours"),
            entry("ben", "ana", 2.0, "hours"),
        ]);
        assert!(b.is_empty(), "{b:?}");
    }
}
