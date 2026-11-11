//! Record store shared by the Field Ops features.
//!
//! Every feature keeps its data as typed records in one table, keyed by `kind`. Each record has a
//! stable id, an `updated_at` stamp and a tombstone flag, which is exactly what is needed to merge
//! two stations' data without duplicates: newest `updated_at` wins, deletions travel as tombstones.
//! Courier bundles and LAN sync can therefore carry any feature's records without knowing what
//! they mean.

use rusqlite::{params, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::db::Database;

/// One stored record, with its payload still as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRecord {
    pub id: String,
    pub kind: String,
    pub data: serde_json::Value,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds. Newest wins on merge.
    pub updated_at: i64,
    pub deleted: bool,
    /// Station that last wrote the record, for provenance after a merge.
    pub origin: Option<String>,
}

/// A record with its payload decoded.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record<T> {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(flatten)]
    pub data: T,
}

pub fn ensure_schema(db: &Database) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS field_records (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                origin TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_field_records_kind
                ON field_records(kind, deleted, updated_at DESC);
            ",
        )
        .map_err(|e| e.to_string())
    })
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRecord> {
    let data_json: String = row.get(2)?;
    Ok(RawRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        data: serde_json::from_str(&data_json).unwrap_or(serde_json::Value::Null),
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        deleted: row.get::<_, i64>(5)? != 0,
        origin: row.get(6)?,
    })
}

const SELECT: &str =
    "SELECT id, kind, data_json, created_at, updated_at, deleted, origin FROM field_records";

/// Insert a new record or replace an existing one with the same id. Returns the stored record.
pub fn put<T: Serialize + DeserializeOwned>(
    db: &Database,
    kind: &str,
    id: Option<&str>,
    data: &T,
) -> Result<Record<T>, String> {
    put_at(db, kind, id, data, now())
}

/// [`put`] with an explicit timestamp, for tests and imports.
pub fn put_at<T: Serialize + DeserializeOwned>(
    db: &Database,
    kind: &str,
    id: Option<&str>,
    data: &T,
    at: i64,
) -> Result<Record<T>, String> {
    let id = id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(new_id);
    let json = serde_json::to_string(data).map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO field_records (id, kind, data_json, created_at, updated_at, deleted, origin)
             VALUES (?1, ?2, ?3, ?4, ?4, 0, NULL)
             ON CONFLICT(id) DO UPDATE SET
               data_json = excluded.data_json,
               updated_at = excluded.updated_at,
               deleted = 0",
            params![id, kind, json, at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    get::<T>(db, kind, &id)?.ok_or_else(|| "Record was not stored.".to_string())
}

pub fn get<T: DeserializeOwned>(db: &Database, kind: &str, id: &str) -> Result<Option<Record<T>>, String> {
    let raw = db.with_conn(|conn| {
        conn.query_row(
            &format!("{SELECT} WHERE id = ?1 AND kind = ?2 AND deleted = 0"),
            params![id, kind],
            row_to_raw,
        )
        .optional()
        .map_err(|e| e.to_string())
    })?;
    raw.map(decode).transpose()
}

fn decode<T: DeserializeOwned>(raw: RawRecord) -> Result<Record<T>, String> {
    let data: T = serde_json::from_value(raw.data)
        .map_err(|e| format!("Stored {} record {} could not be read: {e}", raw.kind, raw.id))?;
    Ok(Record {
        id: raw.id,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        data,
    })
}

/// All live records of a kind, oldest first. Records that no longer decode are skipped, so one
/// bad row cannot blank a screen.
pub fn list<T: DeserializeOwned>(db: &Database, kind: &str) -> Result<Vec<Record<T>>, String> {
    let raws = db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(&format!(
                "{SELECT} WHERE kind = ?1 AND deleted = 0 ORDER BY created_at ASC, id ASC"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![kind], row_to_raw)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })?;
    Ok(raws.into_iter().filter_map(|r| decode(r).ok()).collect())
}

/// Mark a record deleted. The tombstone is kept so the deletion can sync to other stations.
pub fn remove(db: &Database, kind: &str, id: &str) -> Result<bool, String> {
    let at = now();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE field_records SET deleted = 1, updated_at = ?3 WHERE id = ?1 AND kind = ?2 AND deleted = 0",
            params![id, kind, at],
        )
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
    })
}

/// Every record of the given kinds, including tombstones, for export to another station.
pub fn export_raw(db: &Database, kinds: &[&str]) -> Result<Vec<RawRecord>, String> {
    let mut out = Vec::new();
    for kind in kinds {
        let mut rows = db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&format!("{SELECT} WHERE kind = ?1 ORDER BY created_at ASC, id ASC"))
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![kind], row_to_raw)
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
        })?;
        out.append(&mut rows);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    pub added: usize,
    pub updated: usize,
    /// Incoming records that were older than, or the same as, what this station already had.
    pub unchanged: usize,
}

/// Merge records from another station. For each id the newest `updated_at` wins; ties keep the
/// local copy. Deletions arrive as tombstones and win the same way.
pub fn merge_raw(db: &Database, incoming: &[RawRecord], origin: &str) -> Result<MergeOutcome, String> {
    let mut outcome = MergeOutcome::default();
    db.with_conn(|conn| {
        for rec in incoming {
            let local_updated: Option<i64> = conn
                .query_row(
                    "SELECT updated_at FROM field_records WHERE id = ?1",
                    params![rec.id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let json = serde_json::to_string(&rec.data).map_err(|e| e.to_string())?;
            let from = rec.origin.clone().unwrap_or_else(|| origin.to_string());
            match local_updated {
                None => {
                    conn.execute(
                        "INSERT INTO field_records (id, kind, data_json, created_at, updated_at, deleted, origin)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![rec.id, rec.kind, json, rec.created_at, rec.updated_at, rec.deleted as i64, from],
                    )
                    .map_err(|e| e.to_string())?;
                    outcome.added += 1;
                }
                Some(local) if rec.updated_at > local => {
                    conn.execute(
                        "UPDATE field_records SET kind = ?2, data_json = ?3, updated_at = ?4, deleted = ?5, origin = ?6 WHERE id = ?1",
                        params![rec.id, rec.kind, json, rec.updated_at, rec.deleted as i64, from],
                    )
                    .map_err(|e| e.to_string())?;
                    outcome.updated += 1;
                }
                Some(_) => outcome.unchanged += 1,
            }
        }
        Ok(())
    })?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Note {
        text: String,
    }

    fn db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = Database::new(&dir.path().join("t.db")).expect("open db");
        (dir, db)
    }

    #[test]
    fn put_get_list_and_remove() {
        let (_d, db) = db();
        let a = put_at(&db, "note", None, &Note { text: "first".into() }, 100).unwrap();
        let b = put_at(&db, "note", None, &Note { text: "second".into() }, 200).unwrap();
        put_at(&db, "other", None, &Note { text: "elsewhere".into() }, 300).unwrap();

        let listed: Vec<Record<Note>> = list(&db, "note").unwrap();
        assert_eq!(listed.iter().map(|r| r.data.text.as_str()).collect::<Vec<_>>(), vec!["first", "second"]);

        assert_eq!(get::<Note>(&db, "note", &a.id).unwrap().unwrap().data.text, "first");
        assert!(get::<Note>(&db, "other", &a.id).unwrap().is_none(), "kind must match");

        assert!(remove(&db, "note", &b.id).unwrap());
        assert!(!remove(&db, "note", &b.id).unwrap(), "second delete is a no-op");
        assert_eq!(list::<Note>(&db, "note").unwrap().len(), 1);
    }

    #[test]
    fn put_with_existing_id_updates_in_place_and_keeps_created_at() {
        let (_d, db) = db();
        let a = put_at(&db, "note", None, &Note { text: "v1".into() }, 100).unwrap();
        let b = put_at(&db, "note", Some(&a.id), &Note { text: "v2".into() }, 500).unwrap();
        assert_eq!(b.id, a.id);
        assert_eq!(b.created_at, 100);
        assert_eq!(b.updated_at, 500);
        assert_eq!(list::<Note>(&db, "note").unwrap().len(), 1);
    }

    #[test]
    fn merge_newest_wins_and_tombstones_travel() {
        let (_d1, base) = db();
        let (_d2, field) = db();

        let shared = put_at(&base, "note", None, &Note { text: "base v1".into() }, 100).unwrap();
        let doomed = put_at(&base, "note", None, &Note { text: "to delete".into() }, 100).unwrap();

        // First courier run: field station receives both.
        let first = merge_raw(&field, &export_raw(&base, &["note"]).unwrap(), "base").unwrap();
        assert_eq!(first, MergeOutcome { added: 2, updated: 0, unchanged: 0 });

        // Field edits one and adds its own; base deletes the other.
        put_at(&field, "note", Some(&shared.id), &Note { text: "field v2".into() }, 900).unwrap();
        put_at(&field, "note", None, &Note { text: "field only".into() }, 950).unwrap();
        assert!(remove(&base, "note", &doomed.id).unwrap());

        // Courier back to base.
        let back = merge_raw(&base, &export_raw(&field, &["note"]).unwrap(), "field").unwrap();
        assert_eq!(back.added, 1);
        assert_eq!(back.updated, 1);
        let base_notes: Vec<String> = list::<Note>(&base, "note").unwrap().into_iter().map(|r| r.data.text).collect();
        assert_eq!(base_notes, vec!["field v2", "field only"]);

        // And out to the field again: the deletion arrives, nothing else changes.
        let again = merge_raw(&field, &export_raw(&base, &["note"]).unwrap(), "base").unwrap();
        assert_eq!(again.updated, 1, "only the tombstone is newer");
        let field_notes: Vec<String> = list::<Note>(&field, "note").unwrap().into_iter().map(|r| r.data.text).collect();
        assert_eq!(field_notes, vec!["field v2", "field only"]);

        // Merging the same bundle twice changes nothing.
        let repeat = merge_raw(&field, &export_raw(&base, &["note"]).unwrap(), "base").unwrap();
        assert_eq!(repeat.added + repeat.updated, 0);
    }
}
