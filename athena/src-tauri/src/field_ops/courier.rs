//! Courier sync (S9): carry records between stations on a USB stick.
//!
//! A bundle is one file holding a chosen set of records. Importing it merges them into the
//! receiving station: the newest version of each record wins and deletions carry over, so the
//! same stick can go back and forth and both ends converge. When the stations share a group
//! key the bundle is tagged, and a bundle that was altered on the way is refused.

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use super::store::{self, MergeOutcome, RawRecord};
use super::trust;
use crate::auth::AuthSession;
use crate::AppState;

pub const BUNDLE_FORMAT: &str = "athena-courier-1";

/// What can travel, grouped the way an operator thinks about it. Anything not listed never
/// leaves the station: keys, settings, the watch officer's state.
pub const GROUPS: &[(&str, &str, &[&str])] = &[
    (
        "knowledge",
        "Group memory notes",
        &["memory_note"],
    ),
    (
        "coordination",
        "Check-in plans, duties, skills, newcomers, decisions",
        &["checkin_plan", "roster_duty", "roster_exclusion", "skill_entry", "intake", "decision"],
    ),
    (
        "federation",
        "Needs and offers board, people finder",
        &["fed_post", "fed_person"],
    ),
    (
        "medical",
        "Patients and their charts (private)",
        &["patient", "chart_entry"],
    ),
    (
        "ledger",
        "Ledger entries",
        &["ledger_entry"],
    ),
];

pub fn kinds_for(groups: &[String]) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    for (id, _, group_kinds) in GROUPS {
        if groups.iter().any(|g| g == id) {
            kinds.extend_from_slice(group_kinds);
        }
    }
    kinds
}

fn allowed_kind(kind: &str) -> bool {
    GROUPS.iter().any(|(_, _, kinds)| kinds.contains(&kind))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleBody {
    pub format: String,
    /// Name of the station that made the bundle.
    pub station: String,
    /// Unix seconds.
    pub created_at: i64,
    pub groups: Vec<String>,
    pub records: Vec<RawRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleFile {
    /// The body as a JSON string, so the tag covers exact bytes.
    body: String,
    /// HMAC of `body` with the group key; absent when the sending station had no key.
    #[serde(default)]
    tag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleTrust {
    /// Tagged, and the tag matches this station's group key.
    Verified,
    /// Not tagged.
    Unsigned,
    /// Tagged, but this station has no group key to check it with.
    CannotCheck,
    /// Tagged, and the tag does not match: altered, or made with a different key.
    Tampered,
}

/// Serialize a bundle, tagging it when a key is given.
pub fn pack(body: &BundleBody, key: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let body_json = serde_json::to_string(body).map_err(|e| e.to_string())?;
    let tag = key.map(|k| trust::file_tag(k, body_json.as_bytes()));
    serde_json::to_vec_pretty(&BundleFile { body: body_json, tag }).map_err(|e| e.to_string())
}

/// Read a bundle and say how far it can be trusted. Records of kinds that may not travel are
/// dropped, whatever the file claims.
pub fn unpack(bytes: &[u8], key: Option<&[u8]>) -> Result<(BundleBody, BundleTrust), String> {
    let file: BundleFile = serde_json::from_slice(bytes)
        .map_err(|_| "This is not an ATHENA courier bundle, or the file is damaged.".to_string())?;
    let trust_level = match (&file.tag, key) {
        (None, _) => BundleTrust::Unsigned,
        (Some(_), None) => BundleTrust::CannotCheck,
        (Some(tag), Some(k)) => {
            if trust::file_tag_matches(k, file.body.as_bytes(), tag) {
                BundleTrust::Verified
            } else {
                BundleTrust::Tampered
            }
        }
    };
    let mut body: BundleBody = serde_json::from_str(&file.body)
        .map_err(|_| "The bundle's contents could not be read.".to_string())?;
    if body.format != BUNDLE_FORMAT {
        return Err(format!(
            "This bundle is in a format this version of ATHENA does not understand ({}).",
            body.format
        ));
    }
    body.records.retain(|r| allowed_kind(&r.kind));
    Ok((body, trust_level))
}

/// Whether a bundle of this trust level should be merged.
pub fn may_import(level: BundleTrust, this_station_has_key: bool, accept_unverified: bool) -> Result<(), String> {
    match level {
        BundleTrust::Verified => Ok(()),
        BundleTrust::Tampered => Err(
            "This bundle does not match your group key. It was changed after it was made, or it came from a group with a different key. Nothing was imported."
                .to_string(),
        ),
        BundleTrust::Unsigned | BundleTrust::CannotCheck if accept_unverified => Ok(()),
        BundleTrust::Unsigned if this_station_has_key => Err(
            "This bundle is not signed, so there is no way to tell who made it. Import it only if you trust where the drive came from."
                .to_string(),
        ),
        BundleTrust::Unsigned => Ok(()),
        BundleTrust::CannotCheck => Err(
            "This bundle is signed, but this station has no group key to check it with. Enter the group key, or import it only if you trust where the drive came from."
                .to_string(),
        ),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub id: &'static str,
    pub label: &'static str,
    /// Live records of this group on this station.
    pub count: usize,
}

#[tauri::command]
pub fn list_courier_groups(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<GroupInfo>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    GROUPS
        .iter()
        .map(|(id, label, kinds)| {
            let count = store::export_raw(&db, kinds)?.iter().filter(|r| !r.deleted).count();
            Ok(GroupInfo { id, label, count })
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub path: String,
    pub records: usize,
    pub signed: bool,
}

/// Write the chosen groups to a bundle file.
#[tauri::command]
pub fn export_courier_bundle(
    path: String,
    groups: Vec<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<ExportSummary, String> {
    super::require_member(&session)?;
    let kinds = kinds_for(&groups);
    if kinds.is_empty() {
        return Err("Choose at least one kind of record to send.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let settings = trust::load_settings(&db);
    let key = trust::key_bytes(&settings);
    let body = BundleBody {
        format: BUNDLE_FORMAT.to_string(),
        station: settings.station_name.clone().unwrap_or_else(|| "Unnamed station".to_string()),
        created_at: store::now(),
        groups,
        records: store::export_raw(&db, &kinds)?,
    };
    let bytes = pack(&body, key.as_deref())?;
    std::fs::write(&path, bytes).map_err(|e| format!("Could not write the bundle: {e}"))?;
    Ok(ExportSummary { path, records: body.records.len(), signed: key.is_some() })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub station: String,
    pub created_at: i64,
    pub trust: BundleTrust,
    pub records_in_bundle: usize,
    /// `None` when this was only an inspection.
    pub merged: Option<MergeOutcome>,
}

/// Inspect a bundle (`apply = false`) or merge it into this station (`apply = true`).
#[tauri::command]
pub async fn import_courier_bundle(
    path: String,
    apply: bool,
    accept_unverified: Option<bool>,
    app: tauri::AppHandle,
    session: crate::auth::CallerSession,
) -> Result<ImportSummary, String> {
    super::require_member(&session)?;
    tokio::task::spawn_blocking(move || -> Result<ImportSummary, String> {
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the bundle: {e}"))?;
        let app_data_dir = crate::portable::resolved_app_data_dir(&app)?;
        let state = app.state::<AppState>();
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = trust::load_settings(&db);
        let key = trust::key_bytes(&settings);
        let (body, level) = unpack(&bytes, key.as_deref())?;
        let mut summary = ImportSummary {
            station: body.station.clone(),
            created_at: body.created_at,
            trust: level,
            records_in_bundle: body.records.len(),
            merged: None,
        };
        if !apply {
            return Ok(summary);
        }
        may_import(level, key.is_some(), accept_unverified.unwrap_or(false))?;
        summary.merged = Some(store::merge_raw(&db, &body.records, &body.station)?);
        // Notes that arrived need publishing into the library so chat can find them.
        if body.records.iter().any(|r| r.kind == super::memory::KIND) {
            super::memory::republish_all(&db, &app_data_dir);
        }
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn record(id: &str, kind: &str) -> RawRecord {
        RawRecord {
            id: id.to_string(),
            kind: kind.to_string(),
            data: json!({"title": "t", "body": "b"}),
            created_at: 100,
            updated_at: 200,
            deleted: false,
            origin: None,
        }
    }

    fn body(records: Vec<RawRecord>) -> BundleBody {
        BundleBody {
            format: BUNDLE_FORMAT.to_string(),
            station: "Ridge".to_string(),
            created_at: 1_800_000_000,
            groups: vec!["knowledge".to_string()],
            records,
        }
    }

    #[test]
    fn signed_bundle_round_trips_and_verifies() {
        let b = body(vec![record("a", "memory_note")]);
        let bytes = pack(&b, Some(KEY)).unwrap();
        let (out, level) = unpack(&bytes, Some(KEY)).unwrap();
        assert_eq!(out, b);
        assert_eq!(level, BundleTrust::Verified);
    }

    #[test]
    fn a_changed_bundle_is_detected() {
        let bytes = pack(&body(vec![record("a", "memory_note")]), Some(KEY)).unwrap();
        let tampered = String::from_utf8(bytes).unwrap().replace("Ridge", "Rogue");
        let (_, level) = unpack(tampered.as_bytes(), Some(KEY)).unwrap();
        assert_eq!(level, BundleTrust::Tampered);
        assert!(may_import(level, true, true).is_err(), "tampered bundles are refused even when unverified is accepted");
    }

    #[test]
    fn a_bundle_from_another_group_is_refused() {
        let bytes = pack(&body(vec![record("a", "memory_note")]), Some(b"some other group's key, 32 bytes")).unwrap();
        let (_, level) = unpack(&bytes, Some(KEY)).unwrap();
        assert_eq!(level, BundleTrust::Tampered);
    }

    #[test]
    fn unsigned_and_uncheckable_bundles_need_an_explicit_yes_when_it_matters() {
        let unsigned = pack(&body(vec![]), None).unwrap();
        assert_eq!(unpack(&unsigned, Some(KEY)).unwrap().1, BundleTrust::Unsigned);
        assert!(may_import(BundleTrust::Unsigned, true, false).is_err());
        assert!(may_import(BundleTrust::Unsigned, true, true).is_ok());
        // Two stations with no keys at all can still exchange bundles.
        assert!(may_import(BundleTrust::Unsigned, false, false).is_ok());

        let signed = pack(&body(vec![]), Some(KEY)).unwrap();
        assert_eq!(unpack(&signed, None).unwrap().1, BundleTrust::CannotCheck);
        assert!(may_import(BundleTrust::CannotCheck, false, false).is_err());
        assert!(may_import(BundleTrust::CannotCheck, false, true).is_ok());
    }

    #[test]
    fn records_that_may_not_travel_are_dropped_on_import() {
        let b = body(vec![record("a", "memory_note"), record("k", "trust_settings"), record("w", "watch_state")]);
        let bytes = pack(&b, None).unwrap();
        let (out, _) = unpack(&bytes, None).unwrap();
        assert_eq!(out.records.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(), vec!["memory_note"]);
    }

    #[test]
    fn garbage_and_unknown_formats_are_rejected_clearly() {
        assert!(unpack(b"not json at all", None).unwrap_err().contains("not an ATHENA courier bundle"));
        let mut b = body(vec![]);
        b.format = "athena-courier-99".to_string();
        let err = unpack(&pack(&b, None).unwrap(), None).unwrap_err();
        assert!(err.contains("athena-courier-99"), "{err}");
    }

    #[test]
    fn groups_expand_to_their_kinds_and_keys_never_travel() {
        assert_eq!(kinds_for(&["knowledge".to_string()]), vec!["memory_note"]);
        assert!(kinds_for(&["coordination".to_string(), "ledger".to_string()]).contains(&"ledger_entry"));
        assert!(kinds_for(&["nonsense".to_string()]).is_empty());
        for (_, _, kinds) in GROUPS {
            assert!(!kinds.contains(&"trust_settings") && !kinds.contains(&"silence_override") && !kinds.contains(&"watch_state"));
        }
    }

    #[test]
    fn a_bundle_merges_into_another_station() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = crate::db::Database::new(&dir_a.path().join("a.db")).unwrap();
        let b = crate::db::Database::new(&dir_b.path().join("b.db")).unwrap();
        let note = crate::field_ops::memory::MemoryNote {
            title: "North well".into(),
            body: "Contaminated.".into(),
            tags: vec![],
            author: Some("Ana".into()),
        };
        store::put(&a, "memory_note", None, &note).unwrap();

        let bundle = body(store::export_raw(&a, &kinds_for(&["knowledge".to_string()])).unwrap());
        let bytes = pack(&bundle, Some(KEY)).unwrap();
        let (unpacked, level) = unpack(&bytes, Some(KEY)).unwrap();
        may_import(level, true, false).unwrap();
        let outcome = store::merge_raw(&b, &unpacked.records, &unpacked.station).unwrap();
        assert_eq!(outcome.added, 1);
        let got = store::list::<crate::field_ops::memory::MemoryNote>(&b, "memory_note").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].data.title, "North well");
        // Importing the same stick again changes nothing.
        let again = store::merge_raw(&b, &unpacked.records, &unpacked.station).unwrap();
        assert_eq!(again.added + again.updated, 0);
    }
}
