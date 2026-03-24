use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::Serialize;
use sunrise::{Coordinates, SolarDay, SolarEvent};

#[derive(Serialize)]
pub struct LocalTimeInfo {
    pub iso: String,
    /// Offset from UTC in minutes.
    pub offset_minutes: i32,
}

#[derive(Serialize)]
pub struct SunTimes {
    /// Local sunrise time in RFC 3339 format for today.
    pub sunrise_iso: String,
    /// Local sunset time in RFC 3339 format for today.
    pub sunset_iso: String,
}

pub fn get_local_time() -> LocalTimeInfo {
    let now = Local::now();
    let offset_minutes = now.offset().local_minus_utc() / 60;
    LocalTimeInfo {
        iso: now.to_rfc3339(),
        offset_minutes,
    }
}

pub fn get_sun_times(lat: f64, lon: f64) -> Result<SunTimes, String> {
    let date: NaiveDate = Local::now().date_naive();
    let coords = Coordinates::new(lat, lon).ok_or_else(|| "Invalid coordinates".to_string())?;
    let day = SolarDay::new(coords, date);

    let sunrise_utc: DateTime<Utc> = day.event_time(SolarEvent::Sunrise);
    let sunset_utc: DateTime<Utc> = day.event_time(SolarEvent::Sunset);
    let sunrise = sunrise_utc.with_timezone(&Local).to_rfc3339();
    let sunset = sunset_utc.with_timezone(&Local).to_rfc3339();

    Ok(SunTimes {
        sunrise_iso: sunrise,
        sunset_iso: sunset,
    })
}

#[derive(serde::Serialize)]
pub struct BatteryState {
    /// Battery percentage from 0–100, if available.
    pub percentage: Option<f32>,
    /// Whether the system is currently charging (or on AC power), if detectable.
    pub is_charging: Option<bool>,
    /// Estimated minutes until empty when discharging, if the OS reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_to_empty_minutes: Option<u32>,
}

/// Parses `pmset -g batt` lines for `2:30 remaining` / `(no estimate)`.
fn parse_mac_time_to_empty_minutes(line: &str) -> Option<u32> {
    let lower = line.to_lowercase();
    if lower.contains("(no estimate)")
        || lower.contains("not charging")
        || lower.contains("charged;")
        || lower.contains("ac attached")
    {
        return None;
    }
    if !lower.contains("remaining") {
        return None;
    }
    let before_remaining = line.split("remaining").next().unwrap_or("");
    for token in before_remaining.split_whitespace().rev() {
        let t = token.trim_matches(|c| c == '(' || c == ')' || c == ';');
        if t.contains(':') {
            let parts: Vec<&str> = t.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    return Some(h.saturating_mul(60).saturating_add(m));
                }
            } else if parts.len() == 3 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    return Some(h.saturating_mul(60).saturating_add(m));
                }
            }
        }
    }
    None
}

fn parse_mac_battery_output(output: &str) -> BatteryState {
    // Example `pmset -g batt` output:
    // Now drawing from 'Battery Power'
    //  -InternalBattery-0 (id=1234567)   86%; discharging; (no estimate) present: true
    //
    // Or when plugged in:
    // Now drawing from 'AC Power'
    //  -InternalBattery-0 (id=1234567)   100%; charged; AC attached; not charging present: true
    let mut percentage: Option<f32> = None;
    let mut is_charging: Option<bool> = None;
    let mut time_to_empty_minutes: Option<u32> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains('%') {
            // Pick the first token ending with '%'
            for token in trimmed.split_whitespace() {
                if let Some(stripped) = token.strip_suffix('%') {
                    if let Ok(val) = stripped.parse::<f32>() {
                        percentage = Some(val);
                        break;
                    }
                }
            }

            let lower = trimmed.to_lowercase();
            if lower.contains("charging")
                || lower.contains("ac attached")
                || lower.contains("ac power")
            {
                is_charging = Some(true);
            } else if lower.contains("discharging") {
                is_charging = Some(false);
            }
            if time_to_empty_minutes.is_none() {
                time_to_empty_minutes = parse_mac_time_to_empty_minutes(trimmed);
            }
        } else if trimmed.to_lowercase().contains("ac power") {
            // Header line saying "Now drawing from 'AC Power'"
            if is_charging.is_none() {
                is_charging = Some(true);
            }
        }
        if time_to_empty_minutes.is_none() {
            time_to_empty_minutes = parse_mac_time_to_empty_minutes(trimmed);
        }
    }

    BatteryState {
        percentage,
        is_charging,
        time_to_empty_minutes,
    }
}

#[cfg(target_os = "linux")]
fn read_linux_battery_sysfs(name: &str) -> Option<(f32, Option<bool>)> {
    use std::fs;
    use std::path::Path;

    let base = Path::new("/sys/class/power_supply");
    let cap_path = base.join(name).join("capacity");
    let status_path = base.join(name).join("status");
    let pct = fs::read_to_string(&cap_path)
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .map(|p| p.clamp(0.0, 100.0))?;
    let charging = fs::read_to_string(&status_path).ok().and_then(|s| {
        let status = s.trim().to_lowercase();
        if status.contains("charging") || status == "full" {
            Some(true)
        } else if status.contains("discharging") {
            Some(false)
        } else {
            None
        }
    });
    Some((pct, charging))
}

#[cfg(target_os = "linux")]
fn read_linux_battery() -> BatteryState {
    use std::fs;
    use std::path::Path;

    let mut percentage: Option<f32> = None;
    let mut is_charging: Option<bool> = None;
    let mut time_to_empty_minutes: Option<u32> = None;
    let mut bat_name: Option<String> = None;

    for name in ["BAT0", "BAT1"] {
        if let Some((pct, ch)) = read_linux_battery_sysfs(name) {
            percentage = Some(pct);
            is_charging = ch;
            bat_name = Some(name.to_string());
            break;
        }
    }
    if percentage.is_none() {
        let base = Path::new("/sys/class/power_supply");
        if let Ok(entries) = fs::read_dir(base) {
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name.starts_with("BAT") && path.join("capacity").exists() {
                        let cap_path = path.join("capacity");
                        let status_path = path.join("status");
                        if let Ok(s) = fs::read_to_string(&cap_path) {
                            if let Ok(pct) = s.trim().parse::<f32>() {
                                percentage = Some(pct.clamp(0.0, 100.0));
                            }
                        }
                        if let Ok(s) = fs::read_to_string(&status_path) {
                            let status = s.trim().to_lowercase();
                            if status.contains("charging") || status == "full" {
                                is_charging = Some(true);
                            } else if status.contains("discharging") {
                                is_charging = Some(false);
                            }
                        }
                        bat_name = Some(name.to_string());
                        break;
                    }
                }
            }
        }
    }

    if time_to_empty_minutes.is_none() {
        if let Some(ref n) = bat_name {
            time_to_empty_minutes = read_linux_time_to_empty_minutes(n);
        }
    }

    BatteryState {
        percentage,
        is_charging,
        time_to_empty_minutes,
    }
}

#[cfg(target_os = "linux")]
fn read_linux_time_to_empty_minutes(bat_name: &str) -> Option<u32> {
    use std::fs;
    use std::path::Path;
    let path = Path::new("/sys/class/power_supply")
        .join(bat_name)
        .join("time_to_empty");
    let s = fs::read_to_string(&path).ok()?;
    let sec: u32 = s.trim().parse().ok()?;
    if sec == 0 || sec == u32::MAX {
        return None;
    }
    Some((sec / 60).max(1))
}

pub fn get_battery_state() -> Result<BatteryState, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        match Command::new("pmset").arg("-g").arg("batt").output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                Ok(parse_mac_battery_output(&text))
            }
            Ok(out) => Err(format!(
                "pmset exited with status {}",
                out.status.code().unwrap_or(-1)
            )),
            Err(e) => Err(format!("Failed to run pmset: {}", e)),
        }
    }

    #[cfg(target_os = "linux")]
    {
        Ok(read_linux_battery())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(BatteryState {
            percentage: None,
            is_charging: None,
            time_to_empty_minutes: None,
        })
    }
}
