//! LoRa-safe logbook sync: compact JSON after `ATHLB1:` prefix (Meshtastic text limit 233 bytes).

use serde::{Deserialize, Serialize};

use crate::db::LogbookEntry;

pub const LOGBOOK_MESH_PREFIX: &str = "ATHLB1:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogbookWire {
    pub id: String,
    /// Unix time in milliseconds (authoritative ordering).
    pub c: i64,
    pub b: String,
    /// Originating author node (Meshtastic node id); omit on wire when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
}

/// Parsed ATHLB1 payload: logbook row or a sync request from another Athena.
#[derive(Debug, Clone)]
pub enum LogbookMeshInbound {
    Entry(LogbookWire),
    SyncRequest,
}

#[derive(Deserialize)]
struct RawLogbookMesh {
    #[serde(default)]
    t: Option<String>,
    id: Option<String>,
    c: Option<i64>,
    b: Option<String>,
    #[serde(default)]
    n: Option<u32>,
}

pub fn is_logbook_mesh_text(text: &str) -> bool {
    text.trim_start().starts_with(LOGBOOK_MESH_PREFIX)
}

/// Parse inbound mesh text after prefix; rejects absurd sizes (packet is already capped by radio).
pub fn try_parse_logbook_packet(text: &str) -> Option<LogbookMeshInbound> {
    let t = text.trim();
    let rest = t.strip_prefix(LOGBOOK_MESH_PREFIX)?;
    if rest.len() > 220 {
        return None;
    }
    let raw: RawLogbookMesh = serde_json::from_str(rest).ok()?;
    if raw.t.as_deref() == Some("req") {
        return Some(LogbookMeshInbound::SyncRequest);
    }
    let id = raw.id?;
    let c = raw.c?;
    let b = raw.b?;
    if id.len() > 64 || id.is_empty() {
        return None;
    }
    if b.len() > 512 {
        return None;
    }
    if c.abs() >= (1i64 << 50) {
        return None;
    }
    Some(LogbookMeshInbound::Entry(LogbookWire {
        id,
        c,
        b,
        n: raw.n,
    }))
}

/// Ask other Athena instances to broadcast recent logbook rows (Primary channel).
pub fn encode_logbook_sync_request() -> Result<String, String> {
    let json = r#"{"t":"req"}"#;
    let out = format!("{LOGBOOK_MESH_PREFIX}{json}");
    if out.len() > 233 {
        return Err("Logbook sync request packet too long.".to_string());
    }
    Ok(out)
}

/// Build wire for mesh from a stored row (`local_mesh_node` = this radio's id when the row is local).
pub fn wire_from_logbook_entry(entry: &LogbookEntry, local_mesh_node: u32) -> LogbookWire {
    let n = entry
        .from_node_id
        .or((local_mesh_node != 0).then_some(local_mesh_node));
    LogbookWire {
        id: entry.id.clone(),
        c: entry.created_at,
        b: entry.body.clone(),
        n,
    }
}

/// Build the on-air packet; enforces Meshtastic short-text limit used elsewhere in Athena.
pub fn encode_logbook_packet(wire: &LogbookWire) -> Result<String, String> {
    let max_b = max_body_utf8_bytes_for_mesh();
    let mut b = wire.b.clone();
    if b.len() > max_b {
        let mut end = max_b;
        while end > 0 && !b.is_char_boundary(end) {
            end -= 1;
        }
        b.truncate(end);
    }
    let trimmed = LogbookWire { b, ..wire.clone() };
    let json = serde_json::to_string(&trimmed).map_err(|e| e.to_string())?;
    let out = format!("{LOGBOOK_MESH_PREFIX}{json}");
    if out.len() > 233 {
        return Err(format!(
            "This note is too long for one mesh packet ({} bytes; max 233). Shorten the text and try again.",
            out.len()
        ));
    }
    Ok(out)
}

/// Remaining UTF-8 bytes available for `b` given worst-case `id`, `c`, and `n` JSON overhead.
pub fn max_body_utf8_bytes_for_mesh() -> usize {
    let sample = LogbookWire {
        id: "00000000-0000-4000-8000-000000000000".to_string(),
        c: 9_999_999_999_999,
        b: String::new(),
        n: Some(u32::MAX),
    };
    let json = serde_json::to_string(&sample).unwrap_or_default();
    let empty = format!("{LOGBOOK_MESH_PREFIX}{json}");
    233usize.saturating_sub(empty.len())
}
