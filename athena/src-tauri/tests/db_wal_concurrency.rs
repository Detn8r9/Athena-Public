//! Phase 1: stress concurrent readers/writers on a single SQLite file with WAL enabled.
//! Ensures we do not surface `database is locked` under parallel load.

use athena_lib::Database;
use std::sync::{Arc, Mutex};
use std::thread;
use tempfile::tempdir;

#[test]
fn wal_concurrent_read_write_avoids_database_locked() {
    let dir = tempdir().expect("temp dir");
    let db_path = dir.path().join("stress.db");
    let db = Arc::new(Database::new(&db_path).expect("open db"));

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    thread::scope(|s| {
        for worker in 0..8 {
            let db = Arc::clone(&db);
            let errors = Arc::clone(&errors);
            s.spawn(move || {
                for i in 0..100 {
                    let path = format!("stress/w{}/file_{}.txt", worker, i);
                    if let Err(e) = db.upsert_file(
                        &path,
                        "text",
                        "name",
                        Some(1),
                        None,
                        None,
                        None,
                    ) {
                        errors.lock().expect("errors lock").push(format!("upsert {path}: {e}"));
                    }
                    if let Err(e) = db.get_by_path(&path) {
                        errors.lock().expect("errors lock").push(format!("get {path}: {e}"));
                    }
                }
            });
        }
    });

    let errs = errors.lock().expect("errors lock");
    assert!(
        errs.is_empty(),
        "unexpected DB errors under concurrent WAL load: {:?}",
        *errs
    );
}
