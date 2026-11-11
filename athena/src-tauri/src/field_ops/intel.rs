//! Radio intelligence (S2): a log of what was heard on the radio, with watch words.
//!
//! When the internet is gone, radio is the news. Each thing heard is logged with when, where on
//! the dial and what was said. Entries are checked against a list of watch words ("evacuate",
//! "quarantine", a place name); a match raises a notification and shows on the Dashboard. A
//! recording can be transcribed by the local speech-to-text service instead of typed.
//!
//! Not in this version: tuning a software-defined radio through a frequency list on its own.
//! That needs the SDR hardware to build and test against.

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const INTERCEPT_KIND: &str = "intercept";
const WORDS_KIND: &str = "intel_watch_words";
const WORDS_ID: &str = "intel_watch_words";
const HOUR: i64 = 3_600;
/// Matches within this window are shown on the Dashboard.
const RECENT_WINDOW_HOURS: i64 = 12;
const MAX_TEXT_CHARS: usize = 6_000;

pub const DEFAULT_WATCH_WORDS: &[&str] = &[
    "evacuate",
    "evacuation",
    "quarantine",
    "outbreak",
    "contaminated",
    "curfew",
    "checkpoint",
    "shelter",
    "rescue",
    "medical",
    "water",
    "food",
    "fire",
    "flood",
    "storm",
    "attack",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intercept {
    /// Unix seconds it was heard.
    pub at: i64,
    /// Where on the dial: "162.550 MHz", "AM 840", "CB 9".
    #[serde(default)]
    pub frequency: String,
    /// Who or what was transmitting, as far as known.
    #[serde(default)]
    pub source: String,
    pub text: String,
    /// Watch words found in the text when it was logged.
    #[serde(default)]
    pub matched: Vec<String>,
    #[serde(default)]
    pub logged_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchWords {
    pub words: Vec<String>,
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Watch words and phrases found in `text`, matched on whole words, ignoring case and
/// punctuation. Returned in the order they appear in the watch list, once each.
pub fn match_watch_words(text: &str, words: &[String]) -> Vec<String> {
    let haystack = format!(" {} ", normalize(text));
    let mut out: Vec<String> = Vec::new();
    for w in words {
        let needle = normalize(w);
        if needle.is_empty() {
            continue;
        }
        if haystack.contains(&format!(" {needle} ")) && !out.iter().any(|o| normalize(o) == needle) {
            out.push(w.trim().to_string());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    pub total: usize,
    pub matched: usize,
    /// (watch word, times heard), most frequent first.
    pub words: Vec<(String, usize)>,
    /// (frequency, entries), busiest first.
    pub frequencies: Vec<(String, usize)>,
}

/// Summary of what was heard since `since`.
pub fn digest(intercepts: &[Intercept], since: i64) -> Digest {
    use std::collections::BTreeMap;
    let recent: Vec<&Intercept> = intercepts.iter().filter(|i| i.at >= since).collect();
    let mut words: BTreeMap<String, usize> = BTreeMap::new();
    let mut freqs: BTreeMap<String, usize> = BTreeMap::new();
    for i in &recent {
        for w in &i.matched {
            *words.entry(w.to_lowercase()).or_insert(0) += 1;
        }
        let f = i.frequency.trim();
        if !f.is_empty() {
            *freqs.entry(f.to_string()).or_insert(0) += 1;
        }
    }
    let by_count = |m: BTreeMap<String, usize>| {
        let mut v: Vec<(String, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    };
    Digest {
        total: recent.len(),
        matched: recent.iter().filter(|i| !i.matched.is_empty()).count(),
        words: by_count(words),
        frequencies: by_count(freqs),
    }
}

/// Watch officer rule: watch words heard on the radio in the last twelve hours.
pub fn findings(intercepts: &[Intercept], now: i64) -> Vec<Finding> {
    let d = digest(intercepts, now - RECENT_WINDOW_HOURS * HOUR);
    if d.matched == 0 {
        return Vec::new();
    }
    vec![Finding {
        id: "intel.watch_words".to_string(),
        severity: Severity::Warning,
        area: "station",
        title: format!(
            "{} with watch words in the last {RECENT_WINDOW_HOURS} hours",
            if d.matched == 1 { "1 radio intercept".to_string() } else { format!("{} radio intercepts", d.matched) }
        ),
        detail: d
            .words
            .iter()
            .take(6)
            .map(|(w, n)| if *n > 1 { format!("{w} ×{n}") } else { w.clone() })
            .collect::<Vec<_>>()
            .join(", "),
        go_to: Some("field_ops"),
    }]
}

pub fn load_words(db: &crate::db::Database) -> Vec<String> {
    store::get::<WatchWords>(db, WORDS_KIND, WORDS_ID)
        .ok()
        .flatten()
        .map(|r| r.data.words)
        .unwrap_or_else(|| DEFAULT_WATCH_WORDS.iter().map(|w| w.to_string()).collect())
}

pub fn load(db: &crate::db::Database) -> Vec<Intercept> {
    store::list::<Intercept>(db, INTERCEPT_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelView {
    pub intercepts: Vec<Record<Intercept>>,
    pub watch_words: Vec<String>,
    /// Last 24 hours.
    pub digest: Digest,
}

#[tauri::command]
pub fn get_intel(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<IntelView, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut intercepts = store::list::<Intercept>(&db, INTERCEPT_KIND)?;
    intercepts.sort_by(|a, b| b.data.at.cmp(&a.data.at));
    let plain: Vec<Intercept> = intercepts.iter().map(|r| r.data.clone()).collect();
    Ok(IntelView {
        digest: digest(&plain, store::now() - 24 * HOUR),
        watch_words: load_words(&db),
        intercepts,
    })
}

/// Log something heard. Watch words are matched here; a match raises a notification.
#[tauri::command]
pub fn add_intercept(
    mut intercept: Intercept,
    app_handle: tauri::AppHandle,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<Intercept>, String> {
    let user = super::require_member(&session)?;
    intercept.text = intercept.text.trim().to_string();
    if intercept.text.is_empty() {
        return Err("Write down what was heard.".to_string());
    }
    if intercept.text.chars().count() > MAX_TEXT_CHARS {
        return Err("That entry is too long. Split it into separate intercepts.".to_string());
    }
    if intercept.at <= 0 {
        intercept.at = store::now();
    }
    intercept.frequency = intercept.frequency.trim().to_string();
    intercept.source = intercept.source.trim().to_string();
    intercept.logged_by = Some(user.display_name.clone().unwrap_or(user.name.clone()));
    let db = app.db.lock().map_err(|e| e.to_string())?;
    intercept.matched = match_watch_words(&intercept.text, &load_words(&db));
    let rec = store::put(&db, INTERCEPT_KIND, None, &intercept)?;
    if !intercept.matched.is_empty() {
        let preview: String = intercept.text.chars().take(180).collect();
        let where_heard = if intercept.frequency.is_empty() { String::new() } else { format!(" on {}", intercept.frequency) };
        if let Ok(n) = db.command_notification_insert(
            "intel",
            "high",
            &format!("Heard{where_heard}: {}", intercept.matched.join(", ")),
            Some(&preview),
            Some(&rec.id),
        ) {
            let _ = app_handle.emit(crate::mesh_listener::ATHENA_COMMAND_NOTIFICATION, &n);
        }
    }
    Ok(rec)
}

#[tauri::command]
pub fn delete_intercept(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, INTERCEPT_KIND, &id)
}

#[tauri::command]
pub fn set_intel_watch_words(
    words: Vec<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<String>, String> {
    super::require_member(&session)?;
    let mut clean: Vec<String> = Vec::new();
    for w in words {
        let w = w.split_whitespace().collect::<Vec<_>>().join(" ");
        if !w.is_empty() && !clean.iter().any(|c| c.eq_ignore_ascii_case(&w)) {
            clean.push(w);
        }
    }
    if clean.len() > 200 {
        return Err("Keep the watch list to 200 words or fewer.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, WORDS_KIND, Some(WORDS_ID), &WatchWords { words: clean.clone() })?;
    Ok(clean)
}

/// Turn a recording of radio traffic into text with the local speech-to-text service.
#[tauri::command]
pub async fn transcribe_intercept_recording(
    path: String,
    whisper_base_url: String,
    app: tauri::AppHandle,
    session: State<'_, AuthSession>,
) -> Result<String, String> {
    super::require_member(&session)?;
    let _ = app.state::<AppState>();
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the recording: {e}"))?;
        let (samples, rate) = super::modem::decode_wav(&bytes)?;
        crate::ptt_record::transcribe_samples(whisper_base_url.trim(), None, &samples, rate)
            .map(|t| t.trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    fn heard(text: &str, hours_ago: i64, freq: &str, matched: &[&str]) -> Intercept {
        Intercept {
            at: NOW - hours_ago * HOUR,
            frequency: freq.to_string(),
            source: String::new(),
            text: text.to_string(),
            matched: words(matched),
            logged_by: None,
        }
    }

    #[test]
    fn matches_whole_words_ignoring_case_and_punctuation() {
        let list = words(&["evacuate", "water", "Fire"]);
        assert_eq!(
            match_watch_words("All residents must EVACUATE. Boil water; fire crews en route!", &list),
            vec!["evacuate", "water", "Fire"]
        );
        assert!(match_watch_words("The waterfall trail is open and firewood is free.", &list).is_empty());
    }

    #[test]
    fn matches_phrases_and_place_names() {
        let list = words(&["boil order", "Miller's Crossing"]);
        assert_eq!(
            match_watch_words("A boil-order is in effect near Millers Crossing... repeat, Miller's Crossing.", &list),
            vec!["boil order", "Miller's Crossing"]
        );
        assert!(match_watch_words("order a boiler", &list).is_empty());
    }

    #[test]
    fn each_word_is_reported_once_and_blanks_are_ignored() {
        let list = words(&["water", "", "  ", "WATER"]);
        assert_eq!(match_watch_words("water water everywhere", &list), vec!["water"]);
    }

    #[test]
    fn digest_counts_recent_entries_words_and_frequencies() {
        let log = vec![
            heard("evacuate north side", 1, "162.550 MHz", &["evacuate"]),
            heard("water point at school", 3, "162.550 MHz", &["water"]),
            heard("evacuate south side too, bring water", 5, "AM 840", &["evacuate", "Water"]),
            heard("music", 6, "AM 840", &[]),
            heard("old news about a storm", 40, "AM 840", &["storm"]),
        ];
        let d = digest(&log, NOW - 24 * HOUR);
        assert_eq!(d.total, 4);
        assert_eq!(d.matched, 3);
        assert_eq!(d.words, vec![("evacuate".to_string(), 2), ("water".to_string(), 2)]);
        assert_eq!(d.frequencies, vec![("162.550 MHz".to_string(), 2), ("AM 840".to_string(), 2)]);
    }

    #[test]
    fn watch_finding_covers_the_last_twelve_hours_only() {
        let log = vec![
            heard("evacuate north side", 1, "", &["evacuate"]),
            heard("evacuate again", 2, "", &["evacuate"]),
            heard("storm warning", 20, "", &["storm"]),
        ];
        let f = findings(&log, NOW);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].title, "2 radio intercepts with watch words in the last 12 hours");
        assert_eq!(f[0].detail, "evacuate ×2");
        assert!(findings(&log[2..], NOW).is_empty());
    }
}
