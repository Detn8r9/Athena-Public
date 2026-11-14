//! Home Assistant link, both ways.
//!
//! ATHENA to Home Assistant:
//! - New alerts and urgent Watch Officer findings are sent to a Home Assistant webhook. A Home
//!   Assistant automation passes them to phones through the Companion app, which reaches a phone
//!   anywhere without opening any port on the router.
//! - With an access token, ATHENA also keeps a few sensors up to date in Home Assistant
//!   (`sensor.athena_alert_level`, `sensor.athena_alerts`, `binary_sensor.athena_hunker_down`).
//!
//! Home Assistant to ATHENA:
//! - With an access token, ATHENA reads the entities the user lists (UPS, solar, water tank,
//!   smoke and leak sensors) and fixed rules turn the worrying ones into Watch Officer findings.
//!
//! Home Assistant is usually on the home network, so this works with the station set to stay
//! offline from the internet. The token is kept on this station and never sent to a browser.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

use super::alerts::{Alert, AlertState, HunkerState, Level};
use crate::watch_officer::{Finding, Severity};

pub const STATE_KIND: &str = "vault_ha_state";
pub const STATE_ID: &str = "vault_ha_state";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HaSettings {
    pub enabled: bool,
    /// Home Assistant's address, such as `http://homeassistant.local:8123`.
    pub url: String,
    /// The webhook id of the automation that sends ATHENA's messages to phones.
    pub webhook_id: String,
    /// Long-lived access token (Home Assistant profile, Security). Needed for sensors and readings.
    pub token: String,
    /// Alerts at this level or higher are sent to phones.
    pub notify_level: Level,
    /// Keep ATHENA's sensors up to date in Home Assistant.
    pub publish_sensors: bool,
    /// Entities to read, such as `sensor.ups_battery` or `binary_sensor.kitchen_leak`.
    pub entities: Vec<String>,    /// Shown to browsers in place of the token; ignored when saving.
    pub token_set: bool,
}

impl Default for HaSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            webhook_id: String::new(),
            token: String::new(),
            notify_level: Level::Warning,
            publish_sensors: true,
            entities: Vec::new(),
            token_set: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaReading {
    pub entity_id: String,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub device_class: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HaState {
    pub readings: Vec<HaReading>,
    pub last_read: i64,
    pub last_sent: i64,
    pub last_error: Option<String>,
    /// What was last sent to phones: alert id to the level it was sent at. A message is sent again
    /// only when the level rises or the alert returns after it had cleared.
    pub sent: BTreeMap<String, Level>,
}

/// `http(s)://host[:port]` with no trailing slash or `/api`.
pub fn clean_url(url: &str) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/').trim_end_matches("/api").trim_end_matches('/');
    if u.is_empty() {
        return Ok(String::new());
    }
    let rest = u.strip_prefix("http://").or_else(|| u.strip_prefix("https://")).ok_or("The Home Assistant address starts with http:// or https://, for example http://homeassistant.local:8123.")?;
    if rest.is_empty() || rest.contains(char::is_whitespace) || rest.contains('/') {
        return Err("Enter just Home Assistant's address, for example http://192.168.1.50:8123.".to_string());
    }
    Ok(u.to_string())
}

pub fn clean_webhook_id(id: &str) -> Result<String, String> {
    let id = id.trim();
    // A pasted full webhook URL is accepted too.
    let id = id.rsplit("/api/webhook/").next().unwrap_or(id).trim_matches('/');
    if id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        Ok(id.to_string())
    } else {
        Err("The webhook id has only letters, digits, - and _.".to_string())
    }
}

pub fn clean_entities(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = list
        .iter()
        .flat_map(|l| l.split(|c: char| c == ',' || c.is_whitespace()))
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| {
            let mut parts = e.splitn(2, '.');
            matches!((parts.next(), parts.next()), (Some(d), Some(o)) if !d.is_empty() && !o.is_empty())
                && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        })
        .collect();
    out.sort();
    out.dedup();
    out.truncate(40);
    out
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("ATHENA-station")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())
}

fn explain(e: reqwest::Error) -> String {
    if e.is_connect() || e.is_timeout() {
        "Home Assistant did not answer. Check its address and that this station can reach it.".to_string()
    } else if e.status().map(|s| s.as_u16()) == Some(401) {
        "Home Assistant refused the access token. Create a new long-lived token and enter it again.".to_string()
    } else {
        format!("Home Assistant: {e}")
    }
}

/// The message a phone shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Message {
    pub title: String,
    pub message: String,
    /// `info`, `watch`, `warning` or `act`.
    pub level: &'static str,
    /// Same tag replaces the earlier notification on the phone.
    pub tag: String,
    pub source: String,
    pub url: Option<String>,
}

fn level_word(l: Level) -> &'static str {
    match l {
        Level::Info => "info",
        Level::Watch => "watch",
        Level::Warning => "warning",
        Level::Act => "act",
    }
}

fn tag_for(id: &str) -> String {
    let t: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("athena_{}", t.chars().take(60).collect::<String>())
}

pub fn alert_message(a: &Alert, station: &str) -> Message {
    Message {
        title: format!("{}: {}", station, a.title),
        message: a.detail.clone(),
        level: level_word(a.level),
        tag: tag_for(&a.id),
        source: a.source.clone(),
        url: a.url.clone(),
    }
}

pub fn finding_message(f: &Finding, station: &str) -> Message {
    Message {
        title: format!("{}: {}", station, f.title),
        message: f.detail.clone(),
        level: match f.severity {
            Severity::Critical => "act",
            Severity::Warning => "warning",
            Severity::Info => "info",
        },
        tag: tag_for(&f.id),
        source: "watch_officer".to_string(),
        url: None,
    }
}

/// Alerts to send now, and the updated record of what has been sent. Alerts that have gone away
/// are forgotten, so they are sent again if they come back.
pub fn alerts_to_send<'a>(alerts: &'a [Alert], min: Level, sent: &BTreeMap<String, Level>) -> (Vec<&'a Alert>, BTreeMap<String, Level>) {
    let mut next = BTreeMap::new();
    let mut out = Vec::new();
    for a in alerts.iter().filter(|a| a.level >= min) {
        let before = sent.get(&a.id).copied();
        if before.is_none_or(|b| a.level > b) {
            out.push(a);
        }
        next.insert(a.id.clone(), before.map_or(a.level, |b| b.max(a.level)));
    }
    (out, next)
}

pub fn send(base: &str, webhook_id: &str, msg: &Message) -> Result<(), String> {
    if base.is_empty() || webhook_id.is_empty() {
        return Err("Enter Home Assistant's address and the webhook id first.".to_string());
    }
    client()?
        .post(format!("{base}/api/webhook/{webhook_id}"))
        .json(msg)
        .send()
        .and_then(|r| r.error_for_status())
        .map(|_| ())
        .map_err(explain)
}

fn set_state(c: &reqwest::blocking::Client, base: &str, token: &str, entity: &str, state: &str, attributes: serde_json::Value) -> Result<(), String> {
    c.post(format!("{base}/api/states/{entity}"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "state": state, "attributes": attributes }))
        .send()
        .and_then(|r| r.error_for_status())
        .map(|_| ())
        .map_err(explain)
}

/// The sensors ATHENA keeps in Home Assistant.
pub fn sensor_values(alerts: &AlertState, hunker: &HunkerState, headline: &str) -> Vec<(&'static str, String, serde_json::Value)> {
    let top = alerts.alerts.iter().map(|a| a.level).max();
    let serious: Vec<&Alert> = alerts.alerts.iter().filter(|a| a.level >= Level::Warning).collect();
    vec![
        (
            "sensor.athena_alert_level",
            top.map_or("none", level_word).to_string(),
            serde_json::json!({ "friendly_name": "ATHENA alert level", "icon": "mdi:shield-alert" }),
        ),
        (
            "sensor.athena_alerts",
            serious.len().to_string(),
            serde_json::json!({
                "friendly_name": "ATHENA serious alerts",
                "icon": "mdi:alert",
                "unit_of_measurement": "alerts",
                "titles": serious.iter().take(5).map(|a| a.title.clone()).collect::<Vec<_>>(),
            }),
        ),
        (
            "binary_sensor.athena_hunker_down",
            if hunker.active { "on" } else { "off" }.to_string(),
            serde_json::json!({ "friendly_name": "ATHENA hunker-down", "icon": "mdi:home-lock", "started_by": hunker.started_by, "reason": hunker.reason }),
        ),
        (
            "sensor.athena_status",
            headline.chars().take(250).collect(),
            serde_json::json!({ "friendly_name": "ATHENA status", "icon": "mdi:radar" }),
        ),
    ]
}

pub fn publish(base: &str, token: &str, values: &[(&'static str, String, serde_json::Value)]) -> Result<(), String> {
    if base.is_empty() || token.is_empty() {
        return Err("Enter Home Assistant's address and an access token to keep sensors up to date.".to_string());
    }
    let c = client()?;
    for (entity, state, attrs) in values {
        set_state(&c, base, token, entity, state, attrs.clone())?;
    }
    Ok(())
}

pub fn read(base: &str, token: &str, entities: &[String]) -> Result<Vec<HaReading>, String> {
    if entities.is_empty() {
        return Ok(Vec::new());
    }
    if base.is_empty() || token.is_empty() {
        return Err("Enter Home Assistant's address and an access token to read devices.".to_string());
    }
    let c = client()?;
    let mut out = Vec::new();
    for e in entities {
        let resp = c.get(format!("{base}/api/states/{e}")).bearer_auth(token).send().map_err(explain)?;
        if resp.status().as_u16() == 404 {
            out.push(HaReading { entity_id: e.clone(), name: e.clone(), state: "not found".to_string(), unit: None, device_class: None });
            continue;
        }
        let v: serde_json::Value = resp.error_for_status().and_then(|r| r.json()).map_err(explain)?;
        let attrs = &v["attributes"];
        out.push(HaReading {
            entity_id: e.clone(),
            name: attrs["friendly_name"].as_str().unwrap_or(e).to_string(),
            state: v["state"].as_str().unwrap_or("unknown").to_string(),
            unit: attrs["unit_of_measurement"].as_str().map(str::to_string),
            device_class: attrs["device_class"].as_str().map(str::to_string),
        });
    }
    Ok(out)
}

/// The home location set in Home Assistant (Settings, System, General): latitude, longitude and
/// the location's name.
pub fn fetch_location(base: &str, token: &str) -> Result<(f64, f64, Option<String>), String> {
    if base.is_empty() || token.is_empty() {
        return Err("Enter Home Assistant's address and an access token first.".to_string());
    }
    let v: serde_json::Value = client()?
        .get(format!("{base}/api/config"))
        .bearer_auth(token)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(explain)?;
    parse_location(&v)
}

pub fn parse_location(v: &serde_json::Value) -> Result<(f64, f64, Option<String>), String> {
    let lat = v["latitude"].as_f64();
    let lon = v["longitude"].as_f64();
    match (lat, lon) {
        (Some(lat), Some(lon)) if lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0) => {
            Ok((lat, lon, v["location_name"].as_str().map(str::to_string).filter(|n| !n.trim().is_empty())))
        }
        _ => Err("Home Assistant has no home location set (Settings, System, General).".to_string()),
    }
}

/// Fixed rules for what in Home Assistant needs attention.
pub fn findings(state: &HaState) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in &state.readings {
        let class = r.device_class.as_deref().unwrap_or("");
        let domain = r.entity_id.split('.').next().unwrap_or("");
        let on = r.state == "on";
        let number = r.state.parse::<f64>().ok();
        let finding = |severity, title: String, detail: String| Finding {
            id: format!("ha.{}", r.entity_id),
            severity,
            area: "station",
            title,
            detail,
            go_to: Some("archive"),
        };
        if domain == "binary_sensor" && on {
            match class {
                "smoke" | "gas" | "carbon_monoxide" | "heat" => out.push(finding(Severity::Critical, format!("{} detected", r.name), "Reported by Home Assistant. Check it now.".to_string())),
                "moisture" | "safety" | "problem" | "tamper" => out.push(finding(Severity::Warning, format!("{}: problem", r.name), "Reported by Home Assistant.".to_string())),
                _ => {}
            }
        } else if class == "battery" {
            if let Some(pct) = number.filter(|p| *p <= 20.0) {
                out.push(finding(
                    if pct <= 10.0 { Severity::Critical } else { Severity::Warning },
                    format!("{} at {:.0}%", r.name, pct),
                    "Battery reported low by Home Assistant.".to_string(),
                ));
            }
        } else if r.state == "unavailable" || r.state == "not found" {
            out.push(finding(Severity::Info, format!("{} is not reporting", r.name), format!("Home Assistant shows {} as {}.", r.entity_id, r.state)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(id: &str, level: Level) -> Alert {
        Alert { id: id.into(), source: "faa".into(), level, title: id.into(), detail: "d".into(), time: 0, url: None, distance_km: None }
    }

    #[test]
    fn addresses_and_ids_are_cleaned() {
        assert_eq!(clean_url(" http://homeassistant.local:8123/api/ ").unwrap(), "http://homeassistant.local:8123");
        assert_eq!(clean_url("").unwrap(), "");
        assert!(clean_url("homeassistant.local").is_err());
        assert!(clean_url("http://ha.local/lovelace").is_err());
        assert_eq!(clean_webhook_id("http://ha:8123/api/webhook/athena_alerts_x1").unwrap(), "athena_alerts_x1");
        assert!(clean_webhook_id("bad id!").is_err());
        assert_eq!(clean_entities(&["sensor.UPS_battery, binary_sensor.leak".into(), "nonsense".into(), "sensor.ups_battery".into()]), vec!["binary_sensor.leak", "sensor.ups_battery"]);
    }

    #[test]
    fn alerts_are_sent_once_and_again_only_when_they_get_worse() {
        let first = [alert("faa:stops", Level::Watch), alert("quake", Level::Warning), alert("news", Level::Info)];
        let (send, sent) = alerts_to_send(&first, Level::Watch, &BTreeMap::new());
        assert_eq!(send.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["faa:stops", "quake"]);
        let (again, sent) = alerts_to_send(&first, Level::Watch, &sent);
        assert!(again.is_empty(), "nothing new");
        let worse = [alert("faa:stops", Level::Warning), alert("quake", Level::Warning)];
        let (send, sent) = alerts_to_send(&worse, Level::Watch, &sent);
        assert_eq!(send.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["faa:stops"]);
        let (_, sent) = alerts_to_send(&[], Level::Watch, &sent);
        assert!(sent.is_empty(), "cleared alerts are forgotten");
        let (back, _) = alerts_to_send(&worse[..1], Level::Watch, &sent);
        assert_eq!(back.len(), 1, "a returning alert is sent again");
    }

    #[test]
    fn readings_become_findings_by_fixed_rules() {
        let r = |id: &str, state: &str, class: Option<&str>| HaReading { entity_id: id.into(), name: id.into(), state: state.into(), unit: None, device_class: class.map(str::to_string) };
        let state = HaState {
            readings: vec![
                r("binary_sensor.kitchen_smoke", "on", Some("smoke")),
                r("binary_sensor.basement_leak", "on", Some("moisture")),
                r("binary_sensor.front_door", "on", Some("door")),
                r("sensor.ups_battery", "15", Some("battery")),
                r("sensor.phone_battery", "80", Some("battery")),
                r("sensor.tank", "unavailable", None),
            ],
            ..Default::default()
        };
        let f = findings(&state);
        assert_eq!(
            f.iter().map(|f| (f.id.as_str(), f.severity)).collect::<Vec<_>>(),
            vec![
                ("ha.binary_sensor.kitchen_smoke", Severity::Critical),
                ("ha.binary_sensor.basement_leak", Severity::Warning),
                ("ha.sensor.ups_battery", Severity::Warning),
                ("ha.sensor.tank", Severity::Info),
            ]
        );
    }

    #[test]
    fn sensors_summarise_alerts_and_hunker_down() {
        let state = AlertState { fetched_at: 0, alerts: vec![alert("a", Level::Warning), alert("b", Level::Info)], errors: vec![] };
        let v = sensor_values(&state, &HunkerState::default(), "All clear");
        assert_eq!(v[0].1, "warning");
        assert_eq!(v[1].1, "1");
        assert_eq!(v[2].1, "off");
        assert_eq!(v[3].1, "All clear");
        let quiet = sensor_values(&AlertState::default(), &HunkerState { active: true, ..Default::default() }, "");
        assert_eq!((quiet[0].1.as_str(), quiet[2].1.as_str()), ("none", "on"));
    }

    #[test]
    fn home_location_is_read_from_the_config() {
        let v = serde_json::json!({ "latitude": 44.08, "longitude": -103.23, "location_name": "Home" });
        assert_eq!(parse_location(&v).unwrap(), (44.08, -103.23, Some("Home".to_string())));
        assert!(parse_location(&serde_json::json!({ "latitude": 0.0, "longitude": 0.0 })).is_err());
        assert!(parse_location(&serde_json::json!({})).is_err());
    }

    #[test]
    fn messages_carry_a_stable_tag() {
        let m = alert_message(&alert("faa:JFK:stop", Level::Warning), "Home");
        assert_eq!(m.title, "Home: faa:JFK:stop");
        assert_eq!(m.tag, "athena_faa_JFK_stop");
        assert_eq!(m.level, "warning");
    }
}
