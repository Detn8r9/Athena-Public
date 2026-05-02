//! LAN wire types shared by `lan_sync` and `lan_cluster` (no Tokio).

use serde::{Deserialize, Serialize};

use crate::db::{
    LogbookEntry, MapMarker, MeshMessage, RadioTelemetrySample, RouteBlockageRow, SavedPlaceRow,
};

pub const LAN_PROTOCOL_VERSION: u32 = 3;

fn default_lan_epoch_ts() -> String {
    "1970-01-01T00:00:00Z".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanCursors {
    pub map_markers_created_at: String,
    pub logbook_created_at: i64,
    pub mesh_messages_updated_at: String,
    pub radio_telemetry_timestamp: String,
    #[serde(default = "default_lan_epoch_ts")]
    pub saved_places_updated_at: String,
    #[serde(default = "default_lan_epoch_ts")]
    pub route_blockages_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum LanPacket {
    Hello {
        node_id: u32,
        protocol_version: u32,
        cursors: LanCursors,
    },
    Pull {
        node_id: u32,
        protocol_version: u32,
        cursors: LanCursors,
    },
    Snapshot {
        map_markers: Vec<MapMarker>,
        logbook_entries: Vec<LogbookEntry>,
        mesh_messages: Vec<MeshMessage>,
        radio_telemetry: Vec<RadioTelemetrySample>,
        #[serde(default)]
        saved_places: Vec<SavedPlaceRow>,
        #[serde(default)]
        route_blockages: Vec<RouteBlockageRow>,
    },
    PushRow {
        table: String,
        row: serde_json::Value,
    },
    Ping {
        seq: u64,
    },
    Pong {
        seq: u64,
    },
    CapabilitiesAnnounce {
        node_id: u32,
        protocol_version: u32,
        ram_gb: u64,
        cpu_cores: usize,
        cpu_tier: String,
        #[serde(default)]
        gpu_vram_mb: Option<u32>,
        ollama_models: Vec<String>,
        ollama_ok: bool,
        updated_at_ms: u64,
    },
    AiJobOffer {
        job_id: String,
        model: String,
        chunk_index: u32,
        chunk_total: u32,
        chunk_text: String,
        #[serde(default)]
        system_hint: Option<String>,
        #[serde(default = "default_temperature")]
        temperature: f32,
        #[serde(default = "default_num_ctx")]
        num_ctx: u32,
    },
    AiJobResult {
        job_id: String,
        chunk_index: u32,
        ok: bool,
        #[serde(default)]
        partial_text: String,
        #[serde(default)]
        error: Option<String>,
    },
}

fn default_temperature() -> f32 {
    0.7
}

fn default_num_ctx() -> u32 {
    4096
}
