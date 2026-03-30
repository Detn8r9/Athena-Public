use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System as SysinfoSystem};

#[derive(Debug, Clone, Serialize)]
pub struct SystemStatsPayload {
    pub total_memory: f64,
    pub used_memory: f64,
    pub cpu_usage: f32,
    /// Logical processors (SMT threads).
    pub logical_cpus: u32,
    /// Physical cores when the OS reports them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_cores: Option<u32>,
    pub uptime_secs: u64,
    /// Unix-style load averages (Windows often reports zeros).
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_brand: Option<String>,
}

/// CPU usage in sysinfo is a **delta between two refreshes on the same `System`**. A fresh
/// `System::new()` every request makes the reading look “stuck” (often ~same % on macOS).
static SYSTEM_SNAPSHOT: OnceLock<Mutex<SysinfoSystem>> = OnceLock::new();

fn system_lock() -> &'static Mutex<SysinfoSystem> {
    SYSTEM_SNAPSHOT.get_or_init(|| {
        Mutex::new(SysinfoSystem::new_with_specifics(
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

    let mut system = system_lock()
        .lock()
        .map_err(|e| format!("System stats lock: {e}"))?;

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

    let logical_cpus = cpus.len() as u32;
    let physical_cores = system.physical_core_count().map(|n| n as u32);
    let cpu_brand = cpus
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty());

    let load = SysinfoSystem::load_average();
    let uptime_secs = SysinfoSystem::uptime();

    Ok(SystemStatsPayload {
        total_memory: (system.total_memory() as f64) / BYTES_PER_GB,
        used_memory: (system.used_memory() as f64) / BYTES_PER_GB,
        cpu_usage,
        logical_cpus,
        physical_cores,
        uptime_secs,
        load_one: load.one,
        load_five: load.five,
        load_fifteen: load.fifteen,
        host_name: SysinfoSystem::host_name(),
        os_name: SysinfoSystem::name(),
        kernel_version: SysinfoSystem::kernel_version(),
        long_os_version: SysinfoSystem::long_os_version(),
        cpu_arch: SysinfoSystem::cpu_arch(),
        cpu_brand,
    })
}
