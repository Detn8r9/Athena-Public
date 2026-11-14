//! The knowledge vault: collecting, storing and sharing the library.
//!
//! - [`zimwrite`] and [`packs`]: pack library files into compressed ZIM archives that stay
//!   readable in place, with chat search switched on per pack when there is space for its index.
//! - [`coverage`]: which survival topics the library covers and where it is thin.
//! - [`scan`]: find useful documents already on this computer.
//! - [`mesh_library`]: ask other stations on the local network for files on a topic.
//! - [`updates`]: newer releases of installed archives.
//! - [`alerts`]: official warning feeds, graded, and the hunker-down plan.
//! - [`fleet`]: sealed messages between stations of one group through a relay on the internet.

pub mod alerts;
pub mod coverage;
pub mod fleet;
pub mod ha;
pub mod mesh_library;
pub mod packs;
pub mod scan;
pub mod updates;
pub mod zimwrite;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth::AuthSession;
use crate::db::Database;
use crate::field_ops::store;
use crate::field_ops::{require_member, trust};
use crate::AppState;

const SETTINGS_KIND: &str = "vault_settings";
const SETTINGS_ID: &str = "vault_settings";
const UPDATES_KIND: &str = "vault_updates";
const UPDATES_ID: &str = "vault_updates";
const ANSWERS_KIND: &str = "vault_fleet_answers";
const ANSWERS_ID: &str = "vault_fleet_answers";
pub const PROGRESS_EVENT: &str = "athena:vault-progress";
/// Folder inside the library where packs are written.
const PACKS_DIR: &str = "Packs";
/// Folder inside the library for files copied from other stations.
const FROM_STATIONS_SHELF: &str = "From other stations";

static STOP_JOB: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultSettings {
    /// A second folder that holds packs, such as a large external drive.
    pub archive_dir: Option<String>,
    /// Copy useful finds into the library as soon as a scan finds them.
    pub auto_import_scan: bool,
    /// Answer other stations' questions about this library.
    pub mesh_share: bool,
    /// Download newer releases of installed archives without asking.
    pub auto_update: bool,
    pub alerts_enabled: bool,
    /// Two-letter US state code for weather warnings. Empty: none are requested.
    pub alert_region: String,
    /// Station position, used on this computer only to work out distance to an event.
    pub alert_lat: Option<f64>,
    pub alert_lon: Option<f64>,
    pub fleet_enabled: bool,
    pub fleet_relay_url: String,
    /// US air traffic status from the FAA (ground stops, delay programs, closures).
    pub faa_enabled: bool,
    /// Airport codes to watch closely, such as `RAP, DEN`.
    pub faa_airports: String,
    /// News feeds (RSS or Atom). A headline counts only when it contains a watch word.
    pub news_feeds: Vec<String>,
    pub watch_words: Vec<String>,
    /// Home Assistant link. The token never leaves this station.
    pub ha: ha::HaSettings,
}

pub fn default_news_feeds() -> Vec<String> {
    vec!["https://feeds.npr.org/1001/rss.xml".to_string(), "https://feeds.bbci.co.uk/news/world/us_and_canada/rss.xml".to_string()]
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            archive_dir: None,
            auto_import_scan: true,
            mesh_share: true,
            auto_update: false,
            alerts_enabled: false,
            alert_region: String::new(),
            alert_lat: None,
            alert_lon: None,
            fleet_enabled: false,
            fleet_relay_url: String::new(),
            faa_enabled: true,
            faa_airports: String::new(),
            news_feeds: default_news_feeds(),
            watch_words: alerts::default_watch_words(),
            ha: ha::HaSettings::default(),
        }
    }
}

impl VaultSettings {
    fn station_position(&self) -> Option<(f64, f64)> {
        self.alert_lat.zip(self.alert_lon).filter(|(lat, lon)| lat.abs() <= 90.0 && lon.abs() <= 180.0)
    }

    fn feed_options(&self) -> alerts::FeedOptions {
        alerts::FeedOptions {
            station: self.station_position(),
            region: self.alert_region.clone(),
            faa: self.faa_enabled,
            airports: alerts::clean_airports(&self.faa_airports),
            news_feeds: self.news_feeds.clone(),
            watch_words: self.watch_words.clone(),
        }
    }

    /// The settings as a browser sees them: the Home Assistant token is replaced by a flag.
    fn for_display(mut self) -> Self {
        self.ha.token_set = !self.ha.token.is_empty();
        self.ha.token = String::new();
        self
    }
}

pub fn load_settings(db: &Database) -> VaultSettings {
    store::get::<VaultSettings>(db, SETTINGS_KIND, SETTINGS_ID).ok().flatten().map(|r| r.data).unwrap_or_default()
}

fn load_one<T: serde::de::DeserializeOwned + Default>(db: &Database, kind: &str, id: &str) -> T {
    store::get::<T>(db, kind, id).ok().flatten().map(|r| r.data).unwrap_or_default()
}

fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    crate::portable::resolved_app_data_dir(app)
}

fn library_dir(root: &Path) -> PathBuf {
    crate::athena_paths::AthenaPaths::new(root).user_library_dir()
}

fn shared_db(app: &AppHandle) -> Arc<Mutex<Database>> {
    app.state::<AppState>().db.clone()
}

/// Free bytes on the drive that holds `path`.
pub fn free_bytes(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

fn emit_progress(app: &AppHandle, job: &str, done: usize, total: usize, label: &str) {
    let _ = app.emit(PROGRESS_EVENT, serde_json::json!({ "job": job, "done": done, "total": total, "label": label }));
}

fn station_name(db: &Database) -> String {
    trust::load_settings(db)
        .station_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("Station {}", crate::lan_identity::lan_local_node_id()))
}

// ---- packs ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackView {
    pub file_name: String,
    pub path: String,
    pub title: String,
    pub created_at: i64,
    pub doc_count: usize,
    pub packed_bytes: u64,
    pub original_bytes: u64,
    pub text_bytes: u64,
    /// Disk space chat search over this pack needs.
    pub index_bytes_estimate: u64,
    /// Documents of this pack that chat can search now.
    pub indexed_docs: u32,
    /// True when the pack sits in the second (archive) folder.
    pub on_archive_drive: bool,
    /// Topic key -> number of documents.
    pub topics: BTreeMap<String, u32>,
    /// Set when the file could not be read as a pack.
    pub problem: Option<String>,
}

fn pack_dirs(root: &Path, settings: &VaultSettings) -> Vec<(PathBuf, bool)> {
    let mut dirs = vec![(library_dir(root).join(PACKS_DIR), false)];
    if let Some(extra) = settings.archive_dir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        dirs.push((PathBuf::from(extra), true));
    }
    dirs
}

fn find_pack(root: &Path, settings: &VaultSettings, file_name: &str) -> Result<PathBuf, String> {
    if file_name.contains('/') || file_name.contains('\\') || !packs::is_pack_file(Path::new(file_name)) {
        return Err("That is not a pack.".to_string());
    }
    pack_dirs(root, settings)
        .into_iter()
        .map(|(dir, _)| dir.join(file_name))
        .find(|p| p.is_file())
        .ok_or_else(|| "That pack is not on a connected drive.".to_string())
}

fn list_packs(db: &Database, root: &Path, settings: &VaultSettings) -> Vec<PackView> {
    let mut out = Vec::new();
    for (dir, on_archive_drive) in pack_dirs(root, settings) {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !packs::is_pack_file(&path) {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let packed_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let indexed_docs = packs::indexed_doc_count(db, &file_name);
            let view = match packs::read_manifest(&path) {
                Ok(m) => {
                    let mut topics = BTreeMap::new();
                    for d in &m.docs {
                        *topics.entry(d.topic.clone().unwrap_or_else(|| "other".to_string())).or_insert(0u32) += 1;
                    }
                    PackView {
                        file_name,
                        path: path.to_string_lossy().into_owned(),
                        title: m.title,
                        created_at: m.created_at,
                        doc_count: m.docs.len(),
                        packed_bytes,
                        original_bytes: m.original_bytes,
                        text_bytes: m.text_bytes,
                        index_bytes_estimate: packs::estimate_index_bytes(m.text_bytes),
                        indexed_docs,
                        on_archive_drive,
                        topics,
                        problem: None,
                    }
                }
                Err(e) => PackView {
                    title: file_name.clone(),
                    file_name,
                    path: path.to_string_lossy().into_owned(),
                    created_at: 0,
                    doc_count: 0,
                    packed_bytes,
                    original_bytes: 0,
                    text_bytes: 0,
                    index_bytes_estimate: 0,
                    indexed_docs,
                    on_archive_drive,
                    topics: BTreeMap::new(),
                    problem: Some(e),
                },
            };
            out.push(view);
        }
    }
    out.sort_by(|a, b| a.title.cmp(&b.title).then(a.file_name.cmp(&b.file_name)));
    out
}

// ---- overview ------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedUpdates {
    checked_at: i64,
    updates: Vec<CachedUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedUpdate {
    installed_file: String,
    installed_path: String,
    installed_date: String,
    latest_file: String,
    latest_date: String,
    url: String,
    size_text: String,
    size_bytes: Option<u64>,
}

impl From<updates::UpdateInfo> for CachedUpdate {
    fn from(u: updates::UpdateInfo) -> Self {
        Self {
            installed_file: u.installed_file,
            installed_path: u.installed_path,
            installed_date: u.installed_date,
            latest_file: u.latest_file,
            latest_date: u.latest_date,
            url: u.url,
            size_text: u.size_text,
            size_bytes: u.size_bytes,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FleetAnswers {
    /// Newest first: "North has: Title, Title" lines from other stations.
    answers: Vec<FleetAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FleetAnswer {
    station: String,
    query: String,
    titles: Vec<String>,
    at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetView {
    pub enabled: bool,
    pub has_group_key: bool,
    pub relay_url: String,
    pub last_sync: i64,
    pub last_error: Option<String>,
    pub stations: Vec<fleet::FleetStation>,
    /// Archives other stations hold that this one lacks: (file name, stations that have it).
    pub missing_here: Vec<(String, Vec<String>)>,
    answers: Vec<FleetAnswer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultOverview {
    pub settings: VaultSettings,
    pub station: String,
    pub coverage: Vec<coverage::TopicCoverage>,
    pub packs: Vec<PackView>,
    pub packs_dir: String,
    pub free_bytes: Option<u64>,
    pub archive_free_bytes: Option<u64>,
    pub scan_items: Vec<store::Record<scan::ScanItem>>,
    pub default_scan_roots: Vec<String>,
    updates_checked_at: i64,
    updates: Vec<CachedUpdate>,
    pub alerts: alerts::AlertState,
    pub hunker: alerts::HunkerState,
    pub hunker_phrase: &'static str,
    pub fleet: FleetView,
    pub peers: usize,
    pub ha_state: ha::HaState,
}

fn installed_archives(root: &Path) -> Vec<PathBuf> {
    let roots = crate::kiwix_serve::zim_discovery_roots(root);
    crate::kiwix::scan_kiwix_zim_files_multi(&roots)
        .unwrap_or_default()
        .into_iter()
        .map(|z| PathBuf::from(z.path))
        .collect()
}

fn fleet_view(db: &Database, root: &Path, settings: &VaultSettings) -> FleetView {
    let stations: Vec<fleet::FleetStation> = store::list::<fleet::FleetStation>(db, fleet::STATION_KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect();
    let own: Vec<String> = installed_archives(root)
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let cursor: fleet::FleetCursor = load_one(db, fleet::CURSOR_KIND, fleet::CURSOR_ID);
    FleetView {
        enabled: settings.fleet_enabled,
        has_group_key: trust::key_bytes(&trust::load_settings(db)).is_some(),
        relay_url: settings.fleet_relay_url.clone(),
        last_sync: cursor.last_sync,
        last_error: cursor.last_error,
        missing_here: fleet::missing_here(&own, &stations),
        stations,
        answers: load_one::<FleetAnswers>(db, ANSWERS_KIND, ANSWERS_ID).answers,
    }
}

fn overview(db: &Database, root: &Path) -> VaultOverview {
    let settings = load_settings(db);
    let cached: CachedUpdates = load_one(db, UPDATES_KIND, UPDATES_ID);
    let mut scan_items = store::list::<scan::ScanItem>(db, scan::ITEM_KIND).unwrap_or_default();
    scan_items.sort_by(|a, b| {
        b.data.fills_blind_spot.cmp(&a.data.fills_blind_spot).then(b.data.score.cmp(&a.data.score)).then(a.data.name.cmp(&b.data.name))
    });
    VaultOverview {
        station: station_name(db),
        coverage: coverage::build_coverage(&coverage::library_items(db, root)),
        packs: list_packs(db, root, &settings),
        packs_dir: library_dir(root).join(PACKS_DIR).to_string_lossy().into_owned(),
        free_bytes: free_bytes(root),
        archive_free_bytes: settings.archive_dir.as_deref().filter(|s| !s.trim().is_empty()).and_then(|d| free_bytes(Path::new(d))),
        scan_items,
        default_scan_roots: scan::default_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        updates_checked_at: cached.checked_at,
        updates: cached.updates,
        alerts: load_one(db, alerts::STATE_KIND, alerts::STATE_ID),
        hunker: load_one(db, alerts::HUNKER_KIND, alerts::HUNKER_ID),
        hunker_phrase: alerts::HUNKER_CONFIRM_PHRASE,
        fleet: fleet_view(db, root, &settings),
        peers: crate::mesh_discovery::peer_registry().try_read().map(|r| r.len()).unwrap_or(0),
        ha_state: load_one(db, ha::STATE_KIND, ha::STATE_ID),
        settings: settings.for_display(),
    }
}

#[tauri::command]
pub async fn vault_get_overview(app: AppHandle, session: crate::auth::CallerSession) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn vault_save_settings(
    settings: VaultSettings,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<VaultSettings, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let mut s = settings;
    s.alert_region = alerts::clean_region(&s.alert_region).unwrap_or_default();
    s.fleet_relay_url = if s.fleet_relay_url.trim().is_empty() { String::new() } else { fleet::clean_relay_url(&s.fleet_relay_url)? };
    if s.fleet_enabled && s.fleet_relay_url.is_empty() {
        return Err("Enter the relay address before switching the fleet link on.".to_string());
    }
    if let Some(dir) = s.archive_dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        if !Path::new(dir).is_dir() {
            return Err("The archive folder does not exist. Connect the drive or pick another folder.".to_string());
        }
        s.archive_dir = Some(dir.to_string());
    } else {
        s.archive_dir = None;
    }
    if s.station_position().is_none() {
        s.alert_lat = None;
        s.alert_lon = None;
    }
    s.faa_airports = alerts::clean_airports(&s.faa_airports).join(", ");
    let mut feeds = Vec::new();
    for f in s.news_feeds.iter().map(|f| f.trim()).filter(|f| !f.is_empty()) {
        let clean = alerts::clean_feed_url(f).ok_or_else(|| format!("{f} is not a web address (http:// or https://)."))?;
        if !feeds.contains(&clean) {
            feeds.push(clean);
        }
    }
    feeds.truncate(20);
    s.news_feeds = feeds;
    let mut words: Vec<String> = s.watch_words.iter().map(|w| w.trim().to_lowercase()).filter(|w| w.chars().count() >= 3).collect();
    words.sort();
    words.dedup();
    words.truncate(100);
    s.watch_words = words;
    s.ha.url = ha::clean_url(&s.ha.url)?;
    s.ha.webhook_id = ha::clean_webhook_id(&s.ha.webhook_id)?;
    s.ha.entities = ha::clean_entities(&s.ha.entities);
    s.ha.token = s.ha.token.trim().to_string();
    if s.ha.token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The access token should be one line with no spaces.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    if s.ha.token.is_empty() {
        // A browser never sees the token, so an empty box keeps the saved one.
        s.ha.token = load_settings(&db).ha.token;
    }
    if s.ha.enabled && s.ha.url.is_empty() {
        return Err("Enter Home Assistant's address before switching the link on.".to_string());
    }
    store::put(&db, SETTINGS_KIND, Some(SETTINGS_ID), &s)?;
    Ok(s.for_display())
}

#[tauri::command]
pub fn vault_cancel_job(session: State<'_, AuthSession>) -> Result<(), String> {
    require_member(&session)?;
    STOP_JOB.store(true, Ordering::SeqCst);
    Ok(())
}

/// Pack a folder of documents into one compressed archive.
#[tauri::command]
pub async fn vault_create_pack(
    app: AppHandle,
    session: crate::auth::CallerSession,
    source_dir: String,
    title: String,
    to_archive_drive: Option<bool>,
) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let source = PathBuf::from(source_dir.trim());
    if !source.is_dir() {
        return Err("Choose a folder that exists.".to_string());
    }
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Give the pack a name.".to_string());
    }
    let db = shared_db(&app);
    STOP_JOB.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&*db.lock().map_err(|e| e.to_string())?);
        let dirs = pack_dirs(&root, &settings);
        let (dir, _) = match (to_archive_drive.unwrap_or(false), dirs.get(1)) {
            (true, Some(d)) => d.clone(),
            (true, None) => return Err("Set an archive folder first.".to_string()),
            _ => dirs[0].clone(),
        };
        if source.starts_with(&dir) {
            return Err("Choose a folder other than the packs folder.".to_string());
        }
        let out = dir.join(packs::pack_file_name(&title, &chrono::Utc::now().format("%Y-%m-%d").to_string()));
        if out.exists() {
            return Err("A pack with that name was already made today. Use another name.".to_string());
        }
        packs::build_pack(&source, &out, &title, 19, |done, total, label| emit_progress(&app, "pack", done, total, label))?;
        emit_progress(&app, "pack", 1, 1, "");
        crate::kiwix_serve::serve_new_zim(&app, &out);
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Switch chat search over a pack on (index it) or off (give the index space back).
#[tauri::command]
pub async fn vault_set_pack_search(
    app: AppHandle,
    session: crate::auth::CallerSession,
    file_name: String,
    enabled: bool,
) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    STOP_JOB.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&*db.lock().map_err(|e| e.to_string())?);
        if enabled {
            let pack = find_pack(&root, &settings, &file_name)?;
            let manifest = packs::read_manifest(&pack)?;
            let need = packs::estimate_index_bytes(manifest.text_bytes);
            if let Some(free) = free_bytes(&root) {
                // Keep a tenth of the need spare so the station does not fill its drive.
                if free < need + need / 10 {
                    return Err(format!(
                        "Chat search for this pack needs about {} of free space on the ATHENA data drive, and {} is free. Free up space or move ATHENA's data to a larger drive, then try again.",
                        human_bytes(need),
                        human_bytes(free)
                    ));
                }
            }
            packs::index_pack(
                &db,
                &pack,
                |done, total, label| emit_progress(&app, "index", done, total, label),
                || STOP_JOB.load(Ordering::SeqCst),
            )?;
            emit_progress(&app, "index", 1, 1, "");
        } else {
            packs::unindex_pack(&db, &file_name)?;
        }
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} bytes")
    } else if v >= 100.0 {
        format!("{v:.0} {}", UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// Check every document in a pack against the checksum recorded when it was packed.
#[tauri::command]
pub async fn vault_verify_pack(app: AppHandle, session: crate::auth::CallerSession, file_name: String) -> Result<usize, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&*db.lock().map_err(|e| e.to_string())?);
        packs::verify_pack(&find_pack(&root, &settings, &file_name)?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn vault_delete_pack(app: AppHandle, session: crate::auth::CallerSession, file_name: String) -> Result<VaultOverview, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&*db.lock().map_err(|e| e.to_string())?);
        let pack = find_pack(&root, &settings, &file_name)?;
        packs::unindex_pack(&db, &file_name)?;
        std::fs::remove_file(&pack).map_err(|e| e.to_string())?;
        crate::kiwix_serve::restart_after_library_change(&app);
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- device scan ---------------------------------------------------------------------------

fn library_names(dir: &Path) -> HashSet<(String, u64)> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| Some((e.file_name().to_string_lossy().into_owned(), e.metadata().ok()?.len())))
        .collect()
}

fn import_and_record(db: &Mutex<Database>, library: &Path, id: &str, mut item: scan::ScanItem) -> Result<(), String> {
    let dest = scan::import_item(library, &item)?;
    item.status = scan::ItemStatus::Imported;
    item.imported_to = Some(dest.to_string_lossy().into_owned());
    store::put(&*db.lock().map_err(|e| e.to_string())?, scan::ITEM_KIND, Some(id), &item)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub files_seen: u64,
    pub found: usize,
    pub imported: usize,
    pub waiting: usize,
    pub overview: VaultOverview,
}

/// Scan this computer for useful documents. With auto-import on, finds are copied into the
/// library straight away (large files wait for a choice); the list stays for review either way.
#[tauri::command]
pub async fn vault_scan_device(app: AppHandle, session: crate::auth::CallerSession, roots: Option<Vec<String>>) -> Result<ScanReport, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    STOP_JOB.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        let roots: Vec<PathBuf> = match roots {
            Some(r) if !r.is_empty() => r.iter().map(|s| PathBuf::from(s.trim())).filter(|p| p.is_dir()).collect(),
            _ => scan::default_roots(),
        };
        if roots.is_empty() {
            return Err("None of those folders exist on this computer.".to_string());
        }
        let library = library_dir(&root);
        let (settings, blind, known): (VaultSettings, Vec<coverage::Topic>, HashSet<String>) = {
            let guard = db.lock().map_err(|e| e.to_string())?;
            let cov = coverage::build_coverage(&coverage::library_items(&guard, &root));
            let known = store::list::<scan::ScanItem>(&guard, scan::ITEM_KIND)
                .unwrap_or_default()
                .into_iter()
                .map(|r| r.data.path)
                .collect();
            (load_settings(&guard), coverage::blind_spots(&cov), known)
        };
        let already = library_names(&library);
        let mut exclude = vec![root.clone()];
        if let Some(extra) = settings.archive_dir.as_deref().filter(|s| !s.trim().is_empty()) {
            exclude.push(PathBuf::from(extra));
        }
        let outcome = scan::scan(
            &roots,
            &exclude,
            &already,
            &blind,
            |seen| emit_progress(&app, "scan", seen as usize, 0, "Looking through files"),
            || STOP_JOB.load(Ordering::SeqCst),
        );
        let mut imported = 0;
        let mut waiting = 0;
        let mut found = 0;
        let total = outcome.items.len();
        for (i, item) in outcome.items.into_iter().enumerate() {
            // A file already listed keeps whatever was decided about it (imported or dismissed).
            if known.contains(&item.path) {
                continue;
            }
            found += 1;
            let id = store::new_id();
            if settings.auto_import_scan && !item.large {
                emit_progress(&app, "scan", i, total, &item.name);
                match import_and_record(&db, &library, &id, item.clone()) {
                    Ok(()) => {
                        imported += 1;
                        continue;
                    }
                    Err(e) => eprintln!("[athena] vault scan: {e}"),
                }
            }
            waiting += 1;
            store::put(&*db.lock().map_err(|e| e.to_string())?, scan::ITEM_KIND, Some(&id), &item)?;
        }
        emit_progress(&app, "scan", 1, 1, "");
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(ScanReport { files_seen: outcome.files_seen, found, imported, waiting, overview: overview(&guard, &root) })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Copy listed finds into the library (`import`), or drop them from the list (`dismiss`). A
/// dismissed find that was already copied is removed from the library again.
#[tauri::command]
pub async fn vault_resolve_scan_items(
    app: AppHandle,
    session: crate::auth::CallerSession,
    ids: Vec<String>,
    action: String,
) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let library = library_dir(&root);
        for id in &ids {
            let rec = store::get::<scan::ScanItem>(&*db.lock().map_err(|e| e.to_string())?, scan::ITEM_KIND, id)?;
            let Some(rec) = rec else { continue };
            match action.as_str() {
                "import" => import_and_record(&db, &library, id, rec.data)?,
                "dismiss" => {
                    let mut item = rec.data;
                    if let Some(dest) = item.imported_to.take() {
                        // Only ever remove ATHENA's own copy inside the library.
                        let dest = PathBuf::from(dest);
                        if dest.starts_with(&library) && dest.is_file() {
                            let _ = std::fs::remove_file(&dest);
                        }
                    }
                    item.status = scan::ItemStatus::Dismissed;
                    store::put(&*db.lock().map_err(|e| e.to_string())?, scan::ITEM_KIND, Some(id), &item)?;
                }
                _ => return Err("Unknown action.".to_string()),
            }
        }
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- other stations on the local network ---------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerAnswer {
    pub peer_url: String,
    pub station: String,
    pub hits: Vec<mesh_library::LibraryHit>,
    pub error: Option<String>,
}

/// Ask every station found on the local network whether it has anything on `query`.
#[tauri::command]
pub async fn vault_mesh_search(app: AppHandle, session: crate::auth::CallerSession, query: String) -> Result<Vec<PeerAnswer>, String> {
    require_member(&session)?;
    let query = query.trim().to_string();
    if mesh_library::query_terms(&query).is_empty() {
        return Err("Say what you are looking for, such as: alternator repair.".to_string());
    }
    let key = {
        let db = shared_db(&app);
        let guard = db.lock().map_err(|e| e.to_string())?;
        trust::key_bytes(&trust::load_settings(&guard))
    };
    let peers: Vec<crate::mesh_discovery::MeshDiscoveredPeer> =
        crate::mesh_discovery::peer_registry().read().await.values().cloned().collect();
    if peers.is_empty() {
        return Err("No other stations are on the local network right now.".to_string());
    }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(6)).build().map_err(|e| e.to_string())?;
    let ts = chrono::Utc::now().timestamp();
    let sig = key.as_deref().map(|k| mesh_library::sign_query(k, &query, ts)).unwrap_or_default();
    let mut out = Vec::new();
    for peer in peers {
        let base = peer.manifest_base_url.trim_end_matches('/').to_string();
        let lan_safe = reqwest::Url::parse(&base).map(|u| crate::mesh_sync_api::mesh_manifest_url_is_lan_safe(&u)).unwrap_or(false);
        if !lan_safe {
            continue;
        }
        let res = client
            .get(format!("{base}/mesh/library/search"))
            .query(&[("q", query.as_str()), ("ts", &ts.to_string()), ("sig", sig.as_str())])
            .send()
            .await;
        let answer = match res {
            Ok(r) if r.status().is_success() => match r.json::<mesh_library::SearchAnswer>().await {
                Ok(a) => PeerAnswer { peer_url: base, station: a.station, hits: a.hits, error: None },
                Err(e) => PeerAnswer { peer_url: base, station: format!("Station {}", peer.node_id), hits: Vec::new(), error: Some(e.to_string()) },
            },
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                PeerAnswer {
                    peer_url: base,
                    station: format!("Station {}", peer.node_id),
                    hits: Vec::new(),
                    error: Some(if status == reqwest::StatusCode::NOT_FOUND {
                        "This station runs an older ATHENA that cannot answer library questions.".to_string()
                    } else if text.is_empty() {
                        status.to_string()
                    } else {
                        text.chars().take(200).collect()
                    }),
                }
            }
            Err(e) => PeerAnswer { peer_url: base, station: format!("Station {}", peer.node_id), hits: Vec::new(), error: Some(format!("No answer: {e}")) },
        };
        out.push(answer);
    }
    Ok(out)
}

/// Copy one file another station offered into this library.
#[tauri::command]
pub async fn vault_mesh_fetch(
    app: AppHandle,
    session: crate::auth::CallerSession,
    peer_url: String,
    rel_path: String,
) -> Result<String, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    require_member(&session)?;
    let base = peer_url.trim().trim_end_matches('/').to_string();
    let known = crate::mesh_discovery::peer_registry()
        .read()
        .await
        .values()
        .any(|p| p.manifest_base_url.trim_end_matches('/') == base);
    let lan_safe = reqwest::Url::parse(&base).map(|u| crate::mesh_sync_api::mesh_manifest_url_is_lan_safe(&u)).unwrap_or(false);
    if !known || !lan_safe {
        return Err("That station is not on the local network any more.".to_string());
    }
    let name = rel_path.replace('\\', "/").rsplit('/').next().unwrap_or("").trim().to_string();
    if name.is_empty() || name.starts_with('.') || name.contains("..") {
        return Err("That file name is not valid.".to_string());
    }
    let root = data_root(&app)?;
    let dir = library_dir(&root).join(FROM_STATIONS_SHELF);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let dest = dir.join(&name);
    if dest.exists() {
        return Err("A file with that name was already copied from another station.".to_string());
    }
    let res = reqwest::Client::new()
        .get(format!("{base}/mesh/download"))
        .query(&[("p", rel_path.as_str())])
        .send()
        .await
        .map_err(|e| format!("Could not reach that station: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("That station would not send the file ({}).", res.status()));
    }
    if let (Some(len), Some(free)) = (res.content_length(), free_bytes(&root)) {
        if len + 512 * 1024 * 1024 > free {
            return Err(format!("The file is {} and only {} is free.", human_bytes(len), human_bytes(free)));
        }
    }
    let part = dir.join(format!("{name}.athena-part"));
    let mut file = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(format!("The copy stopped part way: {e}"));
            }
        };
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::rename(&part, &dest).await.map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}

// ---- updates -------------------------------------------------------------------------------

fn ensure_online(root: &Path) -> Result<(), String> {
    let config = crate::config::load_config(root);
    if !crate::connectivity::internet_allowed(&config) {
        return Err("This station is set to stay offline. Allow internet access in Settings to do this.".to_string());
    }
    Ok(())
}

fn check_updates_now(db: &Mutex<Database>, root: &Path) -> Result<(), String> {
    let found = updates::check_updates(&installed_archives(root))?;
    let cached = CachedUpdates { checked_at: chrono::Utc::now().timestamp(), updates: found.into_iter().map(Into::into).collect() };
    store::put(&*db.lock().map_err(|e| e.to_string())?, UPDATES_KIND, Some(UPDATES_ID), &cached)?;
    Ok(())
}

/// Look for newer releases of the installed archives.
#[tauri::command]
pub async fn vault_check_updates(app: AppHandle, session: crate::auth::CallerSession) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    ensure_online(&root)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        check_updates_now(&db, &root)?;
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// After a newer release has been downloaded: remove the older file once the new one opens.
#[tauri::command]
pub async fn vault_finish_update(
    app: AppHandle,
    session: crate::auth::CallerSession,
    installed_path: String,
    latest_file: String,
) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let old = PathBuf::from(&installed_path);
        let installed = installed_archives(&root);
        if !installed.iter().any(|p| p == &old) {
            return Err("That archive is not installed here.".to_string());
        }
        let new = installed
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == latest_file))
            .cloned()
            .ok_or_else(|| "The new release has not finished downloading yet.".to_string())?;
        updates::remove_superseded(&old, &new)?;
        {
            let guard = db.lock().map_err(|e| e.to_string())?;
            let _ = guard.delete_by_path_prefix(&installed_path);
            let mut cached: CachedUpdates = load_one(&guard, UPDATES_KIND, UPDATES_ID);
            cached.updates.retain(|u| u.installed_path != installed_path);
            store::put(&guard, UPDATES_KIND, Some(UPDATES_ID), &cached)?;
        }
        crate::kiwix_serve::restart_after_library_change(&app);
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSource {
    pub url: String,
    pub file_name: String,
    pub size_text: String,
    pub size_bytes: Option<u64>,
}

/// The current download for a suggested source (Kiwix renames files with every release).
#[tauri::command]
pub async fn vault_resolve_source(
    app: AppHandle,
    session: crate::auth::CallerSession,
    folder: String,
    base: String,
) -> Result<ResolvedSource, String> {
    require_member(&session)?;
    ensure_online(&data_root(&app)?)?;
    let safe = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !safe(&folder) || !safe(&base) {
        return Err("That source name is not valid.".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let latest = updates::resolve_latest(&folder, &base)?;
        Ok(ResolvedSource {
            url: format!("{}/{folder}/{}", updates::KIWIX_ZIM_ROOT, latest.name),
            size_bytes: updates::size_text_bytes(&latest.size_text),
            file_name: latest.name,
            size_text: latest.size_text,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- alerts and hunker-down ----------------------------------------------------------------

fn refresh_alerts_now(db: &Mutex<Database>) -> Result<(), String> {
    let settings = load_settings(&*db.lock().map_err(|e| e.to_string())?);
    let now = chrono::Utc::now().timestamp();
    let mut state = alerts::fetch_all(&settings.feed_options(), now);
    let guard = db.lock().map_err(|e| e.to_string())?;
    // Notices from the fleet are not in any feed; carry them over.
    let previous: alerts::AlertState = load_one(&guard, alerts::STATE_KIND, alerts::STATE_ID);
    state.alerts.extend(previous.alerts.into_iter().filter(|a| a.source == "fleet"));
    state.alerts = alerts::merge(state.alerts);
    store::put(&guard, alerts::STATE_KIND, Some(alerts::STATE_ID), &state)?;
    drop(guard);
    if settings.ha.enabled {
        ha_send_new_alerts(db, &settings, &state);
    }
    Ok(())
}

#[tauri::command]
pub async fn vault_refresh_alerts(app: AppHandle, session: crate::auth::CallerSession) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    ensure_online(&root)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        refresh_alerts_now(&db)?;
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn fleet_keys(db: &Database, settings: &VaultSettings) -> Result<(String, fleet::FleetKeys), String> {
    if !settings.fleet_enabled || settings.fleet_relay_url.is_empty() {
        return Err("The fleet link is switched off.".to_string());
    }
    let key = trust::key_bytes(&trust::load_settings(db))
        .ok_or_else(|| "Set a group key first (Field Ops, Other stations). The fleet link is sealed with it.".to_string())?;
    Ok((settings.fleet_relay_url.clone(), fleet::FleetKeys::from_group_key(&key)))
}

/// Tell the fleet, without failing the caller if the relay cannot be reached.
fn tell_fleet(db: &Arc<Mutex<Database>>, make: impl FnOnce(String, i64) -> fleet::FleetMessage) {
    let Ok(guard) = db.lock() else { return };
    let settings = load_settings(&guard);
    let Ok((relay, keys)) = fleet_keys(&guard, &settings) else { return };
    let message = make(station_name(&guard), chrono::Utc::now().timestamp());
    drop(guard);
    std::thread::spawn(move || {
        if let Err(e) = fleet::post(&relay, &keys, &message) {
            eprintln!("[athena] fleet: {e}");
        }
    });
}

/// Start hunker-down. A person with clearance 4 or higher types the phrase to confirm; nothing
/// starts it automatically. It silences the radio for a day, records who decided and why, and
/// tells the fleet so other operators can decide for their own stations.
#[tauri::command]
pub fn vault_hunker_down(
    confirm: String,
    reason: String,
    app: AppHandle,
    session: State<'_, AuthSession>,
) -> Result<VaultOverview, String> {
    let user = crate::auth::require_clearance4_or_higher(&session)?;
    if confirm.trim() != alerts::HUNKER_CONFIRM_PHRASE {
        return Err(format!("Type {} to confirm.", alerts::HUNKER_CONFIRM_PHRASE));
    }
    let reason = reason.trim().chars().take(300).collect::<String>();
    if reason.is_empty() {
        return Err("Say why, so everyone who sees this knows what happened.".to_string());
    }
    let root = data_root(&app)?;
    let db = shared_db(&app);
    {
        let guard = db.lock().map_err(|e| e.to_string())?;
        let state = alerts::HunkerState {
            active: true,
            started_at: chrono::Utc::now().timestamp(),
            started_by: user.display_name.clone().filter(|s| !s.trim().is_empty()).unwrap_or(user.name.clone()),
            reason: reason.clone(),
        };
        store::put(&guard, alerts::HUNKER_KIND, Some(alerts::HUNKER_ID), &state)?;
        crate::field_ops::radio_discipline::silence_for_minutes(&guard, 24 * 60)?;
    }
    tell_fleet(&db, |station, sent_at| fleet::FleetMessage::Hunker { station, sent_at, reason });
    let guard = db.lock().map_err(|e| e.to_string())?;
    Ok(overview(&guard, &root))
}

#[tauri::command]
pub fn vault_stand_down(app: AppHandle, session: State<'_, AuthSession>) -> Result<VaultOverview, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    {
        let guard = db.lock().map_err(|e| e.to_string())?;
        store::put(&guard, alerts::HUNKER_KIND, Some(alerts::HUNKER_ID), &alerts::HunkerState::default())?;
        crate::field_ops::radio_discipline::silence_for_minutes(&guard, 0)?;
    }
    tell_fleet(&db, |station, sent_at| fleet::FleetMessage::StandDown { station, sent_at });
    let guard = db.lock().map_err(|e| e.to_string())?;
    Ok(overview(&guard, &root))
}

// ---- fleet relay ---------------------------------------------------------------------------

fn level_key(level: coverage::Level) -> &'static str {
    match level {
        coverage::Level::None => "none",
        coverage::Level::Thin => "thin",
        coverage::Level::Some => "some",
        coverage::Level::Good => "good",
    }
}

fn station_id(name: &str) -> String {
    let slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    format!("fleet-station-{slug}")
}

/// One round with the relay: post what this station holds (if it changed), read what the others
/// posted, answer their questions with titles.
fn fleet_sync_now(db: &Arc<Mutex<Database>>, root: &Path) -> Result<(), String> {
    let (relay, keys, me, holdings, mut cursor) = {
        let guard = db.lock().map_err(|e| e.to_string())?;
        let settings = load_settings(&guard);
        let (relay, keys) = fleet_keys(&guard, &settings)?;
        let me = station_name(&guard);
        let topics = coverage::build_coverage(&coverage::library_items(&guard, root))
            .into_iter()
            .map(|c| (c.topic.key().to_string(), level_key(c.level).to_string()))
            .collect::<BTreeMap<_, _>>();
        let mut archives: Vec<String> = installed_archives(root)
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            // Packs made from this station's own folders may hold private material: not listed.
            .filter(|n| !n.starts_with(packs::PACK_PREFIX))
            .collect();
        archives.sort();
        archives.truncate(400);
        let cursor: fleet::FleetCursor = load_one(&guard, fleet::CURSOR_KIND, fleet::CURSOR_ID);
        (relay, keys, me, (topics, archives), cursor)
    };
    let now = chrono::Utc::now().timestamp();
    let result = (|| -> Result<(), String> {
        let hash = format!("{:?}", holdings);
        if hash != cursor.holdings_hash || now - cursor.last_sync > 6 * 3600 {
            fleet::post(
                &relay,
                &keys,
                &fleet::FleetMessage::Holdings { station: me.clone(), sent_at: now, topics: holdings.0.clone(), archives: holdings.1.clone() },
            )?;
            cursor.holdings_hash = hash;
        }
        let (messages, newest) = fleet::fetch(&relay, &keys, cursor.after)?;
        cursor.after = newest;
        for message in messages {
            if message.station() == me {
                continue;
            }
            match message {
                fleet::FleetMessage::Holdings { station, sent_at, topics, archives } => {
                    let guard = db.lock().map_err(|e| e.to_string())?;
                    store::put(
                        &guard,
                        fleet::STATION_KIND,
                        Some(&station_id(&station)),
                        &fleet::FleetStation { station, last_seen: sent_at, topics, archives },
                    )?;
                }
                fleet::FleetMessage::Want { query, .. } => {
                    let titles: Vec<String> = {
                        let guard = db.lock().map_err(|e| e.to_string())?;
                        if !load_settings(&guard).mesh_share {
                            continue;
                        }
                        mesh_library::search_local(&guard, root, &query).into_iter().map(|h| h.title).take(8).collect()
                    };
                    if !titles.is_empty() {
                        fleet::post(&relay, &keys, &fleet::FleetMessage::Have { station: me.clone(), sent_at: now, query, titles })?;
                    }
                }
                fleet::FleetMessage::Have { station, sent_at, query, titles } => {
                    let guard = db.lock().map_err(|e| e.to_string())?;
                    let mut answers: FleetAnswers = load_one(&guard, ANSWERS_KIND, ANSWERS_ID);
                    answers.answers.insert(0, FleetAnswer { station, query, titles, at: sent_at });
                    answers.answers.truncate(30);
                    store::put(&guard, ANSWERS_KIND, Some(ANSWERS_ID), &answers)?;
                }
                fleet::FleetMessage::Hunker { station, sent_at, reason } => {
                    let guard = db.lock().map_err(|e| e.to_string())?;
                    let mut state: alerts::AlertState = load_one(&guard, alerts::STATE_KIND, alerts::STATE_ID);
                    state.alerts.push(alerts::Alert {
                        id: format!("fleet:hunker:{station}"),
                        source: "fleet".to_string(),
                        level: alerts::Level::Act,
                        title: format!("{station} started hunker-down"),
                        detail: format!("Reason given: {reason}"),
                        time: sent_at,
                        url: None,
                        distance_km: None,
                    });
                    state.alerts = alerts::merge(state.alerts);
                    store::put(&guard, alerts::STATE_KIND, Some(alerts::STATE_ID), &state)?;
                }
                fleet::FleetMessage::StandDown { station, .. } => {
                    let guard = db.lock().map_err(|e| e.to_string())?;
                    let mut state: alerts::AlertState = load_one(&guard, alerts::STATE_KIND, alerts::STATE_ID);
                    let id = format!("fleet:hunker:{station}");
                    state.alerts.retain(|a| a.id != id);
                    store::put(&guard, alerts::STATE_KIND, Some(alerts::STATE_ID), &state)?;
                }
            }
        }
        Ok(())
    })();
    cursor.last_sync = now;
    cursor.last_error = result.as_ref().err().cloned();
    store::put(&*db.lock().map_err(|e| e.to_string())?, fleet::CURSOR_KIND, Some(fleet::CURSOR_ID), &cursor)?;
    result
}

#[tauri::command]
pub async fn vault_fleet_sync(app: AppHandle, session: crate::auth::CallerSession) -> Result<VaultOverview, String> {
    require_member(&session)?;
    let root = data_root(&app)?;
    ensure_online(&root)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        fleet_sync_now(&db, &root)?;
        let guard = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&guard, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ask the whole fleet, over the relay, whether anyone has something on `query`.
#[tauri::command]
pub async fn vault_fleet_ask(app: AppHandle, session: crate::auth::CallerSession, query: String) -> Result<(), String> {
    require_member(&session)?;
    let query = query.trim().chars().take(200).collect::<String>();
    if mesh_library::query_terms(&query).is_empty() {
        return Err("Say what you are looking for, such as: alternator repair.".to_string());
    }
    let root = data_root(&app)?;
    ensure_online(&root)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let (relay, keys, me) = {
            let guard = db.lock().map_err(|e| e.to_string())?;
            let (relay, keys) = fleet_keys(&guard, &load_settings(&guard))?;
            (relay, keys, station_name(&guard))
        };
        fleet::post(&relay, &keys, &fleet::FleetMessage::Want { station: me, sent_at: chrono::Utc::now().timestamp(), query })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- background work and dashboard ---------------------------------------------------------

/// Dashboard entries from the vault: hunker-down, serious alerts and Home Assistant devices
/// that need attention.
pub fn watch_findings(db: &Database) -> Vec<crate::watch_officer::Finding> {
    let mut out = alerts::findings(&load_one(db, alerts::STATE_KIND, alerts::STATE_ID), &load_one(db, alerts::HUNKER_KIND, alerts::HUNKER_ID));
    if load_settings(db).ha.enabled {
        out.extend(ha::findings(&load_one(db, ha::STATE_KIND, ha::STATE_ID)));
    }
    out
}

// ---- Home Assistant ------------------------------------------------------------------------

fn update_ha_state(db: &Mutex<Database>, change: impl FnOnce(&mut ha::HaState)) {
    let Ok(guard) = db.lock() else { return };
    let mut state: ha::HaState = load_one(&guard, ha::STATE_KIND, ha::STATE_ID);
    change(&mut state);
    let _ = store::put(&guard, ha::STATE_KIND, Some(ha::STATE_ID), &state);
}

/// Send alerts at or above the chosen level to phones, each once (again if it gets worse).
/// Runs without holding the database while it waits for Home Assistant.
fn ha_send_new_alerts(db: &Mutex<Database>, settings: &VaultSettings, alerts_now: &alerts::AlertState) {
    let (sent, station) = match db.lock() {
        Ok(g) => (load_one::<ha::HaState>(&g, ha::STATE_KIND, ha::STATE_ID).sent, station_name(&g)),
        Err(_) => return,
    };
    let (to_send, mut next) = ha::alerts_to_send(&alerts_now.alerts, settings.ha.notify_level, &sent);
    let mut error = None;
    let mut delivered = false;
    // A burst is capped; the rest wait for the next round.
    for (i, a) in to_send.iter().enumerate() {
        let result = if i < 6 && error.is_none() { ha::send(&settings.ha.url, &settings.ha.webhook_id, &ha::alert_message(a, &station)) } else { Err(String::new()) };
        match result {
            Ok(()) => delivered = true,
            Err(e) => {
                // Not delivered: keep the earlier record so it is tried again.
                match sent.get(&a.id) {
                    Some(l) => next.insert(a.id.clone(), *l),
                    None => next.remove(&a.id),
                };
                if !e.is_empty() && error.is_none() {
                    error = Some(e);
                }
            }
        }
    }
    let now = chrono::Utc::now().timestamp();
    update_ha_state(db, |s| {
        s.sent = next;
        if delivered {
            s.last_sent = now;
        }
        if let Some(e) = error {
            s.last_error = Some(e);
        }
    });
}

/// Send urgent Watch Officer findings to phones. Called by the Watch Officer for findings it has
/// just raised; the sending happens on its own thread.
pub fn ha_send_findings(db: &Database, findings: &[crate::watch_officer::Finding]) {
    let settings = load_settings(db);
    if !settings.ha.enabled || settings.ha.webhook_id.is_empty() || findings.is_empty() {
        return;
    }
    // Vault alerts reach phones through the alert path at the chosen level; skip them here.
    let messages: Vec<ha::Message> = findings
        .iter()
        .filter(|f| !f.id.starts_with("vault.alert."))
        .map(|f| ha::finding_message(f, &station_name(db)))
        .collect();
    let (url, hook) = (settings.ha.url, settings.ha.webhook_id);
    std::thread::spawn(move || {
        for m in messages.iter().take(6) {
            if let Err(e) = ha::send(&url, &hook, m) {
                eprintln!("[athena] home assistant: {e}");
                break;
            }
        }
    });
}

fn ha_sync_now(db: &Mutex<Database>) -> Result<(), String> {
    let (settings, alert_state, hunker) = {
        let g = db.lock().map_err(|e| e.to_string())?;
        (load_settings(&g), load_one::<alerts::AlertState>(&g, alerts::STATE_KIND, alerts::STATE_ID), load_one::<alerts::HunkerState>(&g, alerts::HUNKER_KIND, alerts::HUNKER_ID))
    };
    let h = &settings.ha;
    if h.token.is_empty() {
        return Ok(());
    }
    let readings = ha::read(&h.url, &h.token, &h.entities);
    let published = if h.publish_sensors {
        let headline = match alert_state.alerts.iter().filter(|a| a.level >= alerts::Level::Warning).count() {
            0 => "No serious alerts".to_string(),
            1 => alert_state.alerts.iter().find(|a| a.level >= alerts::Level::Warning).map(|a| a.title.clone()).unwrap_or_default(),
            n => format!("{n} serious alerts"),
        };
        ha::publish(&h.url, &h.token, &ha::sensor_values(&alert_state, &hunker, &headline))
    } else {
        Ok(())
    };
    let now = chrono::Utc::now().timestamp();
    let error = readings.as_ref().err().or(published.as_ref().err()).cloned();
    update_ha_state(db, |s| {
        if let Ok(r) = readings {
            s.readings = r;
            s.last_read = now;
        }
        s.last_error = error.clone();
    });
    error.map_or(Ok(()), Err)
}

/// Check the Home Assistant link: send a test message to phones, then read devices and update
/// ATHENA's sensors.
#[tauri::command]
pub async fn vault_ha_test(app: AppHandle, session: crate::auth::CallerSession) -> Result<VaultOverview, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let (settings, station) = {
            let g = db.lock().map_err(|e| e.to_string())?;
            (load_settings(&g), station_name(&g))
        };
        if settings.ha.url.is_empty() {
            return Err("Enter Home Assistant's address and save first.".to_string());
        }
        let mut problems = Vec::new();
        if !settings.ha.webhook_id.is_empty() {
            let msg = ha::Message {
                title: format!("{station}: test message"),
                message: "ATHENA can reach your phone through Home Assistant.".to_string(),
                level: "info",
                tag: "athena_test".to_string(),
                source: "test".to_string(),
                url: None,
            };
            match ha::send(&settings.ha.url, &settings.ha.webhook_id, &msg) {
                Ok(()) => update_ha_state(&db, |s| s.last_sent = chrono::Utc::now().timestamp()),
                Err(e) => problems.push(e),
            }
        }
        if let Err(e) = ha_sync_now(&db) {
            problems.push(e);
        }
        if !problems.is_empty() {
            update_ha_state(&db, |s| s.last_error = Some(problems.join(" ")));
            return Err(problems.join(" "));
        }
        update_ha_state(&db, |s| s.last_error = None);
        let g = db.lock().map_err(|e| e.to_string())?;
        Ok(overview(&g, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HaLocation {
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
}

/// Use Home Assistant's home location as this station's position: for alert distances, and as
/// the map's starting point.
#[tauri::command]
pub async fn vault_ha_use_location(app: AppHandle, session: crate::auth::CallerSession) -> Result<HaLocation, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let root = data_root(&app)?;
    let db = shared_db(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(&*db.lock().map_err(|e| e.to_string())?);
        let (lat, lon, name) = ha::fetch_location(&settings.ha.url, &settings.ha.token)?;
        {
            let g = db.lock().map_err(|e| e.to_string())?;
            let mut s = load_settings(&g);
            s.alert_lat = Some(lat);
            s.alert_lon = Some(lon);
            store::put(&g, SETTINGS_KIND, Some(SETTINGS_ID), &s)?;
        }
        let mut c = crate::config::load_config(&root);
        c.last_known_gps_lat = Some(lat);
        c.last_known_gps_lng = Some(lon);
        crate::config::save_config(&root, &c)?;
        Ok(HaLocation { lat, lon, name })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Delete the saved Home Assistant token.
#[tauri::command]
pub fn vault_ha_forget_token(app: State<'_, AppState>, session: crate::auth::CallerSession) -> Result<VaultSettings, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let mut s = load_settings(&db);
    s.ha.token.clear();
    store::put(&db, SETTINGS_KIND, Some(SETTINGS_ID), &s)?;
    Ok(s.for_display())
}

/// While the station is online: keep alerts fresh, keep the fleet in step, and look for newer
/// archive releases. Each part runs only if its setting is on, and never when the station is
/// set to stay offline.
pub fn spawn_background(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_alerts = 0i64;
        let mut last_fleet = 0i64;
        let mut last_updates = 0i64;
        let mut last_ha = 0i64;
        // Let the app finish starting before the first round.
        std::thread::sleep(std::time::Duration::from_secs(90));
        loop {
            let now = chrono::Utc::now().timestamp();
            let ready = (|| {
                let state = app.try_state::<AppState>()?;
                let root = data_root(&app).ok()?;
                let settings = load_settings(&*state.db.lock().ok()?);
                Some((state.db.clone(), root, settings))
            })();
            if let Some((db, root, settings)) = ready {
                // Home Assistant is usually on the home network: no internet check.
                if settings.ha.enabled && now - last_ha >= 5 * 60 {
                    last_ha = now;
                    if let Err(e) = ha_sync_now(&db) {
                        eprintln!("[athena] home assistant: {e}");
                    }
                }
                let wants_network = settings.alerts_enabled || settings.fleet_enabled || settings.auto_update;
                if wants_network && ensure_online(&root).is_ok() && crate::connectivity::network_reachable() {
                    if settings.alerts_enabled && now - last_alerts >= 15 * 60 {
                        last_alerts = now;
                        if let Err(e) = refresh_alerts_now(&db) {
                            eprintln!("[athena] vault alerts: {e}");
                        }
                    }
                    if settings.fleet_enabled && now - last_fleet >= 5 * 60 {
                        last_fleet = now;
                        if let Err(e) = fleet_sync_now(&db, &root) {
                            eprintln!("[athena] vault fleet: {e}");
                        }
                    }
                    if settings.auto_update && now - last_updates >= 24 * 3600 {
                        last_updates = now;
                        if let Err(e) = check_updates_now(&db, &root) {
                            eprintln!("[athena] vault updates: {e}");
                        } else {
                            let _ = app.emit("athena:vault-updates-ready", serde_json::json!({}));
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_bytes(512), "512 bytes");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(326 * 1024 * 1024), "326 MB");
        assert_eq!(human_bytes(49 * 1024 * 1024 * 1024), "49.0 GB");
    }

    #[test]
    fn defaults_keep_the_station_quiet_until_asked() {
        let s = VaultSettings::default();
        assert!(!s.alerts_enabled && !s.fleet_enabled && !s.auto_update, "nothing contacts the internet by default");
        assert!(s.mesh_share && s.auto_import_scan);
        assert_eq!(s.station_position(), None);
        // Older saved settings without the newer fields still load.
        let old: VaultSettings = serde_json::from_str(r#"{"meshShare":false}"#).unwrap();
        assert!(!old.mesh_share && old.auto_import_scan);
    }

    #[test]
    fn pack_lookup_refuses_anything_that_is_not_a_plain_pack_name() {
        let dir = tempfile::tempdir().unwrap();
        let s = VaultSettings::default();
        assert!(find_pack(dir.path(), &s, "../athena-pack_x_2026-11-12.zim").is_err());
        assert!(find_pack(dir.path(), &s, "wikipedia_en_all_2026-01.zim").is_err());
        assert!(find_pack(dir.path(), &s, "athena-pack_x_2026-11-12.zim").unwrap_err().contains("not on a connected drive"));
    }
}
