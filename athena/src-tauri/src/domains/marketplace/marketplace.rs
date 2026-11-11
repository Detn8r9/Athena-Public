//! Desktop Marketplace — Docker-gated apps and voice modules (CLI-backed).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use tauri::Emitter;

use crate::config::{load_config, AppConfig};
use crate::connectivity;
use crate::portable;

const STATE_FILENAME: &str = "marketplace_state.json";
const RESERVED_PORTS: &[u16] = &[8080, 8787, 11434, 8188, 9000, 8880, 22, 80, 443, 1420];
pub const INSTALL_PROGRESS_EVENT: &str = "athena:marketplace-install-progress";

const CATALOG_JSON: &str = include_str!("../../../resources/marketplace_catalog.json");

static INSTALL_IN_FLIGHT: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn install_inflight() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
    let mut g = INSTALL_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(HashSet::new());
    }
    g
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVolume {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub bind: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    pub description: String,
    #[serde(default)]
    pub developer: String,
    /// `app` | `module`
    pub kind: String,
    pub category: String,
    pub image: String,
    pub container_port: u16,
    pub preferred_host_port: Option<u16>,
    #[serde(default)]
    pub volumes: Vec<CatalogVolume>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub health_path: Option<String>,
    #[serde(default)]
    pub size_bytes_estimate: u64,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub featured_rank: u32,
    #[serde(default)]
    pub icon_glyph: String,
    #[serde(default)]
    pub icon_color: String,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub github_url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFile {
    version: u32,
    items: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledItem {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub image: String,
    pub container_name: String,
    pub host_port: u16,
    pub container_port: u16,
    pub status: String,
    #[serde(default)]
    pub health_path: Option<String>,
    #[serde(default)]
    pub volume_names: Vec<String>,
    /// Fingerprint of catalog image+env applied at last create/Update.
    #[serde(default)]
    pub applied_fingerprint: Option<String>,
    /// Computed for the UI (not persisted meaningfully; always refreshed).
    #[serde(default)]
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MarketplaceState {
    installed: HashMap<String, InstalledItem>,
    /// When true, outdated installed items are recreated automatically.
    #[serde(default)]
    auto_update: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerAvailability {
    pub available: bool,
    pub version: Option<String>,
    pub install_hint: String,
    pub install_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceOverview {
    pub docker: DockerAvailability,
    pub internet_allowed: bool,
    pub catalog: Vec<CatalogEntry>,
    pub installed: Vec<InstalledItem>,
    pub auto_update: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgressEvent {
    pub id: String,
    /// validating | pull | create | start | done | error
    pub phase: String,
    pub message: String,
    pub percent: Option<f32>,
    pub error: Option<String>,
}

fn emit_progress(
    app: &tauri::AppHandle,
    id: &str,
    phase: &str,
    message: &str,
    percent: Option<f32>,
    error: Option<String>,
) {
    let _ = app.emit(
        INSTALL_PROGRESS_EVENT,
        InstallProgressEvent {
            id: id.to_string(),
            phase: phase.to_string(),
            message: message.to_string(),
            percent,
            error,
        },
    );
}

fn docker_install_hint() -> (String, Option<String>) {
    #[cfg(target_os = "macos")]
    {
        (
            "Install Docker Desktop, then restart Athena and retry.".to_string(),
            Some("https://www.docker.com/products/docker-desktop/".to_string()),
        )
    }
    #[cfg(target_os = "windows")]
    {
        (
            "Install Docker Desktop for Windows, then restart Athena.".to_string(),
            Some("https://www.docker.com/products/docker-desktop/".to_string()),
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        (
            "Install Docker Engine (docker.io / docker-ce), ensure your user can run `docker`, then retry."
                .to_string(),
            Some("https://docs.docker.com/engine/install/".to_string()),
        )
    }
}

pub fn docker_available() -> DockerAvailability {
    let (hint, url) = docker_install_hint();
    let out = Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout).trim().to_string();
            DockerAvailability {
                available: true,
                version: if ver.is_empty() { None } else { Some(ver) },
                install_hint: hint,
                install_url: url,
            }
        }
        _ => DockerAvailability {
            available: false,
            version: None,
            install_hint: hint,
            install_url: url,
        },
    }
}

fn require_docker() -> Result<(), String> {
    let d = docker_available();
    if d.available {
        Ok(())
    } else {
        Err(format!("Docker is required for Marketplace. {}", d.install_hint))
    }
}

fn load_catalog() -> Result<Vec<CatalogEntry>, String> {
    let file: CatalogFile =
        serde_json::from_str(CATALOG_JSON).map_err(|e| format!("Marketplace catalog: {e}"))?;
    Ok(file.items)
}

fn state_path(app_data: &Path) -> PathBuf {
    crate::athena_paths::AthenaPaths::new(app_data).resolve_marketplace_state_path()
}

fn save_state_path(app_data: &Path) -> PathBuf {
    crate::athena_paths::AthenaPaths::new(app_data).marketplace_state_path()
}

fn load_state(app_data: &Path) -> MarketplaceState {
    let path = state_path(app_data);
    if !path.is_file() {
        return MarketplaceState::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(app_data: &Path, state: &MarketplaceState) -> Result<(), String> {
    let path = save_state_path(app_data);
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

fn docker_cmd(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run docker: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("docker {}: {}", args.join(" "), err.trim()))
    }
}

fn container_status(name: &str) -> String {
    match docker_cmd(&[
        "inspect",
        "-f",
        "{{.State.Status}}",
        name,
    ]) {
        Ok(s) if !s.is_empty() => s,
        _ => "missing".to_string(),
    }
}

fn used_host_ports() -> std::collections::HashSet<u16> {
    let mut used: std::collections::HashSet<u16> = RESERVED_PORTS.iter().copied().collect();
    if let Ok(raw) = docker_cmd(&[
        "ps",
        "-a",
        "--format",
        "{{.Ports}}",
    ]) {
        for line in raw.lines() {
            for part in line.split(',') {
                // e.g. 0.0.0.0:8096->8096/tcp
                if let Some(idx) = part.find("->") {
                    let left = &part[..idx];
                    if let Some(colon) = left.rfind(':') {
                        if let Ok(p) = left[colon + 1..].trim().parse::<u16>() {
                            used.insert(p);
                        }
                    }
                }
            }
        }
    }
    used
}

fn allocate_host_port(preferred: Option<u16>, container_port: u16) -> Result<u16, String> {
    let used = used_host_ports();
    let mut candidates = Vec::new();
    if let Some(p) = preferred {
        candidates.push(p);
    }
    candidates.push(container_port);
    let base = preferred.unwrap_or(container_port);
    candidates.extend(base.saturating_add(1)..base.saturating_add(40));
    candidates.extend(18000..18200);
    for p in candidates {
        if p > 1024 && !used.contains(&p) {
            return Ok(p);
        }
    }
    Err("Could not allocate a free host port for this Marketplace item.".to_string())
}

fn refresh_installed(app_data: &Path) -> Result<Vec<InstalledItem>, String> {
    let catalog = load_catalog().unwrap_or_else(|_| Vec::new());
    let catalog_by_id: HashMap<String, CatalogEntry> =
        catalog.into_iter().map(|e| (e.id.clone(), e)).collect();
    let mut state = load_state(app_data);
    for item in state.installed.values_mut() {
        item.status = container_status(&item.container_name);
        item.update_available = false;
        if item.status == "missing" {
            continue;
        }
        if let Some(entry) = catalog_by_id.get(&item.id) {
            item.update_available = item_needs_update(item, entry);
            // Backfill fingerprint for legacy installs that already match.
            if !item.update_available && item.applied_fingerprint.is_none() {
                item.applied_fingerprint = Some(catalog_fingerprint(entry));
            }
        }
    }
    save_state(app_data, &state)?;
    let mut list: Vec<_> = state.installed.values().cloned().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

fn catalog_fingerprint(entry: &CatalogEntry) -> String {
    let mut pairs: Vec<String> = entry
        .env
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    pairs.sort();
    format!("v1|{}|{}", entry.image, pairs.join("\n"))
}

fn container_env_map(container: &str) -> Option<HashMap<String, String>> {
    let raw = docker_cmd(&["inspect", "-f", "{{json .Config.Env}}", container]).ok()?;
    let list: Vec<String> = serde_json::from_str(&raw).ok()?;
    let mut map = HashMap::new();
    for line in list {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    Some(map)
}

fn container_image(container: &str) -> Option<String> {
    let img = docker_cmd(&["inspect", "-f", "{{.Config.Image}}", container]).ok()?;
    let trimmed = img.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn item_needs_update(item: &InstalledItem, entry: &CatalogEntry) -> bool {
    let want = catalog_fingerprint(entry);
    if let Some(applied) = item.applied_fingerprint.as_deref() {
        return applied != want;
    }
    // Legacy installs (no fingerprint): compare live container image + catalog env.
    let image = container_image(&item.container_name).unwrap_or_else(|| item.image.clone());
    if !images_compatible(&image, &entry.image) {
        return true;
    }
    let Some(env) = container_env_map(&item.container_name) else {
        return true;
    };
    for (k, v) in &entry.env {
        if env.get(k).map(|x| x.as_str()) != Some(v.as_str()) {
            return true;
        }
    }
    false
}

fn images_compatible(running: &str, catalog: &str) -> bool {
    if running == catalog {
        return true;
    }
    if let Some(rest) = running.strip_prefix(catalog) {
        return rest.is_empty() || rest.starts_with('@');
    }
    false
}

pub fn overview(app: &tauri::AppHandle) -> Result<MarketplaceOverview, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let cfg = load_config(&app_data);
    let docker = docker_available();
    let catalog = load_catalog()?;
    let state = load_state(&app_data);
    let auto_update = state.auto_update;
    let installed = if docker.available {
        refresh_installed(&app_data).unwrap_or_default()
    } else {
        load_state(&app_data).installed.values().cloned().collect()
    };
    Ok(MarketplaceOverview {
        docker,
        internet_allowed: connectivity::internet_allowed(&cfg),
        catalog,
        installed,
        auto_update,
    })
}

pub fn set_auto_update(app: &tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let mut state = load_state(&app_data);
    state.auto_update = enabled;
    save_state(&app_data, &state)?;
    if enabled {
        let _ = tick_auto_updates(app);
    }
    Ok(enabled)
}

/// Recreate any installed items whose catalog fingerprint/env is behind (when auto-update is on).
pub fn tick_auto_updates(app: &tauri::AppHandle) -> Result<usize, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let state = load_state(&app_data);
    if !state.auto_update {
        return Ok(0);
    }
    if !docker_available().available {
        return Ok(0);
    }
    let installed = refresh_installed(&app_data).unwrap_or_default();
    let mut started = 0usize;
    for item in installed {
        if !item.update_available || item.status == "missing" {
            continue;
        }
        match recreate_async(app, &item.id) {
            Ok(()) => started += 1,
            Err(e) if e.contains("already updating") || e.contains("already installing") => {}
            Err(_) => {}
        }
    }
    Ok(started)
}

fn find_entry(id: &str) -> Result<CatalogEntry, String> {
    load_catalog()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("Unknown Marketplace id: {id}"))
}

fn parse_pull_percent(line: &str) -> Option<f32> {
    // e.g. "abc123: Downloading [====> ] 12.5MB/50.0MB"
    let lower = line.to_lowercase();
    if !(lower.contains("downloading") || lower.contains("extracting")) {
        return None;
    }
    let bytes_idx = line.find("B/").or_else(|| line.find("b/"))?;
    let before = &line[..bytes_idx];
    let after = &line[bytes_idx + 2..];
    let mut cur_s = String::new();
    for ch in before.chars().rev() {
        if ch.is_ascii_digit() || ch == '.' {
            cur_s.insert(0, ch);
        } else if !cur_s.is_empty() {
            break;
        }
    }
    let mut tot_s = String::new();
    for ch in after.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            tot_s.push(ch);
        } else if !tot_s.is_empty() {
            break;
        }
    }
    let cur: f32 = cur_s.parse().ok()?;
    let tot: f32 = tot_s.parse().ok()?;
    if tot <= 0.0 {
        return None;
    }
    Some(((cur / tot) * 100.0).clamp(0.0, 99.0))
}

fn docker_pull_streaming(app: &tauri::AppHandle, id: &str, image: &str) -> Result<(), String> {
    let mut child = Command::new("docker")
        .args(["pull", image])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start docker pull: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let app_out = app.clone();
    let id_out = id.to_string();
    let out_handle = std::thread::spawn(move || {
        if let Some(out) = stdout {
            let reader = BufReader::new(out);
            for line in reader.lines().flatten() {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                let pct = parse_pull_percent(t);
                emit_progress(&app_out, &id_out, "pull", t, pct, None);
            }
        }
    });
    let app_err = app.clone();
    let id_err = id.to_string();
    let err_handle = std::thread::spawn(move || {
        if let Some(err) = stderr {
            let reader = BufReader::new(err);
            for line in reader.lines().flatten() {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                let pct = parse_pull_percent(t);
                emit_progress(&app_err, &id_err, "pull", t, pct, None);
            }
        }
    });

    let status = child
        .wait()
        .map_err(|e| format!("docker pull wait failed: {e}"))?;
    let _ = out_handle.join();
    let _ = err_handle.join();
    if status.success() {
        Ok(())
    } else {
        Err(format!("docker pull {image} failed (exit {status})"))
    }
}

fn run_install_job(app: &tauri::AppHandle, id: &str) -> Result<InstalledItem, String> {
    require_docker()?;
    let app_data = portable::resolved_app_data_dir(app)?;
    let cfg = load_config(&app_data);
    connectivity::ensure_marketplace_download_allowed(&cfg)?;

    let entry = find_entry(id)?;
    let mut state = load_state(&app_data);
    if let Some(existing) = state.installed.get(id) {
        let st = container_status(&existing.container_name);
        if st != "missing" {
            let mut item = existing.clone();
            item.status = st;
            return Ok(item);
        }
    }

    emit_progress(
        app,
        id,
        "pull",
        &format!("Downloading {} (~{})…", entry.image, format_bytes(entry.size_bytes_estimate)),
        Some(0.0),
        None,
    );
    docker_pull_streaming(app, id, &entry.image)?;

    emit_progress(app, id, "create", "Creating container…", Some(92.0), None);
    let host_port = allocate_host_port(entry.preferred_host_port, entry.container_port)?;
    let item = create_container_from_catalog(&entry, host_port, None)?;
    state.installed.insert(entry.id.clone(), item.clone());
    save_state(&app_data, &state)?;
    emit_progress(app, id, "start", "Starting…", Some(98.0), None);
    Ok(item)
}

fn image_present_locally(image: &str) -> bool {
    docker_cmd(&["image", "inspect", image]).is_ok()
}

fn remove_container(name: &str) {
    let _ = docker_cmd(&["stop", name]);
    let _ = docker_cmd(&["rm", "-f", name]);
}

/// Create (or recreate) a container from the live catalog entry.
/// Named volumes are kept/reused so Whisper weights survive an Update.
fn create_container_from_catalog(
    entry: &CatalogEntry,
    host_port: u16,
    preferred_volume_names: Option<&[String]>,
) -> Result<InstalledItem, String> {
    let container_name = format!("athena-mp-{}", entry.id);
    let mut volume_names = Vec::new();
    let mut args: Vec<String> = vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        container_name.clone(),
        "-p".into(),
        format!("{host_port}:{}", entry.container_port),
        "--restart".into(),
        "unless-stopped".into(),
    ];
    for (k, v) in &entry.env {
        args.push("-e".into());
        args.push(format!("{k}={v}"));
    }

    let mut named_idx = 0usize;
    for vol in &entry.volumes {
        if vol.bind {
            args.push("-v".into());
            args.push(format!("{}:{}:rw", vol.source, vol.target));
        } else {
            let vol_name = preferred_volume_names
                .and_then(|names| names.get(named_idx).cloned())
                .unwrap_or_else(|| format!("athena_mp_{}", vol.source));
            named_idx += 1;
            let _ = docker_cmd(&["volume", "create", &vol_name]);
            volume_names.push(vol_name.clone());
            args.push("-v".into());
            args.push(format!("{vol_name}:{}:rw", vol.target));
        }
    }
    args.push(entry.image.clone());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    docker_cmd(&arg_refs)?;

    Ok(InstalledItem {
        id: entry.id.clone(),
        name: entry.name.clone(),
        kind: entry.kind.clone(),
        image: entry.image.clone(),
        container_name,
        host_port,
        container_port: entry.container_port,
        status: "running".to_string(),
        health_path: entry.health_path.clone(),
        volume_names,
        applied_fingerprint: Some(catalog_fingerprint(entry)),
        update_available: false,
    })
}

/// Recreate an installed module/app with the current catalog image + env (keeps named volumes + port).
/// Offline-friendly when the image is already local — Stop/Start alone cannot apply catalog pins.
fn run_recreate_job(app: &tauri::AppHandle, id: &str) -> Result<InstalledItem, String> {
    require_docker()?;
    let app_data = portable::resolved_app_data_dir(app)?;
    let entry = find_entry(id)?;
    let mut state = load_state(&app_data);
    let existing = state
        .installed
        .get(id)
        .cloned()
        .ok_or_else(|| "Item is not installed. Use Get first.".to_string())?;

    emit_progress(
        app,
        id,
        "validating",
        "Applying catalog settings…",
        Some(5.0),
        None,
    );

    if !image_present_locally(&entry.image) {
        let cfg = load_config(&app_data);
        connectivity::ensure_marketplace_download_allowed(&cfg)?;
        emit_progress(
            app,
            id,
            "pull",
            &format!("Downloading {}…", entry.image),
            Some(10.0),
            None,
        );
        docker_pull_streaming(app, id, &entry.image)?;
    }

    emit_progress(app, id, "create", "Recreating container…", Some(85.0), None);
    remove_container(&existing.container_name);

    // Prefer the previous host port so Conversation URLs stay stable.
    let host_port = {
        let used = used_host_ports();
        if !used.contains(&existing.host_port) {
            existing.host_port
        } else {
            allocate_host_port(Some(existing.host_port), entry.container_port)?
        }
    };

    let item = create_container_from_catalog(
        &entry,
        host_port,
        Some(&existing.volume_names),
    )?;
    state.installed.insert(entry.id.clone(), item.clone());
    save_state(&app_data, &state)?;
    emit_progress(app, id, "start", "Starting…", Some(98.0), None);
    Ok(item)
}

fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let kb = n as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.0} KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.1} MB");
    }
    format!("{:.1} GB", mb / 1024.0)
}

/// Kick off install on a background thread. Progress arrives on `athena:marketplace-install-progress`.
/// Returns immediately so the UI stays responsive (no beach-ball during multi-GB pulls).
pub fn install_async(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("Missing Marketplace id.".to_string());
    }
    find_entry(&id)?;
    require_docker()?;
    let app_data = portable::resolved_app_data_dir(app)?;
    let cfg = load_config(&app_data);
    connectivity::ensure_marketplace_download_allowed(&cfg)?;

    {
        let mut guard = install_inflight();
        let set = guard.as_mut().expect("inflight set");
        if !set.insert(id.clone()) {
            return Err(format!("{id} is already installing."));
        }
    }

    emit_progress(
        app,
        &id,
        "validating",
        "Preparing download…",
        Some(0.0),
        None,
    );

    let app_handle = app.clone();
    std::thread::spawn(move || {
        let result = run_install_job(&app_handle, &id);
        match result {
            Ok(item) => {
                emit_progress(
                    &app_handle,
                    &id,
                    "done",
                    &format!("{} is ready on port {}.", item.name, item.host_port),
                    Some(100.0),
                    None,
                );
            }
            Err(e) => {
                emit_progress(
                    &app_handle,
                    &id,
                    "error",
                    &e,
                    None,
                    Some(e.clone()),
                );
            }
        }
        if let Ok(mut guard) = INSTALL_IN_FLIGHT.lock() {
            if let Some(set) = guard.as_mut() {
                set.remove(&id);
            }
        }
    });
    Ok(())
}

/// Recreate with current catalog env/image. Works offline when the Docker image is already local.
pub fn recreate_async(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("Missing Marketplace id.".to_string());
    }
    find_entry(&id)?;
    require_docker()?;

    {
        let mut guard = install_inflight();
        let set = guard.as_mut().expect("inflight set");
        if !set.insert(id.clone()) {
            return Err(format!("{id} is already updating."));
        }
    }

    emit_progress(
        app,
        &id,
        "validating",
        "Preparing update…",
        Some(0.0),
        None,
    );

    let app_handle = app.clone();
    std::thread::spawn(move || {
        let result = run_recreate_job(&app_handle, &id);
        match result {
            Ok(item) => {
                emit_progress(
                    &app_handle,
                    &id,
                    "done",
                    &format!("{} updated on port {}.", item.name, item.host_port),
                    Some(100.0),
                    None,
                );
            }
            Err(e) => {
                emit_progress(
                    &app_handle,
                    &id,
                    "error",
                    &e,
                    None,
                    Some(e.clone()),
                );
            }
        }
        if let Ok(mut guard) = INSTALL_IN_FLIGHT.lock() {
            if let Some(set) = guard.as_mut() {
                set.remove(&id);
            }
        }
    });
    Ok(())
}

/// Synchronous install (tests / CLI). Prefer `install_async` from the UI.
#[allow(dead_code)]
pub fn install(app: &tauri::AppHandle, id: &str) -> Result<InstalledItem, String> {
    run_install_job(app, id.trim())
}

pub fn start(app: &tauri::AppHandle, id: &str) -> Result<InstalledItem, String> {
    require_docker()?;
    let app_data = portable::resolved_app_data_dir(app)?;
    let mut state = load_state(&app_data);
    let item = state
        .installed
        .get_mut(id)
        .ok_or_else(|| "Item is not installed.".to_string())?;
    docker_cmd(&["start", &item.container_name])?;
    item.status = container_status(&item.container_name);
    let out = item.clone();
    save_state(&app_data, &state)?;
    Ok(out)
}

pub fn stop(app: &tauri::AppHandle, id: &str) -> Result<InstalledItem, String> {
    require_docker()?;
    let app_data = portable::resolved_app_data_dir(app)?;
    let mut state = load_state(&app_data);
    let item = state
        .installed
        .get_mut(id)
        .ok_or_else(|| "Item is not installed.".to_string())?;
    let _ = docker_cmd(&["stop", &item.container_name]);
    item.status = container_status(&item.container_name);
    let out = item.clone();
    save_state(&app_data, &state)?;
    Ok(out)
}

pub fn open_url_for(app: &tauri::AppHandle, id: &str) -> Result<String, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let state = load_state(&app_data);
    let item = state
        .installed
        .get(id)
        .ok_or_else(|| "Item is not installed.".to_string())?;
    let url = format!("http://127.0.0.1:{}", item.host_port);
    Ok(url)
}

pub fn health_check(app: &tauri::AppHandle, id: &str) -> Result<bool, String> {
    let app_data = portable::resolved_app_data_dir(app)?;
    let state = load_state(&app_data);
    let item = state
        .installed
        .get(id)
        .ok_or_else(|| "Item is not installed.".to_string())?;
    if container_status(&item.container_name) != "running" {
        return Ok(false);
    }
    let path = item.health_path.as_deref().unwrap_or("/");
    let url = format!("http://127.0.0.1:{}{}", item.host_port, path);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(3))
        .build();
    Ok(agent.get(&url).call().is_ok())
}

/// Named Docker volumes for installed marketplace apps (for Sister USB include).
pub fn installed_volume_names(app_data: &Path) -> Vec<String> {
    load_state(app_data)
        .installed
        .values()
        .flat_map(|i| i.volume_names.clone())
        .collect()
}

pub fn estimate_volume_bytes(app_data: &Path) -> u64 {
    let names = installed_volume_names(app_data);
    let mut total = 0u64;
    for name in names {
        if let Ok(raw) = docker_cmd(&[
            "system",
            "df",
            "-v",
            "--format",
            "{{.Name}} {{.Size}}",
        ]) {
            // Fallback: inspect mountpoint size roughly via docker volume inspect
            let _ = raw;
        }
        if let Ok(mp) = docker_cmd(&[
            "volume",
            "inspect",
            "-f",
            "{{.Mountpoint}}",
            &name,
        ]) {
            total = total.saturating_add(dir_size_approx(Path::new(mp.trim())));
        }
    }
    total
}

fn dir_size_approx(p: &Path) -> u64 {
    if !p.exists() {
        return 0;
    }
    walkdir::WalkDir::new(p)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn copy_volumes_to_usb(dest_root: &Path, app_data: &Path) -> Result<(), String> {
    require_docker()?;
    let names = installed_volume_names(app_data);
    if names.is_empty() {
        return Err(
            "Include Docker volumes was requested, but no Marketplace Docker volumes are installed."
                .to_string(),
        );
    }
    let out_dir = dest_root
        .join("data")
        .join("user")
        .join("apps")
        .join("docker_volumes");
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    for name in names {
        let dest = out_dir.join(format!("{name}.tar"));
        let dest_str = dest
            .to_str()
            .ok_or_else(|| "Invalid volume export path.".to_string())?;
        // docker run --rm -v name:/data -v dest_dir:/backup alpine tar ...
        let parent = dest
            .parent()
            .and_then(|p| p.to_str())
            .ok_or_else(|| "Invalid volume parent.".to_string())?;
        let file_name = dest
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "Invalid volume filename.".to_string())?;
        docker_cmd(&[
            "run",
            "--rm",
            "-v",
            &format!("{name}:/data:ro"),
            "-v",
            &format!("{parent}:/backup"),
            "alpine:3.20",
            "tar",
            "czf",
            &format!("/backup/{file_name}"),
            "-C",
            "/data",
            ".",
        ])?;
        let _ = dest_str;
    }
    let manifest = out_dir.join("volumes.json");
    let state = load_state(app_data);
    let raw = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    fs::write(manifest, raw).map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub fn verify_usb_volumes(dest_root: &Path) -> Result<(), String> {
    let out_dir = dest_root
        .join("data")
        .join("user")
        .join("apps")
        .join("docker_volumes");
    // Dual-read legacy USB layout
    let out_dir = if out_dir.is_dir() {
        out_dir
    } else {
        dest_root.join("docker_volumes")
    };
    if !out_dir.is_dir() {
        return Err(format!(
            "Verification failed: Docker volumes folder missing on USB: {}",
            out_dir.display()
        ));
    }
    let has_tar = walkdir::WalkDir::new(&out_dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|s| s.eq_ignore_ascii_case("tar") || s.eq_ignore_ascii_case("gz"))
                .unwrap_or(false)
                || e.file_name().to_string_lossy().ends_with(".tar")
        });
    if !has_tar {
        return Err(
            "Verification failed: Include Docker volumes was set, but no volume archives were found on the USB."
                .to_string(),
        );
    }
    Ok(())
}

#[allow(dead_code)]
fn _cfg_unused(_: &AppConfig) {}
