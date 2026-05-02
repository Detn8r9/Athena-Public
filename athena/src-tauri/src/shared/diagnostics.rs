use crate::config::load_config;
use crate::ollama::is_ollama_running;
use serde::Serialize;
use std::collections::BTreeMap;
use std::env::consts::{ARCH, OS};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

const ERROR_LOG_FILENAME: &str = "athena_error_log.txt";
const ERROR_LOG_EXPORT_MAX_BYTES: usize = 24 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFailure {
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarBinaryStatus {
    pub name: String,
    pub resolved_path: Option<String>,
    pub executable: bool,
    pub present: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResilienceStatus {
    pub maintenance_mode: bool,
    pub failures: Vec<DiagnosticFailure>,
    pub sidecars: Vec<SidecarBinaryStatus>,
    pub last_heartbeat_unix_ms: Option<u64>,
    pub heartbeat_components: BTreeMap<String, String>,
}

pub struct ResilienceState(pub Arc<Mutex<ResilienceStatus>>);

pub const RESILIENCE_STATUS_EVENT: &str = "resilience_status";

impl Default for ResilienceState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(ResilienceStatus {
            maintenance_mode: false,
            failures: vec![],
            sidecars: vec![],
            last_heartbeat_unix_ms: None,
            heartbeat_components: BTreeMap::new(),
        })))
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn run_startup_diagnostics(app: &AppHandle) -> ResilienceStatus {
    let mut failures: Vec<DiagnosticFailure> = Vec::new();
    let mut sidecars: Vec<SidecarBinaryStatus> = Vec::new();

    // These are mission-critical for field operation workflows.
    // `ollama` is optional in some deployments, but we still report it here so Maintenance Mode can
    // explain exactly what's missing.
    let sidecar_names: &[&str] = if cfg!(not(mobile)) {
        &["pmtiles", "tesseract", "pdftoppm", "ollama", "kiwix-serve"]
    } else {
        &["pmtiles", "tesseract", "pdftoppm", "ollama"]
    };
    for name in sidecar_names {
        let mut present = false;
        let mut executable = false;
        let resolved_path: Option<String> = None;
        let mut error: Option<String> = None;

        match app.shell().sidecar(*name) {
            Ok(cmd) => {
                present = true;
                // Probe executability by attempting to spawn a harmless command.
                let args: &[&str] = match *name {
                    "pmtiles" => &["--version"],
                    "tesseract" => &["--version"],
                    "pdftoppm" => &["-h"],
                    "ollama" => &["--version"],
                    "kiwix-serve" => &["--version"],
                    _ => &["--version"],
                };
                match cmd.args(args).spawn() {
                    Ok((_rx, child)) => {
                        executable = true;
                        let _ = child.kill();
                    }
                    Err(e) => {
                        error = Some(format!("Failed to spawn sidecar `{name}`: {e}"));
                    }
                }
            }
            Err(e) => {
                error = Some(format!("Failed to resolve sidecar `{name}`: {e}"));
            }
        }

        if !present {
            failures.push(DiagnosticFailure {
                key: format!("sidecar_missing:{name}"),
                message: format!("Required sidecar binary is missing: {name}"),
            });
        } else if !executable {
            failures.push(DiagnosticFailure {
                key: format!("sidecar_not_executable:{name}"),
                message: format!("Sidecar binary is not executable: {name}"),
            });
        }

        if let Some(ref msg) = error {
            failures.push(DiagnosticFailure {
                key: format!("sidecar_error:{name}"),
                message: msg.clone(),
            });
        }

        sidecars.push(SidecarBinaryStatus {
            name: (*name).to_string(),
            resolved_path,
            executable,
            present,
            error,
        });
    }

    ResilienceStatus {
        maintenance_mode: !failures.is_empty(),
        failures,
        sidecars,
        last_heartbeat_unix_ms: None,
        heartbeat_components: BTreeMap::new(),
    }
}

pub fn set_status(app: &AppHandle, next: ResilienceStatus) {
    if let Some(state) = app.try_state::<ResilienceState>() {
        if let Ok(mut guard) = state.0.lock() {
            *guard = next.clone();
        }
    }
    let _ = app.emit(RESILIENCE_STATUS_EVENT, next);
}

pub fn update_heartbeat(app: &AppHandle, components: BTreeMap<String, String>) {
    if let Some(state) = app.try_state::<ResilienceState>() {
        if let Ok(mut guard) = state.0.lock() {
            guard.last_heartbeat_unix_ms = Some(now_unix_ms());
            guard.heartbeat_components = components.clone();
            // Heartbeat should not clear maintenance mode; only explicit re-run of diagnostics should.
        }
        let _ = app.emit(
            RESILIENCE_STATUS_EVENT,
            state.0.lock().map(|g| g.clone()).unwrap_or_else(|_| ResilienceStatus {
                maintenance_mode: true,
                failures: vec![DiagnosticFailure {
                    key: "resilience_state_poisoned".to_string(),
                    message: "Resilience state lock poisoned.".to_string(),
                }],
                sidecars: vec![],
                last_heartbeat_unix_ms: Some(now_unix_ms()),
                heartbeat_components: components,
            }),
        );
    }
}

#[tauri::command]
pub fn get_resilience_status(app: AppHandle) -> Result<ResilienceStatus, String> {
    if let Some(state) = app.try_state::<ResilienceState>() {
        let s = state.0.lock().map_err(|e| e.to_string())?;
        Ok(s.clone())
    } else {
        Ok(ResilienceStatus {
            maintenance_mode: true,
            failures: vec![DiagnosticFailure {
                key: "resilience_state_missing".to_string(),
                message: "Resilience state not initialized.".to_string(),
            }],
            sidecars: vec![],
            last_heartbeat_unix_ms: None,
            heartbeat_components: BTreeMap::new(),
        })
    }
}

fn diagnostics_json_string(app: &AppHandle) -> Result<String, String> {
    let pkg = app.package_info();
    let app_data_dir = crate::portable::resolved_app_data_dir(app)?;
    let config = load_config(&app_data_dir);
    let openai_compatible_api_key_set = config
        .openai_compatible_api_key
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let log_path = app_data_dir.join(ERROR_LOG_FILENAME);
    let mut error_log_excerpt = std::fs::read_to_string(&log_path).unwrap_or_default();
    if error_log_excerpt.len() > ERROR_LOG_EXPORT_MAX_BYTES {
        let start = error_log_excerpt.len().saturating_sub(ERROR_LOG_EXPORT_MAX_BYTES);
        error_log_excerpt = error_log_excerpt[start..].to_string();
    }

    let resilience = get_resilience_status(app.clone())?;
    let ollama_running = is_ollama_running();

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DiagnosticsExport {
        app_version: String,
        app_name: String,
        os: &'static str,
        arch: &'static str,
        app_data_dir: String,
        error_log_excerpt: String,
        openai_compatible_api_key_set: bool,
        ollama_running: bool,
        resilience: ResilienceStatus,
    }

    let out = DiagnosticsExport {
        app_version: pkg.version.to_string(),
        app_name: pkg.name.to_string(),
        os: OS,
        arch: ARCH,
        app_data_dir: app_data_dir.to_string_lossy().into_owned(),
        error_log_excerpt,
        openai_compatible_api_key_set,
        ollama_running,
        resilience,
    };

    serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
}

/// Redacted support bundle (no API key material). Writes UTF-8 JSON to `dest_path`.
#[tauri::command]
pub fn export_diagnostics_json(app: AppHandle, dest_path: String) -> Result<(), String> {
    let s = diagnostics_json_string(&app)?;
    let dest = std::path::PathBuf::from(dest_path.trim());
    if dest.as_os_str().is_empty() {
        return Err("Destination path is empty.".to_string());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dest, s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_app_data_dir_path(app: AppHandle) -> Result<String, String> {
    let p = crate::portable::resolved_app_data_dir(&app)?;
    Ok(p.to_string_lossy().into_owned())
}

