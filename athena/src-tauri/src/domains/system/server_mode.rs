//! Server mode: ATHENA runs on an always-on machine (for example a home server that also hosts
//! games and media) and people use it from browsers on the local network.
//!
//! Turned on by the `--server` argument or `ATHENA_SERVER=1`. In server mode:
//! - the window starts hidden; the station keeps running with nobody at the keyboard;
//! - the LAN web server starts on `ATHENA_LAN_PORT` (default 8787) at launch;
//! - the AI model unloads after `ATHENA_MODEL_IDLE_MINUTES` (default 2) of no use, so it only holds
//!   memory while someone is chatting. Outside server mode the default is Ollama's own 5 minutes.

use tauri::{AppHandle, Manager};

pub fn enabled() -> bool {
    std::env::args().any(|a| a == "--server")
        || std::env::var("ATHENA_SERVER").is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
}

pub fn port() -> u16 {
    std::env::var("ATHENA_LAN_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(crate::lan_bridge::DEFAULT_LAN_PORT)
}

/// Minutes an idle model stays loaded.
pub fn model_idle_minutes() -> u32 {
    parse_minutes(std::env::var("ATHENA_MODEL_IDLE_MINUTES").ok().as_deref(), enabled())
}

fn parse_minutes(raw: Option<&str>, server: bool) -> u32 {
    raw.and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(if server { 2 } else { 5 })
}

/// Ollama's `keep_alive` value for every request.
pub fn keep_alive() -> String {
    format!("{}m", model_idle_minutes())
}

/// Hide the window and open the web server. Called once from app setup.
pub fn start(app: &AppHandle) {
    if !enabled() {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    match crate::lan_bridge::start_lan_bridge(app.clone(), Some(port())) {
        Ok(status) => {
            let ip = status.primary_lan_ip.unwrap_or_else(|| "this-computer".to_string());
            eprintln!(
                "[athena-server] serving http://{ip}:{} (model unloads after {} idle minutes)",
                status.port,
                model_idle_minutes()
            );
        }
        Err(e) => eprintln!("[athena-server] web server did not start: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_minutes_default_by_mode() {
        assert_eq!(parse_minutes(None, true), 2);
        assert_eq!(parse_minutes(None, false), 5);
        assert_eq!(parse_minutes(Some(" 10 "), true), 10);
        assert_eq!(parse_minutes(Some("soon"), false), 5);
    }
}
