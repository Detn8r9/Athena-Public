//! Field Ops: the survival-coordination features (people, supplies, protocols, field tools,
//! communications planning). Each feature is a pure logic module with tests, a thin command
//! layer, and typed records in the shared [`store`].

pub mod checkins;
pub mod courier;
pub mod intake;
pub mod ledger;
pub mod memory;
pub mod mesh_query;
pub mod needs;
pub mod patients;
pub mod picture;
pub mod power;
pub mod radio_discipline;
pub mod roster;
pub mod sensors;
pub mod simple;
pub mod skills;
pub mod store;
pub mod trust;

use crate::auth::{AuthSession, AuthenticatedPersonnel};

/// Field Ops data is for members of the group. Guests (clearance 0) see none of it.
pub(crate) fn require_member(session: &AuthSession) -> Result<AuthenticatedPersonnel, String> {
    let user = crate::auth::authenticated_user(session)?;
    if user.clearance_level <= 0 {
        return Err("This is not available to guests.".to_string());
    }
    Ok(user)
}

/// Watch officer rules contributed by the Field Ops features.
pub fn watch_findings(db: &crate::db::Database, now: i64) -> Vec<crate::watch_officer::Finding> {
    let mut out = Vec::new();
    let plans: Vec<(checkins::CheckinPlan, Option<i64>)> = checkins::load_with_radio(db)
        .into_iter()
        .map(|(r, heard)| (r.data, heard))
        .collect();
    out.extend(checkins::findings(&plans, now));
    out.extend(skills::findings(&skills::load(db)));
    out.extend(intake::findings(&intake::load(db), now));
    out.extend(needs::findings(&needs::report_from_db(db)));
    out.extend(sensors::findings(&sensors::load_with_latest(db), now));
    if let Some(budget) = power::budget_from_db(db) {
        out.extend(power::findings(&budget));
    }
    out
}
