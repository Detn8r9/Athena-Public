use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

#[derive(Debug, Clone, Serialize)]
pub struct SystemStatsPayload {
    pub total_memory: f64,
    pub used_memory: f64,
    pub cpu_usage: f32,
}

/// CPU usage in sysinfo is a **delta between two refreshes on the same `System`**. A fresh
/// `System::new()` every request makes the reading look “stuck” (often ~same % on macOS).
static SYSTEM_SNAPSHOT: OnceLock<Mutex<System>> = OnceLock::new();

fn system_lock() -> &'static Mutex<System> {
    SYSTEM_SNAPSHOT.get_or_init(|| {
        Mutex::new(System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        ))
    })
}

#[tauri::command]
pub async fn get_system_stats() -> Result<SystemStatsPayload, String> {
    use std::thread;
    use std::time::Duration;

    let mut system = system_lock().lock().map_err(|e| format!("System stats lock: {e}"))?;

    system.refresh_memory();
    // Establish baseline, then measure CPU over an interval (required for non-stale usage %).
    system.refresh_cpu_usage();
    thread::sleep(Duration::from_millis(220));
    system.refresh_cpu_usage();

    const BYTES_PER_GB: f64 = 1_073_741_824.0;

    let cpus = system.cpus();
    let cpu_usage = if cpus.is_empty() {
        system.global_cpu_info().cpu_usage()
    } else {
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
    };

    Ok(SystemStatsPayload {
        total_memory: (system.total_memory() as f64) / BYTES_PER_GB,
        used_memory: (system.used_memory() as f64) / BYTES_PER_GB,
        cpu_usage,
    })
}
