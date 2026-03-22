//! USB serial discovery and lifecycle helpers for Meshtastic radios (for example Heltec V4).
//! Opens streams compatible with `meshtastic::api::StreamApi::connect`.

use meshtastic::api::StreamHandle;
use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
use tokio_serial::SerialStream;

/// One candidate serial port with USB metadata when the OS exposes it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeshPortDescriptor {
    pub port_name: String,
    pub usb_vid: Option<u16>,
    pub usb_pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl MeshPortDescriptor {
    fn from_serial_info(info: SerialPortInfo) -> Self {
        let (usb_vid, usb_pid, manufacturer, product) = match &info.port_type {
            SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                manufacturer: m,
                product: p,
                ..
            }) => (Some(*vid), Some(*pid), m.clone(), p.clone()),
            _ => (None, None, None, None),
        };
        Self {
            port_name: info.port_name,
            usb_vid,
            usb_pid,
            manufacturer,
            product,
        }
    }
}

/// Lists serial ports using the same enumeration as the rest of the stack (`serialport` crate).
pub fn list_mesh_serial_ports() -> Result<Vec<MeshPortDescriptor>, String> {
    serialport::available_ports().map_err(|e| e.to_string()).map(|ports| {
        ports
            .into_iter()
            .map(MeshPortDescriptor::from_serial_info)
            .collect()
    })
}

/// Minimum heuristic score before we auto-open a port. Prevents treating a single
/// unrelated serial device (Bluetooth modem, debug adapter with no USB metadata) as Meshtastic.
const MIN_AUTO_CONNECT_SCORE: i32 = 55;

fn score_descriptor(desc: &MeshPortDescriptor) -> i32 {
    let mut score: i32 = 0;
    let name_lower = desc.port_name.to_lowercase();
    let product_lower = desc
        .product
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let mfg_lower = desc
        .manufacturer
        .as_deref()
        .unwrap_or("")
        .to_lowercase();

    if name_lower.contains("cu.") {
        score += 40;
    } else if name_lower.contains("tty.") {
        score += 10;
    }

    if name_lower.contains("usbmodem") || name_lower.contains("usbserial") {
        score += 25;
    }

    if product_lower.contains("meshtastic") || mfg_lower.contains("meshtastic") {
        score += 200;
    }

    if let Some(vid) = desc.usb_vid {
        match vid {
            0x303A => score += 100,
            0x10C4 => score += 85,
            0x1A86 => score += 85,
            0x0403 => score += 75,
            0x0483 => score += 60,
            _ => {}
        }
    }

    if product_lower.contains("cp210")
        || product_lower.contains("ch340")
        || product_lower.contains("ch910")
        || product_lower.contains("cp21")
    {
        score += 35;
    }

    if product_lower.contains("slab")
        || product_lower.contains("silicon labs")
        || mfg_lower.contains("silicon labs")
    {
        score += 30;
    }

    if desc.usb_vid.is_some() {
        score += 15;
    }

    score
}

/// Chooses the most likely Meshtastic USB serial port. Returns the system port name.
pub fn select_best_meshtastic_port(descriptors: &[MeshPortDescriptor]) -> Option<String> {
    if descriptors.is_empty() {
        return None;
    }
    let best = descriptors.iter().max_by_key(|d| score_descriptor(d))?;
    let score = score_descriptor(best);
    if score >= MIN_AUTO_CONNECT_SCORE {
        return Some(best.port_name.clone());
    }
    None
}

/// Opens a Meshtastic-compatible serial stream at 115200 baud with Meshtastic default DTR/RTS.
pub fn open_meshtastic_serial(
    port_name: impl Into<String>,
) -> Result<StreamHandle<SerialStream>, String> {
    let port_name = port_name.into();
    meshtastic::utils::stream::build_serial_stream(port_name, None, None, None)
        .map_err(|e| e.to_string())
}

/// Holds optional user-selected port and resolves which port to open for auto-connect.
#[derive(Debug, Default)]
pub struct MeshRadioSerialLink {
    preferred_port_name: Option<String>,
}

impl MeshRadioSerialLink {
    pub fn new(preferred_port_name: Option<String>) -> Self {
        Self {
            preferred_port_name,
        }
    }

    /// Used when a UI or config supplies an explicit serial device path.
    #[allow(dead_code)]
    pub fn set_preferred_port(&mut self, port: Option<String>) {
        self.preferred_port_name = port;
    }

    #[allow(dead_code)]
    pub fn preferred_port(&self) -> Option<&str> {
        self.preferred_port_name.as_deref()
    }

    /// Resolves the port name to open: explicit preference if set and non-empty, otherwise best USB candidate.
    pub fn resolve_port_to_open(&self) -> Result<String, String> {
        if let Some(ref p) = self.preferred_port_name {
            let trimmed = p.trim();
            if trimmed.is_empty() {
                return Err("Preferred port name is empty".to_string());
            }
            return Ok(trimmed.to_string());
        }
        let list = list_mesh_serial_ports()?;
        select_best_meshtastic_port(&list).ok_or_else(|| {
            "No suitable USB serial port found. Connect the radio and grant serial access if prompted."
                .to_string()
        })
    }

    /// Opens a new serial stream using the resolved port (see `resolve_port_to_open`).
    pub fn open_stream(&self) -> Result<StreamHandle<SerialStream>, String> {
        let port = self.resolve_port_to_open()?;
        open_meshtastic_serial(port)
    }
}
