//! Sensor network (S19): readings from the things you cannot stand next to all day.
//!
//! A sensor is anything that produces a number over time: a water tank level, a soil probe, a
//! Geiger counter, a perimeter trip, a battery. Readings arrive from Meshtastic nodes that report
//! telemetry, or are typed in by hand. Each sensor can have a low limit, a high limit and a
//! "gone quiet" time; crossing any of them raises a watch officer finding.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const SENSOR_KIND: &str = "sensor";
pub const READING_KIND: &str = "sensor_reading";
/// Readings kept per sensor. Older ones are dropped.
const MAX_READINGS_PER_SENSOR: usize = 300;
const MINUTE: i64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sensor {
    pub name: String,
    /// What it measures: `temperature`, `humidity`, `pressure`, `battery`, `water_level`, …
    pub metric: String,
    pub unit: String,
    /// Meshtastic node that reports it, when it is a radio sensor.
    #[serde(default)]
    pub node_id: Option<i64>,
    #[serde(default)]
    pub min_alert: Option<f64>,
    #[serde(default)]
    pub max_alert: Option<f64>,
    /// Raise a finding when no reading has arrived for this long.
    #[serde(default)]
    pub stale_after_minutes: Option<i64>,
    /// An out-of-range reading on an urgent sensor is "act now" rather than "watch".
    #[serde(default)]
    pub urgent: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub sensor_id: String,
    /// Unix seconds.
    pub at: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorState {
    NoData,
    /// Nothing heard for longer than the sensor's quiet limit.
    Stale,
    Low,
    High,
    Ok,
}

/// State of a sensor given its most recent reading as (time, value).
pub fn sensor_state(sensor: &Sensor, latest: Option<(i64, f64)>, now: i64) -> SensorState {
    let Some((at, value)) = latest else {
        return SensorState::NoData;
    };
    if let Some(limit) = sensor.stale_after_minutes {
        if limit > 0 && now - at > limit * MINUTE {
            return SensorState::Stale;
        }
    }
    if sensor.min_alert.map(|m| value < m).unwrap_or(false) {
        SensorState::Low
    } else if sensor.max_alert.map(|m| value > m).unwrap_or(false) {
        SensorState::High
    } else {
        SensorState::Ok
    }
}

fn num(x: f64) -> String {
    if x.fract().abs() < 1e-9 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

/// Watch officer rules: one finding per out-of-range sensor, and one for all silent sensors.
pub fn findings(sensors: &[(Sensor, Option<(i64, f64)>)], now: i64) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut silent: Vec<&str> = Vec::new();
    for (sensor, latest) in sensors {
        match (sensor_state(sensor, *latest, now), latest) {
            (state @ (SensorState::Low | SensorState::High), Some((_, value))) => {
                let (word, limit) = if state == SensorState::Low {
                    ("below", sensor.min_alert.unwrap_or(0.0))
                } else {
                    ("above", sensor.max_alert.unwrap_or(0.0))
                };
                out.push(Finding {
                    id: format!("sensor.range.{}", sensor.name.to_lowercase().replace(' ', "_")),
                    severity: if sensor.urgent { Severity::Critical } else { Severity::Warning },
                    area: "station",
                    title: format!("{} is {} {}", sensor.name, num(*value), sensor.unit).trim_end().to_string(),
                    detail: format!("That is {word} the limit of {} {}.", num(limit), sensor.unit).replace(" .", "."),
                    go_to: Some("field_ops"),
                });
            }
            (SensorState::Stale, _) => silent.push(sensor.name.as_str()),
            _ => {}
        }
    }
    if !silent.is_empty() {
        out.push(Finding {
            id: "sensor.silent".to_string(),
            severity: Severity::Warning,
            area: "station",
            title: format!(
                "{} gone quiet",
                if silent.len() == 1 { "1 sensor has".to_string() } else { format!("{} sensors have", silent.len()) }
            ),
            detail: format!("{}. Check its battery and that it is still in range.", silent.join(", ")),
            go_to: Some("field_ops"),
        });
    }
    out
}

/// One measurement taken out of a telemetry packet.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub metric: &'static str,
    pub unit: &'static str,
    pub value: f64,
}

/// Measurements carried by a Meshtastic telemetry packet. Zero and non-finite values that the
/// firmware sends for "sensor not fitted" are dropped where zero cannot be a real reading.
pub fn metrics_from_telemetry(variant: &meshtastic::protobufs::telemetry::Variant) -> Vec<Metric> {
    use meshtastic::protobufs::telemetry::Variant;
    let mut out = Vec::new();
    let mut push = |metric: &'static str, unit: &'static str, value: Option<f64>, zero_is_valid: bool| {
        if let Some(v) = value {
            if v.is_finite() && (zero_is_valid || v != 0.0) {
                out.push(Metric { metric, unit, value: v });
            }
        }
    };
    match variant {
        Variant::DeviceMetrics(d) => {
            // Meshtastic reports 101 for "on external power".
            push("battery", "%", d.battery_level.map(|b| f64::from(b.min(100))), true);
            push("voltage", "V", d.voltage.map(f64::from), false);
        }
        Variant::EnvironmentMetrics(e) => {
            push("temperature", "°C", e.temperature.map(f64::from), true);
            push("humidity", "%", e.relative_humidity.map(f64::from), false);
            push("pressure", "hPa", e.barometric_pressure.map(f64::from), false);
            push("distance", "mm", e.distance.map(f64::from), false);
            push("light", "lux", e.lux.map(f64::from), true);
        }
        _ => {}
    }
    out
}

fn latest_by_sensor(db: &crate::db::Database) -> std::collections::HashMap<String, (i64, f64)> {
    let mut map: std::collections::HashMap<String, (i64, f64)> = std::collections::HashMap::new();
    for r in store::list::<Reading>(db, READING_KIND).unwrap_or_default() {
        let entry = map.entry(r.data.sensor_id.clone()).or_insert((r.data.at, r.data.value));
        if r.data.at >= entry.0 {
            *entry = (r.data.at, r.data.value);
        }
    }
    map
}

/// Store a reading and trim the sensor's history.
pub fn add_reading(db: &crate::db::Database, sensor_id: &str, at: i64, value: f64) -> Result<(), String> {
    store::put_at(db, READING_KIND, None, &Reading { sensor_id: sensor_id.to_string(), at, value }, at)?;
    let mut mine: Vec<Record<Reading>> = store::list::<Reading>(db, READING_KIND)?
        .into_iter()
        .filter(|r| r.data.sensor_id == sensor_id)
        .collect();
    if mine.len() > MAX_READINGS_PER_SENSOR {
        mine.sort_by_key(|r| r.data.at);
        let extra = mine.len() - MAX_READINGS_PER_SENSOR;
        let ids: Vec<String> = mine.into_iter().take(extra).map(|r| r.id).collect();
        store::purge(db, READING_KIND, &ids)?;
    }
    Ok(())
}

/// Record telemetry from a radio node, creating a sensor for each new measurement it reports.
pub fn ingest_node_metrics(
    db: &crate::db::Database,
    node_id: i64,
    node_label: &str,
    metrics: &[Metric],
    at: i64,
) -> Result<usize, String> {
    if metrics.is_empty() {
        return Ok(0);
    }
    let existing = store::list::<Sensor>(db, SENSOR_KIND)?;
    let mut stored = 0;
    for m in metrics {
        let sensor_id = match existing.iter().find(|s| s.data.node_id == Some(node_id) && s.data.metric == m.metric) {
            Some(s) => s.id.clone(),
            None => {
                store::put(
                    db,
                    SENSOR_KIND,
                    None,
                    &Sensor {
                        name: format!("{node_label} {}", m.metric),
                        metric: m.metric.to_string(),
                        unit: m.unit.to_string(),
                        node_id: Some(node_id),
                        min_alert: None,
                        max_alert: None,
                        stale_after_minutes: None,
                        urgent: false,
                    },
                )?
                .id
            }
        };
        add_reading(db, &sensor_id, at, m.value)?;
        stored += 1;
    }
    Ok(stored)
}

/// Sensors paired with their latest reading, for the watch officer.
pub fn load_with_latest(db: &crate::db::Database) -> Vec<(Sensor, Option<(i64, f64)>)> {
    let latest = latest_by_sensor(db);
    store::list::<Sensor>(db, SENSOR_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|s| {
            let l = latest.get(&s.id).copied();
            (s.data, l)
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorRow {
    #[serde(flatten)]
    pub record: Record<Sensor>,
    pub state: SensorState,
    pub latest_at: Option<i64>,
    pub latest_value: Option<f64>,
    /// Most recent values, oldest first, for a small trend line.
    pub recent: Vec<f64>,
}

#[tauri::command]
pub fn list_sensors(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<SensorRow>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let now = store::now();
    let mut readings = store::list::<Reading>(&db, READING_KIND)?;
    readings.sort_by_key(|r| r.data.at);
    let mut rows: Vec<SensorRow> = store::list::<Sensor>(&db, SENSOR_KIND)?
        .into_iter()
        .map(|record| {
            let mine: Vec<&Record<Reading>> = readings.iter().filter(|r| r.data.sensor_id == record.id).collect();
            let latest = mine.last().map(|r| (r.data.at, r.data.value));
            SensorRow {
                state: sensor_state(&record.data, latest, now),
                latest_at: latest.map(|l| l.0),
                latest_value: latest.map(|l| l.1),
                recent: mine.iter().rev().take(24).rev().map(|r| r.data.value).collect(),
                record,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.record.data.name.to_lowercase().cmp(&b.record.data.name.to_lowercase()));
    Ok(rows)
}

#[tauri::command]
pub fn save_sensor(
    id: Option<String>,
    mut sensor: Sensor,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<Sensor>, String> {
    super::require_member(&session)?;
    sensor.name = sensor.name.trim().to_string();
    sensor.metric = sensor.metric.trim().to_lowercase();
    sensor.unit = sensor.unit.trim().to_string();
    if sensor.name.is_empty() {
        return Err("Name the sensor.".to_string());
    }
    if sensor.metric.is_empty() {
        sensor.metric = "value".to_string();
    }
    if let (Some(lo), Some(hi)) = (sensor.min_alert, sensor.max_alert) {
        if lo >= hi {
            return Err("The low limit must be below the high limit.".to_string());
        }
    }
    if sensor.stale_after_minutes.map(|m| m < 0).unwrap_or(false) {
        return Err("The quiet limit cannot be negative.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, SENSOR_KIND, id.as_deref(), &sensor)
}

#[tauri::command]
pub fn delete_sensor(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let ids: Vec<String> = store::list::<Reading>(&db, READING_KIND)?
        .into_iter()
        .filter(|r| r.data.sensor_id == id)
        .map(|r| r.id)
        .collect();
    store::purge(&db, READING_KIND, &ids)?;
    store::remove(&db, SENSOR_KIND, &id)
}

/// Enter a reading by hand.
#[tauri::command]
pub fn add_sensor_reading(
    sensor_id: String,
    value: f64,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    super::require_member(&session)?;
    if !value.is_finite() {
        return Err("Enter a number.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    if store::get::<Sensor>(&db, SENSOR_KIND, &sensor_id)?.is_none() {
        return Err("That sensor no longer exists.".to_string());
    }
    add_reading(&db, &sensor_id, store::now(), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn sensor(name: &str, min: Option<f64>, max: Option<f64>, stale: Option<i64>) -> Sensor {
        Sensor {
            name: name.to_string(),
            metric: "water_level".to_string(),
            unit: "%".to_string(),
            node_id: None,
            min_alert: min,
            max_alert: max,
            stale_after_minutes: stale,
            urgent: false,
        }
    }

    #[test]
    fn state_follows_limits_and_silence() {
        let s = sensor("Tank", Some(20.0), Some(95.0), Some(60));
        assert_eq!(sensor_state(&s, None, NOW), SensorState::NoData);
        assert_eq!(sensor_state(&s, Some((NOW - 60, 50.0)), NOW), SensorState::Ok);
        assert_eq!(sensor_state(&s, Some((NOW - 60, 12.0)), NOW), SensorState::Low);
        assert_eq!(sensor_state(&s, Some((NOW - 60, 99.0)), NOW), SensorState::High);
        assert_eq!(sensor_state(&s, Some((NOW - 61 * 60, 12.0)), NOW), SensorState::Stale, "silence outranks an old reading");
        assert_eq!(sensor_state(&s, Some((NOW - 60, 20.0)), NOW), SensorState::Ok, "the limit itself is in range");
    }

    #[test]
    fn sensors_without_limits_are_always_ok() {
        let s = sensor("Shed temp", None, None, None);
        assert_eq!(sensor_state(&s, Some((NOW - 999_999, -40.0)), NOW), SensorState::Ok);
    }

    #[test]
    fn findings_name_the_reading_and_the_limit() {
        let mut geiger = sensor("Geiger", None, Some(0.5), None);
        geiger.unit = "µSv/h".to_string();
        geiger.urgent = true;
        let f = findings(
            &[
                (sensor("Tank", Some(20.0), None, None), Some((NOW, 12.0))),
                (geiger, Some((NOW, 2.4))),
                (sensor("Soil", Some(10.0), None, None), Some((NOW, 40.0))),
            ],
            NOW,
        );
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].title, "Tank is 12 %");
        assert_eq!(f[0].detail, "That is below the limit of 20 %.");
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[1].title, "Geiger is 2.4 µSv/h");
        assert_eq!(f[1].severity, Severity::Critical);
    }

    #[test]
    fn silent_sensors_are_reported_together() {
        let f = findings(
            &[
                (sensor("North gate", None, None, Some(30)), Some((NOW - 3600, 1.0))),
                (sensor("Tank", None, None, Some(30)), Some((NOW - 7200, 1.0))),
                (sensor("Never reported", None, None, Some(30)), None),
            ],
            NOW,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].title, "2 sensors have gone quiet");
        assert!(f[0].detail.starts_with("North gate, Tank."));
    }

    #[test]
    fn telemetry_packets_become_metrics() {
        use meshtastic::protobufs::{telemetry::Variant, DeviceMetrics, EnvironmentMetrics};
        let device = Variant::DeviceMetrics(DeviceMetrics {
            battery_level: Some(101),
            voltage: Some(4.1),
            ..Default::default()
        });
        let m = metrics_from_telemetry(&device);
        assert_eq!(m[0], Metric { metric: "battery", unit: "%", value: 100.0 });
        assert_eq!(m[1].metric, "voltage");

        let env = Variant::EnvironmentMetrics(EnvironmentMetrics {
            temperature: Some(0.0),
            relative_humidity: Some(0.0),
            barometric_pressure: Some(1012.5),
            ..Default::default()
        });
        let m = metrics_from_telemetry(&env);
        let names: Vec<&str> = m.iter().map(|x| x.metric).collect();
        assert_eq!(names, vec!["temperature", "pressure"], "0 °C is real; 0% humidity means no sensor");
    }

    #[test]
    fn node_telemetry_creates_sensors_once_and_keeps_readings_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(&dir.path().join("t.db")).unwrap();
        let metric = |v: f64| vec![Metric { metric: "temperature", unit: "°C", value: v }];

        assert_eq!(ingest_node_metrics(&db, 77, "Ridge", &metric(4.0), NOW).unwrap(), 1);
        assert_eq!(ingest_node_metrics(&db, 77, "Ridge", &metric(5.0), NOW + 60).unwrap(), 1);
        let sensors = store::list::<Sensor>(&db, SENSOR_KIND).unwrap();
        assert_eq!(sensors.len(), 1, "the second packet reuses the sensor");
        assert_eq!(sensors[0].data.name, "Ridge temperature");

        for i in 0..(MAX_READINGS_PER_SENSOR as i64 + 20) {
            add_reading(&db, &sensors[0].id, NOW + 120 + i, i as f64).unwrap();
        }
        let readings = store::list::<Reading>(&db, READING_KIND).unwrap();
        assert_eq!(readings.len(), MAX_READINGS_PER_SENSOR);
        let latest = load_with_latest(&db);
        assert_eq!(latest[0].1.unwrap().1, (MAX_READINGS_PER_SENSOR as f64) + 19.0);
        // Trimmed readings leave no tombstones behind.
        assert!(store::export_raw(&db, &[READING_KIND]).unwrap().iter().all(|r| !r.deleted));
    }
}
