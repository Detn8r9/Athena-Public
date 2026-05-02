//! Field logging engine (Chunk 12): `tracing` + non-blocking daily rolling files under AthenaData/logs/.
//! Keeps at most 7 days of logs to prevent disk bloat.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

fn logs_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("logs")
}

fn prune_old_logs(dir: &Path, days: u64) {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(days.saturating_mul(86400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let entries = match fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".log") {
            continue;
        }
        let meta = match ent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified < cutoff {
            let _ = fs::remove_file(p);
        }
    }
}

pub fn init_field_logging(app_data_dir: &Path) {
    if LOG_GUARD.get().is_some() {
        return;
    }

    let dir = logs_dir(app_data_dir);
    let _ = fs::create_dir_all(&dir);
    prune_old_logs(&dir, 7);

    // Daily rolling `athena-YYYY-MM-DD.log` (non-blocking writer).
    let file_appender = tracing_appender::rolling::daily(&dir, "athena.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,athena=info"));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
    let _ = LOG_GUARD.set(guard);

    // Ensure something useful shows up immediately, even on first run.
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis();
    tracing::info!(unix_ms = unix_ms as u64, logs_dir = %dir.display(), "field logging initialized");
}

