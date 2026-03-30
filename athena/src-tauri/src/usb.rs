//! Removable disk detection and (macOS) whole-disk formatting for USB export.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use sysinfo::Disks;

#[derive(Debug, Clone, Serialize)]
pub struct UsbDrive {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub device_path: String,
}

#[tauri::command]
pub fn get_available_usbs() -> Result<Vec<UsbDrive>, String> {
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for disk in disks.list() {
        if !disk.is_removable() {
            continue;
        }
        let mount = disk.mount_point();
        let mount_str = mount.to_string_lossy().to_string();
        if mount_str.is_empty() || mount_str == "/" {
            continue;
        }
        let name = disk
            .name()
            .to_str()
            .unwrap_or("USB")
            .trim()
            .to_string();
        let name = if name.is_empty() {
            "Removable".to_string()
        } else {
            name
        };
        let device_path = device_path_for_mount(mount);
        out.push(UsbDrive {
            name,
            mount_point: mount_str,
            total_space: disk.total_space(),
            device_path,
        });
    }
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    Ok(out)
}

fn device_path_for_mount(mount: &Path) -> String {
    #[cfg(target_os = "macos")]
    {
        return macos_diskutil_whole_disk_for_mount(mount).unwrap_or_default();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mount;
        String::new()
    }
}

#[cfg(target_os = "macos")]
fn macos_diskutil_whole_disk_for_mount(mount: &Path) -> Option<String> {
    let m = mount.to_str()?;
    let output = Command::new("/usr/sbin/diskutil")
        .args(["info", "-plist", m])
        .output()
        .ok()?;
    if output.status.success() {
        if let Ok(v) = plist::from_bytes::<plist::Value>(&output.stdout) {
            if let Some(id) = macos_plist_parent_whole_disk(&v) {
                return Some(format!("/dev/{id}"));
            }
        }
    } else {
        eprintln!(
            "[athena] usb: diskutil info -plist failed for {}",
            mount.display()
        );
    }
    macos_diskutil_whole_disk_text(mount)
}

#[cfg(target_os = "macos")]
fn macos_plist_parent_whole_disk(v: &plist::Value) -> Option<String> {
    let d = v.as_dictionary()?;
    for key in ["ParentWholeDisk", "ParentWholeDiskIdentifier"] {
        if let Some(plist::Value::String(s)) = d.get(key) {
            let t = s.trim();
            if t.starts_with("disk") && !t.contains('s') {
                return Some(t.to_string());
            }
            if t.starts_with("disk") {
                return Some(whole_disk_id_from_partition_id(t));
            }
        }
    }
    if let Some(plist::Value::String(s)) = d.get("DeviceIdentifier") {
        let t = s.trim();
        if t.starts_with("disk") {
            return Some(whole_disk_id_from_partition_id(t));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn whole_disk_id_from_partition_id(id: &str) -> String {
    if let Some(pos) = id.rfind('s') {
        if id[pos + 1..].chars().all(|c| c.is_ascii_digit()) {
            return id[..pos].to_string();
        }
    }
    id.to_string()
}

#[cfg(target_os = "macos")]
fn macos_diskutil_whole_disk_text(mount: &Path) -> Option<String> {
    let output = Command::new("/usr/sbin/diskutil")
        .args(["info", mount.to_str()?])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    for line in s.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Part of Whole:") {
            let id = rest.trim();
            if id.starts_with("disk") {
                return Some(format!("/dev/{id}"));
            }
        }
    }
    None
}

/// Erases the whole disk as ExFAT "ATHENA" (MBR) and returns the mount point (typically `/Volumes/ATHENA`).
#[cfg(target_os = "macos")]
pub fn macos_erase_disk_exfat_and_resolve_mount(device_path: &str) -> Result<PathBuf, String> {
    let dev = device_path.trim();
    if !dev.starts_with("/dev/disk") {
        return Err(format!(
            "Invalid device path for formatting (expected /dev/diskN): {dev}"
        ));
    }
    eprintln!(
        "[athena] usb format: running diskutil eraseDisk ExFAT ATHENA MBRFormat {}",
        dev
    );
    let out = Command::new("/usr/sbin/diskutil")
        .args(["eraseDisk", "ExFAT", "ATHENA", "MBRFormat", dev])
        .output()
        .map_err(|e| {
            eprintln!(
                "[athena] usb format: failed to spawn diskutil (Permission Denied or missing /usr/sbin/diskutil): {e}"
            );
            e.to_string()
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        eprintln!("[athena] usb format: diskutil eraseDisk failed. stdout: {stdout}");
        eprintln!("[athena] usb format: diskutil eraseDisk failed. stderr: {stderr}");
        let combined = format!("{stdout} {stderr}");
        let hint = if combined.contains("Permission denied")
            || combined.contains("permission denied")
        {
            " Permission denied: grant Full Disk Access to ATHENA in System Settings, or run from an admin context."
        } else if combined.contains("Resource busy")
            || combined.contains("resource busy")
            || combined.contains("Could not unmount")
        {
            " Resource busy: quit apps using this volume, eject the drive in Finder, reinsert it, and try again."
        } else {
            ""
        };
        return Err(format!(
            "diskutil eraseDisk failed (status {}).{}",
            out.status, hint
        ));
    }
    thread::sleep(Duration::from_secs(2));
    let ath = PathBuf::from("/Volumes/ATHENA");
    for _ in 0..30 {
        if ath.is_dir() {
            eprintln!(
                "[athena] usb format: volume mounted at {}",
                ath.display()
            );
            return Ok(ath);
        }
        thread::sleep(Duration::from_millis(400));
    }
    Err(
        "Format finished but /Volumes/ATHENA did not appear in time. Check Disk Utility or unplug/replug the drive."
            .to_string(),
    )
}
