//! Valhalla Docker sidecar: optional long-running `docker compose` for local HTTP on port 8002.
//! Tile building from PBF is distribution-specific; use `geocode_rebuild_from_pbf_cmd` (offline FTS) in parallel.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use tauri::Manager;
use tauri::State;

use crate::auth::{authenticated_user, AuthSession};
use crate::AppState;

#[allow(dead_code)]
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValhallaBuildLogEvent {
    pub region_id: String,
    pub line: String,
}

/// True if `docker` is on PATH and responds to `version`.
pub fn docker_cli_available() -> bool {
    Command::new("docker")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve `docker/valhalla/docker-compose.valhalla.yml` (dev tree) or `resources/valhalla/`.
fn compose_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("valhalla").join("docker-compose.valhalla.yml");
        if p.is_file() {
            return Ok(p);
        }
    }
    let mut d = std::env::current_dir().map_err(|e| e.to_string())?;
    for _ in 0..8 {
        let try_p = d.join("docker").join("valhalla").join("docker-compose.valhalla.yml");
        if try_p.is_file() {
            return Ok(try_p);
        }
        if !d.pop() {
            break;
        }
    }
    Err("docker/valhalla/docker-compose.valhalla.yml not found. Copy or bundle it with the app (see repo docker/valhalla).".to_string())
}

/// `docker compose up -d` with `VALHALLA_TILES` and `VALHALLA_PORT` for the long-running service.
#[tauri::command]
pub fn valhalla_docker_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session: State<'_, AuthSession>,
    region_id: String,
) -> Result<String, String> {
    let _u = authenticated_user(&session)?;
    let rid = region_id.trim();
    if rid.is_empty() {
        return Err("region_id required".to_string());
    }
    if !docker_cli_available() {
        return Err("Docker is not available. Install Docker Desktop; offline geocode still works without Valhalla.".to_string());
    }
    let row = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_routing_region(rid)?
            .ok_or_else(|| "Unknown routing region".to_string())?
    };
    if row.tiles_path.trim().is_empty() {
        return Err("Region has no tiles_path set.".to_string());
    }
    let tiles = Path::new(&row.tiles_path);
    if !tiles.is_dir() {
        return Err(format!(
            "tiles_path is not a directory. Build or download Valhalla tiles first: {}",
            tiles.display()
        ));
    }
    let compose = compose_file_path(&app)?;
    let out = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(&compose)
        .arg("up")
        .arg("-d")
        .env("VALHALLA_TILES", tiles)
        .env("VALHALLA_PORT", "8002")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "docker compose up failed: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok("Valhalla (Docker) start requested. HTTP expected at http://127.0.0.1:8002".to_string())
}

#[tauri::command]
pub fn valhalla_docker_stop(
    app: tauri::AppHandle,
    session: State<'_, AuthSession>,
) -> Result<String, String> {
    let _u = authenticated_user(&session)?;
    if !docker_cli_available() {
        return Err("Docker is not available.".to_string());
    }
    let compose = compose_file_path(&app)?;
    let out = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(&compose)
        .arg("down")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    Ok("docker compose down complete.".to_string())
}
