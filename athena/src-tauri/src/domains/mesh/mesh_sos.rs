//! Mesh SOS / emergency: event name and payload for Tauri → React.

use serde::Serialize;

pub const MESH_EMERGENCY_EVENT: &str = "athena:mesh-emergency";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshEmergencyPayload {
    pub node_id: u32,
    pub username: String,
    pub sos_state: i32,
    pub is_registered: bool,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub last_seen_at: Option<i64>,
}
