//! Stations of one group talking over the internet through a small relay (a Cloudflare Worker;
//! see `cloud/fleet-relay`).
//!
//! The relay is a mailbox and nothing more. Every message is sealed on the station with a key
//! derived from the group key, so the relay stores only ciphertext and could not read it if it
//! were seized. The room name and the relay password are derived from the same group key, so a
//! station that holds the group key can find its fleet and nobody else can.
//!
//! This is an extra path. The local network and radio keep working without it.

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;

pub const STATION_KIND: &str = "vault_fleet_station";
pub const CURSOR_KIND: &str = "vault_fleet_cursor";
pub const CURSOR_ID: &str = "vault_fleet_cursor";
const MAX_MESSAGE_BYTES: usize = 48 * 1024;

fn derive(group_key: &[u8], label: &str) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(group_key).expect("HMAC accepts any key length");
    mac.update(b"athena-fleet-v1/");
    mac.update(label.as_bytes());
    mac.finalize().into_bytes().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What a station needs to use the relay, all derived from the group key.
pub struct FleetKeys {
    /// Mailbox name on the relay.
    pub room: String,
    /// Password for that mailbox.
    pub auth: String,
    seal: [u8; 32],
}

impl FleetKeys {
    pub fn from_group_key(group_key: &[u8]) -> Self {
        Self {
            room: hex(&derive(group_key, "room")[..16]),
            auth: hex(&derive(group_key, "auth")),
            seal: derive(group_key, "seal"),
        }
    }

    /// Encrypt a message. Output is base64 of a random 12-byte nonce followed by the ciphertext.
    pub fn seal(&self, message: &FleetMessage) -> Result<String, String> {
        let plain = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        if plain.len() > MAX_MESSAGE_BYTES {
            return Err("That message is too large for the relay.".to_string());
        }
        let cipher = ChaCha20Poly1305::new((&self.seal).into());
        let mut nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        let sealed = cipher.encrypt(Nonce::from_slice(&nonce), plain.as_ref()).map_err(|_| "Could not seal the message.".to_string())?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&sealed);
        Ok(base64::engine::general_purpose::STANDARD.encode(out))
    }

    /// Decrypt a message. Anything not sealed with this group's key is refused.
    pub fn open(&self, sealed: &str) -> Result<FleetMessage, String> {
        let raw = base64::engine::general_purpose::STANDARD.decode(sealed.trim()).map_err(|_| "Not a sealed message.".to_string())?;
        if raw.len() < 12 + 16 {
            return Err("Not a sealed message.".to_string());
        }
        let cipher = ChaCha20Poly1305::new((&self.seal).into());
        let plain = cipher
            .decrypt(Nonce::from_slice(&raw[..12]), &raw[12..])
            .map_err(|_| "This message was not sealed with your group key.".to_string())?;
        serde_json::from_slice(&plain).map_err(|e| e.to_string())
    }
}

/// What stations say to each other. Titles and archive names only: never private-shelf files,
/// patient records or group memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum FleetMessage {
    /// What this station's library holds, so the fleet can see who has what.
    Holdings {
        station: String,
        sent_at: i64,
        /// Topic key -> coverage level (`none`, `thin`, `some`, `good`).
        topics: BTreeMap<String, String>,
        /// File names of installed archives and packs.
        archives: Vec<String>,
    },
    /// "Does anyone have something on this?"
    Want { station: String, sent_at: i64, query: String },
    /// Answer to a `Want`: titles only.
    Have { station: String, sent_at: i64, query: String, titles: Vec<String> },
    /// One station started hunker-down. Others show it and their operators decide.
    Hunker { station: String, sent_at: i64, reason: String },
    StandDown { station: String, sent_at: i64 },
}

impl FleetMessage {
    pub fn station(&self) -> &str {
        match self {
            FleetMessage::Holdings { station, .. }
            | FleetMessage::Want { station, .. }
            | FleetMessage::Have { station, .. }
            | FleetMessage::Hunker { station, .. }
            | FleetMessage::StandDown { station, .. } => station,
        }
    }
}

/// What this station last heard from another station of the fleet.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStation {
    pub station: String,
    pub last_seen: i64,
    #[serde(default)]
    pub topics: BTreeMap<String, String>,
    #[serde(default)]
    pub archives: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetCursor {
    /// Sequence number of the last relay message this station has read.
    pub after: u64,
    pub last_sync: i64,
    /// Fingerprint of the holdings last posted, so unchanged holdings are not re-sent.
    #[serde(default)]
    pub holdings_hash: String,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// The relay must be reached over HTTPS. Plain HTTP is allowed only for a relay under test on
/// this computer.
pub fn clean_relay_url(url: &str) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(u).map_err(|_| "Enter the relay address, such as https://athena-fleet.example.workers.dev".to_string())?;
    let local = matches!(parsed.host_str(), Some("localhost") | Some("127.0.0.1"));
    match parsed.scheme() {
        "https" => Ok(u.to_string()),
        "http" if local => Ok(u.to_string()),
        _ => Err("The relay address must start with https://".to_string()),
    }
}

#[derive(Deserialize)]
struct RelayMessage {
    seq: u64,
    body: String,
}

#[derive(Deserialize)]
struct RelayList {
    messages: Vec<RelayMessage>,
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("ATHENA-station")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

pub fn post(relay: &str, keys: &FleetKeys, message: &FleetMessage) -> Result<(), String> {
    let body = keys.seal(message)?;
    let res = client()?
        .post(format!("{relay}/v1/rooms/{}/messages", keys.room))
        .bearer_auth(&keys.auth)
        .json(&serde_json::json!({ "body": body }))
        .send()
        .map_err(|e| format!("Could not reach the relay: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("The relay refused the message ({}).", res.status()));
    }
    Ok(())
}

/// Messages after `after`, opened. Returns them with the highest sequence number seen. Messages
/// that do not open with the group key are skipped.
pub fn fetch(relay: &str, keys: &FleetKeys, after: u64) -> Result<(Vec<FleetMessage>, u64), String> {
    let res = client()?
        .get(format!("{relay}/v1/rooms/{}/messages?after={after}", keys.room))
        .bearer_auth(&keys.auth)
        .send()
        .map_err(|e| format!("Could not reach the relay: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("The relay refused the request ({}).", res.status()));
    }
    let list: RelayList = res.json().map_err(|e| format!("Unexpected relay answer: {e}"))?;
    let mut newest = after;
    let mut out = Vec::new();
    for m in list.messages {
        newest = newest.max(m.seq);
        if let Ok(msg) = keys.open(&m.body) {
            out.push(msg);
        }
    }
    Ok((out, newest))
}

/// Archives other stations hold that this one does not, with who has them.
pub fn missing_here(own_archives: &[String], stations: &[FleetStation]) -> Vec<(String, Vec<String>)> {
    let base = |name: &str| super::updates::split_release(name).map(|(b, _)| b).unwrap_or_else(|| name.to_string());
    let own: std::collections::HashSet<String> = own_archives.iter().map(|a| base(a)).collect();
    let mut by_base: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for s in stations {
        for a in &s.archives {
            let b = base(a);
            if own.contains(&b) || a.starts_with(super::packs::PACK_PREFIX) {
                continue;
            }
            let slot = by_base.entry(b).or_insert_with(|| (a.clone(), Vec::new()));
            if a > &slot.0 {
                slot.0 = a.clone();
            }
            if !slot.1.contains(&s.station) {
                slot.1.push(s.station.clone());
            }
        }
    }
    by_base.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holdings(station: &str) -> FleetMessage {
        FleetMessage::Holdings {
            station: station.to_string(),
            sent_at: 100,
            topics: BTreeMap::from([("medical".to_string(), "good".to_string())]),
            archives: vec!["wikipedia_en_medicine_nopic_2026-10.zim".to_string()],
        }
    }

    #[test]
    fn a_sealed_message_opens_only_with_the_same_group_key() {
        let ours = FleetKeys::from_group_key(&[7u8; 32]);
        let theirs = FleetKeys::from_group_key(&[8u8; 32]);
        let sealed = ours.seal(&holdings("North")).unwrap();
        assert!(!sealed.contains("North"), "the relay sees no readable text");
        assert_eq!(ours.open(&sealed).unwrap(), holdings("North"));
        assert!(theirs.open(&sealed).is_err());
        // The same message sealed twice differs (fresh nonce), so the relay cannot spot repeats.
        assert_ne!(sealed, ours.seal(&holdings("North")).unwrap());
        // A flipped bit is caught.
        let mut raw = base64::engine::general_purpose::STANDARD.decode(&sealed).unwrap();
        raw[20] ^= 1;
        assert!(ours.open(&base64::engine::general_purpose::STANDARD.encode(raw)).is_err());
    }

    #[test]
    fn room_and_password_come_from_the_group_key_and_differ_between_groups() {
        let a = FleetKeys::from_group_key(&[1u8; 32]);
        let b = FleetKeys::from_group_key(&[2u8; 32]);
        assert_eq!(a.room.len(), 32);
        assert_eq!(a.auth.len(), 64);
        assert_ne!(a.room, b.room);
        assert_ne!(a.auth, b.auth);
        assert_ne!(a.room, a.auth[..32]);
        assert_eq!(a.room, FleetKeys::from_group_key(&[1u8; 32]).room);
    }

    #[test]
    fn messages_keep_their_wire_names() {
        let json = serde_json::to_value(FleetMessage::Want { station: "N".into(), sent_at: 5, query: "alternator".into() }).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "want", "station": "N", "sentAt": 5, "query": "alternator"}));
    }

    /// Two stations talk through a relay: `FLEET_RELAY=http://127.0.0.1:8787 cargo test fleet_probe -- --ignored`.
    #[test]
    #[ignore]
    fn fleet_probe() {
        let Ok(relay) = std::env::var("FLEET_RELAY") else { return };
        let key = [42u8; 32];
        let north = FleetKeys::from_group_key(&key);
        let south = FleetKeys::from_group_key(&key);
        let stranger = FleetKeys::from_group_key(&[1u8; 32]);
        post(&relay, &north, &holdings("North")).unwrap();
        post(&relay, &north, &FleetMessage::Want { station: "North".into(), sent_at: 2, query: "alternator".into() }).unwrap();
        let (got, newest) = fetch(&relay, &south, 0).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], holdings("North"));
        assert_eq!(fetch(&relay, &south, newest).unwrap().0.len(), 0, "nothing new after the cursor");
        // Another group lands in its own room and sees nothing of ours.
        assert_eq!(fetch(&relay, &stranger, 0).unwrap().0.len(), 0);
        // The right room with the wrong password is refused by the relay.
        let wrong = FleetKeys { room: north.room.clone(), auth: "c".repeat(64), seal: north.seal };
        assert!(fetch(&relay, &wrong, 0).unwrap_err().contains("refused"));
    }

    #[test]
    fn relay_address_must_be_https_unless_it_is_this_computer() {
        assert_eq!(clean_relay_url("https://fleet.example.workers.dev/").unwrap(), "https://fleet.example.workers.dev");
        assert!(clean_relay_url("http://fleet.example.workers.dev").is_err());
        assert!(clean_relay_url("http://127.0.0.1:8787").is_ok());
        assert!(clean_relay_url("fleet").is_err());
    }

    #[test]
    fn missing_archives_are_listed_once_with_who_has_them() {
        let own = vec!["wikipedia_en_medicine_nopic_2026-08.zim".to_string()];
        let stations = vec![
            FleetStation {
                station: "North".into(),
                last_seen: 1,
                topics: BTreeMap::new(),
                archives: vec![
                    "wikipedia_en_medicine_nopic_2026-10.zim".into(),
                    "mechanics.stackexchange.com_en_all_2026-08.zim".into(),
                    "athena-pack_private-notes_2026-11-12.zim".into(),
                ],
            },
            FleetStation { station: "South".into(), last_seen: 1, topics: BTreeMap::new(), archives: vec!["mechanics.stackexchange.com_en_all_2026-10.zim".into()] },
        ];
        assert_eq!(
            missing_here(&own, &stations),
            vec![("mechanics.stackexchange.com_en_all_2026-10.zim".to_string(), vec!["North".to_string(), "South".to_string()])]
        );
    }
}
