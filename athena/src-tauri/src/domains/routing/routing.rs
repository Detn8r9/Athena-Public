//! Lightweight Docker bridge for starting the Valhalla engine from the UI.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::Manager;
use tauri::State;

use crate::AppState;

fn run_command_with_timeout(mut child: std::process::Child, timeout_ms: u64) -> Result<std::process::Output, String> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let status = child
                    .wait()
                    .map_err(|e| format!("wait after try_wait: {e}"))?;
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = std::io::Read::read_to_end(&mut s, &mut stdout);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = std::io::Read::read_to_end(&mut s, &mut stderr);
                }
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("timeout".to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}

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
    Err("docker/valhalla/docker-compose.valhalla.yml not found.".to_string())
}

fn looks_like_docker_daemon_offline(s: &str) -> bool {
    let x = s.to_lowercase();
    x.contains("cannot connect to the docker daemon")
        || x.contains("is the docker daemon running")
        || x.contains("error during connect")
        || x.contains("docker daemon")
        || x.contains("connection refused")
}

/// Start the Valhalla Docker engine using the existing project compose setup.
///
/// This intentionally picks an available routing region automatically (first region with a tiles directory)
/// and mounts its parent folder as `VALHALLA_CUSTOM_FILES` so `/custom_files/valhalla_tiles` exists.
#[tauri::command]
pub async fn start_valhalla_engine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    active_region_path: Option<String>,
) -> Result<String, String> {
    let compose = compose_file_path(&app)?;

    let custom_files_dir: PathBuf = if let Some(p) = active_region_path
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        let dir = PathBuf::from(&p);
        if !dir.is_dir() {
            return Err("Active region path is not a directory.".to_string());
        }
        dir
    } else {
        eprintln!("WARN: start_valhalla_engine called without active_region_path; picking first available region.");
        let region = {
            let dbh = state.db.lock().map_err(|e| e.to_string())?;
            let list = dbh.list_routing_regions()?;
            let mut pick: Option<crate::db::RoutingRegionRow> = None;
            for r in list {
                if r.tiles_path.trim().is_empty() {
                    continue;
                }
                let tiles = Path::new(r.tiles_path.trim());
                if tiles.is_dir() {
                    pick = Some(r);
                    break;
                }
            }
            pick.ok_or_else(|| "No routing region found. Download routing data first.".to_string())?
        };

        let tiles_dir = PathBuf::from(region.tiles_path.trim());
        tiles_dir
            .parent()
            .ok_or_else(|| "Invalid routing region tiles path.".to_string())?
            .to_path_buf()
    };

    let run = tokio::task::spawn_blocking(move || {
        Command::new("docker")
            .arg("compose")
            .arg("-f")
            .arg(&compose)
            .arg("up")
            .arg("-d")
            .env("VALHALLA_CUSTOM_FILES", custom_files_dir)
            .env("VALHALLA_PORT", "8002")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    })
    .await
    .map_err(|e| e.to_string())?;

    let out = match run {
        Ok(o) => o,
        Err(e) => return Err(e.to_string()),
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let merged = format!("{stdout}\n{stderr}");
        if looks_like_docker_daemon_offline(&merged) {
            return Err("Docker daemon is not running. Please launch Docker Desktop.".to_string());
        }
        return Err(format!("Could not start Valhalla engine: {merged}"));
    }

    Ok("Valhalla engine start requested. Waiting for local HTTP on port 8002…".to_string())
}

#[tauri::command]
pub async fn stop_valhalla_engine(app: tauri::AppHandle) -> Result<String, String> {
    let compose = compose_file_path(&app)?;
    let run = tokio::task::spawn_blocking(move || {
        let child = Command::new("docker")
            .arg("compose")
            .arg("-f")
            .arg(&compose)
            .arg("down")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        run_command_with_timeout(child, 10_000).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    })
    .await
    .map_err(|e| e.to_string())?;

    let out = match run {
        Ok(o) => o,
        Err(e) => return Err(e.to_string()),
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let merged = format!("{stdout}\n{stderr}");
        if looks_like_docker_daemon_offline(&merged) {
            return Err("Docker daemon is not running. Please launch Docker Desktop.".to_string());
        }
        return Err(format!("Could not stop Valhalla engine: {merged}"));
    }

    Ok("Valhalla engine stopped.".to_string())
}

