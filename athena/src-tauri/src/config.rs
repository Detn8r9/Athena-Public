use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const CONFIG_FILENAME: &str = "athena_config.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub watch_folder: Option<String>,
    #[serde(default)]
    pub model_override: Option<String>,
    /// Max number of previous messages to send as context (user + assistant). Default 20.
    #[serde(default)]
    pub memory_messages: Option<u32>,
    /// When true, the AI can use the user's location for place-based advice (e.g. types of nearby locations).
    #[serde(default)]
    pub feature_map_enabled: Option<bool>,
    /// User's current location (address or area), used when feature_map_enabled is true.
    #[serde(default)]
    pub user_location_address: Option<String>,
    /// User-provided API key for licensed satellite tile downloads (stored locally in app data config).
    #[serde(default)]
    pub satellite_api_key: Option<String>,
    /// Enabled MBTiles raster layers for the main map view (file names including `.mbtiles`).
    #[serde(default)]
    pub enabled_main_map_raster_archive_file_names: Vec<String>,
    /// Currently selected visual theme / persona preset (e.g. "default", "jarvis", "fallout", or a custom id).
    #[serde(default)]
    pub theme_id: Option<String>,
    /// Optional explicit personality id; if None, the active theme determines personality.
    #[serde(default)]
    pub personality_id: Option<String>,
    /// User-defined custom themes persisted in config.
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
    /// When true, show "From your library:" source names under assistant messages. Default true.
    #[serde(default)]
    pub show_sources_in_chat: Option<bool>,
    /// When true, inbound Meshtastic text triggers Ollama and replies by DM to the sender. Default true.
    #[serde(default)]
    pub mesh_ai_auto_reply: Option<bool>,
}

/// A user-defined theme that can override colors, fonts, and personality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    /// Base visual preset this theme started from (e.g. "default", "jarvis", "fallout").
    pub base: String,
    /// Hex colors for background / primary / text / accent.
    pub bg_color: Option<String>,
    pub primary_color: Option<String>,
    pub text_color: Option<String>,
    pub accent_color: Option<String>,
    /// Optional font family tokens understood by the frontend (e.g. "system", "mono-tech", "crt").
    pub font_family: Option<String>,
    /// Optional explicit personality id for this theme.
    pub personality_id: Option<String>,
    /// Horizontal CRT-style scanlines overlay (saved custom themes only).
    #[serde(default)]
    pub scanlines_enabled: Option<bool>,
    /// Scanline opacity 0.0–1.0.
    #[serde(default)]
    pub scanlines_opacity: Option<f64>,
    /// Dark edge vignette.
    #[serde(default)]
    pub vignette_enabled: Option<bool>,
    /// Film grain / noise intensity 0.0–1.0.
    #[serde(default)]
    pub film_grain: Option<f64>,
    /// Sharp square corners (terminal-like) instead of rounded UI.
    #[serde(default)]
    pub sharp_corners: Option<bool>,
}

pub fn config_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join(CONFIG_FILENAME)
}

pub fn load_config(app_data_dir: &Path) -> AppConfig {
    let path = config_path(app_data_dir);
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<AppConfig>(&s) {
                Ok(c) => return c,
                Err(_) => {}
            },
            Err(_) => {}
        }
    }
    AppConfig::default()
}

pub fn save_config(app_data_dir: &Path, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app_data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}
