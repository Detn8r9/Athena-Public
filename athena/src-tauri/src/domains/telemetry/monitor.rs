//! Field telemetry & power-save monitoring (Chunk 11).
//!
//! Runs on a dedicated thread: polls battery + disk every 30s, emits telemetry events, and toggles
//! global power-save mode when battery drops below 20%.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use sysinfo::Disks;
use tauri::{AppHandle, Emitter};

use crate::system;

pub const EVT_TELEMETRY_UPDATE: &str = "athena:telemetry-update";
pub const EVT_POWER_SAVE_MODE: &str = "athena:power-save-mode";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryUpdatePayload {
    pub battery_pct: Option<f32>,
    pub battery_charging: Option<bool>,
    pub disk_available_bytes: Option<u64>,
    pub disk_total_bytes: Option<u64>,
    pub athena_root: String,
}

fn pick_disk_index_for_path(disks: &Disks, path: &PathBuf) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None; // (prefix_len, idx)
    for (idx, d) in disks.list().iter().enumerate() {
        let mp = d.mount_point();
        if path.starts_with(mp) {
            let plen = mp.as_os_str().len();
            if best.map(|(b, _)| plen > b).unwrap_or(true) {
                best = Some((plen, idx));
            }
        }
    }
    best.map(|(_, idx)| idx)
}

pub fn spawn_telemetry_monitor(
    app: AppHandle,
    athena_root: PathBuf,
    power_save_mode: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        // Efficiency: create sysinfo objects once, refresh only what we need.
        let mut disks = Disks::new_with_refreshed_list();
        let mut picked_disk_idx = pick_disk_index_for_path(&disks, &athena_root);

        let mut last_power_save = power_save_mode.load(Ordering::Relaxed);
        loop {
            let battery = system::get_battery_state().unwrap_or(system::BatteryState {
                percentage: None,
                is_charging: None,
                time_to_empty_minutes: None,
            });

            disks.refresh();
            if picked_disk_idx.is_none() || picked_disk_idx.unwrap_or(0) >= disks.list().len() {
                picked_disk_idx = pick_disk_index_for_path(&disks, &athena_root);
            }
            let (avail, total) = if let Some(idx) = picked_disk_idx {
                let d = &disks.list()[idx];
                (Some(d.available_space()), Some(d.total_space()))
            } else {
                (None, None)
            };

            let _ = app.emit(
                EVT_TELEMETRY_UPDATE,
                TelemetryUpdatePayload {
                    battery_pct: battery.percentage,
                    battery_charging: battery.is_charging,
                    disk_available_bytes: avail,
                    disk_total_bytes: total,
                    athena_root: athena_root.to_string_lossy().to_string(),
                },
            );

            let pct = battery.percentage.unwrap_or(100.0);
            let charging = battery.is_charging.unwrap_or(false);
            let want_power_save = !charging && pct.is_finite() && pct < 20.0;

            if want_power_save != last_power_save {
                power_save_mode.store(want_power_save, Ordering::Relaxed);
                last_power_save = want_power_save;
                let _ = app.emit(EVT_POWER_SAVE_MODE, want_power_save);
            }

            thread::sleep(Duration::from_secs(30));
        }
    });
}

