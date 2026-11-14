//! Official warning feeds, graded by fixed rules, and the hunker-down plan.
//!
//! Sources are structured public feeds: earthquakes (USGS), disasters worldwide (GDACS), space
//! weather (NOAA SWPC), US air traffic (FAA airport status: ground stops, delays, closures) and,
//! for a chosen US state, weather warnings (NWS). News feeds the user adds are read too, but only
//! headlines containing one of the user's watch words become alerts. Rules set the level; no model
//! decides anything. The station's position is used only on this computer to work out distance.
//! The only location ever sent is the two-letter state code, and only when the user has entered
//! one; airport codes are matched on this computer, never sent.
//!
//! Hunker-down is never automatic. An alert at the top level asks an operator to confirm.

use serde::{Deserialize, Serialize};

use crate::watch_officer::{Finding, Severity};

pub const STATE_KIND: &str = "vault_alert_state";
pub const STATE_ID: &str = "vault_alert_state";
pub const HUNKER_KIND: &str = "vault_hunker";
pub const HUNKER_ID: &str = "vault_hunker";
/// What the operator types to start hunker-down.
pub const HUNKER_CONFIRM_PHRASE: &str = "HUNKER DOWN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Worth knowing; nothing to do.
    Info,
    /// Could affect this station. Keep an eye on it.
    Watch,
    /// Likely to affect this station. Prepare.
    Warning,
    /// Severe and close. Decide now whether to hunker down.
    Act,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    /// `usgs`, `gdacs`, `swpc`, `nws` or `fleet`.
    pub source: String,
    pub level: Level,
    pub title: String,
    pub detail: String,
    /// Unix seconds.
    pub time: i64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub distance_km: Option<f64>,
}

/// Which feeds to read and what to look for in them.
#[derive(Debug, Clone, Default)]
pub struct FeedOptions {
    pub station: Option<(f64, f64)>,
    /// Two-letter US state for weather warnings; empty for none.
    pub region: String,
    pub faa: bool,
    /// Airports to watch closely, as FAA codes (`DEN`, `RAP`).
    pub airports: Vec<String>,
    pub news_feeds: Vec<String>,
    pub watch_words: Vec<String>,
}

/// Words that make a headline worth an alert when nothing else has been chosen.
pub fn default_watch_words() -> Vec<String> {
    [
        "ground stop", "grounded", "grounding", "evacuate", "evacuation", "shelter in place", "boil water",
        "power outage", "blackout", "grid failure", "cyberattack", "martial law", "curfew", "quarantine",
        "outbreak", "tsunami warning", "dam failure", "chemical spill", "nuclear",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Watch words that mean people are being told to act, not just informed.
const URGENT_WORDS: &[&str] = &["evacuat", "shelter in place", "tsunami warning", "dam failure"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertState {
    pub fetched_at: i64,
    pub alerts: Vec<Alert>,
    /// Feeds that could not be read on the last check.
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkerState {
    pub active: bool,
    pub started_at: i64,
    pub started_by: String,
    pub reason: String,
}

pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1, lat2, lon2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

/// Level for an earthquake of `mag` at `km` from the station (`None` when the station has no
/// position set). Strong shaking reaches further the larger the quake.
pub fn grade_earthquake(mag: f64, km: Option<f64>) -> Option<Level> {
    match km {
        Some(d) if (mag >= 7.5 && d <= 800.0) || (mag >= 6.5 && d <= 400.0) => Some(Level::Act),
        Some(d) if mag >= 6.0 && d <= 300.0 => Some(Level::Warning),
        Some(d) if mag >= 5.0 && d <= 150.0 => Some(Level::Watch),
        _ if mag >= 7.5 => Some(Level::Watch),
        _ if mag >= 7.0 => Some(Level::Info),
        _ => None,
    }
}

pub fn parse_usgs(body: &str, station: Option<(f64, f64)>) -> Vec<Alert> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new() };
    let mut out = Vec::new();
    for f in v["features"].as_array().into_iter().flatten() {
        let p = &f["properties"];
        let Some(mag) = p["mag"].as_f64() else { continue };
        let coords = f["geometry"]["coordinates"].as_array();
        let pos = coords.and_then(|c| Some((c.get(1)?.as_f64()?, c.first()?.as_f64()?)));
        let km = station.zip(pos).map(|(s, q)| distance_km(s, q));
        let Some(level) = grade_earthquake(mag, km) else { continue };
        let place = p["place"].as_str().unwrap_or("unknown location");
        out.push(Alert {
            id: format!("usgs:{}", f["id"].as_str().unwrap_or(place)),
            source: "usgs".to_string(),
            level,
            title: format!("Magnitude {mag:.1} earthquake, {place}"),
            detail: match km {
                Some(d) => format!("About {:.0} km from this station.{}", d, if p["tsunami"].as_i64() == Some(1) { " Tsunami information was issued." } else { "" }),
                None => "Set this station's position to see how far away it is.".to_string(),
            },
            time: p["time"].as_i64().unwrap_or(0) / 1000,
            url: p["url"].as_str().map(str::to_string),
            distance_km: km,
        });
    }
    out
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}");
    let start = xml.find(&open)?;
    let after = &xml[start..];
    let body_start = after.find('>')? + 1;
    let end = after.find(&format!("</{name}>"))?;
    (end >= body_start).then(|| after[body_start..end].trim())
}

fn rfc2822_seconds(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc2822(s.trim()).map(|d| d.timestamp()).unwrap_or(0)
}

pub fn parse_gdacs(body: &str, station: Option<(f64, f64)>) -> Vec<Alert> {
    let mut out = Vec::new();
    for item in body.split("<item>").skip(1) {
        let item = item.split("</item>").next().unwrap_or("");
        let colour = tag(item, "gdacs:alertlevel").unwrap_or("").to_lowercase();
        let pos = tag(item, "geo:lat").zip(tag(item, "geo:long")).and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)));
        let km = station.zip(pos).map(|(s, q)| distance_km(s, q));
        // Distance decides: a red alert on another continent is news, not a reason to prepare.
        let level = match (colour.as_str(), km) {
            ("red", Some(d)) if d <= 500.0 => Level::Act,
            ("red", Some(d)) if d <= 2000.0 => Level::Warning,
            ("red", Some(_)) => Level::Info,
            ("red", None) => Level::Watch,
            ("orange", Some(d)) if d <= 300.0 => Level::Warning,
            ("orange", Some(d)) if d <= 1000.0 => Level::Watch,
            _ => continue,
        };
        let title = tag(item, "title")
            .unwrap_or("Disaster alert")
            .replace("<![CDATA[", "")
            .replace("]]>", "")
            .replace("&amp;", "&");
        out.push(Alert {
            id: format!("gdacs:{}", tag(item, "guid").unwrap_or(&title)),
            source: "gdacs".to_string(),
            level,
            title,
            detail: match km {
                Some(d) => format!("{} alert, about {:.0} km from this station.", colour, d),
                None => format!("{colour} alert."),
            },
            time: tag(item, "pubDate").map(rfc2822_seconds).unwrap_or(0),
            url: tag(item, "link").map(str::to_string),
            distance_km: km,
        });
    }
    out
}

/// The highest NOAA scale number (G, S or R, 1-5) named in a space weather message.
fn noaa_scale(message: &str) -> Option<(char, u32)> {
    let mut best: Option<(char, u32)> = None;
    for line in message.lines().filter(|l| l.contains("NOAA Scale")) {
        let chars: Vec<char> = line.chars().collect();
        for w in chars.windows(2) {
            if matches!(w[0], 'G' | 'S' | 'R') {
                if let Some(n) = w[1].to_digit(10).filter(|n| (1..=5).contains(n)) {
                    if best.is_none_or(|(_, b)| n > b) {
                        best = Some((w[0], n));
                    }
                }
            }
        }
    }
    best
}

pub fn parse_swpc(body: &str, now: i64) -> Vec<Alert> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new() };
    let mut out = Vec::new();
    for a in v.as_array().into_iter().flatten() {
        let message = a["message"].as_str().unwrap_or("");
        let Some((kind, n)) = noaa_scale(message) else { continue };
        let level = match n {
            5 => Level::Act,
            4 => Level::Warning,
            3 => Level::Watch,
            _ => continue,
        };
        let time = a["issue_datetime"]
            .as_str()
            .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").ok())
            .map(|d| d.and_utc().timestamp())
            .unwrap_or(0);
        // Space weather passes; a notice from days ago is history.
        if time > 0 && now - time > 3 * 86_400 {
            continue;
        }
        let what = match kind {
            'G' => "Geomagnetic storm",
            'S' => "Solar radiation storm",
            _ => "Radio blackout",
        };
        out.push(Alert {
            id: format!("swpc:{}:{}", a["product_id"].as_str().unwrap_or("x"), a["issue_datetime"].as_str().unwrap_or("")),
            source: "swpc".to_string(),
            level,
            title: format!("{what}, NOAA scale {kind}{n}"),
            detail: match kind {
                'G' if n >= 4 => "Power grids and long cables can fail, and HF radio and GPS become unreliable. Unplug sensitive equipment.".to_string(),
                'G' => "HF radio and GPS may be unreliable.".to_string(),
                _ => "HF radio on the sunlit side of the Earth may be weak or blacked out.".to_string(),
            },
            time,
            url: Some("https://www.swpc.noaa.gov/".to_string()),
            distance_km: None,
        });
    }
    out
}

pub fn parse_nws(body: &str) -> Vec<Alert> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new() };
    let mut out = Vec::new();
    for f in v["features"].as_array().into_iter().flatten() {
        let p = &f["properties"];
        let level = match p["severity"].as_str().unwrap_or("") {
            "Extreme" => Level::Act,
            "Severe" => Level::Warning,
            "Moderate" => Level::Watch,
            _ => continue,
        };
        out.push(Alert {
            id: format!("nws:{}", p["id"].as_str().unwrap_or("x")),
            source: "nws".to_string(),
            level,
            title: p["headline"].as_str().or(p["event"].as_str()).unwrap_or("Weather alert").to_string(),
            detail: p["areaDesc"].as_str().map(|a| format!("Area: {a}")).unwrap_or_default(),
            time: p["sent"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp()).unwrap_or(0),
            url: p["@id"].as_str().map(str::to_string),
            distance_km: None,
        });
    }
    out
}

/// The bodies of every `<name>…</name>` element in `xml`, in order.
fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    xml.split(open.as_str()).skip(1).filter_map(|b| b.split(close.as_str()).next()).collect()
}

fn faa_time(xml: &str) -> i64 {
    tag(xml, "Update_Time")
        .and_then(|t| chrono::NaiveDateTime::parse_from_str(t.trim_end_matches(" GMT").trim(), "%a %b %d %H:%M:%S %Y").ok())
        .map(|d| d.and_utc().timestamp())
        .unwrap_or(0)
}

/// A closure notice that only restricts some flights ("AD AP CLSD TO NON SKED TRANSIENT GA") is
/// routine; one without a "TO …" qualifier closes the airport.
fn closes_whole_airport(reason: &str) -> bool {
    let r = reason.to_ascii_uppercase();
    r.contains("AD AP CLSD") && !r.contains("AD AP CLSD TO ") && !r.contains(" EXC ")
}

fn list_codes(codes: &[String]) -> String {
    match codes.len() {
        0 => String::new(),
        1..=8 => codes.join(", "),
        n => format!("{} and {} more", codes[..8].join(", "), n - 8),
    }
}

/// FAA airport status (nasstatus.faa.gov): ground stops, ground delay programs, general delays and
/// closures. Nationwide trouble is one alert graded by how many airports it covers; airports the
/// user watches get their own alert.
pub fn parse_faa(body: &str, watched: &[String]) -> Vec<Alert> {
    let time = faa_time(body);
    let watched: Vec<String> = watched.iter().map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty()).collect();
    let is_watched = |code: &str| watched.iter().any(|w| w == code || w.trim_start_matches('K') == code);
    let mut stops: Vec<(String, String, String)> = Vec::new(); // airport, reason, until
    let mut delays: Vec<(String, String, String)> = Vec::new(); // airport, reason, how long
    let mut closures: Vec<(String, String, bool)> = Vec::new(); // airport, notice, whole airport
    for section in blocks(body, "Delay_type") {
        for p in blocks(section, "Program") {
            if let Some(a) = tag(p, "ARPT") {
                stops.push((a.to_string(), tag(p, "Reason").unwrap_or("").to_string(), tag(p, "End_Time").unwrap_or("").to_string()));
            }
        }
        for g in blocks(section, "Ground_Delay") {
            if let Some(a) = tag(g, "ARPT") {
                delays.push((a.to_string(), tag(g, "Reason").unwrap_or("").to_string(), format!("average {}", tag(g, "Avg").unwrap_or("?"))));
            }
        }
        for d in blocks(section, "Delay") {
            if let Some(a) = tag(d, "ARPT") {
                let how = match (tag(d, "Min"), tag(d, "Max")) {
                    (Some(min), Some(max)) => format!("{min} to {max}"),
                    _ => "delays".to_string(),
                };
                delays.push((a.to_string(), tag(d, "Reason").unwrap_or("").to_string(), how));
            }
        }
        for c in blocks(section, "Airport") {
            if let Some(a) = tag(c, "ARPT") {
                let reason = tag(c, "Reason").unwrap_or("");
                closures.push((a.to_string(), reason.to_string(), closes_whole_airport(reason)));
            }
        }
    }
    let url = Some("https://nasstatus.faa.gov/".to_string());
    let mut out = Vec::new();
    let codes = |list: &[(String, String, String)]| {
        let mut c: Vec<String> = list.iter().map(|x| x.0.clone()).collect();
        c.sort();
        c.dedup();
        c
    };
    let stop_codes = codes(&stops);
    if !stop_codes.is_empty() {
        let n = stop_codes.len();
        let level = if n >= 10 { Level::Warning } else if n >= 3 { Level::Watch } else { Level::Info };
        out.push(Alert {
            id: "faa:stops".to_string(),
            source: "faa".to_string(),
            level,
            title: if n >= 10 {
                format!("Widespread ground stops: {n} US airports")
            } else {
                format!("Ground stop{} at {}", if n == 1 { "" } else { "s" }, list_codes(&stop_codes))
            },
            detail: format!(
                "Flights to these airports are held on the ground: {}.{}",
                list_codes(&stop_codes),
                stops.first().map(|s| format!(" First listed reason: {}.", s.1)).unwrap_or_default()
            ),
            time,
            url: url.clone(),
            distance_km: None,
        });
    }
    let delay_codes = codes(&delays);
    if delay_codes.len() >= 10 {
        out.push(Alert {
            id: "faa:delays".to_string(),
            source: "faa".to_string(),
            level: Level::Watch,
            title: format!("Flight delays at {} US airports", delay_codes.len()),
            detail: format!("Ground delay programs or long delays at {}.", list_codes(&delay_codes)),
            time,
            url: url.clone(),
            distance_km: None,
        });
    }
    let whole: Vec<String> = {
        let mut c: Vec<String> = closures.iter().filter(|c| c.2).map(|c| c.0.clone()).collect();
        c.sort();
        c.dedup();
        c
    };
    if !whole.is_empty() {
        out.push(Alert {
            id: "faa:closures".to_string(),
            source: "faa".to_string(),
            level: if whole.len() >= 5 { Level::Watch } else { Level::Info },
            title: format!("Airport{} closed: {}", if whole.len() == 1 { "" } else { "s" }, list_codes(&whole)),
            detail: "The FAA lists these airports as closed to all traffic.".to_string(),
            time,
            url: url.clone(),
            distance_km: None,
        });
    }
    for (code, reason, until) in stops.iter().filter(|s| is_watched(&s.0)) {
        out.push(Alert {
            id: format!("faa:{code}:stop"),
            source: "faa".to_string(),
            level: Level::Warning,
            title: format!("Ground stop at {code}"),
            detail: format!("Reason: {reason}.{}", if until.is_empty() { String::new() } else { format!(" Expected to end {until}.") }),
            time,
            url: url.clone(),
            distance_km: None,
        });
    }
    for (code, reason, how) in delays.iter().filter(|d| is_watched(&d.0)) {
        out.push(Alert {
            id: format!("faa:{code}:delay"),
            source: "faa".to_string(),
            level: Level::Watch,
            title: format!("Flight delays at {code}"),
            detail: format!("{how}. Reason: {reason}."),
            time,
            url: url.clone(),
            distance_km: None,
        });
    }
    for (code, notice, whole) in closures.iter().filter(|c| is_watched(&c.0)) {
        out.push(Alert {
            id: format!("faa:{code}:closed"),
            source: "faa".to_string(),
            level: if *whole { Level::Warning } else { Level::Info },
            title: if *whole { format!("{code} is closed") } else { format!("{code} has a closure notice") },
            detail: notice.trim_start_matches('!').to_string(),
            time,
            url: url.clone(),
            distance_km: None,
        });
    }
    out
}

fn plain_text(s: &str) -> String {
    let s = s.replace("<![CDATA[", "").replace("]]>", "");
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn feed_time(s: &str) -> i64 {
    let s = s.trim();
    chrono::DateTime::parse_from_rfc2822(s)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(s))
        .map(|d| d.timestamp())
        .unwrap_or(0)
}

/// Host part of a URL, for naming where a headline came from.
fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).trim_start_matches("www.").to_string()
}

/// Headlines from one RSS or Atom feed that contain a watch word and are under two days old.
pub fn parse_news(body: &str, feed_url: &str, watch_words: &[String], now: i64) -> Vec<Alert> {
    let words: Vec<String> = watch_words.iter().map(|w| w.trim().to_lowercase()).filter(|w| w.len() >= 3).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let source_name = host_of(feed_url);
    let items: Vec<&str> = if body.contains("<item") { body.split("<item").skip(1).collect() } else { body.split("<entry").skip(1).collect() };
    let mut out = Vec::new();
    for raw in items.into_iter().take(80) {
        let item = raw.split("</item>").next().unwrap_or(raw).split("</entry>").next().unwrap_or(raw);
        let title = plain_text(tag(item, "title").unwrap_or(""));
        if title.is_empty() {
            continue;
        }
        let summary = plain_text(tag(item, "description").or(tag(item, "summary")).or(tag(item, "content")).unwrap_or(""));
        let time = tag(item, "pubDate").or(tag(item, "published")).or(tag(item, "updated")).or(tag(item, "dc:date")).map(feed_time).unwrap_or(0);
        if time > 0 && now - time > 2 * 86_400 {
            continue;
        }
        let haystack = format!("{} {}", title.to_lowercase(), summary.to_lowercase());
        let Some(word) = words.iter().find(|w| haystack.contains(w.as_str())) else { continue };
        let link = tag(item, "link")
            .filter(|l| !l.is_empty())
            .map(|l| plain_text(l))
            .or_else(|| item.split("<link").nth(1).and_then(|l| l.split("href=\"").nth(1)).and_then(|h| h.split('"').next()).map(str::to_string))
            .filter(|l| l.starts_with("http"));
        let urgent = URGENT_WORDS.iter().any(|u| haystack.contains(u));
        let mut snippet: String = summary.chars().take(220).collect();
        if summary.chars().count() > 220 {
            snippet.push('…');
        }
        out.push(Alert {
            id: format!("news:{}", link.clone().unwrap_or_else(|| title.clone())),
            source: "news".to_string(),
            level: if urgent { Level::Warning } else { Level::Watch },
            title,
            detail: format!("Matched \u{201c}{word}\u{201d} in {source_name}.{}", if snippet.is_empty() { String::new() } else { format!(" {snippet}") }),
            time,
            url: link,
            distance_km: None,
        });
    }
    out
}

/// A news feed address the station may fetch: http or https, nothing else.
pub fn clean_feed_url(url: &str) -> Option<String> {
    let u = url.trim();
    ((u.starts_with("https://") || u.starts_with("http://")) && u.len() > 10 && !u.contains(char::is_whitespace)).then(|| u.to_string())
}

/// Airport codes as the FAA lists them: three or four letters or digits.
pub fn clean_airports(list: &str) -> Vec<String> {
    let mut out: Vec<String> = list
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| (3..=4).contains(&c.len()) && c.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Highest level first, then newest; one entry per id.
pub fn merge(mut alerts: Vec<Alert>) -> Vec<Alert> {
    alerts.sort_by(|a, b| b.level.cmp(&a.level).then(b.time.cmp(&a.time)).then(a.id.cmp(&b.id)));
    let mut seen = std::collections::HashSet::new();
    alerts.retain(|a| seen.insert(a.id.clone()));
    alerts.truncate(60);
    alerts
}

fn get(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| e.to_string())
}

/// A US state or territory code, or nothing. Anything else is not sent anywhere.
pub fn clean_region(region: &str) -> Option<String> {
    let r = region.trim().to_ascii_uppercase();
    (r.len() == 2 && r.chars().all(|c| c.is_ascii_uppercase())).then_some(r)
}

/// Read every feed. A feed that fails is reported and the others still count.
pub fn fetch_all(opts: &FeedOptions, now: i64) -> AlertState {
    let client = match reqwest::blocking::Client::builder()
        .user_agent("ATHENA-station")
        .timeout(std::time::Duration::from_secs(25))
        .build()
    {
        Ok(c) => c,
        Err(e) => return AlertState { fetched_at: now, alerts: Vec::new(), errors: vec![e.to_string()] },
    };
    let station = opts.station;
    let mut alerts = Vec::new();
    let mut errors = Vec::new();
    match get(&client, "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/4.5_week.geojson") {
        Ok(b) => alerts.extend(parse_usgs(&b, station)),
        Err(e) => errors.push(format!("Earthquakes (USGS): {e}")),
    }
    match get(&client, "https://www.gdacs.org/xml/rss.xml") {
        Ok(b) => alerts.extend(parse_gdacs(&b, station)),
        Err(e) => errors.push(format!("World disasters (GDACS): {e}")),
    }
    match get(&client, "https://services.swpc.noaa.gov/products/alerts.json") {
        Ok(b) => alerts.extend(parse_swpc(&b, now)),
        Err(e) => errors.push(format!("Space weather (NOAA): {e}")),
    }
    if let Some(state) = clean_region(&opts.region) {
        match get(&client, &format!("https://api.weather.gov/alerts/active?status=actual&area={state}")) {
            Ok(b) => alerts.extend(parse_nws(&b)),
            Err(e) => errors.push(format!("Weather warnings (NWS): {e}")),
        }
    }
    if opts.faa {
        match get(&client, "https://nasstatus.faa.gov/api/airport-status-information") {
            Ok(b) => alerts.extend(parse_faa(&b, &opts.airports)),
            Err(e) => errors.push(format!("Air traffic (FAA): {e}")),
        }
    }
    for feed in opts.news_feeds.iter().filter_map(|f| clean_feed_url(f)).take(20) {
        match get(&client, &feed) {
            Ok(b) => alerts.extend(parse_news(&b, &feed, &opts.watch_words, now)),
            Err(e) => errors.push(format!("News ({}): {e}", host_of(&feed))),
        }
    }
    AlertState { fetched_at: now, alerts: merge(alerts), errors }
}

/// Dashboard entries: the hunker-down state and the alerts that need a decision.
pub fn findings(state: &AlertState, hunker: &HunkerState) -> Vec<Finding> {
    let mut out = Vec::new();
    if hunker.active {
        out.push(Finding {
            id: "vault.hunker".to_string(),
            severity: Severity::Critical,
            area: "alerts",
            title: "Hunker-down is active".to_string(),
            detail: format!("Started by {}. {}", hunker.started_by, hunker.reason),
            go_to: Some("archive"),
        });
    }
    for a in state.alerts.iter().filter(|a| a.level >= Level::Warning).take(3) {
        out.push(Finding {
            id: format!("vault.alert.{}", a.id),
            severity: if a.level == Level::Act { Severity::Critical } else { Severity::Warning },
            area: "alerts",
            title: a.title.clone(),
            detail: if a.level == Level::Act && !hunker.active {
                format!("{} Decide whether to hunker down.", a.detail)
            } else {
                a.detail.clone()
            },
            go_to: Some("archive"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATION: (f64, f64) = (44.08, -103.23);

    #[test]
    fn earthquakes_are_graded_by_size_and_distance() {
        assert_eq!(grade_earthquake(7.8, Some(600.0)), Some(Level::Act));
        assert_eq!(grade_earthquake(6.6, Some(350.0)), Some(Level::Act));
        assert_eq!(grade_earthquake(6.2, Some(250.0)), Some(Level::Warning));
        assert_eq!(grade_earthquake(5.1, Some(100.0)), Some(Level::Watch));
        assert_eq!(grade_earthquake(5.1, Some(900.0)), None);
        assert_eq!(grade_earthquake(7.8, Some(9000.0)), Some(Level::Watch));
        assert_eq!(grade_earthquake(7.1, None), Some(Level::Info));
        assert_eq!(grade_earthquake(6.9, None), None);
    }

    #[test]
    fn usgs_feed_keeps_only_quakes_that_matter_here() {
        let body = r#"{"features":[
          {"id":"near","properties":{"mag":6.3,"place":"40 km S of Rapid City","time":1794000000000,"url":"https://e/near","tsunami":0},"geometry":{"coordinates":[-103.2,43.7,10]}},
          {"id":"far","properties":{"mag":5.2,"place":"Fiji","time":1794000000000,"url":"https://e/far","tsunami":0},"geometry":{"coordinates":[178.0,-17.0,10]}}
        ]}"#;
        let alerts = parse_usgs(body, Some(STATION));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, Level::Warning);
        assert_eq!(alerts[0].time, 1_794_000_000);
        assert!(alerts[0].distance_km.unwrap() < 60.0);
    }

    #[test]
    fn gdacs_red_nearby_asks_for_a_decision_and_green_is_dropped() {
        let body = r#"<rss><channel>
          <item><title>Red flood alert</title><link>https://g/1</link><guid>FL1</guid><pubDate>Thu, 12 Nov 2026 08:00:00 GMT</pubDate>
            <gdacs:alertlevel>Red</gdacs:alertlevel><geo:Point><geo:lat>44.5</geo:lat><geo:long>-103.0</geo:long></geo:Point></item>
          <item><title>Orange cyclone</title><guid>TC2</guid><gdacs:alertlevel>Orange</gdacs:alertlevel><geo:Point><geo:lat>-20</geo:lat><geo:long>60</geo:long></geo:Point></item>
          <item><title>Green quake</title><guid>EQ3</guid><gdacs:alertlevel>Green</gdacs:alertlevel></item>
          <item><title>Red quake far away</title><guid>EQ4</guid><gdacs:alertlevel>Red</gdacs:alertlevel><geo:Point><geo:lat>-20</geo:lat><geo:long>60</geo:long></geo:Point></item>
        </channel></rss>"#;
        let alerts = merge(parse_gdacs(body, Some(STATION)));
        assert_eq!(
            alerts.iter().map(|a| (a.id.as_str(), a.level)).collect::<Vec<_>>(),
            vec![("gdacs:FL1", Level::Act), ("gdacs:EQ4", Level::Info)],
            "a distant orange alert is dropped and a distant red one is only information"
        );
        // Without a station position nothing can be called close: red is a watch at most.
        assert!(parse_gdacs(body, None).iter().all(|a| a.level == Level::Watch));
        assert!(alerts[0].time > 1_700_000_000);
    }

    #[test]
    fn space_weather_uses_the_noaa_scale_and_drops_old_notices() {
        let now = 1_794_000_000;
        let fresh = chrono::DateTime::from_timestamp(now - 3600, 0).unwrap().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let body = serde_json::json!([
            {"product_id":"K08A","issue_datetime":fresh,"message":"ALERT: Geomagnetic K-index of 8\nNOAA Scale: G4 - Severe"},
            {"product_id":"K05A","issue_datetime":fresh,"message":"ALERT: Geomagnetic K-index of 5\nNOAA Scale: G1 - Minor"},
            {"product_id":"OLD","issue_datetime":"2020-01-01 00:00:00.000","message":"NOAA Scale: G5 - Extreme"}
        ])
        .to_string();
        let alerts = parse_swpc(&body, now);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, Level::Warning);
        assert_eq!(alerts[0].title, "Geomagnetic storm, NOAA scale G4");
    }

    #[test]
    fn weather_warnings_map_severity_to_level() {
        let body = r#"{"features":[
          {"properties":{"id":"a","severity":"Extreme","headline":"Tornado Emergency","areaDesc":"Pennington","sent":"2026-11-12T08:00:00-07:00"}},
          {"properties":{"id":"b","severity":"Minor","headline":"Frost Advisory"}}
        ]}"#;
        let alerts = parse_nws(body);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, Level::Act);
    }

    const FAA_ROUTINE: &str = "<AIRPORT_STATUS_INFORMATION><Update_Time>Sat Nov 14 01:48:45 2026 GMT</Update_Time>\
<Delay_type><Name>Ground Delay Programs</Name><Ground_Delay_List><Ground_Delay><ARPT>SFO</ARPT><Reason>wind</Reason><Avg>2 hours and 18 minutes</Avg><Max>11 hours</Max></Ground_Delay>\
<Ground_Delay><ARPT>LGA</ARPT><Reason>wind</Reason><Avg>45 minutes</Avg><Max>2 hours</Max></Ground_Delay></Ground_Delay_List></Delay_type>\
<Delay_type><Name>Airport Closures</Name><Airport_Closure_List><Airport><ARPT>LAX</ARPT><Reason>!LAX 05/277 LAX AD AP CLSD TO NON SKED TRANSIENT GA ACFT EXC 24HR PPR</Reason></Airport>\
<Airport><ARPT>DCA</ARPT><Reason>!DCA 11/261 DCA AD AP CLSD TO DIVERSION ACFT 2611140004-2611141100</Reason></Airport></Airport_Closure_List></Delay_type></AIRPORT_STATUS_INFORMATION>";

    fn faa_with_stops(codes: &[&str]) -> String {
        let programs: String = codes.iter().map(|c| format!("<Program><ARPT>{c}</ARPT><Reason>equipment outage</Reason><End_Time>4:30 pm EST</End_Time></Program>")).collect();
        format!("<AIRPORT_STATUS_INFORMATION><Update_Time>Sat Nov 14 01:48:45 2026 GMT</Update_Time><Delay_type><Name>Ground Stop Programs</Name><Ground_Stop_List>{programs}</Ground_Stop_List></Delay_type></AIRPORT_STATUS_INFORMATION>")
    }

    #[test]
    fn routine_faa_status_raises_nothing_worth_a_decision() {
        let alerts = parse_faa(FAA_ROUTINE, &[]);
        assert!(alerts.is_empty(), "two delays and restriction notices are routine: {alerts:?}");
        // A watched airport is reported even when the day is routine.
        let mine = parse_faa(FAA_ROUTINE, &["SFO".to_string()]);
        assert_eq!(mine.len(), 1);
        assert_eq!((mine[0].id.as_str(), mine[0].level), ("faa:SFO:delay", Level::Watch));
        assert_eq!(mine[0].time, 1_794_620_925);
    }

    #[test]
    fn widespread_ground_stops_are_a_warning() {
        let many: Vec<String> = (0..14).map(|i| format!("A{:02}", i)).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let alerts = parse_faa(&faa_with_stops(&refs), &[]);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, Level::Warning);
        assert_eq!(alerts[0].title, "Widespread ground stops: 14 US airports");
        assert!(alerts[0].detail.contains("and 6 more"));
        let few = parse_faa(&faa_with_stops(&["EWR", "JFK", "LGA"]), &["KJFK".to_string()]);
        assert_eq!(few.iter().map(|a| (a.id.as_str(), a.level)).collect::<Vec<_>>(), vec![("faa:stops", Level::Watch), ("faa:JFK:stop", Level::Warning)]);
        assert!(few[1].detail.contains("Expected to end 4:30 pm EST"));
    }

    #[test]
    fn whole_airport_closures_are_told_apart_from_restrictions() {
        assert!(closes_whole_airport("!DEN 11/001 DEN AD AP CLSD 2611140000-2611141200"));
        assert!(!closes_whole_airport("!LAX 05/277 LAX AD AP CLSD TO NON SKED TRANSIENT GA ACFT"));
        assert!(!closes_whole_airport("!SAN AD AP CLSD EXC PPR"));
    }

    #[test]
    fn news_headlines_only_count_when_a_watch_word_matches() {
        let now = 1_794_620_000;
        let fresh = chrono::DateTime::from_timestamp(now - 3600, 0).unwrap().to_rfc2822();
        let rss = format!(
            "<rss><channel>\
            <item><title><![CDATA[FAA issues nationwide ground stop after system outage]]></title><link>https://news.example/a</link><pubDate>{fresh}</pubDate><description>&lt;p&gt;All departures are held.&lt;/p&gt;</description></item>\
            <item><title>Local team wins</title><link>https://news.example/b</link><pubDate>{fresh}</pubDate></item>\
            <item><title>County orders evacuation ahead of fire</title><link>https://news.example/c</link><pubDate>{fresh}</pubDate></item>\
            <item><title>Old ground stop story</title><link>https://news.example/d</link><pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate></item>\
            </channel></rss>"
        );
        let alerts = merge(parse_news(&rss, "https://www.news.example/rss", &default_watch_words(), now));
        assert_eq!(alerts.len(), 2, "{alerts:?}");
        assert_eq!(alerts[0].level, Level::Warning, "an evacuation order is urgent");
        assert_eq!(alerts[1].title, "FAA issues nationwide ground stop after system outage");
        assert!(alerts[1].detail.starts_with("Matched \u{201c}ground stop\u{201d} in news.example."));
        assert_eq!(alerts[1].url.as_deref(), Some("https://news.example/a"));
        assert!(parse_news(&rss, "https://x/rss", &[], now).is_empty(), "no watch words, no alerts");
    }

    #[test]
    fn atom_entries_are_read_too() {
        let atom = r#"<feed><entry><title>Boil water notice issued</title><link href="https://city.example/n1"/><updated>2026-11-14T01:00:00Z</updated><summary>Until further notice.</summary></entry></feed>"#;
        let alerts = parse_news(atom, "https://city.example/feed", &default_watch_words(), 1_794_620_000);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].url.as_deref(), Some("https://city.example/n1"));
    }

    #[test]
    fn feed_and_airport_inputs_are_cleaned() {
        assert_eq!(clean_airports("rap, KDEN  den x 12345"), vec!["DEN", "KDEN", "RAP"]);
        assert_eq!(clean_feed_url(" https://feeds.npr.org/1001/rss.xml "), Some("https://feeds.npr.org/1001/rss.xml".to_string()));
        assert_eq!(clean_feed_url("file:///etc/passwd"), None);
        assert_eq!(clean_feed_url("javascript:alert(1)"), None);
    }

    /// Manual probe against the live feeds: `cargo test feeds_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn feeds_probe() {
        let opts = FeedOptions { station: Some(STATION), region: "SD".into(), faa: true, airports: vec!["RAP".into(), "DEN".into()], news_feeds: vec!["https://feeds.npr.org/1001/rss.xml".into()], watch_words: default_watch_words() };
        let state = fetch_all(&opts, chrono::Utc::now().timestamp());
        eprintln!("errors: {:?}", state.errors);
        for a in &state.alerts {
            eprintln!("{:?} [{}] {} | {} | t={}", a.level, a.source, a.title, a.detail, a.time);
        }
        eprintln!("{} alerts", state.alerts.len());
    }

    #[test]
    fn only_a_state_code_is_ever_sent() {
        assert_eq!(clean_region(" sd "), Some("SD".to_string()));
        assert_eq!(clean_region("44.08,-103.23"), None);
        assert_eq!(clean_region("South Dakota"), None);
        assert_eq!(clean_region(""), None);
    }

    #[test]
    fn the_dashboard_shows_hunker_down_and_serious_alerts_only() {
        let alert = |id: &str, level| Alert { id: id.into(), source: "usgs".into(), level, title: id.into(), detail: "d.".into(), time: 0, url: None, distance_km: None };
        let state = AlertState { fetched_at: 0, alerts: merge(vec![alert("info", Level::Info), alert("act", Level::Act), alert("watch", Level::Watch)]), errors: vec![] };
        let quiet = findings(&state, &HunkerState::default());
        assert_eq!(quiet.len(), 1);
        assert_eq!(quiet[0].severity, Severity::Critical);
        assert!(quiet[0].detail.ends_with("Decide whether to hunker down."));
        let active = findings(&state, &HunkerState { active: true, started_at: 1, started_by: "Sam".into(), reason: "Storm.".into() });
        assert_eq!(active[0].id, "vault.hunker");
        assert_eq!(active.len(), 2);
    }
}
