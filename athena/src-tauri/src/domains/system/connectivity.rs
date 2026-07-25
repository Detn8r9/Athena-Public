//! NOMAD dual-mode connectivity: auto / online / offline.

use serde::Serialize;

use crate::config::AppConfig;
use crate::openai_chat::is_allowed_openai_base_url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectivityMode {
    Auto,
    Online,
    Offline,
}

impl ConnectivityMode {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("online") => Self::Online,
            Some("offline") => Self::Offline,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Online => "online",
            Self::Offline => "offline",
        }
    }
}

/// Lightweight reachability probe (no HTTP body required).
pub fn network_reachable() -> bool {
    // Prefer a quick TCP-style check via UDP connect (selects a route without sending).
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("1.1.1.1:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if !addr.ip().is_loopback() && !addr.ip().is_unspecified() {
                    return true;
                }
            }
        }
    }
    // Fallback: try a short HTTP HEAD to a well-known host.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(2))
        .timeout_read(std::time::Duration::from_secs(2))
        .build();
    agent
        .head("https://connectivitycheck.gstatic.com/generate_204")
        .call()
        .is_ok()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityStatus {
    pub mode: String,
    pub network_reachable: bool,
    /// Effective: internet downloads / remote LLM allowed.
    pub internet_allowed: bool,
    pub label: String,
}

pub fn status_for_config(config: &AppConfig) -> ConnectivityStatus {
    let mode = ConnectivityMode::parse(config.connectivity_mode.as_deref());
    let reachable = match mode {
        ConnectivityMode::Offline => false,
        ConnectivityMode::Online => true,
        ConnectivityMode::Auto => network_reachable(),
    };
    let internet_allowed = match mode {
        ConnectivityMode::Offline => false,
        ConnectivityMode::Online => true,
        ConnectivityMode::Auto => reachable,
    };
    let label = if internet_allowed {
        "Online".to_string()
    } else {
        "Offline".to_string()
    };
    ConnectivityStatus {
        mode: mode.as_str().to_string(),
        network_reachable: reachable,
        internet_allowed,
        label,
    }
}

pub fn internet_allowed(config: &AppConfig) -> bool {
    status_for_config(config).internet_allowed
}

/// True when the configured OpenAI-compatible base is loopback (safe offline).
pub fn openai_base_is_local(config: &AppConfig) -> bool {
    let Some(base) = config
        .openai_compatible_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return true;
    };
    is_allowed_openai_base_url(base)
}

/// Block remote/cloud LLM when offline. Local Ollama and localhost OpenAI-compatible stay allowed.
pub fn ensure_cloud_llm_allowed(config: &AppConfig) -> Result<(), String> {
    if internet_allowed(config) {
        return Ok(());
    }
    if crate::inference::is_openai_compatible_backend(config) && !openai_base_is_local(config) {
        return Err(
            "Connectivity is Offline. Switch to a local Ollama/GGUF backend, or set Connectivity to Online/Auto."
                .to_string(),
        );
    }
    // Hive Mind seed that is not loopback: still allow local ollama; remote seed used only when online.
    if let Some(url) = config.hive_mind_seed_url.as_deref().map(str::trim).filter(|s| !s.is_empty())
    {
        let lower = url.to_ascii_lowercase();
        let local = lower.contains("127.0.0.1")
            || lower.contains("localhost")
            || lower.contains(".local");
        if !local && crate::inference::is_openai_compatible_backend(config) {
            return Err(
                "Connectivity is Offline — remote Hive Mind / cloud inference is blocked.".to_string(),
            );
        }
    }
    Ok(())
}

pub fn ensure_marketplace_download_allowed(config: &AppConfig) -> Result<(), String> {
    if internet_allowed(config) {
        return Ok(());
    }
    Err(
        "Marketplace downloads require Online connectivity (or Auto with a working network). Switch Connectivity in Settings → AI."
            .to_string(),
    )
}
