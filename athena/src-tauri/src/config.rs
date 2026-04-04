use serde::{Deserialize, Serialize};

/// Saved pin for Library Installer regional Protomaps downloads (US).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryInstallerLocation {
    pub query: String,
    pub display_name: String,
    pub lat: f64,
    pub lon: f64,
    pub state_code: String,
}
use std::fs;
use std::path::{Path, PathBuf};

use crate::user_profile;

const CONFIG_FILENAME: &str = "athena_config.json";

/// Resolved ONNX / chat model directories under the host app data folder (`models/...`).
#[derive(Debug, Clone)]
pub struct ModelDirectoryPaths {
    pub models_root: PathBuf,
    pub chat_models: PathBuf,
    pub embeddings_root: PathBuf,
    pub embedding_all_minilm: PathBuf,
    pub rerankers_root: PathBuf,
    pub reranker_ms_marco: PathBuf,
}

/// Ensures `models/chat`, `models/embeddings/all-MiniLM-L6-v2`, and `models/rerankers/ms-marco-MiniLM-L-6-v2` exist.
/// Default embedding subfolder (matches bootstrap download target).
pub const DEFAULT_EMBEDDING_FOLDER: &str = "all-MiniLM-L6-v2";
/// Default reranker subfolder (matches bootstrap download target).
pub const DEFAULT_RERANKER_FOLDER: &str = "ms-marco-MiniLM-L-6-v2";

pub fn effective_embedding_folder(config: &AppConfig) -> String {
    config
        .active_embedding_model_folder
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_EMBEDDING_FOLDER)
        .to_string()
}

pub fn effective_reranker_folder(config: &AppConfig) -> String {
    config
        .active_reranker_model_folder
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_RERANKER_FOLDER)
        .to_string()
}

pub fn ensure_model_directories(app_data_dir: &Path) -> Result<ModelDirectoryPaths, String> {
    let models_root = app_data_dir.join("models");
    let chat_models = models_root.join("chat");
    let embeddings_root = models_root.join("embeddings");
    let embedding_all_minilm = embeddings_root.join("all-MiniLM-L6-v2");
    let rerankers_root = models_root.join("rerankers");
    let reranker_ms_marco = rerankers_root.join("ms-marco-MiniLM-L-6-v2");
    for p in [&chat_models, &embedding_all_minilm, &reranker_ms_marco] {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    Ok(ModelDirectoryPaths {
        models_root,
        chat_models,
        embeddings_root,
        embedding_all_minilm,
        rerankers_root,
        reranker_ms_marco,
    })
}

fn default_buddy_personality() -> String {
    "friendly".to_string()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
    /// When set, this `ollama` (or `ollama.exe`) binary is tried first for `ollama serve` before PATH / defaults.
    #[serde(default)]
    pub ollama_athena_cli_path: Option<String>,
    /// When true, skip downloading the official Ollama CLI into app data when nothing is reachable on port 11434.
    #[serde(default)]
    pub ollama_disable_auto_cli_download: Option<bool>,
    /// Optional schema version for future migrations (default 1).
    #[serde(default)]
    pub config_schema_version: Option<u32>,
    /// User's preferred display name (local-only).
    #[serde(default)]
    pub user_display_name: Option<String>,
    /// Birthdate `YYYY-MM-DD` for age gate and birthday message (local-only).
    #[serde(default)]
    pub user_birthdate_iso: Option<String>,
    /// When true and user is 18+, show uncensored/NSFW model catalog entries.
    #[serde(default)]
    pub allow_nsfw_unrestricted_models: Option<bool>,
    /// `ollama` (default) or `openai_compatible` for local OpenAI-style HTTP API.
    #[serde(default)]
    pub inference_backend: Option<String>,
    /// When set and installed in Ollama, use this model name instead of tier selection.
    #[serde(default)]
    pub explicit_ollama_model: Option<String>,
    /// Base URL for OpenAI-compatible server, e.g. `http://127.0.0.1:1234/v1`.
    #[serde(default)]
    pub openai_compatible_base_url: Option<String>,
    /// Optional Bearer token for local servers that require it.
    #[serde(default)]
    pub openai_compatible_api_key: Option<String>,
    /// Model id for `/v1/chat/completions` when using OpenAI-compatible backend.
    #[serde(default)]
    pub openai_compatible_model: Option<String>,
    /// Optional path to `kiwix-search` binary (GPL-3; user-installed or bundled with attribution).
    #[serde(default)]
    pub kiwix_search_binary_path: Option<String>,
    /// Optional path to `kiwix-serve` binary for local ZIM HTTP (GPL-3).
    #[serde(default)]
    pub kiwix_serve_binary_path: Option<String>,
    /// Header clock format preference (`"12"` or `"24"`). Defaults to 12-hour if unset.
    #[serde(default)]
    pub time_format: Option<String>,
    /// Calendar holiday region code (e.g. `US`, `CA`, `GB`, `AU`).
    #[serde(default)]
    pub calendar_region: Option<String>,
    /// Reminder quiet hours start (0-23 local hour). Defaults to 22.
    #[serde(default)]
    pub calendar_quiet_hours_start: Option<u8>,
    /// Reminder quiet hours end (0-23 local hour). Defaults to 7.
    #[serde(default)]
    pub calendar_quiet_hours_end: Option<u8>,

    /// Buddy personality id. Defaults to "friendly".
    #[serde(default = "default_buddy_personality")]
    pub buddy_personality: String,

    /// Directory for downloaded AI model files (GGUF).
    #[serde(default)]
    pub models_dir: Option<String>,

    /// Subfolder name under `models/embeddings` for the active ONNX embedding model.
    #[serde(default)]
    pub active_embedding_model_folder: Option<String>,
    /// Subfolder name under `models/rerankers` for the active ONNX reranker model.
    #[serde(default)]
    pub active_reranker_model_folder: Option<String>,

    /// US place pin for Library Installer map tier (Protomaps state extracts).
    #[serde(default)]
    pub library_installer_location: Option<LibraryInstallerLocation>,

    /// Display name stamped on new local logbook entries (mesh/LAN may carry their own).
    #[serde(default)]
    pub logbook_author_name: Option<String>,
    /// Occupation / role shown with logbook entries.
    #[serde(default)]
    pub logbook_author_occupation: Option<String>,
    /// When set, remote Meshtastic nodes may self-register by sending `ATHREG1:{...}` with this phrase.
    #[serde(default)]
    pub mesh_registration_admin_password: Option<String>,
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

fn normalize_age_gate(mut c: AppConfig) -> AppConfig {
    if !user_profile::is_at_least_18(c.user_birthdate_iso.as_deref()) {
        c.allow_nsfw_unrestricted_models = Some(false);
    }
    if c.buddy_personality.trim().is_empty() {
        c.buddy_personality = default_buddy_personality();
    }
    c.models_dir = c.models_dir.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    c.active_embedding_model_folder = c.active_embedding_model_folder.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    c.active_reranker_model_folder = c.active_reranker_model_folder.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    c.mesh_registration_admin_password = c.mesh_registration_admin_password.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    c
}

pub fn load_config(app_data_dir: &Path) -> AppConfig {
    let path = config_path(app_data_dir);
    if path.exists() {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(c) = serde_json::from_str::<AppConfig>(&s) {
                return normalize_age_gate(c);
            }
        }
    }
    normalize_age_gate(AppConfig::default())
}

pub fn save_config(app_data_dir: &Path, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app_data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let to_save = normalize_age_gate(config.clone());
    let s = serde_json::to_string_pretty(&to_save).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}
