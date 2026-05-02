//! Post-AI application: RBAC, dedupe, auto-apply deltas, and staging for new items.

use chrono::Utc;
use serde::Serialize;

use crate::auth::AuthenticatedPersonnel;
use crate::db::Database;
use crate::inventory;
use crate::logistics_ai::{LogisticsAiInventoryLine, LogisticsAiResult};

const DAY_MS: i64 = 86_400_000;

const DIRECTORATE_KEYS: &[&str] = &[
    "Infrastructure",
    "Vanguard",
    "Sustenance",
    "InformationMedical",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineOutcome {
    pub kind: String,
    pub line_id: Option<String>,
    pub proposal_id: Option<String>,
    pub inventory_id: Option<String>,
    pub delta: Option<i64>,
    pub new_qty: Option<i64>,
    pub log_id: Option<String>,
    pub other_user_id: Option<String>,
    pub other_user_name: Option<String>,
    pub prior_at_ms: Option<i64>,
    pub item_name: Option<String>,
    pub directorate: Option<String>,
    pub message: Option<String>,
}

/// Queued to insert with a proposal.
#[derive(Debug, Clone)]
pub struct PendingProposalLine {
    pub id: String,
    pub action: String,
    /// Directorate key (reuses `division` column in DB)
    pub division: String,
    pub item_name: String,
    pub category: String,
    pub quantity: i64,
    pub confidence: Option<f64>,
    pub line_location: Option<String>,
}

fn norm_item(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Resolve which directorate key to use.
fn resolve_directorate(line: &LogisticsAiInventoryLine, actor: &AuthenticatedPersonnel) -> String {
    let t = line.directorate.trim();
    if !t.is_empty() {
        let r = t.to_string();
        let rl = r.to_lowercase();
        for k in DIRECTORATE_KEYS {
            if k.eq_ignore_ascii_case(&r) {
                return (*k).to_string();
            }
        }
        for k in DIRECTORATE_KEYS {
            if rl.contains(&k.to_lowercase()) {
                return (*k).to_string();
            }
        }
    }
    if let Some(ref d) = actor.directorate {
        let t = d.trim();
        if !t.is_empty() {
            for k in DIRECTORATE_KEYS {
                if k.eq_ignore_ascii_case(t) {
                    return (*k).to_string();
                }
            }
        }
    }
    "Infrastructure".to_string()
}

fn line_location(
    line: &LogisticsAiInventoryLine,
    attached_text: Option<&str>,
) -> Option<String> {
    if let Some(ref a) = line.location {
        let t = a.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    attached_text
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn can_write_directorate(actor: &AuthenticatedPersonnel, directorate: &str) -> bool {
    match actor.rank_level {
        0 | 1 | 2 => false,
        3 => {
            if let Some(ref d) = actor.directorate {
                if !d.trim().is_empty() {
                    return d.eq_ignore_ascii_case(directorate);
                }
            }
            actor.division.trim().eq_ignore_ascii_case(directorate)
        }
        _ => true,
    }
}

/// Apply all inventory lines. Returns (outcomes, lines to add to a single pending proposal, if any).
pub fn process_inventory_lines(
    db: &Database,
    actor: &AuthenticatedPersonnel,
    logbook_entry_id: &str,
    ai: &LogisticsAiResult,
    now_ms: i64,
    force_apply: bool,
    attached_location: Option<&str>,
) -> Result<(Vec<LineOutcome>, Vec<PendingProposalLine>), String> {
    let mut outcomes = Vec::new();
    let mut pending: Vec<PendingProposalLine> = Vec::new();

    if ai.inventory.is_empty() {
        return Ok((outcomes, pending));
    }

    for line in &ai.inventory {
        let directorate = resolve_directorate(line, actor);
        let item_name = line.item_name.trim();
        if item_name.is_empty() {
            continue;
        }
        let loc = line_location(line, attached_location);
        let item_norm = norm_item(item_name);
        let loc_norm: Option<String> = loc
            .as_ref()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());

        let action = line.action.to_ascii_lowercase();
        let existing = db.inventory_find_by_directorate_item(&directorate, item_name)?;

        if let Some(ref row) = existing {
            // --- Existing row: delta
            let delta: i64 = if action == "set_quantity" {
                line.quantity - row.quantity
            } else {
                line.quantity
            };

            if !force_apply {
                if let Some((other_id, prior_ms)) = db.intake_find_duplicate_other_user(
                    &directorate,
                    &item_norm,
                    loc_norm.as_deref(),
                    now_ms - DAY_MS,
                    &actor.id,
                )? {
                    let in_id = uuid::Uuid::new_v4().to_string();
                    let other_name = db
                        .personnel_name_by_id(&other_id)?
                        .unwrap_or_else(|| "Unknown".to_string());
                    db.inventory_intake_event_insert(
                        &in_id,
                        logbook_entry_id,
                        None,
                        &directorate,
                        &item_norm,
                        loc_norm.as_deref(),
                        delta,
                        Some(actor.id.as_str()),
                        0,
                        "suppressed_duplicate",
                        Some(other_id.as_str()),
                        now_ms,
                    )?;
                    outcomes.push(LineOutcome {
                        kind: "suppressedDuplicate".to_string(),
                        line_id: None,
                        proposal_id: None,
                        inventory_id: Some(row.id.clone()),
                        delta: Some(delta),
                        new_qty: None,
                        log_id: None,
                        other_user_id: Some(other_id),
                        other_user_name: Some(other_name),
                        prior_at_ms: Some(prior_ms),
                        item_name: Some(item_name.to_string()),
                        directorate: Some(directorate.clone()),
                        message: None,
                    });
                    continue;
                }
            }

            if actor.rank_level < 3 {
                let lid = uuid::Uuid::new_v4().to_string();
                let pline = PendingProposalLine {
                    id: lid.clone(),
                    action: "delta_quantity".to_string(),
                    division: directorate.clone(),
                    item_name: item_name.to_string(),
                    category: line.category.clone(),
                    quantity: delta,
                    confidence: line.confidence,
                    line_location: loc.clone(),
                };
                pending.push(pline);
                outcomes.push(LineOutcome {
                    kind: "pendingProposal".to_string(),
                    line_id: Some(lid),
                    proposal_id: None,
                    inventory_id: None,
                    delta: Some(delta),
                    new_qty: None,
                    log_id: None,
                    other_user_id: None,
                    other_user_name: None,
                    prior_at_ms: None,
                    item_name: Some(item_name.to_string()),
                    directorate: Some(directorate.clone()),
                    message: None,
                });
                continue;
            }
            if !can_write_directorate(actor, &directorate) {
                return Err("You may only modify inventory for your own directorate.".to_string());
            }
            if delta == 0 {
                continue;
            }

            let new_qty = (row.quantity + delta).max(0);
            let old = db
                .inventory_set_quantity(&row.id, new_qty)?
                .ok_or_else(|| "Inventory item missing during update.".to_string())?;
            let cat = inventory::normalize_inventory_category(&line.category);
            if cat != row.category {
                let _ = db.inventory_set_category(&row.id, &cat);
            }

            let log_id = uuid::Uuid::new_v4().to_string();
            let ts = Utc::now().timestamp_millis();
            db.inventory_log_insert(
                &log_id,
                &row.id,
                &actor.id,
                "ai_delta",
                Some(old),
                Some(new_qty),
                ts,
            )?;
            let ev = uuid::Uuid::new_v4().to_string();
            db.inventory_intake_event_insert(
                &ev,
                logbook_entry_id,
                Some(&row.id),
                &directorate,
                &item_norm,
                loc_norm.as_deref(),
                delta,
                Some(actor.id.as_str()),
                1,
                "auto",
                None,
                now_ms,
            )?;
            outcomes.push(LineOutcome {
                kind: "autoApplied".to_string(),
                line_id: None,
                proposal_id: None,
                inventory_id: Some(row.id.clone()),
                delta: Some(delta),
                new_qty: Some(new_qty),
                log_id: Some(log_id),
                other_user_id: None,
                other_user_name: None,
                prior_at_ms: None,
                item_name: Some(item_name.to_string()),
                directorate: Some(directorate),
                message: None,
            });
        } else {
            // New item: prompt via proposal; no row yet
            let start_qty = line.quantity.max(0);
            let lid = uuid::Uuid::new_v4().to_string();
            let pl = PendingProposalLine {
                id: lid.clone(),
                action: "add_item".to_string(),
                division: directorate.clone(),
                item_name: item_name.to_string(),
                category: line.category.clone(),
                quantity: start_qty,
                confidence: line.confidence,
                line_location: loc.clone(),
            };
            pending.push(pl);
            outcomes.push(LineOutcome {
                kind: "newItemPrompt".to_string(),
                line_id: Some(lid),
                proposal_id: None,
                inventory_id: None,
                delta: Some(line.quantity),
                new_qty: None,
                log_id: None,
                other_user_id: None,
                other_user_name: None,
                prior_at_ms: None,
                item_name: Some(item_name.to_string()),
                directorate: Some(directorate),
                message: None,
            });
        }
    }

    Ok((outcomes, pending))
}
