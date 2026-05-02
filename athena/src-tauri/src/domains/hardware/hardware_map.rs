//! Capability flags for the Communications Hub (hardware and feature availability).

use serde::Serialize;

use crate::mesh_listener;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareStatus {
    pub voice_distance: bool,
    pub mesh_data: bool,
    pub broadcast_rx: bool,
    pub lan_remote_ai: bool,
}

pub fn get_hardware_status() -> HardwareStatus {
    HardwareStatus {
        voice_distance: false,
        mesh_data: mesh_listener::mesh_radio_is_connected(),
        broadcast_rx: false,
        lan_remote_ai: true,
    }
}

#[tauri::command]
pub fn check_hardware_status() -> Result<HardwareStatus, String> {
    Ok(get_hardware_status())
}
