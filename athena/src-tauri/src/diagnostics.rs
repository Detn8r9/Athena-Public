use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

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
    for name in ["pmtiles", "tesseract", "pdftoppm", "ollama"] {
        let mut present = false;
        let mut executable = false;
        let resolved_path: Option<String> = None;
        let mut error: Option<String> = None;

        match app.shell().sidecar(name) {
            Ok(cmd) => {
                present = true;
                // Probe executability by attempting to spawn a harmless command.
                let args: &[&str] = match name {
                    "pmtiles" => &["--version"],
                    "tesseract" => &["--version"],
                    "pdftoppm" => &["-h"],
                    "ollama" => &["--version"],
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
            name: name.to_string(),
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

