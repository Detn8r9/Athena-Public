//! Trust between stations (S11): prove a message came from someone holding the group key, and
//! let a person under coercion raise a silent alarm.
//!
//! * **Signing.** Stations that share a group key append a short tag to what they send. A
//!   station that receives a tagged message can tell whether it was made with the same key.
//!   The tag is a truncated HMAC-SHA256, 6 bytes, which fits LoRa's small messages. It covers a
//!   30-minute time slot as well as the text, so a recorded message cannot be replayed later.
//!   Clocks must agree to within about half an hour (see the Trusted time tool).
//! * **Duress word.** A word the group agrees in advance. A message containing it looks normal
//!   to whoever is forcing it to be sent, and raises an urgent alert at every other station.
//!
//! Limits, stated plainly: this is a shared secret. Anyone who has the key can sign as the
//! group, so a captured radio or laptop means the key must be changed. Bare handheld radios
//! cannot sign; only ATHENA stations can.

use std::sync::RwLock;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use super::store;
use crate::auth::AuthSession;
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

const SETTINGS_KIND: &str = "trust_settings";
const SETTINGS_ID: &str = "trust_settings";
const TAG_BYTES: usize = 6;
const TAG_MARK: &str = " ^";
/// Length of the time slot a tag is valid for.
pub const TIME_SLOT_SECS: i64 = 1_800;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustSettings {
    /// 32-byte group key as 64 hex characters.
    #[serde(default)]
    pub key_hex: Option<String>,
    #[serde(default)]
    pub sign_outgoing: bool,
    #[serde(default)]
    pub duress_word: Option<String>,
    /// Name this station gives itself in bundles and reports.
    #[serde(default)]
    pub station_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authenticity {
    /// No tag on the message.
    Unsigned,
    /// Tag matches this station's group key.
    Verified,
    /// Tag present but does not match: wrong key, altered text, or clocks too far apart.
    Invalid,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if clean.len() % 2 != 0 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
        .collect()
}

/// A fresh random group key, as 64 hex characters.
pub fn generate_key() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

/// Accept either a 64-character hex key (spaces and dashes allowed) or a passphrase of at least
/// 12 characters, which is hashed to a key. Returns the key as hex.
pub fn normalize_key(input: &str) -> Result<String, String> {
    let t = input.trim();
    if let Some(bytes) = unhex(t) {
        if bytes.len() == 32 {
            return Ok(hex(&bytes));
        }
    }
    if t.chars().count() < 12 {
        return Err("A group key must be the 64-character key from another station, or a passphrase of at least 12 characters.".to_string());
    }
    Ok(hex(&Sha256::digest(t.as_bytes())))
}

/// Show a key in groups of four so it can be read aloud or copied by hand.
pub fn format_key_for_display(key_hex: &str) -> String {
    key_hex
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn tag_for(key: &[u8], body: &str, slot: i64) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(&slot.to_be_bytes());
    mac.update(&[0]);
    mac.update(body.as_bytes());
    hex(&mac.finalize().into_bytes()[..TAG_BYTES])
}

/// Append a tag to `body`.
pub fn sign(key: &[u8], body: &str, now: i64) -> String {
    format!("{body}{TAG_MARK}{}", tag_for(key, body, now.div_euclid(TIME_SLOT_SECS)))
}

/// Split a trailing tag off a message. Returns the body and the tag when one is present.
pub fn split_tag(text: &str) -> (&str, Option<&str>) {
    if let Some(idx) = text.rfind(TAG_MARK) {
        let tag = &text[idx + TAG_MARK.len()..];
        if tag.len() == TAG_BYTES * 2 && tag.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
            return (&text[..idx], Some(tag));
        }
    }
    (text, None)
}

/// Check a received message. Accepts the current time slot and the one either side, so a
/// message sent just before a slot boundary still verifies.
pub fn verify<'a>(key: Option<&[u8]>, text: &'a str, now: i64) -> (&'a str, Authenticity) {
    let (body, tag) = split_tag(text);
    let Some(tag) = tag else {
        return (body, Authenticity::Unsigned);
    };
    let Some(key) = key else {
        // We cannot check it, but we still strip it so commands parse.
        return (body, Authenticity::Unsigned);
    };
    let slot = now.div_euclid(TIME_SLOT_SECS);
    let ok = (-1..=1).any(|d| constant_time_eq(tag_for(key, body, slot + d).as_bytes(), tag.as_bytes()));
    (body, if ok { Authenticity::Verified } else { Authenticity::Invalid })
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// True when `text` contains the duress word as a whole word, ignoring case.
pub fn contains_duress(text: &str, duress_word: &str) -> bool {
    let word = duress_word.trim().to_lowercase();
    if word.is_empty() {
        return false;
    }
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == word)
}

/// Tag for a whole file (courier bundles): full-length HMAC, no time slot.
pub fn file_tag(key: &[u8], bytes: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(b"athena-bundle-v1");
    mac.update(bytes);
    hex(&mac.finalize().into_bytes())
}

pub fn file_tag_matches(key: &[u8], bytes: &[u8], tag: &str) -> bool {
    constant_time_eq(file_tag(key, bytes).as_bytes(), tag.trim().as_bytes())
}

// ---- station state -------------------------------------------------------------------------

static STATE: RwLock<Option<TrustSettings>> = RwLock::new(None);

pub fn load_settings(db: &crate::db::Database) -> TrustSettings {
    store::get::<TrustSettings>(db, SETTINGS_KIND, SETTINGS_ID)
        .ok()
        .flatten()
        .map(|r| r.data)
        .unwrap_or_default()
}

fn save_settings(db: &crate::db::Database, settings: &TrustSettings) -> Result<(), String> {
    store::put(db, SETTINGS_KIND, Some(SETTINGS_ID), settings)?;
    refresh_from_db(db);
    Ok(())
}

/// Copy the settings into memory, where the radio thread can read them without the database.
pub fn refresh_from_db(db: &crate::db::Database) {
    let settings = load_settings(db);
    if let Ok(mut guard) = STATE.write() {
        *guard = Some(settings);
    }
}

fn current() -> TrustSettings {
    STATE.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

pub fn key_bytes(settings: &TrustSettings) -> Option<Vec<u8>> {
    settings.key_hex.as_deref().and_then(unhex).filter(|k| k.len() == 32)
}

/// Sign outgoing free text when the station is set to. Structured sync packets are left alone.
pub fn sign_if_enabled(text: String, now: i64) -> String {
    let settings = current();
    if !settings.sign_outgoing {
        return text;
    }
    match key_bytes(&settings) {
        Some(key) => sign(&key, &text, now),
        None => text,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    /// The message with any tag removed.
    pub body: String,
    pub authenticity: Authenticity,
    pub duress: bool,
}

/// Inspect a received message against this station's key and duress word.
pub fn inspect_incoming(text: &str, now: i64) -> Incoming {
    let settings = current();
    let key = key_bytes(&settings);
    let (body, authenticity) = verify(key.as_deref(), text, now);
    Incoming {
        duress: settings
            .duress_word
            .as_deref()
            .map(|w| contains_duress(body, w))
            .unwrap_or(false),
        body: body.to_string(),
        authenticity,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustView {
    pub has_key: bool,
    /// Short fingerprint for comparing keys between stations without revealing them.
    pub key_fingerprint: Option<String>,
    pub sign_outgoing: bool,
    pub has_duress_word: bool,
    pub station_name: Option<String>,
}

fn view_of(settings: &TrustSettings) -> TrustView {
    let key = key_bytes(settings);
    TrustView {
        has_key: key.is_some(),
        key_fingerprint: key.map(|k| hex(&Sha256::digest(&k)[..4]).to_uppercase()),
        sign_outgoing: settings.sign_outgoing,
        has_duress_word: settings.duress_word.as_deref().map(|w| !w.trim().is_empty()).unwrap_or(false),
        station_name: settings.station_name.clone(),
    }
}

/// Changing keys and the duress word is for the people who run the group.
fn require_leader(session: &AuthSession) -> Result<(), String> {
    let user = super::require_member(session)?;
    if user.rank_level < 4 {
        return Err("Only senior members (rank 4 and above) can change the group key or duress word.".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn get_trust_settings(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<TrustView, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    Ok(view_of(&load_settings(&db)))
}

/// Create a new group key and return it once, formatted for copying to the other stations.
#[tauri::command]
pub fn generate_group_key(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<String, String> {
    require_leader(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut settings = load_settings(&db);
    let key = generate_key();
    settings.key_hex = Some(key.clone());
    save_settings(&db, &settings)?;
    Ok(format_key_for_display(&key))
}

/// Enter the key another station generated (or a shared passphrase), or pass `None` to remove it.
#[tauri::command]
pub fn set_group_key(
    key: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<TrustView, String> {
    require_leader(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut settings = load_settings(&db);
    settings.key_hex = match key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Some(normalize_key(k)?),
        None => None,
    };
    if settings.key_hex.is_none() {
        settings.sign_outgoing = false;
    }
    save_settings(&db, &settings)?;
    Ok(view_of(&settings))
}

#[tauri::command]
pub fn set_trust_options(
    sign_outgoing: Option<bool>,
    duress_word: Option<String>,
    station_name: Option<String>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<TrustView, String> {
    require_leader(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut settings = load_settings(&db);
    if let Some(sign) = sign_outgoing {
        if sign && key_bytes(&settings).is_none() {
            return Err("Set a group key before turning signing on.".to_string());
        }
        settings.sign_outgoing = sign;
    }
    if let Some(word) = duress_word {
        let w = word.trim().to_string();
        if !w.is_empty() && (w.chars().count() < 4 || w.contains(char::is_whitespace)) {
            return Err("The duress word must be a single word of at least 4 letters.".to_string());
        }
        settings.duress_word = Some(w).filter(|w| !w.is_empty());
    }
    if let Some(name) = station_name {
        settings.station_name = Some(name.trim().to_string()).filter(|n| !n.is_empty());
    }
    save_settings(&db, &settings)?;
    Ok(view_of(&settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";
    const NOW: i64 = 1_800_000_000;

    #[test]
    fn a_signed_message_verifies_and_returns_the_original_text() {
        let signed = sign(KEY, "Supply run leaves at dawn", NOW);
        assert!(signed.starts_with("Supply run leaves at dawn ^"));
        assert_eq!(signed.len(), "Supply run leaves at dawn".len() + 2 + 12);
        let (body, auth) = verify(Some(KEY), &signed, NOW + 60);
        assert_eq!(body, "Supply run leaves at dawn");
        assert_eq!(auth, Authenticity::Verified);
    }

    #[test]
    fn altered_text_or_a_different_key_fails() {
        let signed = sign(KEY, "Meet at the north gate", NOW);
        let forged = signed.replace("north", "south");
        assert_eq!(verify(Some(KEY), &forged, NOW).1, Authenticity::Invalid);
        assert_eq!(verify(Some(b"another key entirely, 32 bytes!!"), &signed, NOW).1, Authenticity::Invalid);
    }

    #[test]
    fn a_recorded_message_cannot_be_replayed_later() {
        let signed = sign(KEY, "All clear", NOW);
        assert_eq!(verify(Some(KEY), &signed, NOW + TIME_SLOT_SECS).1, Authenticity::Verified, "next slot is accepted");
        assert_eq!(verify(Some(KEY), &signed, NOW + 3 * TIME_SLOT_SECS).1, Authenticity::Invalid);
        assert_eq!(verify(Some(KEY), &signed, NOW - 3 * TIME_SLOT_SECS).1, Authenticity::Invalid);
    }

    #[test]
    fn untagged_messages_are_unsigned_and_untouched() {
        let (body, auth) = verify(Some(KEY), "plain message", NOW);
        assert_eq!((body, auth), ("plain message", Authenticity::Unsigned));
        // Something that merely looks a bit like a tag is left alone.
        assert_eq!(split_tag("price went up ^ a lot"), ("price went up ^ a lot", None));
        assert_eq!(split_tag("shout ^ABCDEF012345"), ("shout ^ABCDEF012345", None));
    }

    #[test]
    fn without_a_key_the_tag_is_stripped_but_not_trusted() {
        let signed = sign(KEY, "?inv rice", NOW);
        let (body, auth) = verify(None, &signed, NOW);
        assert_eq!(body, "?inv rice");
        assert_eq!(auth, Authenticity::Unsigned);
    }

    #[test]
    fn keys_can_be_hex_or_a_passphrase() {
        let hex_key = generate_key();
        assert_eq!(hex_key.len(), 64);
        assert_eq!(normalize_key(&format_key_for_display(&hex_key)).unwrap(), hex_key);
        let from_phrase = normalize_key("correct horse battery staple").unwrap();
        assert_eq!(from_phrase.len(), 64);
        assert_eq!(from_phrase, normalize_key("  correct horse battery staple ").unwrap());
        assert!(normalize_key("short").is_err());
        assert_ne!(generate_key(), generate_key());
    }

    #[test]
    fn duress_word_matches_whole_words_only() {
        assert!(contains_duress("All fine here, the weather is BLUEBIRD today", "bluebird"));
        assert!(contains_duress("bluebird.", "Bluebird"));
        assert!(!contains_duress("we saw bluebirds", "bluebird"));
        assert!(!contains_duress("anything", ""));
    }

    #[test]
    fn file_tags_detect_any_change() {
        let tag = file_tag(KEY, b"bundle contents");
        assert_eq!(tag.len(), 64);
        assert!(file_tag_matches(KEY, b"bundle contents", &tag));
        assert!(!file_tag_matches(KEY, b"bundle contentz", &tag));
        assert!(!file_tag_matches(b"another key entirely, 32 bytes!!", b"bundle contents", &tag));
    }
}
