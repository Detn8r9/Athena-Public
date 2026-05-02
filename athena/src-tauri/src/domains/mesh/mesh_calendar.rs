//! LoRa-safe calendar sync over Meshtastic using compact JSON packets (`ATHCAL1:`).

use serde::{Deserialize, Serialize};

use crate::db::CalendarEvent;

pub const CALENDAR_MESH_PREFIX: &str = "ATHCAL1:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarWire {
    pub id: String,
    pub u: i64,
    pub s: i64,
    pub e: i64,
    pub t: String,
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub l: Option<String>,
    #[serde(default)]
    pub m: Option<String>,
    #[serde(default)]
    pub p: Vec<String>,
    #[serde(default)]
    pub r: Option<i64>,
    #[serde(default)]
    pub rr: Option<String>,
    #[serde(default)]
    pub c: Option<String>,
    #[serde(default)]
    pub src: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub d: bool,
    #[serde(default)]
    pub o: Option<u32>,
}

#[derive(Deserialize)]
struct RawCalendarMesh {
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    u: Option<i64>,
    #[serde(default)]
    s: Option<i64>,
    #[serde(default)]
    e: Option<i64>,
    #[serde(default)]
    ti: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    l: Option<String>,
    #[serde(default)]
    m: Option<String>,
    #[serde(default)]
    p: Option<Vec<String>>,
    #[serde(default)]
    r: Option<i64>,
    #[serde(default)]
    rr: Option<String>,
    #[serde(default)]
    c: Option<String>,
    #[serde(default)]
    src: Option<String>,
    #[serde(default)]
    a: Option<bool>,
    #[serde(default)]
    d: Option<bool>,
    #[serde(default)]
    o: Option<u32>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CalendarMeshInbound {
    Entry(CalendarWire),
    SyncRequest,
}

pub fn is_calendar_mesh_text(text: &str) -> bool {
    text.trim_start().starts_with(CALENDAR_MESH_PREFIX)
}

pub fn wire_from_calendar_event(ev: &CalendarEvent, local_node: u32) -> CalendarWire {
    CalendarWire {
        id: ev.id.clone(),
        u: ev.updated_at,
        s: ev.start_ts,
        e: ev.end_ts,
        t: ev.title.clone(),
        n: ev.notes.clone(),
        l: ev.location_label.clone(),
        m: ev.linked_marker_id.clone(),
        p: ev.participant_names.clone(),
        r: ev.reminder_minutes_before,
        rr: ev.recurrence_rule.clone(),
        c: ev.category.clone(),
        src: Some(ev.source.clone()),
        all: ev.all_day,
        d: ev.deleted,
        o: if local_node == 0 {
            None
        } else {
            Some(local_node)
        },
    }
}

pub fn calendar_event_from_wire(wire: CalendarWire, fallback_author: u32) -> CalendarEvent {
    CalendarEvent {
        id: wire.id,
        title: wire.t,
        notes: wire.n,
        start_ts: wire.s,
        end_ts: wire.e,
        all_day: wire.all,
        location_label: wire.l,
        linked_marker_id: wire.m,
        participant_names: wire.p,
        reminder_minutes_before: wire.r,
        recurrence_rule: wire.rr,
        category: wire.c,
        source: wire.src.unwrap_or_else(|| "user".to_string()),
        updated_at: wire.u,
        origin_node_id: wire.o.or(if fallback_author == 0 {
            None
        } else {
            Some(fallback_author)
        }),
        deleted: wire.d,
    }
}

pub fn encode_calendar_packet(wire: &CalendarWire) -> Result<String, String> {
    let payload = serde_json::json!({
        "t": "evt",
        "id": wire.id,
        "u": wire.u,
        "s": wire.s,
        "e": wire.e,
        "ti": wire.t,
        "n": wire.n,
        "l": wire.l,
        "m": wire.m,
        "p": wire.p,
        "r": wire.r,
        "rr": wire.rr,
        "c": wire.c,
        "src": wire.src,
        "a": wire.all,
        "d": wire.d,
        "o": wire.o,
    });
    let json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let out = format!("{CALENDAR_MESH_PREFIX}{json}");
    if out.len() > 233 {
        return Err("Calendar packet is too large for mesh.".to_string());
    }
    Ok(out)
}

pub fn encode_calendar_sync_request() -> Result<String, String> {
    let out = format!("{CALENDAR_MESH_PREFIX}{{\"t\":\"req\"}}");
    if out.len() > 233 {
        return Err("Calendar sync request packet too long.".to_string());
    }
    Ok(out)
}

pub fn try_parse_calendar_packet(text: &str) -> Option<CalendarMeshInbound> {
    let t = text.trim();
    let rest = t.strip_prefix(CALENDAR_MESH_PREFIX)?;
    if rest.len() > 230 {
        return None;
    }
    let raw: RawCalendarMesh = serde_json::from_str(rest).ok()?;
    if raw.t.as_deref() == Some("req") {
        return Some(CalendarMeshInbound::SyncRequest);
    }
    let id = raw.id?;
    let u = raw.u?;
    let s = raw.s?;
    let e = raw.e?;
    let title = raw.ti?;
    if id.trim().is_empty() || id.len() > 64 || title.trim().is_empty() || title.len() > 160 {
        return None;
    }
    Some(CalendarMeshInbound::Entry(CalendarWire {
        id,
        u,
        s,
        e,
        t: title,
        n: raw.n,
        l: raw.l,
        m: raw.m,
        p: raw.p.unwrap_or_default(),
        r: raw.r,
        rr: raw.rr,
        c: raw.c,
        src: raw.src,
        all: raw.a.unwrap_or(false),
        d: raw.d.unwrap_or(false),
        o: raw.o,
    }))
}
