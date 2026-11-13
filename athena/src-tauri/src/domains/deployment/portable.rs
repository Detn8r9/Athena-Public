//! Portable mode: optional `portable.athena` beside the executable (strict AthenaData root), or
//! legacy `data/` beside the app — validated for reachability and writability before use.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crossbeam_channel;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use walkdir::WalkDir;

use crate::config::{effective_embedding_folder, effective_reranker_folder, load_config, save_config, AppConfig};
use crate::db::Database;
use crate::map_manager_download;
use crate::{compute_watcher_folders, kiwix_downloads_dir};
use crate::athena_paths::AthenaPaths;

/// Aegis Protocol manual export sizes (Roman field theme).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AegisManualTier {
    /// Lean station: app + launchers + slim DB + a minimal vector basemap + one small local chat model.
    Outpost,
    /// Stronghold: models + full DB + example-region proximity terrain extract + library & maps excluding satellite rasters.
    Bastion,
    /// Full installation: everything ATHENA uses locally (full payload, all maps, all documents).
    Citadel,
}

/// Left unallocated on the USB for filesystem bookkeeping / small writes during export.
pub const EXPEDITION_SECURITY_BUFFER_BYTES: u64 = 64 * 1024 * 1024;
/// Example-region proximity extract size varies; use a conservative estimate for preflight (actual run uses `pmtiles extract`).
pub const EXAMPLE_REGION_USB_EXTRACT_ESTIMATE_BYTES: u64 = 96 * 1024 * 1024;

/// No byte progress for this long → fail fast with a clear error (stuck USB / hung file).
const EXPORT_STALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const LARGE_FILE_COPY_THRESHOLD: u64 = 8 * 1024 * 1024;
const COPY_CHUNK_BYTES: usize = 4 * 1024 * 1024;

static EXPORT_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
static EXPORT_ABORT_REASON: Mutex<Option<String>> = Mutex::new(None);

/// Operator requested cancel from the UI (`cancel_aegis_export`).
pub fn request_aegis_export_cancel() {
    EXPORT_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

fn clear_aegis_export_cancel() {
    EXPORT_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    if let Ok(mut g) = EXPORT_ABORT_REASON.lock() {
        *g = None;
    }
}

fn export_abort_reason_set(message: String) {
    if let Ok(mut g) = EXPORT_ABORT_REASON.lock() {
        if g.is_none() {
            *g = Some(message);
        }
    }
}

fn export_cancel_requested() -> bool {
    EXPORT_CANCEL_REQUESTED.load(Ordering::SeqCst)
}

fn export_check_cancelled() -> Result<(), String> {
    if export_cancel_requested() {
        return Err("Export cancelled by operator.".to_string());
    }
    if let Ok(g) = EXPORT_ABORT_REASON.lock() {
        if let Some(msg) = g.as_ref() {
            return Err(msg.clone());
        }
    }
    Ok(())
}

#[inline]
fn example_region_reserved_bytes() -> u64 {
    EXAMPLE_REGION_USB_EXTRACT_ESTIMATE_BYTES.saturating_mul(110) / 100
}

/// One row in the Expedition staging preview (order matches `export_expedition_to_usb`).
// Future: add Valhalla `routing_regions` tile trees to Bastion/Citadel/Expedition selection (active region first, then
// others by size) so field kits include turn-by-turn tiles without a manual folder copy.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AegisExpeditionPlanStep {
    pub key: String,
    pub label: String,
    /// Bytes counted toward the budget for this stage (satellite row is the greedy fit, not full archive size).
    pub bytes: u64,
    pub will_copy: bool,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AegisExportPreflight {
    pub expedition_security_buffer_bytes: u64,
    pub assumed_write_mebibytes_per_sec: f64,
    /// Full application tree (Bastion / Citadel / Expedition copy) — includes all bundled `payload/*` packs.
    pub application_bytes: u64,
    /// Outpost copy omits non-launcher folders under any `payload/` directory inside the app/install tree.
    pub application_bundle_bytes_outpost: u64,
    pub bundled_launchers_bytes: u64,
    pub slim_sqlite_bytes: u64,
    pub full_sqlite_bytes: u64,
    pub models_bytes: u64,
    /// `.gguf` chat models under `models/chat` (downloaded local models for the in-app chat).
    pub chat_models_all_bytes: u64,
    /// Smallest `.gguf` in `models/chat` (used for Outpost "one small model" sizing).
    pub chat_models_smallest_bytes: u64,
    pub example_region_extract_estimate_bytes: u64,
    pub example_region_reserved_bytes: u64,
    pub offline_maps_non_satellite_bytes: u64,
    pub offline_maps_satellite_bytes: u64,
    pub offline_maps_all_bytes: u64,
    pub library_non_satellite_bytes: u64,
    pub library_all_bytes: u64,
    pub bundled_payload_children_bytes: u64,
    pub bundled_payload_root_total_bytes: u64,
    pub outpost_total_bytes: u64,
    pub bastion_total_bytes: u64,
    pub citadel_total_bytes: u64,
    pub outpost_estimated_seconds: u64,
    pub bastion_estimated_seconds: u64,
    pub citadel_estimated_seconds: u64,
    pub destination_available_bytes: Option<u64>,
    pub destination_usable_expedition_budget_bytes: Option<u64>,
    pub destination_fits_outpost: Option<bool>,
    pub destination_fits_bastion: Option<bool>,
    pub destination_fits_citadel: Option<bool>,
    /// Same as `destination_fits_outpost` (Fast Node tier).
    pub destination_fits_fast_node: Option<bool>,
    /// Same as `destination_fits_citadel` (Sister Server tier).
    pub destination_fits_sister_server: Option<bool>,
    /// `"ready"` or `"calculating"` for UI while partial preflight streams.
    pub fast_node_measure_status: String,
    pub sister_server_measure_status: String,
    pub expedition_plan: Option<Vec<AegisExpeditionPlanStep>>,
}

/// Progress payload for `athena:aegis-preflight-progress` (non-blocking preflight).
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AegisPreflightProgressEvent {
    pub done: bool,
    pub error: Option<String>,
    pub preflight: Option<AegisExportPreflight>,
}

fn chat_models_dir(app_data_dir: &Path) -> PathBuf {
    AthenaPaths::new(app_data_dir).user_chat_models_dir()
}

fn host_db_path(app_data_dir: &Path) -> PathBuf {
    AthenaPaths::new(app_data_dir).resolve_db_path()
}

/// Prefer `user/models/<kind>/<name>`, then `app/models/<kind>/<name>`, then legacy flat `models/…`.
fn resolve_model_pack_dir(paths: &AthenaPaths, kind: &str, name: &str) -> PathBuf {
    let user = paths.user_models_dir().join(kind).join(name);
    if user.is_dir() {
        return user;
    }
    let app = paths.app_models_dir().join(kind).join(name);
    if app.is_dir() {
        return app;
    }
    let legacy = paths.root.join("models").join(kind).join(name);
    if legacy.is_dir() {
        return legacy;
    }
    user
}

fn usb_athena_paths(dest_root: &Path) -> AthenaPaths {
    AthenaPaths::new(dest_root.join("data"))
}

/// Copy host `AthenaData/app` (bundled defaults, including default Ollama seed) onto the USB data root.
fn copy_host_app_tree_to_usb(app_data_dir: &Path, usb_data: &Path) -> Result<(), String> {
    let _ = crate::athena_paths::ensure_layout(usb_data);
    let src = AthenaPaths::new(app_data_dir).app_dir();
    let dst = AthenaPaths::new(usb_data).app_dir();
    if src.is_dir() {
        crate::athena_paths::copy_tree(&src, &dst)?;
    }
    Ok(())
}

fn pick_smallest_chat_model(chat_dir: &Path) -> Result<Option<(PathBuf, u64)>, String> {
    if !chat_dir.is_dir() {
        return Ok(None);
    }
    let mut best: Option<(PathBuf, u64)> = None;
    for entry in fs::read_dir(chat_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.starts_with('.') || !name.to_ascii_lowercase().ends_with(".gguf") {
            continue;
        }
        let len = entry.metadata().map_err(|e| e.to_string())?.len();
        match best {
            None => best = Some((path, len)),
            Some((_, best_len)) if len < best_len => best = Some((path, len)),
            _ => {}
        }
    }
    Ok(best)
}

fn copy_single_chat_model_to_usb(
    src: &Path,
    len: u64,
    usb_data: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
    frac: f32,
) -> Result<(), String> {
    let dst_dir = usb_data.join("user").join("models").join("chat");
    fs::create_dir_all(&dst_dir).map_err(|e| e.to_string())?;
    let name = src
        .file_name()
        .ok_or_else(|| "Chat model path missing filename.".to_string())?;
    let dst = dst_dir.join(name);
    eprintln!(
        "[athena] usb export: copying smallest chat model {} -> {} ({} B)",
        src.display(),
        dst.display(),
        len
    );
    fs::copy(src, &dst).map_err(|e| format!("Failed to copy chat model {} -> {}: {e}", src.display(), dst.display()))?;
    io.note_file_bytes(app, "Copying chat model to USB…", frac, len);
    Ok(())
}

fn copy_chat_models_dir_to_usb(
    src: &Path,
    usb_data: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
    frac_lo: f32,
    frac_hi: f32,
) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("Chat model folder not found: {}", src.display()));
    }
    let dst = usb_data.join("user").join("models").join("chat");
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    let total = tree_file_bytes(src)?.max(1);
    let mut done = 0u64;
    copy_dir_recursive_impl(
        src,
        &dst,
        Some(CopyProgress {
            app,
            io,
            phase: "Copying chat models to USB…",
            frac_lo,
            frac_hi,
            total,
            done: &mut done,
        }),
        &never_skip_file,
    )?;
    Ok(())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AegisExportProgressPayload {
    phase: String,
    fraction: f32,
    total_bytes_copied: u64,
    read_bytes_per_sec: f64,
    write_bytes_per_sec: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    eta_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AegisExportErrorPayload {
    message: String,
}

fn aegis_export_emit_full(
    app: &tauri::AppHandle,
    phase: impl Into<String>,
    fraction: f32,
    total_bytes_copied: u64,
    read_bytes_per_sec: f64,
    write_bytes_per_sec: f64,
    eta_seconds: Option<f64>,
    detail: Option<String>,
) {
    let fraction = fraction.clamp(0.0, 1.0);
    let payload = AegisExportProgressPayload {
        phase: phase.into(),
        fraction,
        total_bytes_copied,
        read_bytes_per_sec,
        write_bytes_per_sec,
        eta_seconds,
        detail,
    };
    if let Err(e) = app.emit("athena:aegis-export-progress", payload) {
        eprintln!("[athena] usb export: emit athena:aegis-export-progress failed: {e}");
    }
}

fn aegis_export_emit_error(app: &tauri::AppHandle, message: impl Into<String>) {
    let message = message.into();
    eprintln!("[athena] usb export error (emitting to UI): {message}");
    if let Err(e) = app.emit(
        "athena:aegis-export-error",
        AegisExportErrorPayload {
            message: message.clone(),
        },
    ) {
        eprintln!("[athena] usb export: emit athena:aegis-export-error failed: {e}");
    }
}

/// Tracks USB export I/O and emits throttled progress + throughput for the UI.
struct ExportIoReporter {
    total_session_bytes: u64,
    last_emit_wall: Instant,
    window_t0: Instant,
    bytes_at_window_t0: u64,
    pending_since_emit: u64,
    last_read_bps: f64,
    last_write_bps: f64,
    ema_read: f64,
    ema_write: f64,
    session_start: Instant,
    last_progress_at: Instant,
}

impl ExportIoReporter {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            total_session_bytes: 0,
            last_emit_wall: now,
            window_t0: now,
            bytes_at_window_t0: 0,
            pending_since_emit: 0,
            last_read_bps: 0.0,
            last_write_bps: 0.0,
            ema_read: 0.0,
            ema_write: 0.0,
            session_start: now,
            last_progress_at: now,
        }
    }

    fn touch_progress(&mut self) {
        self.last_progress_at = Instant::now();
    }

    fn check_stall(&mut self, phase: &str) -> Result<(), String> {
        export_check_cancelled()?;
        let idle = self.last_progress_at.elapsed();
        if idle >= EXPORT_STALL_TIMEOUT {
            let mins = idle.as_secs() / 60;
            let msg = format!(
                "Export stalled: no progress for {mins} minutes while «{phase}» (~{:.1} GB transferred). \
                 Check the USB cable/drive, cancel, and retry — or use Fast Node to skip full offline maps.",
                self.total_session_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );
            export_abort_reason_set(msg.clone());
            return Err(msg);
        }
        Ok(())
    }

    fn eta_seconds(&self, fraction: f32) -> Option<f64> {
        if fraction <= 0.02 || fraction >= 0.995 {
            return None;
        }
        let elapsed = self.session_start.elapsed().as_secs_f64();
        if elapsed < 3.0 {
            return None;
        }
        Some(elapsed * (1.0 / f64::from(fraction) - 1.0))
    }

    fn emit_payload(
        &mut self,
        app: &tauri::AppHandle,
        phase: impl Into<String>,
        fraction: f32,
        detail: Option<String>,
    ) {
        aegis_export_emit_full(
            app,
            phase,
            fraction,
            self.total_session_bytes,
            self.last_read_bps,
            self.last_write_bps,
            self.eta_seconds(fraction),
            detail,
        );
        self.last_emit_wall = Instant::now();
        self.pending_since_emit = 0;
        self.window_t0 = Instant::now();
        self.bytes_at_window_t0 = self.total_session_bytes;
    }

    fn emit_phase(&mut self, app: &tauri::AppHandle, phase: impl Into<String>, fraction: f32) {
        if self.pending_since_emit > 0 {
            self.push_window_rates();
        }
        self.touch_progress();
        self.emit_payload(app, phase, fraction, None);
    }

    fn emit_phase_detail(
        &mut self,
        app: &tauri::AppHandle,
        phase: impl Into<String>,
        fraction: f32,
        detail: impl Into<String>,
    ) {
        if self.pending_since_emit > 0 {
            self.push_window_rates();
        }
        self.emit_payload(app, phase, fraction, Some(detail.into()));
    }

    fn push_window_rates(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.window_t0).as_secs_f64().max(0.001);
        let db = self
            .total_session_bytes
            .saturating_sub(self.bytes_at_window_t0) as f64;
        let bps = db / dt;
        self.window_t0 = now;
        self.bytes_at_window_t0 = self.total_session_bytes;
        let blend = |prev: f64, v: f64, alpha: f64| {
            if prev <= 1.0 {
                v
            } else {
                prev * (1.0 - alpha) + v * alpha
            }
        };
        self.ema_read = blend(self.ema_read, bps, 0.38);
        self.ema_write = blend(self.ema_write, bps, 0.52);
        self.last_read_bps = self.ema_read;
        self.last_write_bps = self.ema_write;
    }

    fn maybe_emit_after_file(&mut self, app: &tauri::AppHandle, phase: &str, fraction: f32) {
        let _ = self.check_stall(phase);
        let now = Instant::now();
        let time_ok = now.duration_since(self.last_emit_wall) >= Duration::from_millis(140);
        let bulk_ok = self.pending_since_emit >= 256 * 1024;
        if time_ok || bulk_ok {
            self.push_window_rates();
            self.emit_payload(app, phase, fraction, None);
        }
    }

    fn note_file_bytes(&mut self, app: &tauri::AppHandle, phase: &str, fraction: f32, file_len: u64) {
        self.total_session_bytes += file_len;
        self.pending_since_emit += file_len;
        self.touch_progress();
        self.maybe_emit_after_file(app, phase, fraction);
    }

    fn note_poll_transfer(
        &mut self,
        app: &tauri::AppHandle,
        phase: &str,
        fraction: f32,
        new_bytes: u64,
        interval_secs: f64,
    ) {
        self.total_session_bytes += new_bytes;
        self.touch_progress();
        let inst = (new_bytes as f64) / interval_secs.max(0.001);
        let blend = |prev: f64, v: f64, alpha: f64| {
            if prev <= 1.0 {
                v
            } else {
                prev * (1.0 - alpha) + v * alpha
            }
        };
        self.ema_read = blend(self.ema_read, inst, 0.42);
        self.ema_write = blend(self.ema_write, inst, 0.55);
        self.last_read_bps = self.ema_read;
        self.last_write_bps = self.ema_write;
        let _ = self.check_stall(phase);
        self.emit_payload(app, phase, fraction, None);
    }
}

struct CopyProgress<'a> {
    app: &'a tauri::AppHandle,
    io: &'a mut ExportIoReporter,
    phase: &'a str,
    frac_lo: f32,
    frac_hi: f32,
    total: u64,
    done: &'a mut u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedUsbExport {
    Manual(AegisManualTier),
    Expedition { budget_bytes: u64 },
}

fn parse_usb_export_type(
    export_type: &str,
    expedition_budget_bytes: Option<u64>,
) -> Result<ParsedUsbExport, String> {
    let t = export_type.trim().to_ascii_lowercase();
    match t.as_str() {
        "outpost" | "essentials" | "essentials_data" => Ok(ParsedUsbExport::Manual(AegisManualTier::Outpost)),
        "bastion" | "recommended" => Ok(ParsedUsbExport::Manual(AegisManualTier::Bastion)),
        "citadel" | "absolute" | "complete" => Ok(ParsedUsbExport::Manual(AegisManualTier::Citadel)),
        "expedition" | "auto" => {
            let b = expedition_budget_bytes.ok_or_else(|| {
                "expedition_budget_bytes is required when export_type is \"expedition\".".to_string()
            })?;
            if b < 16 * 1024 * 1024 {
                return Err("expedition_budget_bytes is too small (minimum 16 MiB).".to_string());
            }
            Ok(ParsedUsbExport::Expedition { budget_bytes: b.max(1) })
        }
        _ => Err(
            "export_type must be \"outpost\", \"bastion\", \"citadel\", or \"expedition\".".to_string(),
        ),
    }
}

fn export_type_label_for_verify(parsed: &ParsedUsbExport) -> &'static str {
    match parsed {
        ParsedUsbExport::Manual(AegisManualTier::Outpost) => "outpost",
        ParsedUsbExport::Manual(AegisManualTier::Bastion) => "bastion",
        ParsedUsbExport::Manual(AegisManualTier::Citadel) => "citadel",
        ParsedUsbExport::Expedition { .. } => "expedition",
    }
}

/// Satellite / aerial raster archives (MapTiler jobs use `satellite_…` ids). Terrain / OSM / Protomaps extracts are kept.
pub fn is_probably_satellite_map_archive(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".mbtiles") && !lower.ends_with(".pmtiles") {
        return false;
    }
    lower.contains("satellite")
        || lower.starts_with("satellite_")
        || lower.starts_with("satellite_region_")
        || lower.contains("satellite-v2")
        || lower.contains("aerial")
}

fn sqlite_table_or_virtual_exists(conn: &Connection, name: &str) -> Result<bool, String> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1",
            params![name],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// Remove bulky library semantic index + ingested file rows from a **copy** of the station database.
/// Keeps operational tables (personnel, `local_chats`, logbook, radio logs, mesh, calendar, map pins, etc.).
fn strip_heavy_library_tables_from_usb_db(conn: &Connection) -> Result<(), String> {
    conn.execute("PRAGMA foreign_keys = ON", [])
        .map_err(|e| e.to_string())?;

    if sqlite_table_or_virtual_exists(conn, "chunk_embeddings_vec")? {
        conn.execute("DELETE FROM chunk_embeddings_vec", [])
            .map_err(|e| e.to_string())?;
    }
    if sqlite_table_or_virtual_exists(conn, "chunk_embeddings")? {
        conn.execute("DELETE FROM chunk_embeddings", [])
            .map_err(|e| e.to_string())?;
    }
    if sqlite_table_or_virtual_exists(conn, "library_chunks_fts")? {
        let _ = conn.execute("DELETE FROM library_chunks_fts", []);
    }
    if sqlite_table_or_virtual_exists(conn, "indexed_files")? {
        conn.execute("DELETE FROM indexed_files", [])
            .map_err(|e| e.to_string())?;
    }
    if sqlite_table_or_virtual_exists(conn, "rag_documents")? {
        conn.execute("DELETE FROM rag_documents", [])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_slim_index_database_to_usb(
    db: &Arc<Mutex<Database>>,
    db_dst: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    io.emit_phase(
        app,
        "Preparing data-focused database copy (library vectors & ingested files omitted)…",
        0.53,
    );
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let tmp = dir.path().join("athena_index_slim.db");
    {
        let g = db.lock().map_err(|e| e.to_string())?;
        g.backup_to_path(&tmp)?;
    }
    let conn = Connection::open(&tmp).map_err(|e| e.to_string())?;
    strip_heavy_library_tables_from_usb_db(&conn)?;
    conn.execute("VACUUM", []).map_err(|e| e.to_string())?;
    drop(conn);

    if let Some(parent) = db_dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if db_dst.exists() {
        fs::remove_file(db_dst).map_err(|e| e.to_string())?;
    }
    fs::copy(&tmp, db_dst).map_err(|e| e.to_string())?;
    let len = fs::metadata(db_dst).map_err(|e| e.to_string())?.len();
    io.note_file_bytes(
        app,
        "Copying data-focused index database…",
        0.56,
        len,
    );
    Ok(())
}

fn approx_bbox_for_radius_miles(
    center_lon: f64,
    center_lat: f64,
    radius_miles: f64,
) -> (f64, f64, f64, f64) {
    // WGS84 mean Earth radius in statute miles.
    let earth_radius_miles = 3958.7613_f64;
    let dlat = (radius_miles / earth_radius_miles).to_degrees();
    let cos_lat = center_lat.to_radians().cos().abs().max(1e-6);
    let dlon = (radius_miles / earth_radius_miles).to_degrees() / cos_lat;
    (
        (center_lon - dlon).max(-180.0),
        (center_lat - dlat).max(-90.0),
        (center_lon + dlon).min(180.0),
        (center_lat + dlat).min(90.0),
    )
}

fn run_sidecar_blocking(
    app: &tauri::AppHandle,
    name: &str,
    args: &[&str],
    timeout_secs: u64,
) -> Result<(), String> {
    let cmd = app
        .shell()
        .sidecar(name)
        .map_err(|e| format!("Failed to resolve sidecar `{name}`: {e}"))?;
    let (mut rx, _child) = cmd
        .args(args)
        .spawn()
        .map_err(|e| format!("Failed to spawn `{name}`: {e}"))?;
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(timeout_secs) {
            return Err(format!("Sidecar `{name}` timed out after {timeout_secs}s."));
        }
        match tauri::async_runtime::block_on(rx.recv()) {
            Some(CommandEvent::Terminated(payload)) => {
                if payload.code == Some(0) {
                    return Ok(());
                }
                return Err(format!(
                    "Sidecar `{name}` failed (code {:?}, signal {:?}).",
                    payload.code, payload.signal
                ));
            }
            Some(_) => continue,
            None => return Err(format!("Sidecar `{name}` ended unexpectedly.")),
        }
    }
}

fn pick_best_local_pmtiles_source(app_data_dir: &Path) -> Option<PathBuf> {
    let dir = map_manager_download::offline_maps_directory(app_data_dir);
    if !dir.is_dir() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file()
                && p.extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .eq_ignore_ascii_case("pmtiles")
            {
                candidates.push(p);
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

/// When `require_local_basemap` is true (Outpost), require a local `.pmtiles` on the host
/// instead of fetching a remote source.
fn ensure_example_region_proximity_pmtiles(
    app: &tauri::AppHandle,
    dest_root: &Path,
    app_data_dir: &Path,
    require_local_basemap: bool,
) -> Result<(), String> {
    const PROTOMAPS_SOURCE_URL: &str =
        "https://data.source.coop/protomaps/openstreetmap/tiles/v3.pmtiles";

    // Conterminous US geographic center (widely published reference point), not an operator home.
    let center_lat = 39.828175_f64;
    let center_lon = -98.579534_f64;
    let radius_miles = 25.0_f64;
    let (min_lon, min_lat, max_lon, max_lat) =
        approx_bbox_for_radius_miles(center_lon, center_lat, radius_miles);

    let out_dir = dest_root.join("data").join("user").join("maps");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out = out_dir.join("example_region_25mi.pmtiles");

    let local = pick_best_local_pmtiles_source(app_data_dir);
    let source = match local.as_ref().and_then(|p| p.to_str().map(|s| s.to_string())) {
        Some(s) => s,
        None if require_local_basemap => {
            return Err(
                "No local offline basemap (.pmtiles) found under data/user/maps (or legacy data/maps). Download a small basemap first, then retry Outpost export."
                    .to_string(),
            );
        }
        None => PROTOMAPS_SOURCE_URL.to_string(),
    };

    let bbox = format!("--bbox={min_lon:.8},{min_lat:.8},{max_lon:.8},{max_lat:.8}");
    let out_str = out
        .to_str()
        .ok_or_else(|| "PMTiles output path contains invalid characters.".to_string())?;

    run_sidecar_blocking(
        app,
        "pmtiles",
        &["extract", source.as_str(), out_str, bbox.as_str()],
        600,
    )?;
    Ok(())
}

fn dir_contains_extension(dir: &Path, ext_lc: &str) -> bool {
    let Ok(rd) = fs::read_dir(dir) else {
        return false;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file() {
            let ok = p
                .extension()
                .and_then(|x| x.to_str())
                .map(|s| s.eq_ignore_ascii_case(ext_lc))
                .unwrap_or(false);
            if ok {
                return true;
            }
        } else if p.is_dir() && dir_contains_extension(&p, ext_lc) {
            return true;
        }
    }
    false
}

fn usb_has_offline_map_artifact(usb_data: &Path) -> bool {
    let maps = usb_data.join("user").join("maps");
    dir_contains_extension(&maps, "pmtiles") || dir_contains_extension(&maps, "mbtiles")
}

fn available_space_bytes_for_destination(dest_root: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let target = dest_root.canonicalize().unwrap_or_else(|_| dest_root.to_path_buf());
    let mut best: Option<(usize, u64)> = None;
    for d in disks.list() {
        let mp = d.mount_point();
        let mp = mp.canonicalize().unwrap_or_else(|_| mp.to_path_buf());
        if target.starts_with(&mp) {
            let len = mp.as_os_str().len();
            let avail = d.available_space();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, avail));
            }
        }
    }
    best.map(|(_, a)| a)
}

fn dir_size_bytes(p: &Path) -> u64 {
    if p.is_file() {
        return std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    }
    preflight_dir_bytes_fast(p).map(|m| m.bytes).unwrap_or(0)
}

#[derive(Debug, serde::Deserialize)]
struct PayloadMetadata {
    #[serde(default)]
    priority: Option<i64>,
}

fn payload_priority_for_dir(dir: &Path) -> Option<i64> {
    let meta = dir.join("metadata.json");
    let raw = std::fs::read_to_string(meta).ok()?;
    let parsed: PayloadMetadata = serde_json::from_str(&raw).ok()?;
    parsed.priority
}

fn tier2_copy_payload_priority_fill(
    app: &tauri::AppHandle,
    dest_root: &Path,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    let payload_root = app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Could not resolve bundled payload dir: {e}"))?;
    if !payload_root.is_dir() {
        return Ok(());
    }

    let mut entries: Vec<(i64, PathBuf, u64)> = Vec::new();
    for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let src = e.path();
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("Linux") {
            continue;
        }
        if !src.is_dir() {
            continue;
        }
        let prio = payload_priority_for_dir(&src).unwrap_or(0);
        let sz = dir_size_bytes(&src);
        entries.push((prio, src, sz));
    }

    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let reserve_bytes = 500u64 * 1024u64 * 1024u64;
    let mut plan: Vec<(PathBuf, PathBuf, i64, u64)> = Vec::new();
    for (prio, src, sz) in entries {
        let avail = available_space_bytes_for_destination(dest_root).unwrap_or(0);
        if avail <= reserve_bytes {
            break;
        }
        if avail.saturating_sub(sz) <= reserve_bytes {
            continue;
        }
        let name = src
            .file_name()
            .ok_or_else(|| "payload entry missing filename".to_string())?;
        let dst = dest_root.join(name);
        plan.push((src, dst, prio, sz));
    }

    let total: u64 = plan.iter().map(|p| p.3).sum();
    let total = total.max(1);
    let mut done = 0u64;

    for (src, dst, prio, sz) in plan {
        eprintln!(
            "[athena] usb export: tier2 copy payload pack prio={prio} size={} -> {}",
            sz,
            dst.display()
        );
        if dst.exists() {
            std::fs::remove_dir_all(&dst).map_err(|e| {
                format!(
                    "Failed to remove existing destination {} before payload pack copy: {e}",
                    dst.display()
                )
            })?;
        }
        copy_dir_recursive_impl(
            &src,
            &dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Filling priority payload packs…",
                frac_lo: 0.64,
                frac_hi: 0.70,
                total,
                done: &mut done,
            }),
            &never_skip_file,
        )?;
    }

    Ok(())
}

static PORTABLE_APP_DATA_OVERRIDE: OnceLock<Option<PathBuf>> = OnceLock::new();
static PORTABLE_LAYOUT_DETECTED: OnceLock<bool> = OnceLock::new();

/// Marker beside the executable (or macOS `.app` sibling). When present, AthenaData root is
/// taken **only** from this file — the legacy `data/` directory fallback is not used.
pub const PORTABLE_ATHENA_MARKER_FILENAME: &str = "portable.athena";

fn macos_app_bundle_walking_up(exe: &Path) -> Option<PathBuf> {
    let mut cur = exe.parent()?;
    loop {
        if cur.extension().map_or(false, |e| e == "app") {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
}

/// Candidate `portable.athena` paths in priority order (mirrors legacy `data/` discovery).
fn portable_athena_marker_candidates(exe: &Path) -> Vec<PathBuf> {
    let exe = fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            if let Some(bundle_parent) = bundle.parent() {
                out.push(
                    bundle_parent.join(PORTABLE_ATHENA_MARKER_FILENAME),
                );
            }
            out.push(
                bundle
                    .join("Contents")
                    .join(PORTABLE_ATHENA_MARKER_FILENAME),
            );
        }
    }

    if let Some(exe_dir) = exe.parent() {
        out.push(exe_dir.join(PORTABLE_ATHENA_MARKER_FILENAME));
    }

    out
}

/// Read AthenaData root from `portable.athena`: first non-empty line is the path (absolute or
/// relative to the marker file's directory). If the file is empty or whitespace-only, the
/// directory containing the marker is the root.
pub fn resolve_athena_root_from_portable_athena_marker(marker: &Path) -> Result<PathBuf, String> {
    let raw = fs::read_to_string(marker).map_err(|e| {
        format!(
            "failed to read portable marker {}: {e}",
            marker.display()
        )
    })?;
    // Windows PowerShell 5 and Notepad can save a UTF-8 byte-order mark; it is not part of the path.
    let line = raw
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let base = marker.parent().ok_or_else(|| {
        format!(
            "invalid portable.athena path (no parent directory): {}",
            marker.display()
        )
    })?;
    let root = if line.is_empty() {
        base.to_path_buf()
    } else {
        let p = Path::new(line);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            base.join(p)
        }
    };
    fs::canonicalize(&root).map_err(|e| {
        format!(
            "AthenaData root from {} does not exist or is unreachable (drive disconnected?): {} — {e}",
            marker.display(),
            root.display()
        )
    })
}

/// Fail-fast checks: path exists as a directory and is writable (`.athena/` probe write).
pub fn validate_athena_data_root(root: &Path) -> Result<(), String> {
    let meta = fs::metadata(root).map_err(|e| {
        format!(
            "AthenaData root missing or unreadable (drive disconnected?): {} — {e}",
            root.display()
        )
    })?;
    if !meta.is_dir() {
        return Err(format!(
            "AthenaData root is not a directory: {}",
            root.display()
        ));
    }
    let probe_dir = root.join(".athena");
    fs::create_dir_all(&probe_dir).map_err(|e| {
        format!(
            "AthenaData root not writable (cannot create .athena): {} — {e}",
            root.display()
        )
    })?;
    let probe = probe_dir.join(".write_probe_tmp");
    let _ = fs::remove_file(&probe);
    fs::write(&probe, b"athena").map_err(|e| {
        format!(
            "AthenaData root not writable: {} — {e}",
            root.display()
        )
    })?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

fn detect_portable_data_dir_legacy(exe: &Path) -> Option<PathBuf> {
    let exe = fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());

    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            if let Some(bundle_parent) = bundle.parent() {
                let sibling = bundle_parent.join("data");
                if sibling.is_dir() {
                    return Some(sibling);
                }
            }
            let inside_contents = bundle.join("Contents").join("data");
            if inside_contents.is_dir() {
                return Some(inside_contents);
            }
        }
    }

    let exe_dir = exe.parent()?;
    let beside_exe = exe_dir.join("data");
    if beside_exe.is_dir() {
        return Some(beside_exe);
    }

    None
}

/// Optional portable app data root. If `portable.athena` exists, the root is **strictly** derived
/// from it (no `data/` fallback). Otherwise uses a `data/` directory beside the app when present.
pub fn discover_portable_root_for_exe(exe: &Path) -> Result<Option<PathBuf>, String> {
    for marker in portable_athena_marker_candidates(exe) {
        if marker.is_file() {
            let root = resolve_athena_root_from_portable_athena_marker(&marker)?;
            validate_athena_data_root(&root)?;
            eprintln!(
                "[athena] portable.athena: AthenaData root {} (marker {})",
                root.display(),
                marker.display()
            );
            return Ok(Some(root));
        }
    }

    let exe = fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    if let Some(p) = detect_portable_data_dir_legacy(&exe) {
        validate_athena_data_root(&p)?;
        eprintln!(
            "[athena] portable mode: app data directory {}",
            p.display()
        );
        return Ok(Some(p));
    }

    Ok(None)
}

fn user_override_marker_path() -> Option<PathBuf> {
    // Windows sets USERPROFILE rather than HOME.
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok()?;
    Some(PathBuf::from(home).join(".athena").join(PORTABLE_ATHENA_MARKER_FILENAME))
}

pub fn init_portable_app_data_override() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    // Allow field deployments to set a per-user override without writing into the app bundle.
    let detected = if let Some(marker) = user_override_marker_path().filter(|p| p.is_file()) {
        let root = resolve_athena_root_from_portable_athena_marker(&marker)?;
        validate_athena_data_root(&root)?;
        Some(root)
    } else {
        discover_portable_root_for_exe(&exe)?
    };
    if detected.is_none() {
        tracing::warn!("portable layout not detected; using OS app data directory");
    }
    let _ = PORTABLE_LAYOUT_DETECTED.set(detected.is_some());
    PORTABLE_APP_DATA_OVERRIDE
        .set(detected)
        .map_err(|_| "init_portable_app_data_override: already initialized".to_string())?;
    Ok(())
}

pub fn portable_layout_detected() -> bool {
    *PORTABLE_LAYOUT_DETECTED.get().unwrap_or(&false)
}

#[tauri::command]
pub fn oobe_set_athena_data_root(root_path: String) -> Result<(), String> {
    let p = PathBuf::from(root_path.trim());
    if p.as_os_str().is_empty() {
        return Err("path is empty".to_string());
    }
    validate_athena_data_root(&p)?;
    let marker = user_override_marker_path().ok_or_else(|| "HOME is not set.".to_string())?;
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&marker, format!("{}\n", p.display())).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn resolved_app_data_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    if let Some(Some(p)) = PORTABLE_APP_DATA_OVERRIDE.get() {
        return Ok(p.clone());
    }
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Resolve a configured library / watch root: relative paths are anchored at `app_data_dir`
/// (including portable `data` beside the executable) so the same config works across machines.
pub fn resolve_library_watch_root_path(raw: &str, app_data_dir: &Path) -> PathBuf {
    let t = raw.trim();
    if t.is_empty() {
        return PathBuf::from(t);
    }
    let p = Path::new(t);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        app_data_dir.join(p)
    }
}

/// Recursive copy with optional byte-accurate progress + throughput.
/// `skip_file` returns **true** to omit a file (directories are still created as needed).
fn copy_dir_recursive_impl(
    src: &Path,
    dst: &Path,
    mut progress: Option<CopyProgress<'_>>,
    skip_file: &dyn Fn(&Path) -> bool,
) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("Missing source: {}", src.display()));
    }
    for entry in WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(src).map_err(|e| e.to_string())?;
        let target = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else if path.is_file() {
            if skip_file(path) {
                continue;
            }
            export_check_cancelled()?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
            fs::copy(path, &target).map_err(|e| {
                format!(
                    "copy {} -> {}: {}",
                    path.display(),
                    target.display(),
                    e
                )
            })?;
            if let Some(p) = progress.as_mut() {
                *p.done = p.done.saturating_add(len);
                let frac = if p.total > 0 {
                    p.frac_lo
                        + (p.frac_hi - p.frac_lo)
                            * ((*p.done as f32) / (p.total as f32)).min(1.0)
                } else {
                    p.frac_lo
                };
                p.io.note_file_bytes(p.app, p.phase, frac, len);
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct CopyJob {
    src: PathBuf,
    dst: PathBuf,
    len: u64,
}

/// Chunked copy with cancel checks — large archives report incremental progress instead of appearing frozen.
fn copy_file_resilient(
    src: &Path,
    dst: &Path,
    expected_len: u64,
    on_chunk: &dyn Fn(u64) -> Result<(), String>,
) -> Result<u64, String> {
    export_check_cancelled()?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut reader = fs::File::open(src).map_err(|e| format!("open {}: {e}", src.display()))?;
    let mut writer = fs::File::create(dst).map_err(|e| format!("create {}: {e}", dst.display()))?;
    let mut buf = vec![0u8; COPY_CHUNK_BYTES];
    let mut copied: u64 = 0;
    loop {
        export_check_cancelled()?;
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buf[..n])
            .map_err(|e| format!("write {}: {e}", dst.display()))?;
        copied = copied.saturating_add(n as u64);
        on_chunk(n as u64)?;
    }
    writer.flush().map_err(|e| e.to_string())?;
    if expected_len > 0 && copied != expected_len {
        return Err(format!(
            "Short copy {} -> {} (expected {expected_len} B, wrote {copied} B)",
            src.display(),
            dst.display()
        ));
    }
    Ok(copied)
}

/// Parallel recursive copy for large trees (maps/wiki/routing). Builds a manifest, then copies files using a small worker pool.
/// Progress is throttled: updates at most every ~500ms or every 100 files (whichever comes first).
fn copy_dir_recursive_parallel_impl(
    src: &Path,
    dst: &Path,
    mut progress: Option<CopyProgress<'_>>,
    skip_file: &dyn Fn(&Path) -> bool,
) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("Missing source: {}", src.display()));
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;

    // Phase 1: manifest (index files + total bytes so progress stays accurate for huge archives)
    let mut jobs: Vec<CopyJob> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut scan_ix: u64 = 0;
    let scan_started = Instant::now();
    for entry in WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        export_check_cancelled()?;
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(src).map_err(|e| e.to_string())?;
        let target = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if skip_file(path) {
            continue;
        }
        scan_ix = scan_ix.saturating_add(1);
        if scan_ix % 5_000 == 0 {
            if let Some(p) = progress.as_mut() {
                p.io.emit_phase_detail(
                    p.app,
                    p.phase,
                    p.frac_lo,
                    format!("Indexing {scan_ix} files ({:.1} GB)…", total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)),
                );
            }
        }
        if scan_started.elapsed() >= Duration::from_secs(2) && scan_ix % 500 == 0 {
            if let Some(p) = progress.as_mut() {
                let _ = p.io.check_stall(p.phase);
            }
        }
        let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
        total_bytes = total_bytes.saturating_add(len);
        jobs.push(CopyJob {
            src: path.to_path_buf(),
            dst: target,
            len,
        });
    }
    if jobs.is_empty() {
        return Ok(());
    }
    if let Some(p) = progress.as_mut() {
        p.total = total_bytes.max(1);
        *p.done = 0;
        p.io.emit_phase_detail(
            p.app,
            p.phase,
            p.frac_lo,
            format!("Copying {} files ({:.1} GB)…", jobs.len(), total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)),
        );
    }

    // Ensure each parent directory exists once (huge map trees: avoid millions of redundant create_dir_all calls).
    let mut parents: HashSet<PathBuf> = HashSet::new();
    for j in &jobs {
        if let Some(p) = j.dst.parent() {
            parents.insert(p.to_path_buf());
        }
    }
    for p in parents {
        fs::create_dir_all(&p).map_err(|e| {
            format!(
                "Failed to create parent directory {}: {}",
                p.display(),
                e
            )
        })?;
    }

    let expected_copy_results = jobs.len() as u64;
    let progress_total_bytes = total_bytes.max(1);
    let n_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(4, 8);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n_workers)
        .thread_name(|i| format!("athena-export-copy-{i}"))
        .build()
        .map_err(|e| e.to_string())?;

    let (tx_res, rx_res) = crossbeam_channel::unbounded::<Result<u64, String>>();
    let tx_shared = Arc::new(tx_res);
    let copy_errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let copy_errors_recv = Arc::clone(&copy_errors);
    let yield_counter = AtomicU64::new(0);

    std::thread::scope(|sp| {
        sp.spawn(|| {
            let mut bytes_since_emit: u64 = 0;
            let mut files_done: u64 = 0;
            let mut last_emit = Instant::now();
            while let Ok(msg) = rx_res.recv() {
                match msg {
                    Ok(len) => {
                        if len > 0 {
                            bytes_since_emit = bytes_since_emit.saturating_add(len);
                        }
                        if len == 0 {
                            files_done = files_done.saturating_add(1);
                        }
                    }
                    Err(e) => {
                        if let Ok(mut g) = copy_errors_recv.lock() {
                            if g.len() < 200 {
                                g.push(e);
                            }
                        }
                        files_done = files_done.saturating_add(1);
                    }
                }
                if let Some(p) = progress.as_mut() {
                    if bytes_since_emit > 0 {
                        *p.done = p.done.saturating_add(bytes_since_emit);
                    }
                    let now = Instant::now();
                    let emit = now.duration_since(last_emit) >= Duration::from_millis(400);
                    if emit {
                        last_emit = now;
                        let frac = if progress_total_bytes > 0 {
                            p.frac_lo
                                + (p.frac_hi - p.frac_lo)
                                    * ((*p.done as f32) / (progress_total_bytes as f32)).min(1.0)
                        } else {
                            p.frac_lo
                        };
                        let _ = p.io.check_stall(p.phase);
                        if bytes_since_emit > 0 {
                            p.io.note_poll_transfer(p.app, p.phase, frac, bytes_since_emit, 0.4);
                            bytes_since_emit = 0;
                        } else {
                            p.io.emit_payload(p.app, p.phase, frac, None);
                        }
                    }
                }
            }
        });

        pool.install(|| {
            jobs.par_iter().for_each(|job| {
                let tx_chunk = Arc::clone(&tx_shared);
                let res: Result<(), String> = (|| {
                    export_check_cancelled()?;
                    if job.len >= LARGE_FILE_COPY_THRESHOLD {
                        copy_file_resilient(&job.src, &job.dst, job.len, &|n| {
                            if n > 0 {
                                let _ = tx_chunk.send(Ok(n));
                            }
                            export_check_cancelled()
                        })?;
                    } else {
                        let n = fs::copy(&job.src, &job.dst).map_err(|e| {
                            format!(
                                "copy {} -> {}: {}",
                                job.src.display(),
                                job.dst.display(),
                                e
                            )
                        })?;
                        let _ = tx_chunk.send(Ok(n));
                    }
                    let _ = tx_chunk.send(Ok(0));
                    Ok(())
                })();
                if let Err(e) = res {
                    let _ = tx_shared.send(Err(e));
                }
                let n = yield_counter.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 256 == 0 {
                    std::thread::yield_now();
                }
            });
        });
    });

    let mut failures: Vec<String> = Vec::new();
    if let Ok(mut g) = copy_errors.lock() {
        failures.append(&mut *g);
    }

    let files_done = progress
        .as_ref()
        .map(|p| *p.done)
        .unwrap_or(expected_copy_results);
    if files_done < expected_copy_results {
        failures.push(format!(
            "Parallel copy incomplete: processed {files_done} of {expected_copy_results} file results."
        ));
    }

    if !failures.is_empty() {
        let n = failures.len();
        let preview: String = failures.into_iter().take(6).collect::<Vec<_>>().join(" | ");
        return Err(format!("Copy finished with {n} error(s). First: {preview}"));
    }
    export_check_cancelled()?;
    Ok(())
}

fn never_skip_file(_: &Path) -> bool {
    false
}

/// Result of [`preflight_dir_bytes_fast`]: byte total plus whether a shallow sum replaced a slow/failed `du`.
#[derive(Clone, Copy, Default, Debug)]
struct PreflightMeasured {
    bytes: u64,
    partial: bool,
}

impl PreflightMeasured {
    fn exact(bytes: u64) -> Self {
        Self {
            bytes,
            partial: false,
        }
    }

    fn saturating_add(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.saturating_add(other.bytes),
            partial: self.partial || other.partial,
        }
    }
}

fn tree_file_bytes(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut sum = 0u64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        if entry.path().is_file() {
            sum += entry.metadata().map_err(|e| e.to_string())?.len();
        }
    }
    Ok(sum)
}

/// Shallow sizing: immediate children files only (no recursion). Used when `du` fails or times out.
fn shallow_dir_top_level_bytes(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    if path.is_file() {
        return Ok(fs::metadata(path).map_err(|e| e.to_string())?.len());
    }
    let mut sum = 0u64;
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let md = entry.metadata().map_err(|e| e.to_string())?;
        if md.is_file() {
            sum = sum.saturating_add(md.len());
        }
    }
    Ok(sum)
}

/// Run `du -sk` with a wall-clock cap; on timeout the child is killed and reaped so it cannot pin CPU in the background.
#[cfg(unix)]
fn du_sk_bytes_child_timeout(path: &Path, timeout: Duration) -> Result<u64, String> {
    let mut child = Command::new("du")
        .arg("-sk")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("du timed out for {}", path.display()));
        }
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                if !status.success() {
                    return Err(format!("du failed for {}", path.display()));
                }
                let mut stdout = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_end(&mut stdout);
                }
                let line = String::from_utf8_lossy(&stdout);
                let tok = line
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| format!("du produced no output for {}", path.display()))?;
                let kib: u64 = tok
                    .parse()
                    .map_err(|_| format!("du parse failed for {}: {tok}", path.display()))?;
                return Ok(kib.saturating_mul(1024));
            }
            None => std::thread::sleep(Duration::from_millis(40)),
        }
    }
}

/// Preflight-only sizing: prefer `du -sk` (with timeout) over deep walks; shallow fallback on failure/timeout.
fn preflight_dir_bytes_fast(path: &Path) -> Result<PreflightMeasured, String> {
    if !path.exists() {
        return Ok(PreflightMeasured::default());
    }
    if path.is_file() {
        return Ok(PreflightMeasured::exact(
            fs::metadata(path).map_err(|e| e.to_string())?.len(),
        ));
    }
    #[cfg(unix)]
    {
        match du_sk_bytes_child_timeout(path, Duration::from_secs(2)) {
            Ok(n) => Ok(PreflightMeasured::exact(n)),
            Err(_) => Ok(PreflightMeasured {
                bytes: shallow_dir_top_level_bytes(path)?,
                partial: true,
            }),
        }
    }
    #[cfg(not(unix))]
    {
        Ok(PreflightMeasured {
            bytes: shallow_dir_top_level_bytes(path)?,
            partial: true,
        })
    }
}

fn application_bundle_root_for_preflight() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            return Ok(bundle);
        }
    }
    exe.parent()
        .ok_or_else(|| "Could not resolve executable directory.".to_string())
        .map(|p| p.to_path_buf())
}

fn close_db_for_disk_wipe(db: &std::sync::Arc<std::sync::Mutex<Database>>) -> Result<(), String> {
    let mut guard = db.lock().map_err(|e| e.to_string())?;
    let _old = std::mem::replace(&mut *guard, Database::new_in_memory_placeholder()?);
    drop(_old);
    Ok(())
}

fn remove_sqlite_sidecar_files(db_path: &Path) {
    let base = db_path.to_string_lossy().to_string();
    for suf in ["-wal", "-shm", "-journal"] {
        let p = format!("{base}{suf}");
        if let Err(e) = fs::remove_file(&p) {
            eprintln!("[athena] scorched earth: remove {p}: {e}");
        }
    }
}

fn wipe_directory_contents(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn verify_usb_export(
    dest_root: &Path,
    export_type: &str,
    app_data_dir: &Path,
    emb_name: &str,
    rr_name: &str,
) -> Result<(), String> {
    let usb_data = dest_root.join("data");
    let emb_src = AthenaPaths::new(app_data_dir).user_embeddings_dir().join(emb_name);
    let rr_src = AthenaPaths::new(app_data_dir).user_rerankers_dir().join(rr_name);
    let emb_dst = usb_athena_paths(dest_root)
        .user_embeddings_dir()
        .join(emb_name);
    let rr_dst = usb_athena_paths(dest_root)
        .user_rerankers_dir()
        .join(rr_name);

    let is_outpost = export_type == "outpost"
        || export_type == "essentials"
        || export_type == "essentials_data";
    let is_expedition = export_type == "expedition" || export_type == "auto";
    let needs_models = export_type == "bastion"
        || export_type == "recommended"
        || export_type == "citadel"
        || export_type == "absolute"
        || export_type == "complete";

    if needs_models {
        if !emb_dst.is_dir() {
            return Err(format!(
                "Verification failed: missing embedding tree on USB: {}",
                emb_dst.display()
            ));
        }
        if !rr_dst.is_dir() {
            return Err(format!(
                "Verification failed: missing reranker tree on USB: {}",
                rr_dst.display()
            ));
        }

        let s_emb = tree_file_bytes(&emb_src)?;
        let d_emb = tree_file_bytes(&emb_dst)?;
        if s_emb != d_emb {
            return Err(format!(
                "Verification failed: embedding byte count mismatch (local {s_emb}, USB {d_emb})."
            ));
        }
        let s_rr = tree_file_bytes(&rr_src)?;
        let d_rr = tree_file_bytes(&rr_dst)?;
        if s_rr != d_rr {
            return Err(format!(
                "Verification failed: reranker byte count mismatch (local {s_rr}, USB {d_rr})."
            ));
        }
    } else if is_expedition {
        if emb_dst.is_dir() {
            let s_emb = tree_file_bytes(&emb_src)?;
            let d_emb = tree_file_bytes(&emb_dst)?;
            if s_emb != d_emb {
                return Err(format!(
                    "Verification failed: embedding byte count mismatch (local {s_emb}, USB {d_emb})."
                ));
            }
        }
        if rr_dst.is_dir() {
            let s_rr = tree_file_bytes(&rr_src)?;
            let d_rr = tree_file_bytes(&rr_dst)?;
            if s_rr != d_rr {
                return Err(format!(
                    "Verification failed: reranker byte count mismatch (local {s_rr}, USB {d_rr})."
                ));
            }
        }
    }

    let db_src = host_db_path(app_data_dir);
    let db_dst = usb_data.join("user").join("db").join("athena_index.db");

    if export_type == "citadel" || export_type == "complete" || export_type == "absolute" {
        if db_src.is_file() {
            if !db_dst.is_file() {
                return Err(format!(
                    "Verification failed: database missing on USB: {}",
                    db_dst.display()
                ));
            }
            let m1 = fs::metadata(&db_src).map_err(|e| e.to_string())?.len();
            let m2 = fs::metadata(&db_dst).map_err(|e| e.to_string())?.len();
            if m1 != m2 {
                return Err(format!(
                    "Verification failed: database size mismatch (local {m1} B, USB {m2} B)."
                ));
            }
        }
    } else if export_type == "bastion" || export_type == "recommended" {
        if db_src.is_file() {
            if !db_dst.is_file() {
                return Err(format!(
                    "Verification failed: database missing on USB: {}",
                    db_dst.display()
                ));
            }
            let m1 = fs::metadata(&db_src).map_err(|e| e.to_string())?.len();
            let m2 = fs::metadata(&db_dst).map_err(|e| e.to_string())?.len();
            if m1 != m2 {
                return Err(format!(
                    "Verification failed: database size mismatch (local {m1} B, USB {m2} B)."
                ));
            }
        }
    } else if is_outpost {
        if db_src.is_file() {
            if !db_dst.is_file() {
                return Err(format!(
                    "Verification failed: database missing on USB: {}",
                    db_dst.display()
                ));
            }
            let m2 = fs::metadata(&db_dst).map_err(|e| e.to_string())?.len();
            if m2 == 0 {
                return Err(
                    "Verification failed: exported database on USB is empty (unexpected)."
                        .to_string(),
                );
            }
        }
        // Outpost checklist: bundled app Ollama seed and/or one chat .gguf + minimal offline map.
        let chat_dst = usb_data.join("user").join("models").join("chat");
        let app_ollama = usb_data.join("app").join("models").join("ollama");
        let has_gguf = dir_contains_extension(&chat_dst, "gguf");
        let has_app_ollama = crate::athena_paths::ollama_store_has_model_blobs(&app_ollama);
        if !has_gguf && !has_app_ollama {
            return Err(
                "Verification failed: Outpost USB has neither a chat .gguf under data/user/models/chat nor a bundled Ollama seed under data/app/models/ollama. Download a chat model or seed app/models/ollama on the host, then re-export."
                    .to_string(),
            );
        }
        if !usb_has_offline_map_artifact(&usb_data) {
            return Err(
                "Verification failed: Outpost USB has no offline map (.pmtiles/.mbtiles) under data/user/maps. Download a small basemap first, then re-export."
                    .to_string(),
            );
        }
    }

    let is_citadel = export_type == "citadel"
        || export_type == "complete"
        || export_type == "absolute";
    if is_citadel {
        // Sister Server checklist: models tree, library mirrors when host had watch folders.
        let chat_src = chat_models_dir(app_data_dir);
        let chat_dst = usb_data.join("user").join("models").join("chat");
        if dir_contains_extension(&chat_src, "gguf") && !dir_contains_extension(&chat_dst, "gguf") {
            return Err(
                "Verification failed: Sister Server USB is missing chat models under data/user/models/chat."
                    .to_string(),
            );
        }
        if emb_src.is_dir() && !emb_dst.is_dir() {
            return Err(format!(
                "Verification failed: Sister Server USB missing embeddings at {}.",
                emb_dst.display()
            ));
        }
        if rr_src.is_dir() && !rr_dst.is_dir() {
            return Err(format!(
                "Verification failed: Sister Server USB missing rerankers at {}.",
                rr_dst.display()
            ));
        }
        let docs_dst = dest_root.join("documents");
        if !docs_dst.is_dir() {
            return Err(format!(
                "Verification failed: Sister Server USB missing library mirrors folder: {}",
                docs_dst.display()
            ));
        }
        if usb_has_offline_map_artifact(app_data_dir) && !usb_has_offline_map_artifact(&usb_data) {
            return Err(
                "Verification failed: Sister Server USB has no offline map artifacts under data/user/maps."
                    .to_string(),
            );
        }
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            let name = bundle
                .file_name()
                .ok_or_else(|| "Verification failed: bundle name.".to_string())?;
            let out = dest_root.join("Mac").join(name);
            if !out.is_dir() {
                return Err(format!(
                    "Verification failed: application bundle missing on USB: {}",
                    out.display()
                ));
            }
        } else {
            let out = dest_root.join("ATHENA");
            if !out.is_dir() {
                return Err(format!(
                    "Verification failed: install folder missing on USB: {}",
                    out.display()
                ));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let out = dest_root.join("ATHENA");
        if !out.is_dir() {
            return Err(format!(
                "Verification failed: install folder missing on USB: {}",
                out.display()
            ));
        }
    }

    Ok(())
}

fn execute_scorched_wipe(
    db: &Arc<Mutex<Database>>,
    app_data_dir: &Path,
    cfg: &AppConfig,
    kiwix_str: &str,
) -> Result<(), String> {
    let folders = {
        let g = db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, app_data_dir, cfg)?
    };

    close_db_for_disk_wipe(db)?;

    let db_path = host_db_path(app_data_dir);
    remove_sqlite_sidecar_files(&db_path);

    let app_data_canon = fs::canonicalize(app_data_dir).ok();

    for folder in folders {
        if folder == kiwix_str {
            continue;
        }
        let p = Path::new(&folder);
        if !p.is_dir() {
            continue;
        }
        if let Some(ref ad) = app_data_canon {
            if let Ok(c) = fs::canonicalize(p) {
                if c.starts_with(ad) {
                    eprintln!(
                        "[athena] scorched earth: skipping watch path inside app data (removed with app data): {}",
                        folder
                    );
                    continue;
                }
            }
        }
        eprintln!("[athena] scorched earth: wiping watch folder contents: {}", folder);
        wipe_directory_contents(p)?;
    }

    eprintln!(
        "[athena] scorched earth: removing app data directory {}",
        app_data_dir.display()
    );
    if app_data_dir.is_dir() {
        fs::remove_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    }

    eprintln!("[athena] scorched earth: finished");
    Ok(())
}

fn write_usb_readme(dest_root: &Path) -> Result<(), String> {
    let text = r#"ATHENA USB drive — how to use this drive

Open the folder that matches your computer's operating system.

On a Mac, open the "Mac" folder and run ATHENA from the application inside.

On Windows, open the "Windows" folder, then open the subfolder that matches your system type. Most modern computers use the "64-bit" folder. Older PCs may use the "32-bit" folder. Newer tablet-style devices with ARM processors should try the "ARM" folder. Run ATHENA.exe from the folder that matches your machine.

On Linux, open the "Linux" folder. Most desktops and laptops use "64-bit". ARM-based devices (including many tablets and single-board computers) should use the "ARM" folder. Run the ATHENA AppImage from the matching folder.

All intelligence, indexes, models, and application data for this portable install are stored in the "data" folder at the root of this drive. Keep the "data" folder together with these launchers.
"#;
    let p = dest_root.join("README.txt");
    fs::write(&p, text).map_err(|e| format!("Failed to write {}: {e}", p.display()))?;
    eprintln!("[athena] usb export: wrote {}", p.display());
    Ok(())
}

/// Copies `payload/<branch>` from app resources to `<dest_root>/<branch>` (e.g. Windows, Linux).
/// Missing resource paths are skipped; copy failures propagate so the UI is not left at 0%.
fn copy_bundled_payload_branches_to_usb(
    app: &tauri::AppHandle,
    dest_root: &Path,
    io: &mut ExportIoReporter,
    frac_lo: f32,
    frac_hi: f32,
) -> Result<(), String> {
    let branches = ["Windows", "Linux"];
    let mut pairs: Vec<(PathBuf, PathBuf, String)> = Vec::new();
    let mut total = 0u64;
    for branch in branches {
        let rel = format!("payload/{branch}");
        match app.path().resolve(&rel, tauri::path::BaseDirectory::Resource) {
            Ok(src) if src.is_dir() => {
                total = total.saturating_add(tree_file_bytes(&src).map_err(|e| {
                    format!(
                        "Failed to measure bundled launcher tree {} ({rel}): {e}",
                        src.display()
                    )
                })?);
                pairs.push((src, dest_root.join(branch), rel));
            }
            Ok(_) => eprintln!(
                "[athena] usb export: warning: {rel} is not a directory in resources (skipping)"
            ),
            Err(e) => eprintln!(
                "[athena] usb export: warning: could not resolve {rel} in app resources: {e} (skipping)"
            ),
        }
    }
    let total = total.max(1);
    let mut done = 0u64;
    for (src, dst, rel) in pairs {
        eprintln!(
            "[athena] usb export: bundled branch copy read_from={} write_to={} rel={}",
            src.display(),
            dst.display(),
            rel
        );
        if dst.exists() {
            if dst.is_dir() {
                fs::remove_dir_all(&dst).map_err(|e| {
                    format!(
                        "Failed to remove existing bundled launcher destination {}: {e}",
                        dst.display()
                    )
                })?;
            } else {
                fs::remove_file(&dst).map_err(|e| {
                    format!(
                        "Failed to remove existing bundled launcher destination {}: {e}",
                        dst.display()
                    )
                })?;
            }
        }
        copy_dir_recursive_impl(
            &src,
            &dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Copying Windows/Linux launchers…",
                frac_lo,
                frac_hi,
                total,
                done: &mut done,
            }),
            &never_skip_file,
        )
        .map_err(|e| {
            format!(
                "Failed copying bundled {rel} from {} to {}: {e}",
                src.display(),
                dst.display()
            )
        })?;
        eprintln!(
            "[athena] usb export: copied bundled {} -> {}",
            rel,
            dst.display()
        );
    }
    Ok(())
}

/// Skip files inside `**/payload/<pack>/…` when `<pack>` is not `Windows` or `Linux`.
/// Keeps loose files directly under `payload/` (only one path segment after `payload`).
fn outpost_skip_bundled_payload_pack_file(anchor: &Path, path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let rel = match path.strip_prefix(anchor) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let parts: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    let Some(pidx) = parts.iter().position(|p| p.eq_ignore_ascii_case("payload")) else {
        return false;
    };
    if parts.len() < pidx + 3 {
        return false;
    }
    let pack = parts[pidx + 1];
    !pack.eq_ignore_ascii_case("windows") && !pack.eq_ignore_ascii_case("linux")
}

/// Bundled default ONNX / model weights live under `**/Resources/{embedding,reranker}/` (see `tauri.conf.json`),
/// not under `payload/`. Outpost omits these so the USB Mac copy matches "no bundled AI weights".
fn outpost_skip_bundled_model_resource_file(anchor: &Path, path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let rel = match path.strip_prefix(anchor) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let parts: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    for w in parts.windows(2) {
        if !w[0].eq_ignore_ascii_case("resources") {
            continue;
        }
        let inner = w[1].to_ascii_lowercase();
        if inner == "embedding"
            || inner == "embeddings"
            || inner == "reranker"
            || inner == "rerankers"
        {
            return true;
        }
    }
    false
}

fn outpost_skip_file_for_slim_usb_export(anchor: &Path, path: &Path) -> bool {
    outpost_skip_bundled_payload_pack_file(anchor, path)
        || outpost_skip_bundled_model_resource_file(anchor, path)
}

fn tree_file_bytes_respecting_outpost_skip(anchor: &Path) -> Result<u64, String> {
    if !anchor.exists() {
        return Ok(0);
    }
    let anchor = fs::canonicalize(anchor).unwrap_or_else(|_| anchor.to_path_buf());
    let mut sum = 0u64;
    for entry in WalkDir::new(&anchor)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() && !outpost_skip_file_for_slim_usb_export(&anchor, p) {
            sum = sum.saturating_add(p.metadata().map_err(|e| e.to_string())?.len());
        }
    }
    Ok(sum)
}

#[cfg(target_os = "macos")]
fn copy_macos_app_bundle_filtered(
    src: &Path,
    dest_root: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    io.emit_phase(
        app,
        "Copying application bundle to USB (Outpost — omitting payload packs, embedding & reranker weights)…",
        0.08,
    );
    let src = fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let mac_dir = dest_root.join("Mac");
    fs::create_dir_all(&mac_dir).map_err(|e| e.to_string())?;
    let name = src
        .file_name()
        .ok_or_else(|| "Invalid bundle path.".to_string())?;
    let out = mac_dir.join(name);
    if out.exists() {
        eprintln!(
            "[athena] usb export: replacing existing {}",
            out.display()
        );
        if out.is_dir() {
            fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&out).map_err(|e| e.to_string())?;
        }
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;

    let total_src = tree_file_bytes_respecting_outpost_skip(&src)?.max(1);
    let mut done = 0u64;
    let anchor = src.clone();
    copy_dir_recursive_impl(
        &src,
        &out,
        Some(CopyProgress {
            app,
            io,
            phase: "Copying application bundle to USB (Outpost — slim copy)…",
            frac_lo: 0.08,
            frac_hi: 0.28,
            total: total_src,
            done: &mut done,
        }),
        &|p: &Path| outpost_skip_file_for_slim_usb_export(&anchor, p),
    )?;
    io.emit_phase(app, "Application copied.", 0.28);
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_macos_app_bundle_with_progress(
    src: &Path,
    dest_root: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    let mac_dir = dest_root.join("Mac");
    fs::create_dir_all(&mac_dir).map_err(|e| e.to_string())?;
    let name = src
        .file_name()
        .ok_or_else(|| "Invalid bundle path.".to_string())?;
    let out = mac_dir.join(name);
    if out.exists() {
        eprintln!(
            "[athena] usb export: replacing existing {}",
            out.display()
        );
        if out.is_dir() {
            fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&out).map_err(|e| e.to_string())?;
        }
    }

    let total_src = tree_file_bytes(src)?.max(1);
    let mut child = Command::new("/usr/bin/ditto")
        .arg(src.as_os_str())
        .arg(out.as_os_str())
        .spawn()
        .map_err(|e| e.to_string())?;

    let mut prev_dst = 0u64;
    let mut last_instant = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(280));
        let interval = last_instant.elapsed().as_secs_f64().max(0.001);
        last_instant = Instant::now();

        let dst_now = tree_file_bytes(&out).unwrap_or(prev_dst);
        let new_b = dst_now.saturating_sub(prev_dst);
        prev_dst = dst_now;

        let frac = 0.08 + 0.20 * (dst_now as f32 / total_src as f32).min(0.995);
        io.note_poll_transfer(
            app,
            "Copying application bundle to USB…",
            frac,
            new_b,
            interval,
        );

        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                if !status.success() {
                    return Err(format!("ditto failed copying bundle (status {status})"));
                }
                break;
            }
            None => {}
        }
    }

    io.emit_phase(app, "Application copied.", 0.28);
    Ok(())
}

fn copy_non_macos_install_folder_filtered(
    src: &Path,
    dest_root: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    io.emit_phase(
        app,
        "Copying application to USB (Outpost — omitting payload packs, embedding & reranker weights)…",
        0.08,
    );
    let src = fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let out = dest_root.join("ATHENA");
    if out.exists() {
        eprintln!(
            "[athena] usb export: replacing existing {}",
            out.display()
        );
        fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let total = tree_file_bytes_respecting_outpost_skip(&src)?.max(1);
    let mut done = 0u64;
    let anchor = src.clone();
    copy_dir_recursive_impl(
        &src,
        &out,
        Some(CopyProgress {
            app,
            io,
            phase: "Copying application to USB (Outpost — skipping extra payload packs)…",
            frac_lo: 0.08,
            frac_hi: 0.28,
            total,
            done: &mut done,
        }),
        &|p: &Path| outpost_skip_file_for_slim_usb_export(&anchor, p),
    )?;
    io.emit_phase(app, "Application copied.", 0.28);
    Ok(())
}

fn copy_non_macos_install_folder(
    src: &Path,
    dest_root: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    let out = dest_root.join("ATHENA");
    if out.exists() {
        eprintln!("[athena] usb export: replacing existing {}", out.display());
        fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let total = tree_file_bytes(src)?;
    let mut done = 0u64;
    copy_dir_recursive_impl(
        src,
        &out,
        Some(CopyProgress {
            app,
            io,
            phase: "Copying application to USB…",
            frac_lo: 0.08,
            frac_hi: 0.28,
            total,
            done: &mut done,
        }),
        &never_skip_file,
    )?;
    io.emit_phase(app, "Application copied.", 0.28);
    Ok(())
}

fn copy_application_to_destination(
    app: &tauri::AppHandle,
    dest_root: &Path,
    io: &mut ExportIoReporter,
    strip_non_launcher_payload_packs: bool,
) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    eprintln!(
        "[athena] usb export: copy_application_to_destination exe={} exists={} dest_root={} strip_non_launcher_payload={}",
        exe.display(),
        exe.is_file(),
        dest_root.display(),
        strip_non_launcher_payload_packs
    );

    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_app_bundle_walking_up(&exe) {
            eprintln!(
                "[athena] usb export: macOS app bundle source={} is_dir={}",
                bundle.display(),
                bundle.is_dir()
            );
            if strip_non_launcher_payload_packs {
                return copy_macos_app_bundle_filtered(&bundle, dest_root, app, io);
            }
            io.emit_phase(app, "Copying application bundle to USB…", 0.08);
            return copy_macos_app_bundle_with_progress(&bundle, dest_root, app, io);
        }
    }

    let install_dir = exe
        .parent()
        .ok_or_else(|| "Could not resolve executable directory.".to_string())?;
    eprintln!(
        "[athena] usb export: non-mac install_dir={} is_dir={}",
        install_dir.display(),
        install_dir.is_dir()
    );
    if strip_non_launcher_payload_packs {
        return copy_non_macos_install_folder_filtered(install_dir, dest_root, app, io);
    }
    io.emit_phase(app, "Copying application to USB…", 0.08);
    copy_non_macos_install_folder(install_dir, dest_root, app, io)
}

fn sanitize_documents_subdir(raw: &str) -> String {
    let s: String = raw
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0'))
        .collect();
    let t = s.trim();
    if t.is_empty() {
        "library".to_string()
    } else {
        t.chars().take(120).collect()
    }
}

fn skip_satellite_map_file(p: &Path) -> bool {
    is_probably_satellite_map_archive(p)
}

/// Bytes for the full application tree `copy_application_to_destination(..., false)` copies.
fn estimate_application_bundle_bytes() -> Result<PreflightMeasured, String> {
    let root = application_bundle_root_for_preflight()?;
    preflight_dir_bytes_fast(&root)
}

/// Preflight-only: full install/bundle size via fast sizing minus bundled `payload/*` pack dirs.
fn estimate_application_bundle_bytes_outpost(app: &tauri::AppHandle) -> Result<PreflightMeasured, String> {
    let full = estimate_application_bundle_bytes()?;
    let payload_root = match app.path().resolve("payload", tauri::path::BaseDirectory::Resource) {
        Ok(p) if p.is_dir() => p,
        _ => return Ok(full),
    };
    let mut skipped = PreflightMeasured::default();
    for entry in fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("Linux") {
            continue;
        }
        if path.is_dir() {
            skipped = skipped.saturating_add(preflight_dir_bytes_fast(&path)?);
        }
    }
    Ok(PreflightMeasured {
        bytes: full.bytes.saturating_sub(skipped.bytes),
        partial: full.partial || skipped.partial,
    })
}

#[inline]
fn expedition_remaining_bytes(budget_bytes: u64, io: &ExportIoReporter) -> u64 {
    budget_bytes.saturating_sub(io.total_session_bytes)
}

fn bundled_payload_launchers_bytes(app: &tauri::AppHandle) -> PreflightMeasured {
    let mut acc = PreflightMeasured::default();
    for branch in ["Windows", "Linux"] {
        let rel = format!("payload/{branch}");
        if let Ok(src) = app.path().resolve(&rel, tauri::path::BaseDirectory::Resource) {
            if src.is_dir() {
                if let Ok(m) = preflight_dir_bytes_fast(&src) {
                    acc = acc.saturating_add(m);
                }
            }
        }
    }
    acc
}

fn sum_payload_children_bytes_except_launchers(app: &tauri::AppHandle) -> Result<PreflightMeasured, String> {
    let payload_root = match app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
    {
        Ok(p) => p,
        Err(_) => return Ok(PreflightMeasured::default()),
    };
    if !payload_root.is_dir() {
        return Ok(PreflightMeasured::default());
    }
    let mut sum = PreflightMeasured::default();
    for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let src = e.path();
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("Linux") {
            continue;
        }
        if src.is_dir() {
            sum = sum.saturating_add(preflight_dir_bytes_fast(&src)?);
        } else if src.is_file() {
            sum = sum.saturating_add(PreflightMeasured::exact(
                fs::metadata(&src).map_err(|e| e.to_string())?.len(),
            ));
        }
    }
    Ok(sum)
}

fn bundled_payload_root_total_bytes(app: &tauri::AppHandle) -> Result<PreflightMeasured, String> {
    let payload_root = match app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
    {
        Ok(p) => p,
        Err(_) => return Ok(PreflightMeasured::default()),
    };
    if !payload_root.is_dir() {
        return Ok(PreflightMeasured::default());
    }
    preflight_dir_bytes_fast(&payload_root)
}

/// Fast offline-maps sizing for preflight: one `du` on the maps root plus a rough sat/non split.
fn offline_maps_preflight_split(app_data_dir: &Path) -> Result<(u64, u64, u64, bool), String> {
    let dir = map_manager_download::offline_maps_directory(app_data_dir);
    if !dir.is_dir() {
        return Ok((0, 0, 0, false));
    }
    let m = preflight_dir_bytes_fast(&dir)?;
    let all = m.bytes;
    let partial = m.partial;
    let satellite = all.saturating_mul(45) / 100;
    let non = all.saturating_sub(satellite);
    Ok((non, satellite, all, partial))
}

fn offline_maps_non_satellite_bytes(app_data_dir: &Path) -> Result<u64, String> {
    Ok(offline_maps_preflight_split(app_data_dir)?.0)
}

fn list_sorted_payload_pack_dirs(
    app: &tauri::AppHandle,
) -> Result<Vec<(i64, PathBuf, u64)>, String> {
    let payload_root = match app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
    {
        Ok(p) => p,
        Err(_) => return Ok(Vec::new()),
    };
    if !payload_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<(i64, PathBuf, u64)> = Vec::new();
    for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let src = e.path();
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("Linux") {
            continue;
        }
        if !src.is_dir() {
            continue;
        }
        let prio = payload_priority_for_dir(&src).unwrap_or(0);
        let sz = dir_size_bytes(&src);
        entries.push((prio, src, sz));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(entries)
}

fn collect_satellite_map_file_sizes_sorted_asc(app_data_dir: &Path) -> Result<Vec<u64>, String> {
    let sat = offline_maps_preflight_split(app_data_dir)?.1;
    if sat == 0 {
        Ok(Vec::new())
    } else {
        Ok(vec![sat])
    }
}

fn preflight_estimated_seconds(total_bytes: u64, mib_per_sec: f64) -> u64 {
    if total_bytes == 0 {
        return 0;
    }
    let denom = mib_per_sec * 1024.0 * 1024.0;
    ((total_bytes as f64 / denom).ceil() as u64).max(1)
}

fn push_expedition_plan_step(
    out: &mut Vec<AegisExpeditionPlanStep>,
    used: &mut u64,
    budget: u64,
    key: &str,
    label: &str,
    bytes: u64,
) {
    let will = bytes <= budget.saturating_sub(*used);
    if will {
        *used = used.saturating_add(bytes);
    }
    out.push(AegisExpeditionPlanStep {
        key: key.to_string(),
        label: label.to_string(),
        bytes,
        will_copy: will,
    });
}

fn simulate_expedition_plan(
    budget: u64,
    application_bytes: u64,
    launchers_bytes: u64,
    models_bytes: u64,
    full_sqlite_bytes: u64,
    example_region_reserved: u64,
    maps_non_sat: u64,
    library_rows: Vec<(String, String, u64)>,
    tier2_packs: Vec<(String, u64)>,
    satellite_sizes_asc: Vec<u64>,
) -> Vec<AegisExpeditionPlanStep> {
    let mut out: Vec<AegisExpeditionPlanStep> = Vec::new();
    let mut used: u64 = 0;

    push_expedition_plan_step(
        &mut out,
        &mut used,
        budget,
        "application",
        "Application bundle",
        application_bytes,
    );
    push_expedition_plan_step(
        &mut out,
        &mut used,
        budget,
        "launchers",
        "Windows / Linux launchers",
        launchers_bytes,
    );
    push_expedition_plan_step(
        &mut out,
        &mut used,
        budget,
        "models",
        "Embedding + reranker models",
        models_bytes,
    );
    push_expedition_plan_step(
        &mut out,
        &mut used,
        budget,
        "database",
        "Full SQLite index (athena_index.db)",
        full_sqlite_bytes,
    );
    push_expedition_plan_step(
        &mut out,
        &mut used,
        budget,
        "exampleRegion",
        "Example region terrain extract (estimate)",
        example_region_reserved,
    );
    push_expedition_plan_step(
        &mut out,
        &mut used,
        budget,
        "mapsTerrain",
        "Offline maps (terrain / non-satellite rasters excluded)",
        maps_non_sat,
    );

    for (key, label, sz) in library_rows {
        push_expedition_plan_step(&mut out, &mut used, budget, &key, &label, sz);
    }

    for (i, (pack_name, sz)) in tier2_packs.into_iter().enumerate() {
        let label = format!("Bundled payload pack ({pack_name})");
        let key = format!("payloadPack_{i}");
        push_expedition_plan_step(&mut out, &mut used, budget, &key, &label, sz);
    }

    let mut sat_greedy: u64 = 0;
    for sz in &satellite_sizes_asc {
        if used.saturating_add(*sz) <= budget {
            used = used.saturating_add(*sz);
            sat_greedy = sat_greedy.saturating_add(*sz);
        }
    }
    out.push(AegisExpeditionPlanStep {
        key: "satellite".to_string(),
        label: "Satellite / aerial map rasters (greedy fit)".to_string(),
        bytes: sat_greedy,
        will_copy: sat_greedy > 0,
    });

    out
}

/// Byte totals for manual tiers plus optional drive fit checks and an Expedition staging plan.
///
/// Always computes the full component set in one pass (no `preflight_tier` / export-scope branching).
/// When a destination path is set, [`destination_fits_fast_node`] and [`destination_fits_sister_server`]
/// are both derived from the same measured totals as the legacy outpost/citadel fit flags.
pub fn aegis_export_preflight(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    destination_path: Option<String>,
) -> Result<AegisExportPreflight, String> {
    const MIB_PER_S: f64 = 30.0;

    let app_data_dir = resolved_app_data_dir(app)?;
    let cfg = load_config(&app_data_dir);
    let emb_name = effective_embedding_folder(&cfg);
    let rr_name = effective_reranker_folder(&cfg);
    let host = AthenaPaths::new(&app_data_dir);
    let emb_src = resolve_model_pack_dir(&host, "embeddings", &emb_name);
    let rr_src = resolve_model_pack_dir(&host, "rerankers", &rr_name);
    let chat_dir = chat_models_dir(&app_data_dir);

    let launch_m = bundled_payload_launchers_bytes(app);
    let bundled_launchers_bytes = launch_m.bytes;
    let slim_sqlite_bytes = compute_slim_db_export_bytes(db)?;
    let db_path = host_db_path(&app_data_dir);
    let example_region_extract_estimate_bytes = EXAMPLE_REGION_USB_EXTRACT_ESTIMATE_BYTES;
    let example_region_r = example_region_reserved_bytes();
    let kiwix_str = kiwix_downloads_dir(&app_data_dir)
        .to_string_lossy()
        .to_string();

    let app_full_m = estimate_application_bundle_bytes()?;
    let application_bytes = app_full_m.bytes;
    let app_out_m = estimate_application_bundle_bytes_outpost(app)?;
    let application_bundle_bytes_outpost = app_out_m.bytes;
    let full_sqlite_bytes = if db_path.is_file() {
        fs::metadata(&db_path).map_err(|e| e.to_string())?.len()
    } else {
        0
    };
    let models_m = if emb_src.is_dir() && rr_src.is_dir() {
        preflight_dir_bytes_fast(&emb_src)?.saturating_add(preflight_dir_bytes_fast(&rr_src)?)
    } else {
        PreflightMeasured::default()
    };
    let models_bytes = models_m.bytes;
    let chat_m = if chat_dir.is_dir() {
        preflight_dir_bytes_fast(&chat_dir)?
    } else {
        PreflightMeasured::default()
    };
    let chat_models_all_bytes = chat_m.bytes;
    let chat_models_smallest_bytes = pick_smallest_chat_model(&chat_dir)?
        .map(|(_, len)| len)
        .unwrap_or(0);
    let (
        offline_maps_non_satellite_bytes,
        offline_maps_satellite_bytes,
        offline_maps_all_bytes,
        maps_partial,
    ) = offline_maps_preflight_split(&app_data_dir)?;
    let payload_children_m = sum_payload_children_bytes_except_launchers(app)?;
    let bundled_payload_children_bytes = payload_children_m.bytes;
    let root_payload_m = bundled_payload_root_total_bytes(app)?;
    let bundled_payload_root_total_bytes = root_payload_m.bytes;
    let lib_non_m = library_watch_folders_non_satellite_bytes(db, &app_data_dir, &cfg, &kiwix_str)?;
    let library_non_satellite_bytes = lib_non_m.bytes;
    let lib_all_m = library_watch_folders_all_bytes(db, &app_data_dir, &cfg, &kiwix_str)?;
    let library_all_bytes = lib_all_m.bytes;

    let outpost_total_bytes = application_bundle_bytes_outpost
        .saturating_add(bundled_launchers_bytes)
        .saturating_add(example_region_r)
        .saturating_add(chat_models_smallest_bytes)
        .saturating_add(slim_sqlite_bytes);

    let mut bastion_total_bytes = application_bytes
        .saturating_add(bundled_launchers_bytes)
        .saturating_add(example_region_r)
        .saturating_add(full_sqlite_bytes)
        .saturating_add(models_bytes)
        .saturating_add(chat_models_all_bytes)
        .saturating_add(offline_maps_non_satellite_bytes)
        .saturating_add(library_non_satellite_bytes)
        .saturating_add(bundled_payload_children_bytes);
    bastion_total_bytes = bastion_total_bytes.max(outpost_total_bytes);

    let mut citadel_total_bytes = application_bytes
        .saturating_add(bundled_payload_root_total_bytes)
        .saturating_add(example_region_r)
        .saturating_add(full_sqlite_bytes)
        .saturating_add(models_bytes)
        .saturating_add(chat_models_all_bytes)
        .saturating_add(offline_maps_all_bytes)
        .saturating_add(library_all_bytes);
    citadel_total_bytes = citadel_total_bytes.max(bastion_total_bytes);
    // Sister Server (citadel) must never size below Fast Node (outpost), even if estimates overlap.
    citadel_total_bytes = citadel_total_bytes.max(outpost_total_bytes);

    let fast_node_measure_status = if app_out_m.partial
        || launch_m.partial
        || models_m.partial
        || chat_m.partial
    {
        "calculating".to_string()
    } else {
        "ready".to_string()
    };
    let sister_server_measure_status = if app_full_m.partial
        || launch_m.partial
        || models_m.partial
        || chat_m.partial
        || maps_partial
        || lib_all_m.partial
        || payload_children_m.partial
        || root_payload_m.partial
    {
        "calculating".to_string()
    } else {
        "ready".to_string()
    };

    let dest_trim = destination_path
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let (
        destination_available_bytes,
        destination_usable_expedition_budget_bytes,
        destination_fits_outpost,
        destination_fits_bastion,
        destination_fits_citadel,
        expedition_plan,
    ) = if let Some(ref mount) = dest_trim {
        let root = PathBuf::from(mount);
        let destination_available_bytes = available_space_bytes_for_destination(&root);
        let usable = destination_available_bytes.map(|a| a.saturating_sub(EXPEDITION_SECURITY_BUFFER_BYTES));
        let fits_out = destination_available_bytes.map(|a| a >= outpost_total_bytes);
        let fits_bas = destination_available_bytes.map(|a| a >= bastion_total_bytes);
        let fits_cit = destination_available_bytes.map(|a| a >= citadel_total_bytes);

        let plan = if let Some(budget) = usable {
            let folders = {
                let g = db.lock().map_err(|e| e.to_string())?;
                compute_watcher_folders(&*g, &app_data_dir, &cfg)?
            };
            let mut lib_rows: Vec<(String, String, u64)> = Vec::new();
            for (i, folder) in folders.iter().enumerate() {
                if folder == &kiwix_str {
                    continue;
                }
                let p = Path::new(folder);
                if !p.is_dir() {
                    continue;
                }
                let sz = preflight_dir_bytes_fast(p)?.bytes;
                let name = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("library");
                lib_rows.push((
                    format!("library_{i}"),
                    format!("Library folder: {name}"),
                    sz,
                ));
            }

            let packs = list_sorted_payload_pack_dirs(app)?;
            let tier2: Vec<(String, u64)> = packs
                .into_iter()
                .map(|(_prio, path, sz)| {
                    let nm = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("pack")
                        .to_string();
                    (nm, sz)
                })
                .collect();

            let sat_sizes = collect_satellite_map_file_sizes_sorted_asc(&app_data_dir)?;

            Some(simulate_expedition_plan(
                budget,
                application_bytes,
                bundled_launchers_bytes,
                models_bytes,
                full_sqlite_bytes,
                example_region_r,
                offline_maps_non_satellite_bytes,
                lib_rows,
                tier2,
                sat_sizes,
            ))
        } else {
            None
        };

        (
            destination_available_bytes,
            usable,
            fits_out,
            fits_bas,
            fits_cit,
            plan,
        )
    } else {
        (None, None, None, None, None, None)
    };

    Ok(AegisExportPreflight {
        expedition_security_buffer_bytes: EXPEDITION_SECURITY_BUFFER_BYTES,
        assumed_write_mebibytes_per_sec: MIB_PER_S,
        application_bytes,
        application_bundle_bytes_outpost,
        bundled_launchers_bytes,
        slim_sqlite_bytes,
        full_sqlite_bytes,
        models_bytes,
        chat_models_all_bytes,
        chat_models_smallest_bytes,
        example_region_extract_estimate_bytes,
        example_region_reserved_bytes: example_region_r,
        offline_maps_non_satellite_bytes,
        offline_maps_satellite_bytes,
        offline_maps_all_bytes,
        library_non_satellite_bytes,
        library_all_bytes,
        bundled_payload_children_bytes,
        bundled_payload_root_total_bytes,
        outpost_total_bytes,
        bastion_total_bytes,
        citadel_total_bytes,
        outpost_estimated_seconds: preflight_estimated_seconds(outpost_total_bytes, MIB_PER_S),
        bastion_estimated_seconds: preflight_estimated_seconds(bastion_total_bytes, MIB_PER_S),
        citadel_estimated_seconds: preflight_estimated_seconds(citadel_total_bytes, MIB_PER_S),
        destination_available_bytes,
        destination_usable_expedition_budget_bytes,
        destination_fits_outpost,
        destination_fits_bastion,
        destination_fits_citadel,
        destination_fits_fast_node: destination_fits_outpost,
        destination_fits_sister_server: destination_fits_citadel,
        fast_node_measure_status,
        sister_server_measure_status,
        expedition_plan,
    })
}

/// Runs [`aegis_export_preflight`] on a background thread and emits the result on `athena:aegis-preflight-progress`.
pub fn start_aegis_export_preflight_background(
    app: &tauri::AppHandle,
    db: Arc<Mutex<Database>>,
    destination_path: Option<String>,
) {
    let app = app.clone();
    std::thread::spawn(move || {
        let res = aegis_export_preflight(&app, &db, destination_path);
        let ev = match res {
            Ok(pf) => AegisPreflightProgressEvent {
                done: true,
                error: None,
                preflight: Some(pf),
            },
            Err(e) => AegisPreflightProgressEvent {
                done: true,
                error: Some(e),
                preflight: None,
            },
        };
        if let Err(err) = app.emit("athena:aegis-preflight-progress", &ev) {
            eprintln!("[athena] aegis preflight emit failed: {err}");
        }
    });
}

fn tree_file_bytes_filtered(path: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut sum = 0u64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() && !skip(p) {
            sum = sum.saturating_add(p.metadata().map_err(|e| e.to_string())?.len());
        }
    }
    Ok(sum)
}

fn library_watch_folders_non_satellite_bytes(
    db: &Arc<Mutex<Database>>,
    app_data_dir: &Path,
    cfg: &AppConfig,
    kiwix_str: &str,
) -> Result<PreflightMeasured, String> {
    let folders = {
        let g = db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, app_data_dir, cfg)?
    };
    let mut sum = PreflightMeasured::default();
    for folder in folders {
        if folder == kiwix_str {
            continue;
        }
        let p = Path::new(&folder);
        if p.is_dir() {
            sum = sum.saturating_add(preflight_dir_bytes_fast(p)?);
        }
    }
    Ok(sum)
}

fn library_watch_folders_all_bytes(
    db: &Arc<Mutex<Database>>,
    app_data_dir: &Path,
    cfg: &AppConfig,
    kiwix_str: &str,
) -> Result<PreflightMeasured, String> {
    let folders = {
        let g = db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, app_data_dir, cfg)?
    };
    let mut sum = PreflightMeasured::default();
    for folder in folders {
        if folder == kiwix_str {
            continue;
        }
        let p = Path::new(&folder);
        if p.is_dir() {
            sum = sum.saturating_add(preflight_dir_bytes_fast(p)?);
        }
    }
    Ok(sum)
}

fn copy_watcher_documents_to_usb_filtered(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    app_data_dir: &Path,
    cfg: &AppConfig,
    dest_root: &Path,
    io: &mut ExportIoReporter,
    skip_satellite_archives: bool,
) -> Result<(), String> {
    let kiwix_str = kiwix_downloads_dir(app_data_dir)
        .to_string_lossy()
        .to_string();
    let folders = {
        let g = db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, app_data_dir, cfg)?
    };
    let docs_root = dest_root.join("documents");
    fs::create_dir_all(&docs_root).map_err(|e| e.to_string())?;
    let skip_ref: &dyn Fn(&Path) -> bool = if skip_satellite_archives {
        &skip_satellite_map_file
    } else {
        &never_skip_file
    };

    let n_lib = folders
        .iter()
        .filter(|f| *f != &kiwix_str)
        .filter(|f| Path::new(f).is_dir())
        .count();
    let mut lib_i = 0usize;
    let mut used_names = std::collections::HashSet::<String>::new();
    for folder in &folders {
        if folder == &kiwix_str {
            continue;
        }
        let p = Path::new(folder);
        if !p.is_dir() {
            continue;
        }
        let base = p
            .file_name()
            .and_then(|s| s.to_str())
            .map(sanitize_documents_subdir)
            .unwrap_or_else(|| "library".to_string());
        let mut name = base.clone();
        let mut n = 0u32;
        while used_names.contains(&name) {
            n += 1;
            name = format!("{base}_{n}");
        }
        used_names.insert(name.clone());
        let dst = docs_root.join(&name);
        lib_i += 1;
        let frac = if n_lib == 0 {
            0.88_f32
        } else {
            0.72_f32 + (lib_i as f32 / n_lib as f32) * (0.86_f32 - 0.72_f32)
        };
        io.emit_phase(
            app,
            format!("Copying library folder ({lib_i}/{n_lib})…"),
            frac,
        );
        fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
        // Parallelize library folder copies: lots of small files in deep trees.
        let total_files = WalkDir::new(p)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file() && !skip_ref(e.path()))
            .count() as u64;
        let mut folder_done_files = 0u64;
        let frac_lo = if n_lib == 0 {
            0.72_f32
        } else {
            0.72_f32 + ((lib_i - 1) as f32 / n_lib as f32) * (0.86_f32 - 0.72_f32)
        };
        let frac_hi = if n_lib == 0 {
            0.86_f32
        } else {
            0.72_f32 + (lib_i as f32 / n_lib as f32) * (0.86_f32 - 0.72_f32)
        };
        copy_dir_recursive_parallel_impl(
            p,
            &dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Copying library to USB…",
                frac_lo,
                frac_hi,
                total: total_files.max(1),
                done: &mut folder_done_files,
            }),
            skip_ref,
        )?;
    }
    Ok(())
}

fn write_portable_marker_for_export(dest_root: &Path) -> Result<(), String> {
    // Export layout: `data/` at the USB root, binaries in OS-specific subfolders.
    // We place `portable.athena` beside each executable/bundle so portable mode resolves AthenaData
    // to the USB's root `data/` directory.
    let data_rel_from_athena_dir = "../data\n";
    let marker_winlin = dest_root.join("ATHENA").join(PORTABLE_ATHENA_MARKER_FILENAME);
    if marker_winlin.parent().map(|p| p.is_dir()).unwrap_or(false) {
        fs::write(&marker_winlin, data_rel_from_athena_dir).map_err(|e| {
            format!(
                "Failed to write portable marker {}: {e}",
                marker_winlin.display()
            )
        })?;
    }

    #[cfg(target_os = "macos")]
    {
        let mac_dir = dest_root.join("Mac");
        if mac_dir.is_dir() {
            // Find first .app bundle under Mac/ (export copies the running bundle there).
            for entry in fs::read_dir(&mac_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let p = entry.path();
                if p.is_dir() && p.extension().map_or(false, |e| e == "app") {
                    let marker = p.join("Contents").join(PORTABLE_ATHENA_MARKER_FILENAME);
                    let parent = marker.parent().ok_or_else(|| {
                        format!("Invalid portable marker path (no parent): {}", marker.display())
                    })?;
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    // Contents/ -> ../../../data
                    fs::write(&marker, "../../../data\n").map_err(|e| {
                        format!(
                            "Failed to write portable marker {}: {e}",
                            marker.display()
                        )
                    })?;
                    break;
                }
            }
        }
    }

    Ok(())
}

fn copy_offline_maps_to_usb(
    app_data_dir: &Path,
    usb_data: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
    skip_satellite: bool,
    frac_lo: f32,
    frac_hi: f32,
) -> Result<(), String> {
    let src = map_manager_download::offline_maps_directory(app_data_dir);
    if !src.is_dir() {
        return Ok(());
    }
    let dst = usb_data.join("user").join("maps");
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    let skip_ref: &dyn Fn(&Path) -> bool = if skip_satellite {
        &skip_satellite_map_file
    } else {
        &never_skip_file
    };
    let mut done_bytes = 0u64;
    copy_dir_recursive_parallel_impl(
        &src,
        &dst,
        Some(CopyProgress {
            app,
            io,
            phase: "Copying offline map archives…",
            frac_lo,
            frac_hi,
            total: 1,
            done: &mut done_bytes,
        }),
        skip_ref,
    )?;
    Ok(())
}

fn copy_full_payload_to_usb_root(
    app: &tauri::AppHandle,
    dest_root: &Path,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    io.emit_phase(
        app,
        "Copying full payload (this can take a long time)…",
        0.30,
    );
    let payload_root = app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
        .ok();
    if let Some(payload_root) = payload_root {
        if payload_root.is_dir() {
            let mut items: Vec<(PathBuf, PathBuf, u64, bool)> = Vec::new();
            for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
                let e = entry.map_err(|e| e.to_string())?;
                let src = e.path();
                let dst = dest_root.join(e.file_name());
                let (sz, is_dir) = if src.is_dir() {
                    (tree_file_bytes(&src)?, true)
                } else if src.is_file() {
                    (fs::metadata(&src).map_err(|e| e.to_string())?.len(), false)
                } else {
                    continue;
                };
                items.push((src, dst, sz, is_dir));
            }
            let total: u64 = items.iter().map(|i| i.2).sum();
            let total = total.max(1);
            let mut done = 0u64;
            for (src, dst, _sz, is_dir) in items {
                if dst.exists() {
                    if dst.is_dir() {
                        std::fs::remove_dir_all(&dst).map_err(|e| {
                            format!(
                                "Failed to remove existing payload destination {}: {e}",
                                dst.display()
                            )
                        })?;
                    } else {
                        std::fs::remove_file(&dst).map_err(|e| {
                            format!(
                                "Failed to remove existing payload destination {}: {e}",
                                dst.display()
                            )
                        })?;
                    }
                }
                if is_dir {
                    copy_dir_recursive_impl(
                        &src,
                        &dst,
                        Some(CopyProgress {
                            app,
                            io,
                            phase: "Copying full payload…",
                            frac_lo: 0.30,
                            frac_hi: 0.42,
                            total,
                            done: &mut done,
                        }),
                        &never_skip_file,
                    )?;
                } else {
                    let len = fs::metadata(&src).map_err(|e| e.to_string())?.len();
                    fs::copy(&src, &dst).map_err(|e| {
                        format!(
                            "Failed to copy payload file {} -> {}: {e}",
                            src.display(),
                            dst.display()
                        )
                    })?;
                    done = done.saturating_add(len);
                    let frac = 0.30 + (0.42 - 0.30) * (done as f32 / total as f32).min(1.0);
                    io.note_file_bytes(app, "Copying full payload…", frac, len);
                }
            }
        }
    }
    Ok(())
}

fn copy_full_database_to_usb(
    app_data_dir: &Path,
    usb_data: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
    phase_frac: f32,
) -> Result<(), String> {
    let db_src = host_db_path(app_data_dir);
    if !db_src.is_file() {
        eprintln!(
            "[athena] usb export: database missing at {}",
            db_src.display()
        );
        return Ok(());
    }
    io.emit_phase(app, "Copying full database…", phase_frac);
    let db_dst = usb_data.join("user").join("db").join("athena_index.db");
    let len = fs::metadata(&db_src).map_err(|e| e.to_string())?.len();
    fs::copy(&db_src, &db_dst).map_err(|e| e.to_string())?;
    io.note_file_bytes(app, "Copying full database…", phase_frac + 0.02, len);
    Ok(())
}

fn compute_slim_db_export_bytes(db: &Arc<Mutex<Database>>) -> Result<u64, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let tmp = dir.path().join("athena_slim_size.db");
    {
        let g = db.lock().map_err(|e| e.to_string())?;
        g.backup_to_path(&tmp)?;
    }
    let conn = Connection::open(&tmp).map_err(|e| e.to_string())?;
    strip_heavy_library_tables_from_usb_db(&conn)?;
    conn.execute("VACUUM", []).map_err(|e| e.to_string())?;
    drop(conn);
    fs::metadata(&tmp).map_err(|e| e.to_string()).map(|m| m.len())
}

/// Greedy payload packs by `metadata.json` priority, stopping before exceeding `max_bytes` of **new** data.
fn tier2_copy_payload_priority_fill_within_budget(
    app: &tauri::AppHandle,
    dest_root: &Path,
    io: &mut ExportIoReporter,
    max_bytes: u64,
) -> Result<(), String> {
    if max_bytes < 1024 * 1024 {
        return Ok(());
    }
    let payload_root = app
        .path()
        .resolve("payload", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Could not resolve bundled payload dir: {e}"))?;
    if !payload_root.is_dir() {
        return Ok(());
    }

    let mut entries: Vec<(i64, PathBuf, u64)> = Vec::new();
    for entry in std::fs::read_dir(&payload_root).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let src = e.path();
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.eq_ignore_ascii_case("Windows") || name.eq_ignore_ascii_case("Linux") {
            continue;
        }
        if !src.is_dir() {
            continue;
        }
        let prio = payload_priority_for_dir(&src).unwrap_or(0);
        let sz = dir_size_bytes(&src);
        entries.push((prio, src, sz));
    }

    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut plan: Vec<(PathBuf, PathBuf, i64, u64)> = Vec::new();
    let mut committed: u64 = 0;
    for (prio, src, sz) in entries {
        if sz > max_bytes.saturating_sub(committed) {
            continue;
        }
        let name = src
            .file_name()
            .ok_or_else(|| "payload entry missing filename".to_string())?;
        let dst = dest_root.join(name);
        plan.push((src, dst, prio, sz));
        committed = committed.saturating_add(sz);
    }

    let total: u64 = plan.iter().map(|p| p.3).sum();
    if total == 0 {
        return Ok(());
    }
    let total = total.max(1);
    let mut done = 0u64;

    for (src, dst, prio, sz) in plan {
        eprintln!(
            "[athena] usb export (expedition): tier2 pack prio={prio} size={} -> {}",
            sz,
            dst.display()
        );
        if dst.exists() {
            std::fs::remove_dir_all(&dst).map_err(|e| {
                format!(
                    "Failed to remove existing expedition payload {}: {e}",
                    dst.display()
                )
            })?;
        }
        copy_dir_recursive_impl(
            &src,
            &dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Filling priority payload packs…",
                frac_lo: 0.70,
                frac_hi: 0.78,
                total,
                done: &mut done,
            }),
            &never_skip_file,
        )?;
    }

    Ok(())
}

fn copy_satellite_map_archives_greedy(
    app_data_dir: &Path,
    usb_maps: &Path,
    app: &tauri::AppHandle,
    io: &mut ExportIoReporter,
    budget_bytes: u64,
) -> Result<(), String> {
    let src_dir = map_manager_download::offline_maps_directory(app_data_dir);
    if !src_dir.is_dir() {
        return Ok(());
    }
    let mut files: Vec<(PathBuf, u64)> = Vec::new();
    for entry in WalkDir::new(&src_dir)
        .follow_links(false)
        .into_iter()
        .map(|e| e.map_err(|e| e.to_string()))
    {
        let entry = entry?;
        let p = entry.path().to_path_buf();
        if p.is_file() && skip_satellite_map_file(&p) {
            let len = p.metadata().map_err(|e| e.to_string())?.len();
            files.push((p, len));
        }
    }
    files.sort_by_key(|(_, sz)| *sz);
    let mut used = io.total_session_bytes;
    for (path, len) in files {
        if used.saturating_add(len) > budget_bytes {
            continue;
        }
        let rel = path.strip_prefix(&src_dir).map_err(|e| e.to_string())?;
        let dst = usb_maps.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&path, &dst).map_err(|e| e.to_string())?;
        io.note_file_bytes(
            app,
            "Copying satellite map archives (expedition)…",
            0.88,
            len,
        );
        used = used.saturating_add(len);
    }
    Ok(())
}

fn export_expedition_to_usb(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    dest_root: &Path,
    app_data_dir: &Path,
    cfg: &AppConfig,
    emb_name: &str,
    rr_name: &str,
    emb_src: &Path,
    rr_src: &Path,
    budget_bytes: u64,
    io: &mut ExportIoReporter,
) -> Result<(), String> {
    let usb_data = dest_root.join("data");
    fs::create_dir_all(&usb_data).map_err(|e| e.to_string())?;
    let _ = crate::athena_paths::ensure_layout(&usb_data);
    copy_host_app_tree_to_usb(app_data_dir, &usb_data)?;
    let models_root = usb_data.join("user").join("models");
    let emb_dst = models_root.join("embeddings").join(emb_name);
    let rr_dst = models_root.join("rerankers").join(rr_name);
    if let Some(p) = emb_dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    if let Some(p) = rr_dst.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }

    let lz = bundled_payload_launchers_bytes(app).bytes;
    let rem0 = expedition_remaining_bytes(budget_bytes, io);
    if lz > rem0 {
        return Err(format!(
            "Expedition budget is too small for bundled launchers (need about {lz} B, {rem0} B remaining after the application copy)."
        ));
    }
    io.emit_phase(app, "Copying Windows/Linux launchers…", 0.30);
    copy_bundled_payload_branches_to_usb(app, dest_root, io, 0.30, 0.42)?;
    io.emit_phase(app, "Payload stage complete.", 0.42);

    let emb_sz = tree_file_bytes(emb_src).map_err(|e| {
        format!(
            "Failed to measure embedding folder {} for expedition sizing: {e}",
            emb_src.display()
        )
    })?;
    let rr_sz = tree_file_bytes(rr_src).map_err(|e| {
        format!(
            "Failed to measure reranker folder {} for expedition sizing: {e}",
            rr_src.display()
        )
    })?;
    let models_total = emb_sz.saturating_add(rr_sz);
    if emb_src.is_dir()
        && rr_src.is_dir()
        && models_total > 0
        && models_total <= expedition_remaining_bytes(budget_bytes, io)
    {
        io.emit_phase(app, "Copying embedding model…", 0.54);
        let mt = models_total.max(1);
        let mut models_done = 0u64;
        copy_dir_recursive_impl(
            emb_src,
            &emb_dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Copying AI models to USB…",
                frac_lo: 0.54,
                frac_hi: 0.62,
                total: mt,
                done: &mut models_done,
            }),
            &never_skip_file,
        )?;
        copy_dir_recursive_impl(
            rr_src,
            &rr_dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Copying AI models to USB…",
                frac_lo: 0.54,
                frac_hi: 0.62,
                total: mt,
                done: &mut models_done,
            }),
            &never_skip_file,
        )?;
        io.emit_phase(app, "Models copied.", 0.62);
    } else if !emb_src.is_dir() || !rr_src.is_dir() {
        return Err(format!(
            "Embedding or reranker model folder missing (embedding: {}, reranker: {}).",
            emb_src.display(),
            rr_src.display()
        ));
    } else {
        io.emit_phase(
            app,
            "Skipping AI models (expedition budget — USB too small for bundled models).",
            0.58,
        );
    }

    let db_src = host_db_path(app_data_dir);
    if db_src.is_file() {
        let db_sz = fs::metadata(&db_src).map_err(|e| e.to_string())?.len();
        if db_sz <= expedition_remaining_bytes(budget_bytes, io) {
            copy_full_database_to_usb(app_data_dir, &usb_data, app, io, 0.52)?;
        } else {
            io.emit_phase(
                app,
                "Skipping full database (expedition budget — USB too small for the full index).",
                0.52,
            );
        }
    }

    if example_region_reserved_bytes() <= expedition_remaining_bytes(budget_bytes, io) {
        io.emit_phase(app, "Building proximity offline map…", 0.44);
        ensure_example_region_proximity_pmtiles(app, dest_root, app_data_dir, false)?;
        io.emit_phase(app, "Offline map ready.", 0.50);
    } else {
        io.emit_phase(
            app,
            "Skipping example region terrain extract (expedition budget).",
            0.48,
        );
    }

    let mns = offline_maps_non_satellite_bytes(app_data_dir)?;
    if mns > 0 && mns <= expedition_remaining_bytes(budget_bytes, io) {
        copy_offline_maps_to_usb(
            app_data_dir,
            &usb_data,
            app,
            io,
            true,
            0.64,
            0.68,
        )?;
    } else if mns > expedition_remaining_bytes(budget_bytes, io) && mns > 0 {
        io.emit_phase(
            app,
            "Skipping non-satellite offline maps (expedition budget).",
            0.66,
        );
    }

    io.emit_phase(app, "Copying library folders within expedition budget…", 0.72);
    let kiwix_str = kiwix_downloads_dir(app_data_dir)
        .to_string_lossy()
        .to_string();
    let folders = {
        let g = db.lock().map_err(|e| e.to_string())?;
        compute_watcher_folders(&*g, app_data_dir, cfg)?
    };
    let docs_root = dest_root.join("documents");
    fs::create_dir_all(&docs_root).map_err(|e| e.to_string())?;
    let mut used_names = std::collections::HashSet::<String>::new();
    let mut lib_i = 0usize;
    let n_lib = folders
        .iter()
        .filter(|f| *f != &kiwix_str)
        .filter(|f| Path::new(f).is_dir())
        .count();

    for folder in folders {
        if folder == kiwix_str {
            continue;
        }
        let p = Path::new(&folder);
        if !p.is_dir() {
            continue;
        }
        let folder_sz = tree_file_bytes_filtered(p, &skip_satellite_map_file)?;
        if folder_sz > expedition_remaining_bytes(budget_bytes, io) {
            continue;
        }
        let base = p
            .file_name()
            .and_then(|s| s.to_str())
            .map(sanitize_documents_subdir)
            .unwrap_or_else(|| "library".to_string());
        let mut name = base.clone();
        let mut n = 0u32;
        while used_names.contains(&name) {
            n += 1;
            name = format!("{base}_{n}");
        }
        used_names.insert(name.clone());
        let dst = docs_root.join(&name);
        lib_i += 1;
        let frac_lo = if n_lib == 0 {
            0.72_f32
        } else {
            0.72_f32 + ((lib_i - 1) as f32 / n_lib as f32) * (0.82_f32 - 0.72_f32)
        };
        let frac_hi = if n_lib == 0 {
            0.82_f32
        } else {
            0.72_f32 + (lib_i as f32 / n_lib as f32) * (0.82_f32 - 0.72_f32)
        };
        io.emit_phase(
            app,
            format!("Copying library folder ({lib_i})…"),
            frac_lo,
        );
        fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
        let ft = folder_sz.max(1);
        let mut folder_done = 0u64;
        copy_dir_recursive_impl(
            p,
            &dst,
            Some(CopyProgress {
                app,
                io,
                phase: "Copying library to USB…",
                frac_lo,
                frac_hi,
                total: ft,
                done: &mut folder_done,
            }),
            &skip_satellite_map_file,
        )?;
    }

    let cap = expedition_remaining_bytes(budget_bytes, io);
    tier2_copy_payload_priority_fill_within_budget(app, dest_root, io, cap)?;

    let usb_maps = usb_data.join("user").join("maps");
    fs::create_dir_all(&usb_maps).map_err(|e| e.to_string())?;
    copy_satellite_map_archives_greedy(app_data_dir, &usb_maps, app, io, budget_bytes)?;

    Ok(())
}

pub fn export_athena_to_usb(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Database>>,
    destination_path: String,
    export_type: String,
    wipe_after: bool,
    format_first: bool,
    device_path: Option<String>,
    expedition_budget_bytes: Option<u64>,
    selected_external_chat_model_path: Option<String>,
    include_docker_volumes: bool,
) -> Result<(), String> {
    clear_aegis_export_cancel();
    let export_result = (|| -> Result<(), String> {
    let mut io = ExportIoReporter::new();
    let mut dest_root = PathBuf::from(destination_path.trim());

    #[cfg(target_os = "macos")]
    if format_first {
        let dev = device_path
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "device_path is required when format_first is true (select a USB drive)."
                    .to_string()
            })?;
        io.emit_phase(app, "Formatting drive (ExFAT)…", 0.01);
        dest_root = crate::usb::macos_erase_disk_exfat_and_resolve_mount(dev)?;
    }
    #[cfg(not(target_os = "macos"))]
    if format_first {
        drop(device_path);
        return Err("Drive formatting is only supported on macOS.".to_string());
    }

    if dest_root.as_os_str().is_empty() {
        return Err("Destination path is empty.".to_string());
    }
    if !dest_root.is_dir() {
        return Err("Destination is not a directory.".to_string());
    }

    io.emit_phase(app, "Preparing USB folders…", 0.04);
    let usb_data_prep = dest_root.join("data");
    fs::create_dir_all(&usb_data_prep).map_err(|e| e.to_string())?;
    let _ = crate::athena_paths::ensure_layout(&usb_data_prep);
    fs::create_dir_all(dest_root.join("documents")).map_err(|e| e.to_string())?;

    eprintln!(
        "[athena] usb export: begin export_type={} dest={} wipe_after={} format_first={}",
        export_type.trim(),
        dest_root.display(),
        wipe_after,
        format_first
    );
    let probe = dest_root.join(".athena_usb_export_write_probe");
    fs::write(&probe, b"ok").map_err(|e| {
        format!(
            "Cannot write to USB root {} (permission denied, read-only volume, or sandbox — check Full Disk Access / mount): {e}",
            dest_root.display()
        )
    })?;
    if let Err(e) = fs::remove_file(&probe) {
        return Err(format!(
            "Failed to remove USB write probe file {}: {e}",
            probe.display()
        ));
    }

    let parsed = parse_usb_export_type(&export_type, expedition_budget_bytes)?;
    if wipe_after {
        match &parsed {
            ParsedUsbExport::Manual(AegisManualTier::Citadel) => {}
            _ => {
                return Err(
                    "Scorched Earth (wipe_after) is only supported for citadel export.".to_string(),
                );
            }
        }
    }
    if include_docker_volumes {
        match &parsed {
            ParsedUsbExport::Manual(AegisManualTier::Citadel) => {}
            _ => {
                return Err(
                    "Include Docker volumes is only supported for Sister Server (citadel) export."
                        .to_string(),
                );
            }
        }
    }

    let app_data_dir = resolved_app_data_dir(app)?;
    let cfg = load_config(&app_data_dir);
    let emb_name = effective_embedding_folder(&cfg);
    let rr_name = effective_reranker_folder(&cfg);
    let host = AthenaPaths::new(&app_data_dir);
    let emb_src = resolve_model_pack_dir(&host, "embeddings", &emb_name);
    let rr_src = resolve_model_pack_dir(&host, "rerankers", &rr_name);
    let chat_src = chat_models_dir(&app_data_dir);

    let strip_non_launcher_payload_packs = matches!(
        &parsed,
        ParsedUsbExport::Manual(AegisManualTier::Outpost)
    );
    eprintln!(
        "[athena] usb export ({parsed:?}): copying application to {}",
        dest_root.display()
    );
    copy_application_to_destination(
        app,
        &dest_root,
        &mut io,
        strip_non_launcher_payload_packs,
    )?;
    write_portable_marker_for_export(&dest_root)?;

    let usb_data = dest_root.join("data");
    fs::create_dir_all(&usb_data).map_err(|e| e.to_string())?;
    let _ = crate::athena_paths::ensure_layout(&usb_data);
    io.emit_phase(app, "Copying AthenaData/app (bundled defaults)…", 0.28);
    copy_host_app_tree_to_usb(&app_data_dir, &usb_data)?;
    // Inject a minimal default config for Hive Mind behavior.
    // Fast Node: join a seed/coordinator; Sister Server: broadcast resources.
    let mut injected_cfg = AppConfig::default();
    injected_cfg.hive_mind_seed_url = Some("http://athena-hivemind.local:5055".to_string());
    injected_cfg.hive_mind_broadcast_resources = Some(matches!(&parsed, ParsedUsbExport::Manual(AegisManualTier::Citadel)));
    save_config(&usb_data, &injected_cfg)?;

    match &parsed {
        ParsedUsbExport::Manual(AegisManualTier::Outpost) => {
            io.emit_phase(app, "Copying Windows/Linux launchers…", 0.30);
            copy_bundled_payload_branches_to_usb(app, &dest_root, &mut io, 0.30, 0.42)?;
            io.emit_phase(app, "Payload stage complete.", 0.42);
            io.emit_phase(app, "Building minimal vector basemap…", 0.44);
            ensure_example_region_proximity_pmtiles(app, &dest_root, &app_data_dir, true)?;
            io.emit_phase(app, "Basemap ready.", 0.50);

            io.emit_phase(app, "Copying one small chat model…", 0.52);
            let picked: Option<(PathBuf, u64)> = if let Some(ref p) = selected_external_chat_model_path {
                let src = PathBuf::from(p.trim());
                if !src.is_file() {
                    return Err(format!("Selected model path is not a file: {}", src.display()));
                }
                let name_ok = src
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase().ends_with(".gguf"))
                    .unwrap_or(false);
                if !name_ok {
                    return Err("Selected model must be a .gguf file.".to_string());
                }
                let len = fs::metadata(&src).map_err(|e| e.to_string())?.len();
                Some((src, len))
            } else {
                pick_smallest_chat_model(&chat_src)?
            };
            let Some((small_chat, small_len)) = picked else {
                return Err(format!(
                    "No local chat models found in {}. Download at least one `.gguf` model or select one from the device scanner.",
                    chat_src.display()
                ));
            };
            copy_single_chat_model_to_usb(&small_chat, small_len, &usb_data, app, &mut io, 0.54)?;
            io.emit_phase(app, "Chat model copied.", 0.56);

            io.emit_phase(app, "Copying local index database…", 0.58);
            let db_dst = usb_data.join("user").join("db").join("athena_index.db");
            copy_slim_index_database_to_usb(db, &db_dst, app, &mut io)?;
            io.emit_phase(app, "Index database stage complete.", 0.64);
        }
        ParsedUsbExport::Manual(AegisManualTier::Bastion) => {
            io.emit_phase(app, "Copying Windows/Linux launchers…", 0.30);
            copy_bundled_payload_branches_to_usb(app, &dest_root, &mut io, 0.30, 0.42)?;
            io.emit_phase(app, "Payload stage complete.", 0.42);
            io.emit_phase(app, "Building proximity offline map…", 0.44);
            ensure_example_region_proximity_pmtiles(app, &dest_root, &app_data_dir, false)?;
            io.emit_phase(app, "Offline map ready.", 0.50);

            if !emb_src.is_dir() {
                return Err(format!(
                    "Embedding model folder not found: {}",
                    emb_src.display()
                ));
            }
            if !rr_src.is_dir() {
                return Err(format!(
                    "Reranker model folder not found: {}",
                    rr_src.display()
                ));
            }

            let models_root = usb_data.join("user").join("models");
            let emb_dst = models_root.join("embeddings").join(&emb_name);
            let rr_dst = models_root.join("rerankers").join(&rr_name);
            if let Some(p) = emb_dst.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            if let Some(p) = rr_dst.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }

            io.emit_phase(app, "Copying embedding model…", 0.52);
            let emb_sz = tree_file_bytes(&emb_src)?;
            let rr_sz = tree_file_bytes(&rr_src)?;
            let models_total = emb_sz.saturating_add(rr_sz).max(1);
            let mut models_done = 0u64;
            copy_dir_recursive_impl(
                &emb_src,
                &emb_dst,
                Some(CopyProgress {
                    app,
                    io: &mut io,
                    phase: "Copying AI models to USB…",
                    frac_lo: 0.52,
                    frac_hi: 0.58,
                    total: models_total,
                    done: &mut models_done,
                }),
                &never_skip_file,
            )?;
            copy_dir_recursive_impl(
                &rr_src,
                &rr_dst,
                Some(CopyProgress {
                    app,
                    io: &mut io,
                    phase: "Copying AI models to USB…",
                    frac_lo: 0.52,
                    frac_hi: 0.58,
                    total: models_total,
                    done: &mut models_done,
                }),
                &never_skip_file,
            )?;
            io.emit_phase(app, "Embedding & reranker copied.", 0.58);

            io.emit_phase(app, "Copying chat models…", 0.585);
            copy_chat_models_dir_to_usb(&chat_src, &usb_data, app, &mut io, 0.585, 0.605)?;
            io.emit_phase(app, "Chat models copied.", 0.61);

            copy_full_database_to_usb(&app_data_dir, &usb_data, app, &mut io, 0.62)?;

            io.emit_phase(app, "Filling priority payload packs…", 0.64);
            tier2_copy_payload_priority_fill(app, &dest_root, &mut io)?;
            io.emit_phase(app, "Priority payload stage complete.", 0.70);

            copy_offline_maps_to_usb(
                &app_data_dir,
                &usb_data,
                app,
                &mut io,
                true,
                0.72,
                0.78,
            )?;
            copy_watcher_documents_to_usb_filtered(
                app,
                db,
                &app_data_dir,
                &cfg,
                &dest_root,
                &mut io,
                true,
            )?;
        }
        ParsedUsbExport::Manual(AegisManualTier::Citadel) => {
            let db_src = host_db_path(&app_data_dir);
            if wipe_after && !db_src.is_file() {
                return Err(
                    "Citadel export with scorched earth requires a local athena_index.db.".to_string(),
                );
            }

            copy_full_payload_to_usb_root(app, &dest_root, &mut io)?;
            io.emit_phase(app, "Building proximity offline map…", 0.44);
            ensure_example_region_proximity_pmtiles(app, &dest_root, &app_data_dir, false)?;
            io.emit_phase(app, "Offline map ready.", 0.50);

            if !emb_src.is_dir() {
                return Err(format!(
                    "Embedding model folder not found: {}",
                    emb_src.display()
                ));
            }
            if !rr_src.is_dir() {
                return Err(format!(
                    "Reranker model folder not found: {}",
                    rr_src.display()
                ));
            }

            let models_root = usb_data.join("user").join("models");
            let emb_dst = models_root.join("embeddings").join(&emb_name);
            let rr_dst = models_root.join("rerankers").join(&rr_name);
            if let Some(p) = emb_dst.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            if let Some(p) = rr_dst.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }

            io.emit_phase(app, "Copying embedding model…", 0.52);
            let emb_sz = tree_file_bytes(&emb_src)?;
            let rr_sz = tree_file_bytes(&rr_src)?;
            let models_total = emb_sz.saturating_add(rr_sz).max(1);
            let mut models_done = 0u64;
            copy_dir_recursive_impl(
                &emb_src,
                &emb_dst,
                Some(CopyProgress {
                    app,
                    io: &mut io,
                    phase: "Copying AI models to USB…",
                    frac_lo: 0.52,
                    frac_hi: 0.58,
                    total: models_total,
                    done: &mut models_done,
                }),
                &never_skip_file,
            )?;
            copy_dir_recursive_impl(
                &rr_src,
                &rr_dst,
                Some(CopyProgress {
                    app,
                    io: &mut io,
                    phase: "Copying AI models to USB…",
                    frac_lo: 0.52,
                    frac_hi: 0.58,
                    total: models_total,
                    done: &mut models_done,
                }),
                &never_skip_file,
            )?;
            io.emit_phase(app, "Embedding & reranker copied.", 0.58);

            io.emit_phase(app, "Copying chat models…", 0.585);
            copy_chat_models_dir_to_usb(&chat_src, &usb_data, app, &mut io, 0.585, 0.605)?;
            io.emit_phase(app, "Chat models copied.", 0.61);

            copy_full_database_to_usb(&app_data_dir, &usb_data, app, &mut io, 0.62)?;

            copy_offline_maps_to_usb(
                &app_data_dir,
                &usb_data,
                app,
                &mut io,
                false,
                0.64,
                0.72,
            )?;
            copy_watcher_documents_to_usb_filtered(
                app,
                db,
                &app_data_dir,
                &cfg,
                &dest_root,
                &mut io,
                false,
            )?;
            if include_docker_volumes {
                io.emit_phase(app, "Exporting Docker Marketplace volumes…", 0.86);
                crate::marketplace::copy_volumes_to_usb(&dest_root, &app_data_dir)?;
                io.emit_phase(app, "Docker volumes exported.", 0.88);
            }
        }
        ParsedUsbExport::Expedition { budget_bytes } => {
            export_expedition_to_usb(
                app,
                db,
                &dest_root,
                &app_data_dir,
                &cfg,
                &emb_name,
                &rr_name,
                &emb_src,
                &rr_src,
                *budget_bytes,
                &mut io,
            )?;
        }
    }

    io.emit_phase(app, "Writing USB instructions…", 0.90);
    write_usb_readme(&dest_root)?;
    io.emit_phase(app, "Readme written.", 0.92);

    let kiwix_str = kiwix_downloads_dir(&app_data_dir)
        .to_string_lossy()
        .to_string();

    let et_label = export_type_label_for_verify(&parsed).to_string();
    io.emit_phase(app, "Verifying export…", 0.93);
    verify_usb_export(&dest_root, &et_label, &app_data_dir, &emb_name, &rr_name)?;
    if include_docker_volumes {
        crate::marketplace::verify_usb_volumes(&dest_root)?;
    }

    if wipe_after {
        io.emit_phase(app, "Scorched Earth: erasing local data…", 0.96);
        execute_scorched_wipe(db, &app_data_dir, &cfg, &kiwix_str)?;
        io.emit_phase(app, "Local wipe finished.", 0.99);
    }

    eprintln!("[athena] usb export: finished");
    io.emit_phase(app, "Export complete.", 1.0);
    Ok(())
    })();
    if let Err(ref e) = export_result {
        aegis_export_emit_error(app, e.clone());
    }
    clear_aegis_export_cancel();
    export_result
}

#[cfg(test)]
mod example_region_tests {
    use super::approx_bbox_for_radius_miles;

    #[test]
    fn example_region_25mi_bbox_contains_center() {
        let center_lon = -98.579534_f64;
        let center_lat = 39.828175_f64;
        let (min_lon, min_lat, max_lon, max_lat) =
            approx_bbox_for_radius_miles(center_lon, center_lat, 25.0_f64);
        assert!(center_lon >= min_lon && center_lon <= max_lon, "lon range");
        assert!(center_lat >= min_lat && center_lat <= max_lat, "lat range");
        assert!(max_lon > min_lon && max_lat > min_lat);
    }
}

#[cfg(test)]
mod slim_export_tests {
    use super::strip_heavy_library_tables_from_usb_db;
    use rusqlite::Connection;

    #[test]
    fn strip_usb_db_clears_library_tables_keeps_personnel_and_chats() {
        let conn = Connection::open_in_memory().unwrap();
        conn
            .execute_batch(
                r#"
            CREATE TABLE personnel (id TEXT PRIMARY KEY);
            INSERT INTO personnel VALUES ('p1');
            CREATE TABLE local_chats (
              id TEXT PRIMARY KEY NOT NULL,
              user_id TEXT NOT NULL,
              title TEXT NOT NULL,
              description TEXT,
              messages_json TEXT NOT NULL,
              context_summary TEXT,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );
            INSERT INTO local_chats VALUES ('c1', 'p1', 't', NULL, '[]', NULL, 0, 0);
            CREATE TABLE indexed_files (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, file_type TEXT NOT NULL, name TEXT NOT NULL, created_at INTEGER NOT NULL);
            INSERT INTO indexed_files VALUES (1, '/doc.pdf', 'pdf', 'doc', 0);
            CREATE TABLE chunk_embeddings (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              file_path TEXT NOT NULL,
              chunk_index INTEGER NOT NULL,
              chunk_layer TEXT,
              parent_id INTEGER REFERENCES chunk_embeddings(id) ON DELETE CASCADE,
              page_start INTEGER,
              page_end INTEGER,
              chunk_text TEXT NOT NULL,
              retrieval_text TEXT,
              vector BLOB NOT NULL,
              created_at INTEGER NOT NULL
            );
            INSERT INTO chunk_embeddings VALUES (1, '/doc.pdf', 0, NULL, NULL, NULL, NULL, 'x', 'x', X'00', 0);
            "#,
            )
            .unwrap();
        strip_heavy_library_tables_from_usb_db(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM personnel", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM local_chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunk_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}

#[cfg(test)]
mod athena_data_portable_tests {
    use super::{
        discover_portable_root_for_exe, resolve_athena_root_from_portable_athena_marker,
        validate_athena_data_root, PORTABLE_ATHENA_MARKER_FILENAME,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn marker_empty_file_uses_parent_as_root() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join(PORTABLE_ATHENA_MARKER_FILENAME);
        fs::write(&marker, "  \n  \n").unwrap();
        let root = resolve_athena_root_from_portable_athena_marker(&marker).unwrap();
        assert_eq!(root, fs::canonicalize(dir.path()).unwrap());
    }

    #[test]
    fn marker_first_line_absolute_path() {
        let dir = tempdir().unwrap();
        let athena = dir.path().join("AthenaSSD");
        fs::create_dir_all(&athena).unwrap();
        let marker = dir.path().join(PORTABLE_ATHENA_MARKER_FILENAME);
        let abs = fs::canonicalize(&athena).unwrap();
        fs::write(&marker, format!("{}\n", abs.display())).unwrap();
        let root = resolve_athena_root_from_portable_athena_marker(&marker).unwrap();
        assert_eq!(root, abs);
    }

    #[test]
    fn marker_with_utf8_byte_order_mark() {
        let dir = tempdir().unwrap();
        let athena = dir.path().join("AthenaData");
        fs::create_dir_all(&athena).unwrap();
        let marker = dir.path().join(PORTABLE_ATHENA_MARKER_FILENAME);
        let abs = fs::canonicalize(&athena).unwrap();
        fs::write(&marker, format!("\u{feff}{}\r\n", abs.display())).unwrap();
        let root = resolve_athena_root_from_portable_athena_marker(&marker).unwrap();
        assert_eq!(root, abs);
    }

    #[test]
    fn marker_relative_path_resolved_from_marker_parent() {
        let dir = tempdir().unwrap();
        let athena = dir.path().join("edata");
        fs::create_dir_all(&athena).unwrap();
        let marker = dir.path().join(PORTABLE_ATHENA_MARKER_FILENAME);
        fs::write(&marker, "edata\n").unwrap();
        let root = resolve_athena_root_from_portable_athena_marker(&marker).unwrap();
        assert_eq!(root, fs::canonicalize(&athena).unwrap());
    }

    #[test]
    fn discover_prefers_portable_athena_over_legacy_data_dir() {
        let dir = tempdir().unwrap();
        let legacy_data = dir.path().join("data");
        fs::create_dir_all(&legacy_data).unwrap();
        let marker = dir.path().join(PORTABLE_ATHENA_MARKER_FILENAME);
        let wanted = dir.path().join("wanted_root");
        fs::create_dir_all(&wanted).unwrap();
        fs::write(
            &marker,
            format!("{}\n", fs::canonicalize(&wanted).unwrap().display()),
        )
        .unwrap();
        let fake_exe = dir.path().join("athena.bin");
        fs::write(&fake_exe, []).unwrap();
        let root = discover_portable_root_for_exe(&fake_exe).unwrap().unwrap();
        assert_eq!(root, fs::canonicalize(&wanted).unwrap());
    }

    #[test]
    fn marker_points_to_missing_path_errors() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join(PORTABLE_ATHENA_MARKER_FILENAME);
        fs::write(&marker, "/nonexistent/athena/root/00000000\n").unwrap();
        let fake_exe = dir.path().join("athena.bin");
        fs::write(&fake_exe, []).unwrap();
        let err = discover_portable_root_for_exe(&fake_exe).unwrap_err();
        assert!(
            err.contains("unreachable") || err.contains("drive disconnected"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn validate_rejects_missing_root() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(validate_athena_data_root(&missing).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn validate_rejects_non_writable_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let ro = dir.path().join("readonly_root");
        fs::create_dir_all(&ro).unwrap();
        let mut perms = fs::metadata(&ro).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(&ro, perms).unwrap();
        let r = validate_athena_data_root(&ro);
        let mut perms = fs::metadata(&ro).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ro, perms).unwrap();
        assert!(r.is_err(), "expected write probe to fail: {r:?}");
    }
}

