//! Shared live picture (S8): map reports that travel by radio and fade as they age.
//!
//! * **Ageing.** A hazard seen an hour ago matters; one reported last week may be gone. Each
//!   report has a freshness that depends on what kind of thing it is.
//! * **Radio sync.** A marker is packed into one short text packet any station on the mesh can
//!   read, so everyone's map shows the same thing without a network.

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::auth::AuthSession;
use crate::db::MapMarker;
use crate::AppState;

pub const PACKET_PREFIX: &str = "ATHMK1:";
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
/// Longest title carried over the radio, in characters.
const MAX_TITLE_CHARS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    /// Old enough that it should be confirmed before relying on it.
    Ageing,
    /// Probably out of date.
    Stale,
}

/// (fresh for, stale after) in seconds, by marker category. Things that move or change fast age
/// fast; fixed things barely age at all.
pub fn lifetime(category: &str) -> (i64, i64) {
    match category.to_ascii_lowercase().as_str() {
        "hazard" | "threat" | "blockage" | "fire" => (6 * HOUR, 3 * DAY),
        "contact" | "sighting" | "person" => (2 * HOUR, DAY),
        "water" | "cache" | "med" | "shelter" | "rally" => (30 * DAY, 180 * DAY),
        _ => (3 * DAY, 30 * DAY),
    }
}

pub fn freshness(category: &str, created_at: i64, now: i64) -> Freshness {
    let age = (now - created_at).max(0);
    let (fresh_for, stale_after) = lifetime(category);
    if age <= fresh_for {
        Freshness::Fresh
    } else if age <= stale_after {
        Freshness::Ageing
    } else {
        Freshness::Stale
    }
}

/// How solid to draw a marker, 1.0 (fresh) down to 0.35 (stale).
pub fn opacity(category: &str, created_at: i64, now: i64) -> f64 {
    let age = (now - created_at).max(0) as f64;
    let (fresh_for, stale_after) = lifetime(category);
    if age <= fresh_for as f64 {
        1.0
    } else if age >= stale_after as f64 {
        0.35
    } else {
        1.0 - 0.65 * (age - fresh_for as f64) / (stale_after - fresh_for) as f64
    }
}

/// Id a marker is known by on the air. Local ids are long; this is short and the same on every
/// station, so a marker that comes back round the mesh is recognised instead of duplicated.
pub fn wire_id(local_id: &str) -> String {
    if local_id.starts_with("mk-") && local_id.len() <= 20 {
        return local_id.to_string();
    }
    let digest = Sha256::digest(local_id.as_bytes());
    format!("mk-{}", digest[..6].iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn clean_field(s: &str) -> String {
    s.replace(['|', '\n', '\r'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pack a marker into one radio text packet.
pub fn encode_marker(marker: &MapMarker, created_at: i64) -> String {
    let title: String = clean_field(&marker.title).chars().take(MAX_TITLE_CHARS).collect();
    format!(
        "{PACKET_PREFIX}{}|{:.5}|{:.5}|{}|{}|{}",
        wire_id(&marker.id),
        marker.lat,
        marker.lng,
        clean_field(&marker.category).to_lowercase(),
        created_at,
        title
    )
}

pub fn is_marker_packet(text: &str) -> bool {
    text.starts_with(PACKET_PREFIX)
}

/// Unpack a marker packet. Returns the marker (with its wire id) and when it was created.
pub fn decode_marker(text: &str) -> Option<(MapMarker, i64)> {
    let rest = text.trim().strip_prefix(PACKET_PREFIX)?;
    let parts: Vec<&str> = rest.splitn(6, '|').collect();
    if parts.len() != 6 {
        return None;
    }
    let id = parts[0].trim();
    if !id.starts_with("mk-") || id.len() > 20 || !id[3..].chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let lat: f64 = parts[1].parse().ok()?;
    let lng: f64 = parts[2].parse().ok()?;
    if !lat.is_finite() || !lng.is_finite() || lat.abs() > 90.0 || lng.abs() > 180.0 {
        return None;
    }
    let created_at: i64 = parts[4].parse().ok()?;
    if created_at <= 0 {
        return None;
    }
    let category = parts[3].trim();
    let created = chrono::DateTime::from_timestamp(created_at, 0)?;
    Some((
        MapMarker {
            id: id.to_string(),
            lat,
            lng,
            title: parts[5].trim().to_string(),
            description: String::new(),
            created_at: created.to_rfc3339(),
            category: if category.is_empty() { "hazard".to_string() } else { category.to_string() },
        },
        created_at,
    ))
}

fn created_unix(marker: &MapMarker) -> i64 {
    chrono::DateTime::parse_from_rfc3339(&marker.created_at)
        .map(|d| d.timestamp())
        .unwrap_or(0)
}

/// Store a marker received by radio. Returns true when the map changed. A marker this station
/// already holds under its own local id is recognised by its wire id and left alone.
pub fn ingest_marker(db: &crate::db::Database, incoming: &MapMarker) -> Result<bool, String> {
    let existing = db.list_map_markers()?;
    if existing.iter().any(|m| m.id != incoming.id && wire_id(&m.id) == incoming.id) {
        return Ok(false);
    }
    db.upsert_map_marker_if_newer(incoming)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PictureMarker {
    #[serde(flatten)]
    pub marker: MapMarker,
    pub created_unix: i64,
    pub freshness: Freshness,
    pub opacity: f64,
    /// True when the marker arrived over the radio.
    pub from_radio: bool,
}

#[tauri::command]
pub fn list_shared_picture(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Vec<PictureMarker>, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp();
    let mut out: Vec<PictureMarker> = db
        .list_map_markers()?
        .into_iter()
        .map(|marker| {
            let created = created_unix(&marker);
            PictureMarker {
                freshness: freshness(&marker.category, created, now),
                opacity: opacity(&marker.category, created, now),
                from_radio: marker.id.starts_with("mk-"),
                created_unix: created,
                marker,
            }
        })
        .collect();
    out.sort_by(|a, b| b.created_unix.cmp(&a.created_unix));
    Ok(out)
}

/// Send one marker to every station on the mesh.
#[tauri::command]
pub async fn share_marker_over_mesh(
    id: String,
    channel: Option<u32>,
    control: State<'_, crate::mesh_listener::MeshRadioControl>,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<(), String> {
    super::require_member(&session)?;
    let packet = {
        let db = app.db.lock().map_err(|e| e.to_string())?;
        let marker = db
            .list_map_markers()?
            .into_iter()
            .find(|m| m.id == id)
            .ok_or("That marker no longer exists.")?;
        let created = created_unix(&marker);
        encode_marker(&marker, if created > 0 { created } else { chrono::Utc::now().timestamp() })
    };
    control.send_text_message(packet, false, channel.unwrap_or(1), None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn marker(id: &str, title: &str, category: &str) -> MapMarker {
        MapMarker {
            id: id.to_string(),
            lat: 44.42801,
            lng: -110.58852,
            title: title.to_string(),
            description: "long description that stays on the station".to_string(),
            created_at: chrono::DateTime::from_timestamp(NOW, 0).unwrap().to_rfc3339(),
            category: category.to_string(),
        }
    }

    #[test]
    fn hazards_age_in_hours_and_caches_in_months() {
        assert_eq!(freshness("hazard", NOW - 2 * HOUR, NOW), Freshness::Fresh);
        assert_eq!(freshness("hazard", NOW - DAY, NOW), Freshness::Ageing);
        assert_eq!(freshness("hazard", NOW - 4 * DAY, NOW), Freshness::Stale);
        assert_eq!(freshness("cache", NOW - 20 * DAY, NOW), Freshness::Fresh);
        assert_eq!(freshness("WATER", NOW - 60 * DAY, NOW), Freshness::Ageing);
        assert_eq!(freshness("something else", NOW - 40 * DAY, NOW), Freshness::Stale);
    }

    #[test]
    fn opacity_fades_from_solid_to_faint() {
        assert_eq!(opacity("hazard", NOW, NOW), 1.0);
        assert_eq!(opacity("hazard", NOW - 10 * DAY, NOW), 0.35);
        let mid = opacity("hazard", NOW - (6 * HOUR + (3 * DAY - 6 * HOUR) / 2), NOW);
        assert!((mid - 0.675).abs() < 1e-6, "{mid}");
        // A marker dated in the future is treated as new, not negative.
        assert_eq!(opacity("hazard", NOW + DAY, NOW), 1.0);
    }

    #[test]
    fn a_marker_survives_the_radio_round_trip() {
        let m = marker("2b6c1f6e-9c3a-4e55-8a1d-0f2f5a3c9e11", "Bridge out | use ford", "Hazard");
        let packet = encode_marker(&m, NOW);
        assert!(packet.starts_with("ATHMK1:mk-"));
        assert!(packet.len() < 120, "must fit one LoRa message: {}", packet.len());
        let (decoded, created) = decode_marker(&packet).expect("decodes");
        assert_eq!(created, NOW);
        assert_eq!(decoded.id, wire_id(&m.id));
        assert!((decoded.lat - m.lat).abs() < 1e-5 && (decoded.lng - m.lng).abs() < 1e-5);
        assert_eq!(decoded.category, "hazard");
        assert_eq!(decoded.title, "Bridge out use ford", "separators in the title cannot break the packet");
        assert_eq!(decoded.description, "");
    }

    #[test]
    fn long_titles_are_cut_and_wire_ids_are_stable() {
        let m = marker("abc", &"x".repeat(200), "water");
        let (decoded, _) = decode_marker(&encode_marker(&m, NOW)).unwrap();
        assert_eq!(decoded.title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(wire_id("abc"), wire_id("abc"));
        assert_ne!(wire_id("abc"), wire_id("abd"));
        assert_eq!(wire_id("mk-0011223344ff"), "mk-0011223344ff", "a wire id is its own wire id");
    }

    #[test]
    fn malformed_packets_are_rejected() {
        for bad in [
            "hello",
            "ATHMK1:",
            "ATHMK1:mk-aa|91.0|0|hazard|1800000000|t",
            "ATHMK1:mk-aa|10|200|hazard|1800000000|t",
            "ATHMK1:xx-aa|10|20|hazard|1800000000|t",
            "ATHMK1:mk-aa|10|20|hazard|notatime|t",
            "ATHMK1:mk-aa|10|20|hazard",
            "ATHMK1:mk-../../etc|10|20|hazard|1800000000|t",
        ] {
            assert!(decode_marker(bad).is_none(), "{bad}");
        }
        assert!(is_marker_packet("ATHMK1:anything"));
        assert!(!is_marker_packet("athmk1:anything"));
    }

    #[test]
    fn a_marker_that_comes_back_round_the_mesh_is_not_duplicated() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(&dir.path().join("t.db")).unwrap();
        let local = marker("2b6c1f6e-9c3a-4e55-8a1d-0f2f5a3c9e11", "Bridge out", "hazard");
        db.insert_map_marker(&local).unwrap();

        // Our own marker, heard back from another station.
        let (echo, _) = decode_marker(&encode_marker(&local, NOW)).unwrap();
        assert!(!ingest_marker(&db, &echo).unwrap());
        assert_eq!(db.list_map_markers().unwrap().len(), 1);

        // Someone else's marker is added once, and repeats change nothing.
        let other = marker("another-station-marker", "Clean spring", "water");
        let (incoming, _) = decode_marker(&encode_marker(&other, NOW)).unwrap();
        assert!(ingest_marker(&db, &incoming).unwrap());
        assert!(!ingest_marker(&db, &incoming).unwrap());
        assert_eq!(db.list_map_markers().unwrap().len(), 2);
    }
}
